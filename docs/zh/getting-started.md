# 快速开始

`esp-serial-mux` 的目标是在不改硬件的情况下，把 ESP32 的单个串口/USB CDC/JTAG 风格连接拆成多个软件通道。

当前首期结构：

- ESP32 侧组件：`sources/esp32/components/esp_serial_mux`
- ESP32 示例：`sources/esp32/examples/console_mux_demo`
- Host 侧 Rust CLI：`sources/host`

## Host 侧运行

当前 host 工具是非 TUI CLI，首期用于验证协议解析和 mixed stream 行为。

```bash
cd sources/host
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200
```

`/dev/tty.usbmodem2101` 是当前开发设备名，实际使用时可以替换成自己的设备路径。

## ESP32 侧运行

在已配置 ESP-IDF v5.4 或更新版本的 shell 中：

```bash
cd sources/esp32/examples/console_mux_demo
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

示例启动后不会立即退出。它会启动 ESP-IDF console REPL，并每 2 秒通过 mux telemetry channel 输出一条示例数据，方便 host 工具在设备 reset 后继续观察数据。

## 当前限制

- Host CLI 暂时只打开设备路径并读取字节流，尚未配置平台串口 termios 参数。
- ESP32 侧首期实现 line-mode console adapter。
- `ratatui` TUI 不在首期范围内。
- panic、early boot、ROM log 捕获不在首期范围内。
