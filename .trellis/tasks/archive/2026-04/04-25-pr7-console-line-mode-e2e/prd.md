# PR7: Mux Console Line-Mode End-to-End Demo

## Goal

Make the console channel operational through mux line-mode so users can run ESP console commands from the host and see responses on the console channel.

## Requirements

* Bind channel 1 to console input and output in line-mode.
* Dispatch host-provided command lines through `esp_console_run()` or equivalent console dispatcher.
* Route command output back through the console mux channel.
* Keep the public console config mode-compatible with future passthrough mode.
* Update the demo README and Chinese docs with exact commands for running `help`, `mux_manifest`, `mux_hello`, and `mux_log` through mux.

## Acceptance Criteria

* [ ] Host can send `help` to channel 1 and receive console output through channel 1.
* [ ] Telemetry and log periodic output continue while console commands are used.
* [ ] Unknown console commands return visible error output through mux.
* [ ] Manual verification steps are documented for ESP-IDF monitor and host CLI.
* [ ] No hard-coded demo-only command path is placed in the mux core.

## Non-Goals

* No linenoise/ANSI transparent terminal passthrough.
* No ratatui UI.
* No early boot or panic log capture guarantee.

