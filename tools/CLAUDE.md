# tools

数据转换和 ABI 检查工具：rime2tsv、热词合并、abi_smoke（C 端契约冒烟）。

- 关键文件：`tools/rime2tsv.py`、`tools/add_hotwords.py`、`tools/abi_smoke.c`。
- 不变式：abi_smoke 只测公开契约；转换脚本输入输出都是 UTF-8 文本。
- 依赖：core 的头文件（abi_smoke.c）。
