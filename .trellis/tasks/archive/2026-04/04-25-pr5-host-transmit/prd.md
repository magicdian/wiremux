# PR5: Host Transmit Command and Channel Input Framing

## Goal

Add host-side transmit support so the Rust tool can build `MuxEnvelope` input records, wrap them in `ESMX` frames, and write them to a selected serial device/channel.

## Requirements

* Add a host command or mode for channel input, for example `send --port <path> --channel <id> --line <text>`.
* Reuse the existing frame builder, CRC32 implementation, and protobuf-compatible envelope encoder.
* Do not duplicate wire-format constants in CLI-only code.
* Preserve current `listen` behavior, including reconnect and `--channel` filtering.
* Support at least one non-interactive send path suitable for tests and demo scripts.
* Keep the device path runtime-configurable; do not hard-code `/dev/cu.usbmodem2101`.

## Acceptance Criteria

* [ ] Host can emit a valid input `MuxEnvelope` with `direction = input`.
* [ ] Host can write the framed bytes to the configured port.
* [ ] Unit tests verify input frame construction and round-trip decode through the existing scanner.
* [ ] Invalid `--channel` or missing input text returns a clear CLI error.
* [ ] `cargo test`, `cargo check`, and `cargo fmt --check` pass in `sources/host`.

## Non-Goals

* No TUI.
* No ESP inbound parser in this task.
* No claim that console is operational until PR6 and PR7 are complete.

