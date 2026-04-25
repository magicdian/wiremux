# Host CLI

Host 侧首期使用 Rust 实现，目标是单文件可执行程序。

当前命令形态：

```bash
esp-serial-mux listen --port /dev/tty.usbmodem2101 --baud 115200
```

当前能力：

- 打开指定设备路径。
- 从 mixed stream 中扫描 `ESMX` magic。
- 校验 version、length、CRC32。
- 有效 mux frame 输出摘要。
- 非 mux 字节按普通终端输出保留。

## 后续计划

- 添加真正的串口配置 backend。
- 添加 capture/replay 子命令。
- 解码 protobuf envelope。
- 协议稳定后再加入 `ratatui` TUI。
