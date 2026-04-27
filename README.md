# Wiremux

[简体中文](README_CN.md)

[![Version](https://img.shields.io/badge/version-2604.27.2-blue)](VERSION)
[![License](https://img.shields.io/badge/license-Apache--2.0-green)](LICENSE)

Wiremux is a lightweight channel multiplexer for serial-style byte streams. It lets one UART, USB CDC, USB Serial/JTAG, TCP bridge, or other ordered byte transport carry multiple logical channels at the same time, so logs, console commands, telemetry, and structured diagnostics do not have to fight over one raw stream.

The current reference device integration is an ESP32/ESP-IDF component and demo, but the protocol core is intentionally platform-neutral C code.

The repository contains:

- A portable C protocol core in `sources/core/c`.
- An ESP-IDF adapter component in `sources/esp32/components/esp-wiremux`.
- An ESP-IDF console demo in `sources/esp32/examples/esp_wiremux_console_demo`.
- A Rust host tool in `sources/host` with `listen`, `send`, and interactive TUI modes.

## Why Wiremux

Serial development often starts with one connection carrying everything: boot logs, application logs, console input, diagnostic text, and ad-hoc telemetry. That works until tools need to filter one stream, send commands while watching output, or distinguish data sources after the device reconnects.

Wiremux keeps the transport simple but adds a small framed protocol:

- Channel 0 carries system/control messages such as device manifests.
- Channel 1 can carry line-mode console input and output in the ESP-IDF demo.
- Channel 2 can carry log output in the ESP-IDF demo.
- Channel 3 and later channels can carry telemetry, diagnostics, binary payloads, or application-specific data.
- Host tools can filter by channel, preserve ordinary terminal bytes, and request a manifest for channel names and capabilities.

## Screenshots

| All channels | Console channel |
| --- | --- |
| ![TUI showing all channels](docs/images/tui_all.png) | ![TUI filtered to console channel](docs/images/tui_channel1.png) |

| Log channel | UTF-8 channel |
| --- | --- |
| ![TUI filtered to log channel](docs/images/tui_channel2.png) | ![TUI displaying UTF-8 channel output](docs/images/tui_utf8.png) |

## Device Integration

Wiremux has two layers:

- `sources/core/c`: portable frame, envelope, manifest, batch, and compression primitives.
- `sources/esp32/components/esp-wiremux`: ESP-IDF integration built on top of the portable core.

Use the portable core directly when you are building a new platform adapter:

```c
#include "wiremux_frame.h"

uint8_t payload[] = {0x01, 0x02, 0x03};
uint8_t frame[128];
size_t written = 0;

wiremux_frame_header_t header = {
    .version = WIREMUX_FRAME_VERSION,
    .flags = 0,
};

wiremux_status_t status = wiremux_frame_encode(&header,
                                               payload,
                                               sizeof(payload),
                                               frame,
                                               sizeof(frame),
                                               &written);
```

For ESP-IDF, initialize the adapter, register application channels, then write records to those channels:

```c
#include "esp_wiremux.h"

void app_main(void)
{
    esp_wiremux_config_t config;
    esp_wiremux_config_init(&config);
    ESP_ERROR_CHECK(esp_wiremux_init(&config));

    esp_wiremux_channel_config_t telemetry = {
        .channel_id = 3,
        .name = "telemetry",
        .description = "Application telemetry",
        .directions = ESP_WIREMUX_DIRECTION_OUTPUT,
        .default_payload_kind = ESP_WIREMUX_PAYLOAD_KIND_TEXT,
        .interaction_mode = ESP_WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
    };
    ESP_ERROR_CHECK(esp_wiremux_register_channel(&telemetry));

    ESP_ERROR_CHECK(esp_wiremux_start());
    ESP_ERROR_CHECK(esp_wiremux_write_text(3,
                                           ESP_WIREMUX_DIRECTION_OUTPUT,
                                           "temperature=24.8\n",
                                           100));
}
```

Bind an ESP-IDF console to a Wiremux line-mode channel:

```c
#include "esp_wiremux_console.h"

esp_wiremux_console_config_t console_config;
esp_wiremux_console_config_init(&console_config);
console_config.channel_id = 1;
console_config.mode = ESP_WIREMUX_CONSOLE_MODE_LINE;
ESP_ERROR_CHECK(esp_wiremux_bind_console(&console_config));
```

Capture ESP log output on a separate channel:

```c
#include "esp_wiremux_log.h"

esp_wiremux_log_config_t log_config;
esp_wiremux_log_config_init(&log_config);
log_config.channel_id = 2;
log_config.tee_to_previous = true;
ESP_ERROR_CHECK(esp_wiremux_bind_esp_log(&log_config));
```

## Host Tool

Build and run the Rust host tool:

```bash
cd sources/host
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
cargo run -- tui --port /dev/tty.usbmodem2101 --baud 115200
```

Common commands:

- `listen`: decode mixed terminal output and Wiremux frames from a serial port.
- `listen --channel N`: print only decoded payload bytes from one channel.
- `listen --line TEXT`: send one host-to-device input frame after connecting, then continue listening on the same serial handle.
- `send`: send one input frame and exit.
- `tui`: open a ratatui interface for channel filtering, scrollback, manifest display, and line-mode input.

On macOS, passing `/dev/tty.usbmodem*` is accepted, but the host tool prefers the paired `/dev/cu.usbmodem*` path for application-side connections.

## ESP-IDF Demo

Use ESP-IDF v5.4 or newer:

```bash
cd sources/esp32/examples/esp_wiremux_console_demo
idf.py set-target esp32s3
idf.py build flash monitor
```

After flashing, stop `idf.py monitor` before starting the host tool. Most serial devices should be owned by only one process at a time.

Run a console command through Wiremux channel 1:

```bash
cd sources/host
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
```

Observe other demo channels while sending commands to the console channel:

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 2 --line mux_log
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 3 --line mux_hello
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 4 --line mux_utf8
```

## Documentation

- [Host CLI](docs/zh/host-tool.md)
- [Getting Started](docs/zh/getting-started.md)
- [ESP-IDF Console Integration](docs/zh/esp-idf-console-integration.md)
- [Troubleshooting](docs/zh/troubleshooting.md)

## License

Wiremux is released under the [Apache License 2.0](LICENSE).

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=magicdian/wiremux&type=Date)](https://www.star-history.com/#magicdian/wiremux&Date)
