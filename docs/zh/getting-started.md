# 快速开始

`wiremux` 的目标是在不改硬件的情况下，把 ESP32 的单个串口/USB CDC/JTAG 风格连接拆成多个软件通道。

当前首期结构：

- ESP32 侧组件：`sources/esp32/components/esp-wiremux`
- ESP32 示例：`sources/esp32/examples/esp_wiremux_console_demo`
- Host 侧 Rust CLI：`sources/host`

## Host 侧运行

当前 host 工具提供非 TUI CLI 和 ratatui TUI。CLI 适合脚本和回归验证；TUI 适合交互式
调试、channel 过滤切换和 console line-mode 输入。

```bash
cd sources/host
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
cargo run -- tui --port /dev/tty.usbmodem2101 --baud 115200
```

`/dev/tty.usbmodem2101` 是当前开发设备名，实际使用时可以替换成自己的设备路径。macOS 上如果传入 `/dev/tty.usbmodem*`，host 工具会优先尝试配对的 `/dev/cu.usbmodem*`。

## ESP32 侧运行

在已配置 ESP-IDF v5.4 或更新版本的 shell 中：

```bash
cd sources/esp32/examples/esp_wiremux_console_demo
idf.py set-target esp32s3
idf.py build flash monitor
```

如果你之前已经生成过 `sdkconfig`，`sdkconfig.defaults` 的新配置不会自动覆盖旧配置。ESP32-S3 USB Serial/JTAG 场景建议重新生成：

```bash
rm -f sdkconfig
rm -rf build
idf.py set-target esp32s3
idf.py build flash monitor
```

如果本机没有 `idf.py`，只能先做源码检查，不能完成 ESP-IDF 构建验证。

示例启动后不会立即退出。它会注册 ESP-IDF console 命令，并每 2 秒通过 mux telemetry channel 输出一条示例数据，方便 host 工具在设备 reset 后继续观察数据。

line-mode console 通过 mux channel 1 收发。烧录完成后，不要让 `idf.py monitor` 和 host 工具同时占用同一个串口；关闭 monitor 后运行 host listen/send。

## 当前限制

- Host CLI 使用 `serialport` backend 打开 macOS/Linux/Windows 串口。
- ESP32 侧首期实现 line-mode console adapter。
- `ratatui` TUI 不在首期范围内。
- panic、early boot、ROM log 捕获不在首期范围内。
