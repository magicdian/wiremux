# ESP-IDF Console 接入

如果现有项目参考 ESP-IDF v5.4 `examples/system/console/advanced`，接入改动应集中在 console 初始化和 REPL 主循环附近。

`esp_wiremux_console_demo` 已引入官方 console REPL 形态，并注册了几个演示命令：

```text
help
hello
mux_manifest
mux_hello
mux_log
```

可以使用 line-mode 或 passthrough mode：

- Host 侧发送完整命令行。
- ESP32 侧验证 `WMUX` frame、CRC、`MuxEnvelope direction=input` 和 channel 方向后调用 `esp_console_run()`。
- 命令注册逻辑继续使用原来的 `esp_console_cmd_register()`。
- Passthrough mode 下，Host 可以逐键发送 framed input；ESP adapter 可选择 raw callback、轻量 line discipline，或 ESP REPL 风格后端。

示例：

```c
esp_wiremux_console_config_t console_config;
esp_wiremux_console_config_init(&console_config);
console_config.channel_id = 1;
console_config.mode = ESP_WIREMUX_CONSOLE_MODE_LINE;
ESP_ERROR_CHECK(esp_wiremux_bind_console(&console_config));
```

Passthrough console 示例：

```c
esp_wiremux_console_config_t console_config;
esp_wiremux_console_config_init(&console_config);
console_config.channel_id = 1;
console_config.mode = ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH;
console_config.passthrough_backend = ESP_WIREMUX_PASSTHROUGH_BACKEND_CONSOLE_LINE_DISCIPLINE;
ESP_ERROR_CHECK(esp_wiremux_bind_console(&console_config));
```

执行一行命令：

```c
// Host:
// wiremux listen --port /dev/tty.usbmodem2101 --channel 1 --line help
```

`esp_wiremux_console_demo` 中 channel 1 已注册为 console input/output。`help` 和 `hello` 命令会把输出写回 channel 1。`mux_manifest` 会在 channel 1 返回回执并在 system channel 0 输出 `wiremux.v1.DeviceManifest` protobuf manifest，`mux_hello` 会在 channel 1 返回回执并在 telemetry channel 3 输出示例数据，`mux_log` 会在 channel 1 返回回执并触发 log channel 2 输出。

Demo 还提供运行时切换命令，不需要重启设备：

```text
mux_console_mode line
mux_console_mode passthrough
```

切换后 demo 会重新发送 manifest，host/TUI 可以看到 channel 1 的 interaction mode 变化。

## 为什么 API 保留 mode

console API 不写死 line-mode：

```c
typedef enum {
    ESP_WIREMUX_CONSOLE_MODE_DISABLED = WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
    ESP_WIREMUX_CONSOLE_MODE_LINE = WIREMUX_CHANNEL_INTERACTION_LINE,
    ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH = WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH,
} esp_wiremux_console_mode_t;
```

interaction mode 由 core/proto 定义，ESP console API 只映射这些通用能力。
`PASSTHROUGH` 是通用 channel 能力，不等同于 ESP-IDF REPL。ESP component 中的
`ESP_WIREMUX_PASSTHROUGH_BACKEND_ESP_REPL` 只是 ESP-facing alias，core 层命名保持为
`WIREMUX_PASSTHROUGH_BACKEND_REPL`。
