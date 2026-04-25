# 故障排查

## 找不到设备

确认设备路径：

```bash
ls /dev/tty.usbmodem*
ls /dev/tty.usbserial*
```

运行时通过 `--port` 指定实际路径，不要写死 `/dev/tty.usbmodem2101`。

## 没有 mux frame

可能原因：

- ESP32 侧没有调用 `esp_serial_mux_start()`。
- 没有注册输出通道。
- host 连接到了错误设备。
- ESP32 示例还没有烧录到当前板子。

## 普通日志和 mux 数据混在一起

这是预期行为。host 解析器只处理带 `ESMX` magic 且 length/CRC 校验通过的 mux frame，其他字节保留为普通终端输出。

## ESP-IDF 构建失败

确认：

- ESP-IDF 版本为 v5.4 或更新。
- 当前 shell 已执行 ESP-IDF export 脚本。
- 从 `sources/esp32/examples/console_mux_demo` 目录执行 `idf.py build`。

## 日志重复

log adapter 默认 `tee_to_previous = true`，因此日志会同时输出到原始 ESP log backend 和 mux channel。需要避免重复时，后续可以把该配置设为 false。

`console_mux_demo` 示例中设置了 `tee_to_previous = false`，因此安装 log adapter 之后的应用日志主要通过 mux frame 输出。安装 adapter 之前的启动日志仍然会按 ESP-IDF 原始日志输出。

## idf.py monitor 中看到 ESMX 乱码

这是预期现象。ESP32 侧输出的是：

```text
普通 ESP-IDF 日志 + ESMX 二进制帧 + protobuf envelope
```

`idf.py monitor` 不理解 mux 协议，所以会把二进制 frame header 和 protobuf 字节显示成乱码。请使用 host 工具读取同一个串口，host 会通过 magic、length、CRC 识别 mux frame。
