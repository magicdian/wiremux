# PR4: Move Host Wiremux Path

## Goal

Move the current Rust host tool under a future-proof host product namespace.

## Requirements

- Move `sources/host` to `sources/host/wiremux`.
- Preserve the public binary name `wiremux`.
- Update `build.rs` relative paths after prior source-layout changes.
- Update README/docs/specs/release docs to reference `sources/host/wiremux`.
- Do not create compatibility symlinks for the old path.
- Do not deeply split host modules in this PR.

## Acceptance Criteria

- [ ] `sources/host/wiremux/Cargo.toml` builds the current `wiremux` binary.
- [ ] `sources/host/wiremux/build.rs` can find the C core from its new location.
- [ ] No active docs/specs refer to `sources/host` as the Cargo package path
  except migration notes.

## Validation

- `cd sources/host/wiremux && cargo test && cargo check && cargo fmt --check`
- `rg "sources/host(?!/wiremux)" README.md README_CN.md docs .trellis/spec sources`
