# PR5: Add Host Workspace Skeleton

## Goal

Introduce a host Cargo workspace skeleton without performing a deep behavior
refactor.

## Requirements

- Keep the public `wiremux` binary behavior unchanged.
- Add a workspace structure that can later host crates such as host core, CLI,
  TUI, and adapters.
- Move code only as much as needed to compile cleanly.
- Add non-restrictive feature skeleton:
  - `generic`
  - `esp32`
  - `all-vendors`
  - `all-features` if useful for build profile mapping
- Do not enforce real feature gating until later refactors clarify boundaries.

## Acceptance Criteria

- [ ] Host is a Cargo workspace or workspace-ready layout.
- [ ] Existing CLI/TUI tests still pass.
- [ ] The `wiremux` binary name and commands remain unchanged.
- [ ] Feature skeleton exists but does not remove existing behavior.

## Validation

- `cd sources/host/wiremux && cargo test && cargo check && cargo fmt --check`
- `cargo check --features generic`
- `cargo check --features esp32` if the feature is declared.
