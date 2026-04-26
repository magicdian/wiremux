# PR8: Serial Backend Hardening and Release Packaging

## Goal

Replace the current pragmatic raw-device listener with a robust serial backend and document the single-binary release path for the Rust host tool.

## Requirements

* Evaluate and adopt a Rust serial backend that supports macOS, Linux, and Windows.
* Preserve macOS `/dev/cu.*` preference for USB serial/JTAG devices.
* Keep reconnect behavior deterministic across unplug/reset cycles.
* Remove shelling out to `stty` if the chosen serial crate can configure raw mode directly.
* Document build/release commands for a single executable.

## Acceptance Criteria

* [ ] Host listen and send paths use the same serial abstraction.
* [ ] macOS USB Serial/JTAG reset behavior is covered by tests where feasible or documented manual verification.
* [ ] Linux and Windows path handling is documented.
* [ ] `cargo test`, `cargo check`, and `cargo fmt --check` pass.
* [ ] Chinese docs explain how users select the correct port.

## Non-Goals

* No TUI.
* No protocol changes.
* No ESP firmware behavior changes except docs/examples if needed.

