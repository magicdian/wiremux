# PR5 host TUI docs verification

## Goal

Document the new host TUI and verify the full cross-layer change.

## Requirements

* Update Chinese host documentation for `wiremux tui`.
* Update specs/docs for manifest request and interaction mode contracts.
* Run required Rust host checks.
* Run C core checks.
* Run ESP build if ESP-IDF is available.

## Acceptance Criteria

* [x] Docs describe launch, shortcuts, input routing, and manifest request.
* [x] `cargo fmt --check`, `cargo check`, and `cargo test` pass in
      `sources/host`.
* [x] C core configure/build/test pass.
* [x] ESP build result is reported.

## Technical Notes

Parent task: `.trellis/tasks/04-27-host-ratatui-tui`.
