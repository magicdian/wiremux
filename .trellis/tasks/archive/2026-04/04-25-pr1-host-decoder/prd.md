# PR1: Protocol and Rust Host Decoder Core

## Goal

Create the first host-side implementation slice under `sources/host`: a Rust CLI/protocol core that can read bytes from a configured serial-like path, extract mux frames from a mixed terminal stream, validate frame integrity, and provide unit-tested capture/replay-friendly decoding.

## Requirements

* Source code lives under `sources/host`.
* Use Rust for the host tool.
* Do not implement `ratatui` in this PR.
* Define the initial binary mux frame format:
  * magic
  * version
  * flags
  * payload length
  * CRC32
  * protobuf payload bytes
* Implement a mixed-stream scanner:
  * ordinary non-mux bytes are emitted as terminal bytes
  * valid mux frames are emitted as decoded frame events
  * invalid candidate frames resynchronize without losing unrelated terminal output
* Provide a CLI that accepts `--port /dev/tty.usbmodem2101` and optional `--baud 115200`.
* Keep serial-path handling runtime-configurable; `/dev/tty.usbmodem2101` is only the current test device.
* Include unit tests for parser success, partial frames, false magic, bad CRC, mixed terminal text, and replay-style chunking.

## Acceptance Criteria

* [x] `sources/host` contains a buildable Rust crate.
* [x] Frame parser has deterministic unit tests for valid and invalid streams.
* [x] CLI can open a configured path and stream decoded output.
* [x] No TUI dependency or UI framework is introduced.
* [x] Protocol constants and frame format are centralized.

## Definition of Done

* [x] `cargo test` passes for `sources/host`.
* [x] Parser edge cases are covered by unit tests.
* [x] Parent brainstorm PRD remains the source of product direction.

## Technical Notes

* Parent task: `.trellis/tasks/04-25-esp32-serial-mux-design`.
* Start with a dependency-light implementation to make protocol tests fast and reliable.
* Protobuf payload decoding may be limited in PR1; the frame scanner must preserve payload bytes and expose metadata for later protobuf integration.
