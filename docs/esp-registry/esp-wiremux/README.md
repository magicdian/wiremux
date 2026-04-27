# esp-wiremux

ESP-IDF adapter for Wiremux, a lightweight multiplexing layer for carrying multiple logical channels over one serial-style transport.

`esp-wiremux` lets an ESP32 application expose console input/output, logs, telemetry, UTF-8 text, diagnostics, or application data through one USB serial, USB Serial/JTAG, UART, or CDC-like connection. Host tools can decode and filter each logical channel independently.

This package depends on `{{namespace}}/wiremux-core` at the same release version.

## Add to a Project

Add the component dependency to an ESP-IDF component manifest:

```yaml
dependencies:
  {{namespace}}/esp-wiremux: "{{version}}"
```

Then include and initialize the adapter from your application:

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

## Host Usage

Use the Wiremux host tool to listen to all channels:

```bash
wiremux listen --port /dev/tty.usbmodem2101 --baud 115200
```

Send one console command on channel 1 and keep listening on the same serial handle:

```bash
wiremux listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
```

Open the interactive TUI:

```bash
wiremux tui --port /dev/tty.usbmodem2101 --baud 115200
```

## Example

This component includes `esp_wiremux_console_demo` as a Registry example. It
shows line-mode console input on channel 1, ESP log output on channel 2,
telemetry on channel 3, and UTF-8 text output on channel 4.

## Channels

The demo uses these channel conventions:

- Channel 0: system/control manifest.
- Channel 1: line-mode console.
- Channel 2: ESP log adapter.
- Channel 3: telemetry text.
- Channel 4: UTF-8 text demo.

Applications can register their own channel descriptors and input handlers.

## Requirements

- ESP-IDF v5.4 or newer.
- A host-side Wiremux tool for decoding multiplexed output.

## Source

Canonical source: {{repository_url}}/tree/main/sources/esp32/components/esp-wiremux

Release packaging: {{repository_url}}/blob/main/tools/esp-registry/generate-packages.sh
