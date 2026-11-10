#!/usr/bin/env bash
# macOS 前端的完整检查，按顺序跑，第一个失败就停：
#   1. 守卫（含自测）和图标检查
#   2. cargo test --workspace --locked
#   3. scripts/abi-smoke.sh（C ABI 冒烟）
#   4. swift test（WowTypeKit 的单元测试）
set -euo pipefail
cd "$(dirname "$0")/../.."

echo "==> 1/4 守卫"
macos/scripts/guards.sh
macos/scripts/make-icons.sh --check

echo "==> 2/4 cargo test"
cargo test --workspace --locked

echo "==> 3/4 C ABI 冒烟"
scripts/abi-smoke.sh

echo "==> 4/4 swift test"
(cd macos && swift test)

echo "全部通过"
