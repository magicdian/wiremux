# ESP Wiremux Advanced Demo

This example shows the explicit `wmux_*` simple API: `wmux_config_t`,
`wmux_init()`, `wmux_channel_open_with_config()`, opaque channel handles, and
`wmux_start()`.

Build from this directory with ESP-IDF v5.4 or newer:

```bash
idf.py set-target esp32s3
idf.py build flash monitor
```

After flashing, stop `idf.py monitor` before running the host tool.

```bash
cd sources/host/wiremux
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line hello
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 3
```

The manifest intentionally exposes three endpoints:

- `ch0 system`: internal system channel used by the protocol.
- `ch1 control`: line-mode control channel. It reads host input from its
  per-channel queue and echoes it back. This is not the ESP-IDF console.
- `ch3 data`: stream-mode data channel that emits periodic tick output.

`ch2` is intentionally unused in this example.
