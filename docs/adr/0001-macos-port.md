# ADR 0001：macOS 版把输入会话放进 Rust 核心，前端只做平台事

状态：已接受（2026-11，macOS 版首次落地）

## 背景

给 WowType 做 macOS 版时，fcitx5 插件（2400 行 C++）里约 1600 行是平台无关的会话逻辑：分段选词、退格撤销、上屏和学习、emoji/日期/计算器候选、翻页、单按 Shift。如果 Swift 再写一份，两份会慢慢不一样。

## 决定

1. **会话放进核心。** 平台无关的会话写成 `core/src/session.rs`，用新增的 `wowtype_session_*` C ABI 给前端用；行为对照 `fcitx5-addon/src/wowtype_engine.cpp`，向量逐条搬进 `core/tests/session_keys.rs`。librime（鼠须管/小狼毫/ibus-rime 共用）和 Mozc 都这么分：引擎里有会话，前端只翻译按键、画界面。fcitx5 插件这一版不动；以后迁到同一 ABI，两条路用编译开关并存，key_harness 都跑通后删掉 C++ 里的会话。
2. **设置键名只有一份。** `core/src/config.rs` 是 wowtype.conf 的键名、取值、默认值的唯一来源，`wowtype_config_default_ini` 给前端写第一份设置文件，`wowtype_engine_configure` 应用引擎相关键。
3. **自安装的 .app，不用 .pkg。** 没有 Developer ID Installer 证书，也不想要管理员密码：复制到 `~/Library/Input Methods`，`TISRegisterInputSource` + 启用 + 选中；关旧进程放在换文件之后（防止系统按需从旧 .app 再启动旧版本）。第一次装 macOS 会在系统设置里问「允许 WowType 启用 WowType Pinyin」，没批准前 `--install` 算成功并指路，不报失败。
4. **v1 不带语音和互译。** 核心按 `--no-default-features` 构建（无语音 crate，不需要麦克风权限）；互译要联网，v1 不做。表情面板用 macOS 自带的 Ctrl+Cmd+空格。
5. **会话共享状态每线程一组。** 中英文模式、上文、学习计数在 `SessionGroup` 里共用；FFI 层用 thread_local 组，前端不用加新符号。失去焦点/切输入法时把没存的学习要求存盘（对齐插件的 deactivate → maybeSaveUserDict(true)）。

## 后果

- 正面：行为一份实现，两个平台不会漂移；`session_keys.rs` 把插件里几处容易被当成 bug 的行为也钉住了。
- 负面：核心的会话 ABI 要小心演进（只加不改，abi-smoke.sh 守符号漂移）；fcitx5 迁到 ABI 前，C++ 和 Rust 两份会话暂时并存（靠同一批测试向量对齐）。
