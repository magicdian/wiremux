# PR2: Migrate Proto API Boundary

## Goal

Move protocol schema from the core implementation tree into a dedicated API
boundary.

## Requirements

- Move `sources/core/proto` to `sources/api/proto`.
- Preserve API version snapshots and current proto contents.
- Update docs/specs/build references from `sources/core/proto` to
  `sources/api/proto`.
- Do not change proto semantics.
- Do not move C core, host, or ESP32 implementation files in this PR.

## Acceptance Criteria

- [ ] `sources/api/proto/wiremux.proto` exists.
- [ ] `sources/api/proto/api/current/wiremux.proto` and numbered snapshots are preserved.
- [ ] No active docs/specs refer to `sources/core/proto` except migration notes.
- [ ] Core and host checks that do not require ESP-IDF still pass.

## Validation

- `rg "sources/core/proto|sources/api/proto"`
- `cmake -S sources/core/c -B sources/core/c/build`
- `cmake --build sources/core/c/build`
- `ctest --test-dir sources/core/c/build --output-on-failure`
- `cd sources/host && cargo test && cargo check && cargo fmt --check`
