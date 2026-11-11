# voice-daemon

wowtype-voiced：语音识别的常驻守护进程 crate，负责录音捕获和把识别结果交给输入法。

- 关键文件：`src/`。
- 不变式：不联网；录音只在用户按住语音键时进行。
- 依赖：voice（wowtype-voice，feature capture）。
