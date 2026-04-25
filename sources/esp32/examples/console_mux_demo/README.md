# Console Mux Demo

This ESP-IDF example demonstrates the first integration shape for `esp_serial_mux`:

- channel 0: system/control manifest
- channel 1: console line-mode adapter
- channel 2: ESP log adapter
- channel 3: demo telemetry/text output

The mux component writes magic-framed records to the same stdout transport used by the board's serial connection. A host tool can parse records with the `ESMX` magic while preserving ordinary terminal output.

The demo also starts an ESP-IDF console REPL based on the official console examples. Try these commands from a serial monitor:

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

If you already generated `sdkconfig` before this example switched to USB Serial/JTAG console defaults, regenerate the project config:

```bash
rm -f sdkconfig
rm -rf build
idf.py set-target esp32s3
idf.py build flash monitor
```

If VS Code reports that this is not a complete ESP-IDF project, open the folder
`sources/esp32/examples/console_mux_demo` directly and make sure the ESP-IDF
extension has a configured `IDF_PATH`. The root `CMakeLists.txt`,
`sdkconfig.defaults`, and `main/CMakeLists.txt` are all project-local.

Do not run `idf.py monitor` and the Rust host tool on the same serial device at the same time. Only one process should own the port.

The local development environment for this repository may not have ESP-IDF installed; this example is intended to validate in a configured ESP-IDF shell.
