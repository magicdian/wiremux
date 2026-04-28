# esp-wiremux

Wiremux 的 ESP-IDF adapter。Wiremux 是一个轻量多路复用层，用于在单个串口类 transport 上承载多个逻辑 channel。

`esp-wiremux` 可以让 ESP32 应用通过一个 USB serial、USB Serial/JTAG、UART 或 CDC 风格连接输出 console、log、telemetry、UTF-8 文本、诊断信息或业务数据。Host 工具可以独立解码和过滤每个逻辑 channel。

该包依赖同版本的 `{{namespace}}/wiremux-core`。

## 添加依赖

在 ESP-IDF 项目的 component manifest 中添加：

```yaml
dependencies:
  {{namespace}}/esp-wiremux: "{{version}}"
```

然后在应用中 include 并初始化 adapter：

```c
#include "esp_wiremux.h"
#include "esp_wiremux_console.h"

void app_main(void)
{
    esp_wiremux_config_t config;
    esp_wiremux_config_init(&config);
    ESP_ERROR_CHECK(esp_wiremux_init(&config));

    esp_wiremux_console_config_t console_config;
    esp_wiremux_console_config_init(&console_config);
    console_config.channel_id = 1;
    console_config.mode = ESP_WIREMUX_CONSOLE_MODE_LINE;
    ESP_ERROR_CHECK(esp_wiremux_bind_console(&console_config));

    ESP_ERROR_CHECK(esp_wiremux_start());
}
```

## Host 使用

监听所有 channel：

```bash
wiremux listen --port /dev/tty.usbmodem2101 --baud 115200
```

在 channel 1 发送一条 console 命令，并继续使用同一个串口 handle 监听输出：

```bash
wiremux listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
```

打开交互式 TUI：

```bash
wiremux tui --port /dev/tty.usbmodem2101 --baud 115200
```

## 示例

该 component 包含 `esp_wiremux_console_demo` Registry example。示例展示
channel 1 line-mode console 输入、channel 2 ESP log 输出、channel 3 telemetry
和 channel 4 UTF-8 文本输出。

## Channel 约定

示例工程使用这些 channel：

- Channel 0：system/control manifest。
- Channel 1：line-mode console。
- Channel 2：ESP log adapter。
- Channel 3：telemetry text。
- Channel 4：UTF-8 text demo。

应用可以注册自己的 channel descriptor 和 input handler。

## 要求

- ESP-IDF v5.4 或更新版本。
- 用于解码 multiplexed output 的 host-side Wiremux 工具。

## 源码

Canonical source: {{repository_url}}/tree/main/sources/vendor/espressif/generic/components/esp-wiremux

Release packaging: {{repository_url}}/blob/main/tools/esp-registry/generate-packages.sh
