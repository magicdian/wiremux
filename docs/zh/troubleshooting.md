# 故障排查

## 找不到设备

确认设备路径：

```bash
ls /dev/tty.usbmodem*
ls /dev/tty.usbserial*
ls /dev/cu.usbmodem*
ls /dev/cu.usbserial*
```

运行时通过 `--port` 指定实际路径，不要写死 `/dev/tty.usbmodem2101`。macOS 上优先使用 `/dev/cu.*`；如果传入 `/dev/tty.*`，host 工具会先尝试配对的 `/dev/cu.*`。

## 没有 mux frame

可能原因：

- ESP32 侧没有调用 `esp_wiremux_start()`。
- 没有注册输出通道。
- host 连接到了错误设备。
- ESP32 示例还没有烧录到当前板子。

## 普通日志和 mux 数据混在一起

这是预期行为。host 解析器只处理带 `WMUX` magic 且 length/CRC 校验通过的 mux frame，其他字节保留为普通终端输出。

## ESP-IDF 构建失败

确认：

- ESP-IDF 版本为 v5.4 或更新。
- 当前 shell 已执行 ESP-IDF export 脚本。
- 从 `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo` 目录执行 `idf.py build`。

## 日志重复

log adapter 默认 `tee_to_previous = true`，因此日志会同时输出到原始 ESP log backend 和 mux channel。需要避免重复时，后续可以把该配置设为 false。

`esp_wiremux_console_demo` 示例中设置了 `tee_to_previous = false`，因此安装 log adapter 之后的应用日志主要通过 mux frame 输出。安装 adapter 之前的启动日志仍然会按 ESP-IDF 原始日志输出。

## idf.py monitor 中看到 WMUX 乱码

这是预期现象。ESP32 侧输出的是：

```text
普通 ESP-IDF 日志 + WMUX 二进制帧 + protobuf envelope
```

`idf.py monitor` 不理解 mux 协议，所以会把二进制 frame header 和 protobuf 字节显示成乱码。请使用 host 工具读取同一个串口，host 会通过 magic、length、CRC 识别 mux frame。

## send 后没有 console 回复

确认：

- `esp_wiremux_console_demo` 已烧录并重启。
- 没有同时运行 `idf.py monitor` 占用同一个端口。
- 单进程发送并监听 channel 1，例如：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --channel 1 --line help
```

- 如果要看命令触发的其他 channel，不要再开第二个进程占用串口。使用 `--send-channel 1` 指定发送到 console，再用 `--channel` 过滤目标输出：

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --send-channel 1 --channel 2 --line mux_log
cargo run -- listen --port /dev/tty.usbmodem2101 --send-channel 1 --channel 3 --line mux_hello
```

## 启动时在 usb_serial_jtag_read_bytes 崩溃

如果崩溃栈指向 `usb_serial_jtag_read_bytes()`，说明 USB Serial/JTAG driver 没有在 mux RX task 启动前安装。当前组件在使用默认 USB Serial/JTAG transport 时会自动安装 driver；如果你提供了自定义 transport，需要在自己的初始化代码里先完成对应外设 driver 初始化。
