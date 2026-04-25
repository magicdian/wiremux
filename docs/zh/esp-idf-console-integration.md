# ESP-IDF Console 接入

如果现有项目参考 ESP-IDF v5.4 `examples/system/console/advanced`，接入改动应集中在 console 初始化和 REPL 主循环附近。

`console_mux_demo` 已引入官方 console REPL 形态，并注册了几个演示命令：

```text
help
hello
mux_manifest
mux_hello
mux_log
```

首期推荐使用 line-mode：

- Host 侧发送完整命令行。
- ESP32 侧调用 `esp_console_run()`。
- 命令注册逻辑继续使用原来的 `esp_console_cmd_register()`。

示例：

```c
esp_serial_mux_console_config_t console_config;
esp_serial_mux_console_config_init(&console_config);
console_config.channel_id = 1;
console_config.mode = ESP_SERIAL_MUX_CONSOLE_MODE_LINE;
ESP_ERROR_CHECK(esp_serial_mux_bind_console(&console_config));
```

执行一行命令：

```c
int command_ret = 0;
ESP_ERROR_CHECK(esp_serial_mux_console_run_line("help", &command_ret));
```

## 为什么 API 保留 mode

console API 不写死 line-mode：

```c
typedef enum {
    ESP_SERIAL_MUX_CONSOLE_MODE_DISABLED = 0,
    ESP_SERIAL_MUX_CONSOLE_MODE_LINE = 1,
    ESP_SERIAL_MUX_CONSOLE_MODE_PASSTHROUGH = 2,
} esp_serial_mux_console_mode_t;
```

`PASSTHROUGH` 首期返回 `ESP_ERR_NOT_SUPPORTED`，但 public API 已经为后续全透传保留位置，避免后续破坏用户项目。
