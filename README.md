# Wiremux

[简体中文](README_CN.md)

[![Version](https://img.shields.io/badge/version-2605.8.1-blue)](VERSION)
[![License](https://img.shields.io/badge/license-Apache--2.0-green)](LICENSE)

## Support Wiremux

Wiremux needs support to expand platform coverage. The project does not
currently have the Apple Developer Program subscription required for some macOS
platform work; sponsorship would help fund that subscription and make a
DriverKit-based macOS solution possible. If this project helps your work, you
can sponsor development through [Buy Me a Coffee](https://buymeacoffee.com/magicdian).

Wiremux is a lightweight channel multiplexer for serial-style byte streams. It lets one UART, USB CDC, USB Serial/JTAG, TCP bridge, or other ordered byte transport carry multiple logical channels at the same time, so logs, console commands, telemetry, and structured diagnostics do not have to fight over one raw stream.

The current reference device integration is an ESP32/ESP-IDF component and demo, but the protocol core is intentionally platform-neutral C code.

The repository contains:

- A portable C protocol core in `sources/core/c`.
- An ESP-IDF adapter component in `sources/vendor/espressif/generic/components/esp-wiremux`.
- ESP-IDF examples in `sources/vendor/espressif/generic/examples`: beginner,
  advanced, and professional API demos.
- A Rust host tool in `sources/host/wiremux` with `listen`, `send`, and interactive TUI modes.

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
- `sources/vendor/espressif/generic/components/esp-wiremux`: ESP-IDF integration built on top of the portable core.

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
cd sources/host/wiremux
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
cargo run -- passthrough --port /dev/tty.usbmodem2101 --baud 115200 --channel 1
cargo run -- tui --port /dev/tty.usbmodem2101 --baud 115200 --tui-fps 120
```

Common commands:

- `listen`: decode mixed terminal output and Wiremux frames from a serial port.
- `listen --channel N`: print only decoded payload bytes from one channel.
- `listen --line TEXT`: send one host-to-device input frame after connecting, then continue listening on the same serial handle.
- `send`: send one input frame and exit.
- `passthrough --channel N`: attach to one mux channel and forward key bytes immediately; `Ctrl-]` exits when supported by the terminal, and `Esc` then `x` is the portable exit sequence. `--interactive-backend auto|compat|mio` is optional; `auto` prefers `mio` on Unix and uses `compat` elsewhere.
- `tui`: open a ratatui interface for channel filtering, scrollback, selectable output/status text, manifest display, backend/FPS status, manifest-driven line/passthrough input with a native input cursor, and generic enhanced virtual serial endpoints when enabled; `Ctrl-C`, `Ctrl-]`, or `Esc` then `x` exits. `Left`/`Right` switches status pages outside passthrough mode; `Ctrl-B Left`/`Ctrl-B Right` or `Ctrl-B [`/`Ctrl-B ]` switches status pages without stealing passthrough arrows. `Ctrl-B v` toggles virtual serial for the current session, and `Ctrl-B o` toggles active-channel input ownership between host and virtual serial. `--interactive-backend auto|compat|mio` selects the event backend, and `--tui-fps 60|120` overrides the automatic 60 fps default / Ghostty 120 fps detection.

On macOS, passing `/dev/tty.usbmodem*` is accepted, but the host tool prefers the paired `/dev/cu.usbmodem*` path for application-side connections.

Host global config may also control generic enhanced virtual serial behavior.
Generic host builds do not include this overlay and ignore `[virtual_serial]`.
When `[virtual_serial]` is omitted, generic enhanced, vendor enhanced, and
all-feature builds enable it by default and export every manifest channel.
Output-only channels are read-only; input-capable channels accept virtual serial
writes only after the input owner is switched to virtual serial.

Vendor enhanced ESP32 host builds add a TUI-only enhanced endpoint for matched
Espressif manifests. While `wiremux tui` owns the physical serial port, it
creates a stable `tty.wiremux-esp-enhanced` alias; use the exact path shown by
TUI, usually `/dev/tty.wiremux-esp-enhanced` when `/dev` aliases are permitted
or `/tmp/wiremux/tty/tty.wiremux-esp-enhanced` as the user-writable fallback.
Terminal tools can open it as an aggregate channel monitor. `idf.py flash
--port <shown-esp-enhanced-path> --baud 115200` is detected from a complete
esptool SYNC frame, then TUI uses DTR/RTS to enter the ROM bootloader and
bridges raw bytes until the flashing client disconnects. The explicit
`--baud 115200` keeps esptool from issuing a high-baud PTY ioctl that macOS PTY
aliases reject; native DriverKit virtual serial support is the roadmap path for
default high-baud flashing. Normal channel virtual serial input ownership is
unchanged.

```toml
[virtual_serial]
enabled = true
export = "all-manifest-channels"
name_template = "wiremux-{device}-{channel}"
```

Windows keeps the virtual serial interface as an unsupported placeholder until a
native virtual COM backend is added.

## ESP-IDF Examples

Use `esp_wiremux_beginner_demo` for the global `wmux_*` quick-start API,
`esp_wiremux_advanced_demo` for explicit `wmux_channel_*` handles, and
`esp_wiremux_professional_demo` for the full `esp_wiremux_*` console demo.

Use ESP-IDF v5.4 or newer:

```bash
cd sources/vendor/espressif/generic/examples/esp_wiremux_professional_demo
idf.py set-target esp32s3
idf.py build flash monitor
```

After flashing, stop `idf.py monitor` before starting the host tool. Most serial devices should be owned by only one process at a time.

Run a console command through Wiremux channel 1:

```bash
cd sources/host/wiremux
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
```

Observe other demo channels while sending commands to the console channel:

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 2 --line mux_log
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 3 --line mux_hello
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 4 --line mux_utf8
```

## Documentation

- [Product Architecture](docs/product-architecture.md)
- [Source Layout and Build Orchestration](docs/source-layout-build.md)
- [Host CLI](docs/zh/host-tool.md)
- [Getting Started](docs/zh/getting-started.md)
- [ESP-IDF Console Integration](docs/zh/esp-idf-console-integration.md)
- [Troubleshooting](docs/zh/troubleshooting.md)

## License

Wiremux is released under the [Apache License 2.0](LICENSE).

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=magicdian/wiremux&type=Date)](https://www.star-history.com/#magicdian/wiremux&Date)
