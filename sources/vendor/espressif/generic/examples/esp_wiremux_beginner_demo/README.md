# ESP Wiremux Beginner Demo

This example shows the limited global `wmux_*` quick-start API. It uses
`wmux_begin()`, sends text on the default channel, and registers a callback for
input on channel 1.

Build from this directory with ESP-IDF v5.4 or newer:

```bash
idf.py set-target esp32s3
idf.py build flash monitor
```

After flashing, stop `idf.py monitor` before running the host tool.

```bash
cd sources/host/wiremux
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line hello
```
