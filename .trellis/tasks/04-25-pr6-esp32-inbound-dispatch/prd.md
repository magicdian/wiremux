# PR6: ESP32 Inbound Frame Parser and Channel Dispatch

## Goal

Implement the ESP32 receive path for host-to-device mux frames and dispatch validated input payloads to registered channel handlers.

## Requirements

* Add an inbound frame scanner compatible with the existing `ESMX` frame layout.
* Validate magic, version, payload length, and CRC before protobuf/envelope dispatch.
* Add an input handler registration API for channels that allow `ESP_SERIAL_MUX_DIRECTION_INPUT`.
* Reject frames for unregistered channels or channels that are output-only.
* Keep parser state bounded by configured maximum payload length.
* Avoid `ESP_LOGx` from mux internals to prevent recursion when the log adapter is installed.

## Acceptance Criteria

* [ ] ESP component exposes a stable input handler registration API.
* [ ] Valid host input frames reach the registered channel handler.
* [ ] Bad CRC, unsupported version, oversized payload, and invalid channel are rejected without callback invocation.
* [ ] Receive-path implementation does not block the mux output service indefinitely.
* [ ] Demo or test coverage documents at least one valid input frame and one rejected corrupt frame.

## Non-Goals

* No full console integration in this task.
* No transparent passthrough mode.
* No dynamic user proto loading on ESP32.

