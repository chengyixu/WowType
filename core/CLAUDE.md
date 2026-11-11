# core

平台无关的 Rust 核心：引擎（候选、打分、学习）和输入会话（按键 → 编辑区 / 候选 / 上屏），以及 wowtype.conf 的键名和取值。两个前端都只依赖它。

- 关键文件：`src/session.rs`（会话）、`src/session_ffi.rs`（会话 C ABI）、`src/config.rs`（设置的唯一来源）、`src/ffi.rs`（引擎 C ABI）、`include/wowtype_core.h`（契约头文件）、`tests/session_keys.rs`（42 个行为向量，和 fcitx5 的 key_harness 同源）。
- 不变式：C ABI 只加不改；会话和引擎同一线程；返回的字符串在下一次改状态的 `wowtype_session_*` 前有效；学习每 20 次要求存盘，reset/commit_composition 时把没存的学习也要求存盘；中英文模式、上文、学习计数在会话间共用（每线程一组）。
- 依赖：无（voice 是可选 feature）。被 fcitx5-addon 和 macos 依赖。
