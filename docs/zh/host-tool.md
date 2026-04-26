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
- 默认终端输出保持简洁：有 `--channel` 时只显示该 channel 的原始 payload；无
  `--channel` 时普通终端字节原样保留，mux record 以 `chN> ` 标识来源。
- 完整 mux frame 诊断写入系统临时目录下的 `wiremux` 日志文件；启动时 host 会打印
  一行 `wiremux> diagnostics: <path>` 指出文件位置。
- 诊断日志包含 frame metadata 和 `payload_type`；manifest 会以
  `wiremux.v1.DeviceManifest` 标识。
- batch frame 会以 `wiremux.v1.MuxBatch` 标识；host 会根据 compression metadata
  解压并逐条显示其中的 channel record，batch summary 和完整 record metadata 写入
  diagnostics 日志。
- 非 mux 字节按普通终端输出保留。
- 构造 host-to-device input `MuxEnvelope`，并通过同一个 `WMUX` frame 格式发送到指定 channel。

## 输出格式

过滤单个 channel 时，host 不会给 payload 额外添加前缀或换行：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
```

这类模式适合 console channel，因为 payload 中的 `CRLF`、`CR`、`LF` 会按实际换行显示，
不会被打印成 `\r` 或 `\n` 字符串。

不指定 `--channel` 时，host 会保留普通 terminal bytes，并用 `chN> ` 标识 decoded mux
record 的 channel：

```text
ch3> mock stress seq=090 component=wiremux
```

如果一个 channel 的可见行尚未结束就切换到另一个 channel，host 会为可读性插入一个独占
一行的提示：

```text
ch1> booting subsystem
wiremux> continued after partial ch1 line
ch2> sensor ready
```

这行由 host 生成，只表示 display 为避免跨 channel 混行而补了一次展示换行，不代表设备
payload 或协议 decode 错误。

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
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_stress
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_diag
```

观察其他 channel：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 2 --line mux_log
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 3 --line mux_hello
```

`--channel 2` 应看到 log adapter 输出，`--channel 3` 应看到 telemetry 输出。
`mux_manifest` 会触发 channel 0 的 protobuf manifest 输出；当前 CLI 会显示
`payload_type` 和 payload 摘要，后续可增加结构化 manifest decode。
`mux_diag` 会输出 batch/compression 统计，包含 raw bytes、encoded bytes、
ratio、encode_us、decode_ok、fallback_count 和 heap_peak。
`mux_stress` 会向 channel 2 和 channel 3 发送相同的高重复 mock payload，便于在
115200 等实际波特率下比较 heatshrink 与 LZ4。

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
