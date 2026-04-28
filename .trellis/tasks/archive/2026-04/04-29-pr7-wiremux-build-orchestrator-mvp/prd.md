# PR7: Add Wiremux Build Orchestrator MVP

## Goal

Add the first version of Wiremux's own build/product orchestration layer while
delegating actual compilation to native build systems.

## Requirements

- Add `tools/wiremux-build` Python bootstrap launcher.
- Add `tools/wiremux-build-helper/` Rust helper crate.
- Add committed TOML config under `build/`, such as:
  - product/device definitions
  - host presets
  - toolchain/version contracts
- Add `.gitignore` entries for generated/local state:
  - `/.wiremux/`
  - `/build/out/`
  - `/tools/wiremux-build-helper/target/`
  - any generated metadata that should not be committed.
- Implement MVP commands:
  - `lunch <device> <host-preset>`
  - `env --shell bash|zsh`
  - `doctor`
  - `check core`
  - `check host`
  - `check vendor-espressif`
  - `check all`
  - `build core`
  - `build host`
  - `build vendor-espressif` may initially be equivalent to check/build path
  - `package esp-registry`
- Lunch rules:
  - `.wiremux/build/selected.toml` is source of truth.
  - `env` emits optional shell exports derived from selected state.
  - Resolution priority is `CLI args > selected.toml > product defaults`.
  - Environment variables do not normally override selected config.
  - `core-only + device-only` is invalid.
- Reproducibility:
  - CI strictness configurable per tool.
  - Local compatible version mismatch warns and records metadata.
  - Build metadata records actual tool versions and dirty/deviated events.
- Every wrapper command must print the native command it runs.

## Acceptance Criteria

- [ ] `tools/wiremux-build lunch core-only generic-only` writes selected config.
- [ ] `tools/wiremux-build lunch core-only device-only` fails clearly.
- [ ] `tools/wiremux-build env --shell bash` and `--shell zsh` print exports.
- [ ] `doctor` reports Rust/Cargo/CMake/Python and ESP-IDF status where present.
- [ ] `check core` invokes CMake/CTest native commands.
- [ ] `check host` invokes Cargo native commands.
- [ ] `package esp-registry` invokes existing packaging script.
- [ ] Generated local state is ignored by Git.

## Validation

- `tools/wiremux-build doctor`
- `tools/wiremux-build lunch core-only generic-only`
- `tools/wiremux-build env --shell bash`
- `tools/wiremux-build check core`
- `tools/wiremux-build check host`
- `tools/wiremux-build package esp-registry`
