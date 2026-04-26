# ESP Wiremux Console Demo

This ESP-IDF example demonstrates the first integration shape for `esp_wiremux`:

- channel 0: system/control manifest
- channel 1: console line-mode adapter
- channel 2: ESP log adapter
- channel 3: demo telemetry/text output

The mux component writes magic-framed records to the same stdout transport used by the board's serial connection. A host tool can parse records with the `WMUX` magic while preserving ordinary terminal output.

The demo registers ESP-IDF console commands and dispatches them from mux channel 1 in line-mode. Host input is framed as `WMUX` + `MuxEnvelope(direction=input)`, validated on the ESP32, then passed to `esp_console_run()`.

```text
help
hello
mux_manifest
mux_hello
mux_log
```

The app keeps running after boot and emits a telemetry mux frame every two seconds so the host listener has continuous data to observe after reset.

Build from this directory with ESP-IDF v5.4 or newer:

```bash
idf.py set-target esp32s3
idf.py build flash monitor
```

After flashing, stop `idf.py monitor` before running the host tool. Only one process should own the serial device.

Send a command and listen to the console channel from one process:

```bash
cd sources/host
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line help
```

Run the other demo commands the same way:

```bash
cd sources/host
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_manifest
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_hello
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --channel 1 --line mux_log
```

Observe logs and telemetry on separate channels:

```bash
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 2 --line mux_log
cargo run -- listen --port /dev/tty.usbmodem2101 --baud 115200 --send-channel 1 --channel 3 --line mux_hello
```

To see every channel in one run, omit `--channel`; `--line` defaults to sending on channel 1.

For a corrupt inbound frame check, flip any payload byte in a captured `send` frame without updating the CRC and write it to the same serial port. The ESP32 inbound scanner drops the candidate frame before channel dispatch, so the channel 1 listener should show no command output and the app should keep emitting channel 2 logs and channel 3 telemetry.

If you already generated `sdkconfig` before this example switched to USB Serial/JTAG console defaults, regenerate the project config:

```bash
rm -f sdkconfig
rm -rf build
idf.py set-target esp32s3
idf.py build flash monitor
```

If VS Code reports that this is not a complete ESP-IDF project, open the folder
`sources/esp32/examples/esp_wiremux_console_demo` directly and make sure the ESP-IDF
extension has a configured `IDF_PATH`. The root `CMakeLists.txt`,
`sdkconfig.defaults`, and `main/CMakeLists.txt` are all project-local.

On macOS, `/dev/cu.usbmodem*` is usually the preferred application port. Passing `/dev/tty.usbmodem*` is also accepted; the host tool tries the paired `/dev/cu.*` path first.

The local development environment for this repository may not have ESP-IDF installed; this example is intended to validate in a configured ESP-IDF shell.
