# PR3: Console and Log Adapters

## Goal

Add ESP-IDF console and logging integration on top of the mux component without hard-coding console mode.

## Requirements

* Implement `mux_bind_console()` with mode-configurable config.
* Support `MUX_CONSOLE_MODE_LINE` using `esp_console_run()`.
* Reserve `MUX_CONSOLE_MODE_PASSTHROUGH` in the public API.
* Implement ESP log adapter using `esp_log_set_vprintf()`.
* Avoid recursive logging from mux internals.

## Acceptance Criteria

* [x] Advanced-console-style ESP-IDF app can bind console to a mux channel.
* [x] Log output can be forwarded to a mux channel.
* [x] Passthrough mode can be added later without breaking the public API.

## Out of Scope

* Full transparent terminal passthrough implementation.
* Early boot, panic, or ROM log capture.
