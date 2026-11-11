# scripts

构建、数据和 ABI 的 shell 脚本：make-data（词库生成）、abi-smoke（头文件↔dylib 符号漂移门）。

- 关键文件：`scripts/make-data.sh`、`scripts/abi-smoke.sh`。
- 不变式：make-data 的 sha256 校验要在 macOS 和 Linux 都能跑（不依赖 GNU 专属选项）；abi-smoke 头文件里声明的函数必须全部导出。
- 依赖：tools（abi_smoke.c）、core 的产物。
