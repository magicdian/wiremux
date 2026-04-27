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

首期推荐使用 line-mode：

- Host 侧发送完整命令行。
- ESP32 侧验证 `WMUX` frame、CRC、`MuxEnvelope direction=input` 和 channel 方向后调用 `esp_console_run()`。
- 命令注册逻辑继续使用原来的 `esp_console_cmd_register()`。

示例：

```c
esp_wiremux_console_config_t console_config;
esp_wiremux_console_config_init(&console_config);
console_config.channel_id = 1;
console_config.mode = ESP_WIREMUX_CONSOLE_MODE_LINE;
ESP_ERROR_CHECK(esp_wiremux_bind_console(&console_config));
```

执行一行命令：

```c
// Host:
// wiremux listen --port /dev/tty.usbmodem2101 --channel 1 --line help
```

`esp_wiremux_console_demo` 中 channel 1 已注册为 console input/output。`help` 和 `hello` 命令会把输出写回 channel 1。`mux_manifest` 会在 channel 1 返回回执并在 system channel 0 输出 `wiremux.v1.DeviceManifest` protobuf manifest，`mux_hello` 会在 channel 1 返回回执并在 telemetry channel 3 输出示例数据，`mux_log` 会在 channel 1 返回回执并触发 log channel 2 输出。

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
`PASSTHROUGH` 首期返回 `ESP_ERR_NOT_SUPPORTED`，但 public API 和 manifest 已经为后续全透传保留位置，避免后续破坏用户项目。
