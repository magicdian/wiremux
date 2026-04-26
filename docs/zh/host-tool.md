# Host CLI

Host 侧首期使用 Rust 实现，目标是单文件可执行程序。

当前命令形态：

```bash
wiremux listen --port /dev/tty.usbmodem2101 --baud 115200
wiremux send --port /dev/tty.usbmodem2101 --channel 1 --line help
wiremux listen --port /dev/tty.usbmodem2101 --channel 1 --line help
```

当前能力：

- 使用 `serialport` backend 打开指定设备路径，并配置波特率。
- 从 mixed stream 中扫描 `WMUX` magic。
- 校验 version、length、CRC32。
- C core 侧也提供同等的单帧 decode/validate API，ESP 入站路径复用该公共规则。
- 有效 mux frame 输出摘要。
- frame 摘要包含 `payload_type`；manifest 会以 `wiremux.v1.DeviceManifest` 标识。
- 非 mux 字节按普通终端输出保留。
- 构造 host-to-device input `MuxEnvelope`，并通过同一个 `WMUX` frame 格式发送到指定 channel。

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
`mux_manifest` 会触发 channel 0 的 protobuf manifest 输出；当前 CLI 会显示
`payload_type` 和 payload 摘要，后续可增加结构化 manifest decode。

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
sources/host/target/release/wiremux
```

## 后续计划

- 添加 capture/replay 子命令。
- 协议稳定后再加入 `ratatui` TUI。
- 增加 service/broker 模式，由一个 host 进程独占真实串口并向多个 frontend 分发 channel。
- 在 service/broker 基础上支持 Unix PTY 暴露，让用户用 `screen`、`minicom` 等工具打开单独 channel。
- Windows native virtual COM 支持进入长期 roadmap，短期不作为首期跨平台虚拟设备目标。
