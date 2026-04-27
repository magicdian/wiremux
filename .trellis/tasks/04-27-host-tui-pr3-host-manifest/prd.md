# PR3 Rust host manifest protocol

## Goal

Teach the Rust host to request, decode, and cache device manifest capability
data.

## Requirements

* Encode `DeviceManifestRequest` as a system-channel mux input frame.
* Decode `DeviceManifest` and channel interaction mode from device output.
* Cache latest manifest in host session state.
* Add parser support for `wiremux tui`.
* Preserve existing `listen` and `send` behavior.

## Acceptance Criteria

* [x] Host tests cover manifest request frame construction.
* [x] Host tests cover manifest decode with interaction modes.
* [x] Existing parser tests continue to pass.

## Technical Notes

Parent task: `.trellis/tasks/04-27-host-ratatui-tui`.
