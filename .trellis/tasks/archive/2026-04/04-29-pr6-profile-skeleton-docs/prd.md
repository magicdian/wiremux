# PR6: Add Profile Skeleton Docs

## Goal

Create the official profile-contract source tree skeleton without implementing
profile behavior.

## Requirements

- Add README-tracked skeleton directories under `sources/profiles`.
- Include at least:
  - `sources/profiles/README.md`
  - `sources/profiles/transfer/README.md`
  - `sources/profiles/console/README.md`
  - `sources/profiles/pty/README.md`
- Explain that profiles are HAL-like contracts above core and below host/vendor
  adapters.
- Do not add runtime profile protocol changes in this PR.

## Acceptance Criteria

- [ ] Profile directories are tracked by Git through README files.
- [ ] Documentation explains profile responsibilities and non-responsibilities.
- [ ] No production code is changed.

## Validation

- Documentation review.
- `git diff --stat`.
