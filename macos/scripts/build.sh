#!/usr/bin/env bash
# 构建 macos/build/WowType.app（arm64 + x86_64 通用二进制）。
#
#   macos/scripts/build.sh               用 dist/data 的词库（scripts/make-data.sh 生成）
#   macos/scripts/build.sh --demo-data   用 core/data/demo_lexicon.tsv（只供开发，不发布）
#   macos/scripts/build.sh --install     构建后运行 WowType.app --install（装到 ~/Library/Input Methods）
#
# 环境变量（都可以不设）：
#   WOWTYPE_BUNDLE_ID       bundle id，要含 .inputmethod.（默认 io.github.gloridust.inputmethod.WowType）
#   WOWTYPE_SIGN_IDENTITY   codesign 身份（默认 -，临时签名，只能本机用）
#   WOWTYPE_DATA_DIR        词库目录（默认 dist/data）
#   WOWTYPE_VERSION         版本号 x.y.z（默认 core/Cargo.toml 的版本）；在线更新按它比新旧，发布方每次要加
#   WOWTYPE_UPDATE_FEED_URL     在线更新源（Sparkle appcast，必须 https）
#   WOWTYPE_UPDATE_PUBLIC_KEY   更新包 EdDSA 签名的公钥（Sparkle generate_keys 打出的 base64）
#                           这两个要么都给、要么都不给；不给就没有在线更新（菜单里也没有「检查更新」）。
#                           私钥只在发布方的钥匙串里，用 Sparkle 的 sign_update 给 DMG 签名，见 docs/MACOS.md。
#
# 核心用 --no-default-features 构建：不带语音，也就不需要麦克风权限。
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT=$PWD
MAC=$ROOT/macos

die() { echo "build.sh：$*" >&2; exit 1; }

DEMO=0
INSTALL=0
for arg in "$@"; do
  case "$arg" in
    --demo-data) DEMO=1 ;;
    --install) INSTALL=1 ;;
    *) die "不认识的参数 $arg（可用：--demo-data --install）" ;;
  esac
done

BUNDLE_ID=${WOWTYPE_BUNDLE_ID:-io.github.gloridust.inputmethod.WowType}
case "$BUNDLE_ID" in
  *.inputmethod.*) ;;
  *) die "WOWTYPE_BUNDLE_ID=$BUNDLE_ID 不含 .inputmethod.，系统不会把它当输入法" ;;
esac
[[ "$BUNDLE_ID" =~ ^[A-Za-z0-9.-]+$ ]] || die "WOWTYPE_BUNDLE_ID 只能有字母、数字、点和减号"
SIGN=${WOWTYPE_SIGN_IDENTITY:--}
DATA=${WOWTYPE_DATA_DIR:-$ROOT/dist/data}

VERSION=${WOWTYPE_VERSION:-$(sed -n 's/^version = "\(.*\)"$/\1/p' core/Cargo.toml | head -n 1)}
[[ -n "$VERSION" ]] || die "读不到 core/Cargo.toml 的版本"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "版本 $VERSION 不是 x.y.z"

FEED=${WOWTYPE_UPDATE_FEED_URL:-}
EDKEY=${WOWTYPE_UPDATE_PUBLIC_KEY:-}
if [[ -n "$FEED" || -n "$EDKEY" ]]; then
  [[ -n "$FEED" && -n "$EDKEY" ]] || die "WOWTYPE_UPDATE_FEED_URL 和 WOWTYPE_UPDATE_PUBLIC_KEY 要一起给"
  [[ "$FEED" =~ ^https://[A-Za-z0-9.-]+(:[0-9]+)?/[^[:space:]]*$ ]] || die "更新源必须是 https 地址：$FEED"
  [[ "$EDKEY" =~ ^[A-Za-z0-9+/]{43}=$ ]] && [[ $(printf %s "$EDKEY" | base64 -D | wc -c | tr -d ' ') == 32 ]] \
    || die "WOWTYPE_UPDATE_PUBLIC_KEY 不是 32 字节 ed25519 公钥的 base64"
fi

# ---- 先检查数据，缺了就别白编译 ----
S2T=$ROOT/core/data/s2t.tsv
DATA_FILES=()
if [[ $DEMO == 1 ]]; then
  DATA_FILES=("$ROOT/core/data/demo_lexicon.tsv:lexicon.tsv")
else
  for f in lexicon.tsv english.txt words.lm LICENSE SOURCE.txt; do
    DATA_FILES+=("$DATA/$f:$f")
  done
fi
DATA_FILES+=("$S2T:s2t.tsv")
missing=()
for pair in "${DATA_FILES[@]}"; do
  [[ -f "${pair%%:*}" ]] || missing+=("${pair%%:*}")
done
if (( ${#missing[@]} )); then
  printf '缺少数据文件：\n' >&2
  printf '  %s\n' "${missing[@]}" >&2
  die "先运行 scripts/make-data.sh（或用 --demo-data 做开发构建）"
fi
for f in Info.plist.template WowType.entitlements AppIcon.icns MenuIcon.tiff; do
  [[ -f "$MAC/Resources/$f" ]] || die "缺少 macos/Resources/$f"
done

BUILD=$MAC/build
APP=$BUILD/WowType.app
mkdir -p "$BUILD/lib"

# ---- Rust 核心：两个架构的静态库，再合成一个 ----
export MACOSX_DEPLOYMENT_TARGET=13.0
LIBS=()
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  echo "==> cargo build（$target）"
  cargo build --release --locked -p wowtype-core --no-default-features --lib --target "$target"
  LIBS+=("$ROOT/target/$target/release/libwowtype_core.a")
done
CORE_LIB=$BUILD/lib/libwowtype_core.a
lipo -create "${LIBS[@]}" -output "$CORE_LIB"

# ---- Swift：两个架构分别编译链接，再合成一个 ----
# 输入法程序动态链接 Sparkle.framework（通用二进制，两个架构编出来的是同一份），组装时放进 Contents/Frameworks。
# Package.swift 看到 WOWTYPE_CORE_LIB 才声明输入法程序，并按这个完整路径链接核心。
BINS=()
for arch in arm64 x86_64; do
  case $arch in
    arm64) lib="$ROOT/target/aarch64-apple-darwin/release/libwowtype_core.a" ;;
    x86_64) lib="$ROOT/target/x86_64-apple-darwin/release/libwowtype_core.a" ;;
  esac
  scratch=$BUILD/swift-$arch
  echo "==> swift build（$arch）"
  (cd "$MAC" && WOWTYPE_CORE_LIB="$lib" swift build -c release --arch "$arch" --product WowType \
      --scratch-path "$scratch")
  bin=$(cd "$MAC" && WOWTYPE_CORE_LIB="$lib" swift build -c release --arch "$arch" \
      --scratch-path "$scratch" --show-bin-path)
  BINS+=("$bin/WowType")
  SPARKLE=$bin/Sparkle.framework
done
[[ -d "$SPARKLE" ]] || die "swift build 没有产出 Sparkle.framework（$SPARKLE）"

# ---- 组装 .app ----
echo "==> 组装 $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "${BINS[@]}" -output "$APP/Contents/MacOS/WowType"
printf 'APPL????' > "$APP/Contents/PkgInfo"

subst() { sed -e "s/@BUNDLE_ID@/$BUNDLE_ID/g" -e "s/@VERSION@/$VERSION/g" "$1" > "$2"; }
subst "$MAC/Resources/Info.plist.template" "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null || die "生成的 Info.plist 不合法"
for lproj in zh-Hans.lproj en.lproj; do
  mkdir -p "$APP/Contents/Resources/$lproj"
  subst "$MAC/Resources/$lproj/InfoPlist.strings.template" "$APP/Contents/Resources/$lproj/InfoPlist.strings"
  plutil -lint "$APP/Contents/Resources/$lproj/InfoPlist.strings" >/dev/null || die "$lproj/InfoPlist.strings 不合法"
done
if [[ -n "$FEED" ]]; then
  PB=/usr/libexec/PlistBuddy
  $PB -c "Add :SUFeedURL string $FEED" \
      -c "Add :SUPublicEDKey string $EDKEY" \
      -c "Add :SUEnableAutomaticChecks bool true" \
      -c "Add :SUScheduledCheckInterval integer 86400" \
      -c "Add :SUEnableSystemProfiling bool false" "$APP/Contents/Info.plist"
  plutil -lint "$APP/Contents/Info.plist" >/dev/null || die "加了更新源的 Info.plist 不合法"
  echo "在线更新：$FEED"
else
  echo "没有给 WOWTYPE_UPDATE_FEED_URL：这一版没有在线更新"
fi
mkdir -p "$APP/Contents/Frameworks"
ditto "$SPARKLE" "$APP/Contents/Frameworks/Sparkle.framework"
cp "$MAC/Resources/AppIcon.icns" "$MAC/Resources/MenuIcon.tiff" "$APP/Contents/Resources/"
for pair in "${DATA_FILES[@]}"; do
  cp "${pair%%:*}" "$APP/Contents/Resources/${pair##*:}"
done

# ---- 签名 ----
# 先从里往外签 Sparkle（照 Sparkle 文档「Code Signing」里不用 Xcode 时的顺序），再签整个 .app。
# 都用同一个身份：公证要求每个可执行文件都是 Developer ID + hardened runtime + 时间戳，
# hardened runtime 的库验证也要求框架和主程序同一个团队。
COMMON=(--force --sign "$SIGN")
if [[ "$SIGN" != "-" ]]; then
  COMMON+=(--options runtime --timestamp)
fi
echo "==> codesign（${SIGN}）"
SP=$APP/Contents/Frameworks/Sparkle.framework/Versions/B
codesign "${COMMON[@]}" "$SP/XPCServices/Installer.xpc"
codesign "${COMMON[@]}" --preserve-metadata=entitlements "$SP/XPCServices/Downloader.xpc"
codesign "${COMMON[@]}" "$SP/Autoupdate"
codesign "${COMMON[@]}" "$SP/Updater.app"
codesign "${COMMON[@]}" "$APP/Contents/Frameworks/Sparkle.framework"
codesign "${COMMON[@]}" --entitlements "$MAC/Resources/WowType.entitlements" "$APP"
codesign --verify --strict --verbose=2 "$APP"

lipo -archs "$APP/Contents/MacOS/WowType"
for exe in "$APP/Contents/MacOS/WowType" "$SP/Sparkle" "$SP/Autoupdate"; do
  archs=$(lipo -archs "$exe")
  [[ "$archs" == *arm64* && "$archs" == *x86_64* ]] || die "$exe 不是通用二进制（$archs）"
done
echo "已生成 $APP（$BUNDLE_ID $VERSION）"

if [[ $INSTALL == 1 ]]; then
  echo "==> 安装"
  "$APP/Contents/MacOS/WowType" --install
fi
