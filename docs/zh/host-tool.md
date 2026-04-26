# Host CLI

Host 侧首期使用 Rust 实现，目标是单文件可执行程序。

当前命令形态：

```bash
esp-serial-mux listen --port /dev/tty.usbmodem2101 --baud 115200
esp-serial-mux send --port /dev/tty.usbmodem2101 --channel 1 --line help
esp-serial-mux listen --port /dev/tty.usbmodem2101 --channel 1 --line help
```

当前能力：

- 使用 `serialport` backend 打开指定设备路径，并配置波特率。
- 从 mixed stream 中扫描 `ESMX` magic。
- 校验 version、length、CRC32。
- 有效 mux frame 输出摘要。
- 非 mux 字节按普通终端输出保留。
- 构造 host-to-device input `MuxEnvelope`，并通过同一个 `ESMX` frame 格式发送到指定 channel。

## 端口选择

macOS 上 ESP32 USB Serial/JTAG 通常同时出现 `/dev/tty.*` 和 `/dev/cu.*`。如果传入的是 `/dev/tty.usbmodem*`，host 工具会优先尝试配对的 `/dev/cu.usbmodem*`，因为 `cu.*` 更适合应用程序主动连接。

Linux 常见路径是 `/dev/ttyACM0` 或 `/dev/ttyUSB0`。Windows 常见路径是 `COM3`、`COM4` 这类端口名。

不要在代码或脚本里写死 `/dev/tty.usbmodem2101`；它只是本机示例。

## Console line-mode 验收

单终端发送并监听 console channel：

```bash
cd sources/host
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
```

同样方式执行其他命令：

```bash
cd sources/host
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_manifest
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_hello
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_log
```

观察其他 channel：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 2 --line mux_log
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 3 --line mux_hello
```

`--channel 2` 应看到 log adapter 输出，`--channel 3` 应看到 telemetry 输出。

如果想在一次运行中看到所有 channel，不要传 `--channel`：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --line mux_log
```

## 发布构建

host 工具是单个 Rust 可执行文件：

```bash
cd sources/host
cargo build --release
```

构建产物位于：

```text
sources/host/target/release/esp-serial-mux
```

## 后续计划

- 添加 capture/replay 子命令。
- 协议稳定后再加入 `ratatui` TUI。
