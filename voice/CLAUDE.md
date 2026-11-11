# voice

离线语音识别 crate（wowtype-voice）：本机识别，不上传录音。macOS v1 不带它（core 用 --no-default-features 构建）。

- 关键文件：`src/wtlocal/`（本地识别内核）、`src/bin/asr_eval.rs`。
- 不变式：识别在本机；不联网。
- 依赖：无。被 voice-daemon 依赖，被 core 以 feature 方式可选依赖。
