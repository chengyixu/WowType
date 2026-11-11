# CONTEXT

WowType 是开源中文拼音输入法：Rust 核心 + Linux（fcitx5）和 macOS（InputMethodKit）两个前端。本仓库原主是 [Gloridust/WowType](https://github.com/Gloridust/WowType)（MIT OR Apache-2.0）；macOS 版由 Panor 维护并向上游提 PR。

## 构建前提

- Rust（stable），目标 `rustup target add aarch64-apple-darwin x86_64-apple-darwin`（macOS 通用二进制）。
- macOS：Xcode 16+（Swift 6，SwiftPM，无 xcodeproj）。Linux：fcitx5 >= 5.1、CMake。
- 数据：`scripts/make-data.sh` 生成 `dist/data/`（rime-ice 词库 + 热词 + words.lm 语言模型）；本机已有文件可 `WOWTYPE_LM_FILE=… scripts/make-data.sh` 跳过下载。

## 直接验证命令（没有 CI；验证都在本机跑）

```bash
macos/scripts/verify.sh        # 守卫 + cargo test --workspace --locked + C ABI 冒烟（93 符号）+ swift test + 编译输入法程序
./scripts/abi-smoke.sh         # 头文件与 dylib 符号漂移
macos/scripts/build.sh         # 通用二进制 macos/build/WowType.app
macos/scripts/build.sh --install
```

## 签名与公证（发布方提供身份，仓库里不写死）

| 变量 | 作用 | 默认 |
|---|---|---|
| `WOWTYPE_BUNDLE_ID` | bundle id，要含 `.inputmethod.` | `io.github.gloridust.inputmethod.WowType`（Panor 发布用 `tech.panor.inputmethod.WowType`） |
| `WOWTYPE_SIGN_IDENTITY` | codesign 身份 | `-`（临时签名，只本机） |
| `WOWTYPE_NOTARY_PROFILE` | notarytool 钥匙串配置；Panor 用 `asc notarization submit` | 不公证 |
| `WOWTYPE_DATA_DIR` | 词库目录 | `dist/data` |

发布流程：`package.sh` 签名（hardened runtime）→ 公证 → 装订 → `spctl -a -t open --context context:primary-signature` 确认「Notarized Developer ID」。

## 数据落在哪（macOS）

- 程序：`~/Library/Input Methods/WowType.app`（只装给当前用户，不用管理员密码）
- 设置：`~/Library/Application Support/WowType/wowtype.conf`（改了马上生效）
- 用户数据：`~/Library/Application Support/WowType/` 的 `custom.tsv`、`user.tsv`、`history.tsv`
- 词库快照：`~/Library/Caches/<bundle id>/`（不写进签过名的 .app）

## 上线核对（deploy parity）

- 开发机就是发布机（同一台 macOS）；DMG 在本机打好、公证、装订后再上传。
- `macos/scripts/verify.sh` 通过 = 可以打包；打完包还要 `package.sh` 里的 spctl 检查过才算发布可用。
- 隐私：打的字不进日志（guards.sh 有门），macOS 版不接互译（不联网）、不带语音（`--no-default-features`）。
