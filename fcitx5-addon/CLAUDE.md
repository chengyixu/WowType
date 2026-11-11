# fcitx5-addon

Linux 前端（fcitx5 插件，C++）。这一版行为是输入会话 ABI 的对照来源；以后迁到会话 ABI（先编译开关两条路并存，key_harness 都跑通后删掉 C++ 里的会话）。

- 关键文件：`src/wowtype_engine.cpp`（行为来源）、`test/key_harness.cpp`（行为向量，与 core/tests/session_keys.rs 同源）。
- 不变式：行为照此实现；fcitx5 >= 5.1。
- 依赖：core（C ABI）。
