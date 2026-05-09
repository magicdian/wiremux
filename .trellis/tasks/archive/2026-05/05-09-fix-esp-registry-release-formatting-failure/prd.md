# Fix ESP registry release formatting failure

## Goal

Restore the 2605.8.1 ESP registry release workflow by fixing the Rust host formatting drift that makes `tools/wiremux-build check host` fail during the release validation job.

## What I already know

* The release workflow is `.github/workflows/esp-registry-release.yml`.
* The validate job runs `tools/wiremux-build check core` and `tools/wiremux-build check host` before ESP-IDF validation and package generation.
* The supplied CI log shows C core CMake build and CTest passed, then `cargo fmt --check` failed.
* The reported formatting drift is in `sources/host/wiremux/crates/cli/src/main.rs` and `sources/host/wiremux/crates/tui/src/lib.rs`.
* OMV reports current managed version `2605.8.1` and no version target drift.

## Assumptions

* No version bump is needed; this is a release validation fix for the existing `2605.8.1` sources.
* The failure is formatting-only unless local verification exposes a deeper host check failure.

## Requirements

* Apply canonical `rustfmt` formatting to the host Rust workspace.
* Preserve runtime behavior.
* Verify `cargo fmt --check` passes after formatting.
* Verify OMV drift stays clean.

## Acceptance Criteria

* [x] `cargo fmt --check` passes in `sources/host/wiremux`.
* [x] `tools/wiremux-build check host` passes, or any environmental blocker is reported.
* [x] `omv sync --check --json` reports no drift.

## Definition of Done

* Formatting fix committed to the working tree.
* Relevant checks run and results reported.
* No native manifest versions edited directly.

## Out of Scope

* Publishing to the ESP registry.
* Changing release workflow behavior unless verification proves it is necessary.
* Bumping the managed OMV version.

## Technical Notes

* Backend specs read: directory structure, error handling, quality guidelines.
* OMV instructions read from `.omv/ai/instructions.md` and Trellis OMV guide.
* `omv plan --json` shows all configured version targets are already at `2605.8.1`.
