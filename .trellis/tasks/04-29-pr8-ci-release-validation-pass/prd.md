# PR8: CI and Release Validation Pass

## Goal

Update CI/release validation after the architecture and build-orchestrator
changes have landed.

## Requirements

- Update GitHub Actions to use new paths and, where appropriate, `wiremux-build`.
- Ensure ESP registry release packaging still publishes `wiremux-core` and
  `esp-wiremux` from the new layout.
- Ensure CI uses strict per-tool reproducibility rules.
- Update release documentation and specs to reflect final commands.
- Do not introduce new runtime behavior.

## Acceptance Criteria

- [ ] CI paths use `sources/api`, `sources/host/wiremux`, and
  `sources/vendor/espressif/generic`.
- [ ] Release packaging uses the new vendor/core paths.
- [ ] CI command list is documented.
- [ ] Local and CI reproducibility policies are documented.

## Validation

- `tools/wiremux-build check all`
- `tools/wiremux-build package esp-registry`
- Existing GitHub Actions syntax review.
