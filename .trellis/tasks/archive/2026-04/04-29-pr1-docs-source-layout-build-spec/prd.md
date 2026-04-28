# PR1: Docs and Source Layout Build Spec Finalization

## Goal

Finalize the source-layout and build-orchestration architecture before physical
file moves begin.

## Requirements

- Update product architecture/docs to describe the target source layout:
  `sources/api`, `sources/core`, `sources/profiles`, `sources/host/wiremux`,
  `sources/vendor/espressif`, `build`, `tools`, and `.wiremux`.
- Document `wiremux-build` as a long-term product orchestrator, not a compiler
  or replacement for Cargo/CMake/idf.py.
- Document lunch semantics:
  `.wiremux/build/selected.toml` is source of truth, optional `env --shell`
  output is derived state, and priority is `CLI args > selected.toml > product defaults`.
- Document CI strict vs local tolerant reproducibility and dirty/deviated build
  metadata.
- Update Trellis specs so future sessions use the target paths and boundaries.
- Do not move runtime source files in this PR.

## Acceptance Criteria

- [ ] A design doc describes target layout, build profiles, lunch, and
  reproducibility policy.
- [ ] Backend specs reference the new intended structure.
- [ ] No source files under `sources/` are moved.
- [ ] `rg "sources/host|sources/esp32|sources/core/proto"` in docs/specs has
  only intentional historical or migration references.

## Suggested Files

- `docs/product-architecture.md`
- new `docs/source-layout-build.md` if useful
- `.trellis/spec/backend/directory-structure.md`
- `.trellis/spec/backend/quality-guidelines.md`
- `.trellis/spec/frontend/*` path references if needed

## Validation

- Documentation review.
- `git diff --stat`.
