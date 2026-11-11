# macos

macOS 前端（Swift，InputMethodKit）。只有按键翻译、候选框、安装、设置这些平台事；输入行为在 core 的会话里。

- 关键文件：`Sources/WowTypeKit/`（不碰 IMK，可单测：按键翻译、候选框位置、设置文件、安装步骤）、`Sources/WowTypeEngine/`（引擎/会话封装）、`Sources/WowTypeIME/`（IMKServer、输入控制器、候选框、安装窗口、设置窗口）、`Tests/WowTypeKitTests/`（93 个测试）、`scripts/{verify,build,package,guards}.sh`。
- 不变式：打的字不进 public 日志（guards.sh）；Command 组合键交给应用；deactivate/commitComposition 时已选词 + 原始字母上屏；词库快照写 ~/Library/Caches/<bundle id>/，不写进签过名的 .app；装完才关旧进程；`--install` 在系统等批准时算成功并指路。
- 依赖：core（C ABI，经 CWowTypeCore 模块）。
