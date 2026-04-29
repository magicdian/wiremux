# brainstorm: optimize wiremux-build commands

## Goal

Improve the `wiremux-build` developer tool so routine command usage is quieter,
more focused, and aligned with the selected lunch profile. The goal is to reduce
implementation leakage in terminal output and make `check` and `build` command
semantics match how developers use the project.

## What I already know

* The shell wrapper currently prints the underlying `cargo run --quiet ...`
  command before the Rust helper reports usage errors.
* The print is emitted directly by `tools/wiremux-build`, not by Cargo or the
  Rust helper.
* The previous lunch optimization was introduced in commit
  `47532015ecb53e02a9d7ba25c36222216e300608`.
* `check` is intended as a developer gate and should probably default to
  checking all relevant modules, independent of lunch selection.
* `build` should probably default to the current lunch profile in
  `selected.toml`.
* The user proposes removing the public `vendor-espressif` build/check selector
  in favor of `vendor`.

## Assumptions (temporary)

* Hiding the implementation command only means suppressing wrapper tracing; real
  helper/build errors should still be visible.
* `check` can safely default to `all` without surprising developers because it is
  a gate command, not a selected-profile build command.
* `build host` and `build vendor` should resolve concrete build variants from
  the selected lunch profile instead of building every possible flavor.

## Open Questions

* None.

## Requirements (evolving)

* Suppress the `+ cargo run ...` wrapper trace from `tools/wiremux-build`.
* Consider making `wiremux-build check` default to `all`.
* Consider limiting public `check` selectors to `core`, `host`, `vendor`, and
  `all`.
* Consider making `wiremux-build build` default to selected lunch configuration.
* Consider limiting public `build` selectors to `core`, `host`, and `vendor`.
* When building `host` or `vendor`, use `selected.toml` to choose the concrete
  variant.
* If vendor is skipped in the selected profile, `build vendor` should warn that
  no vendor build was performed.
* Update documentation and CI references that still use `vendor-espressif`.
* Remove `vendor-espressif` entirely; do not keep a hidden compatibility alias.

## Acceptance Criteria

* [x] `tools/wiremux-build` no longer prints the underlying `cargo run` command.
* [x] `wiremux-build check` with no selector runs the intended default gate.
* [x] `wiremux-build build` with no selector follows `selected.toml`.
* [x] Help/usage text matches the implemented command surface.
* [x] Relevant automated tests or validation commands cover changed behavior.

## Implementation Summary

* Removed the Python bootstrap trace that printed the internal `cargo run`
  command.
* Added explicit `check` and `build` target parsers:
  `check [core|host|vendor|all]` defaults to `all`; `build [core|host|vendor]`
  defaults to `selected`.
* Made `check host` run the product host feature matrix instead of the selected
  lunch host mode.
* Made `check vendor` dispatch all implemented vendor models instead of the
  selected lunch vendor scope.
* Kept `build host` and `build vendor` selected-profile oriented.
* Removed `vendor-espressif` from accepted CLI targets and updated CI/docs/specs.
* Added regression tests for default targets, rejected legacy selector, host gate
  features, and implemented vendor target discovery.

## Validation

* `cargo fmt --check --manifest-path tools/wiremux-build-helper/Cargo.toml`
* `cargo test --manifest-path tools/wiremux-build-helper/Cargo.toml`
* `cargo check --manifest-path tools/wiremux-build-helper/Cargo.toml`
* `./tools/wiremux-build`
* `./tools/wiremux-build check vendor-espressif`
* `./tools/wiremux-build build vendor-espressif`
* `./tools/wiremux-build env --shell zsh`
* `./tools/wiremux-build check host`
* `./tools/wiremux-build check vendor` (local `idf.py` missing; skipped by
  local policy)
* `./tools/wiremux-build check core`
* `./tools/wiremux-build check`
* `git diff --check`

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / validation commands are green.
* Docs/notes updated if behavior changes.
* Compatibility impact of CLI changes is explicit.

## Out of Scope (explicit)

* Reworking lunch profile selection beyond what is needed for check/build
  semantics.
* Adding new hardware vendors beyond the existing Espressif path.
* Maintaining backward compatibility for `vendor-espressif`.

## Technical Notes

* Initial PRD created before code inspection per brainstorm workflow.
* `tools/wiremux-build` currently prints `+ cargo run ...` before invoking the
  helper via `subprocess.run`.
* `tools/wiremux-build-helper/src/main.rs` currently requires exactly one
  selector for both `check` and `build`.
* `check vendor` and `check vendor-espressif` are currently aliases that use the
  selected lunch profile, including local skip behavior for missing `idf.py`.
* `build vendor` and `build vendor-espressif` are currently aliases that use the
  selected lunch profile.
* `run_host_check` and `run_host_build` already derive Cargo features from the
  selected host mode, so `build host` profile-aware behavior mostly exists.
* `selected_vendor_targets` already returns an empty list for vendor `skip`,
  and `run_vendor_build` prints `skip: vendor scope is skip; vendor build
  skipped`.
* `.github/workflows/esp-registry-release.yml` and
  `docs/esp-registry-release.md` still reference `check vendor-espressif`.
* `docs/source-layout-build.md` defines selected state as the source of truth and
  documents priority as `CLI args > selected.toml > defaults`.

## Research Notes

### Constraints from our repo/project

* `check` is used in CI as a gate, so defaulting to `all` is coherent but may
  require making CI vendor selection explicit before `check` if defaults remain
  `vendor=skip`.
* `build` is closer to a selected product operation, so defaulting to the current
  lunch selection is coherent with `selected.toml` as source of truth.
* Removing public `vendor-espressif` keeps the CLI at product layer instead of
  exposing a current vendor-family implementation detail.

### Feasible approaches here

**Approach A: Product-level CLI cleanup** (Recommended)

* How it works: public selectors become `check [core|host|vendor|all]` and
  `build [core|host|vendor]`; `check` defaults to `all`; `build` defaults to the
  selected profile; docs/CI/tests are updated.
* Pros: matches the product orchestrator boundary; removes implementation
  leakage; keeps future vendor families possible.
* Cons: small compatibility break for scripts using `vendor-espressif`.

**Approach B: Backward-compatible alias**

* How it works: usage/docs show only `vendor`, but `vendor-espressif` remains as
  a hidden accepted alias for one release.
* Pros: safer for existing scripts outside the repo.
* Cons: keeps legacy behavior in code/tests and delays simplification.

**Approach C: Full explicit subcommands**

* How it works: add more explicit selectors or flags for exact host/vendor
  variants.
* Pros: maximum control for special cases.
* Cons: works against the lunch model and makes this tool feel like a lower
  level build matrix runner.

## Decision (ADR-lite)

**Context**: `wiremux-build` should expose product-level commands. The current
`vendor-espressif` selector leaks the first implemented vendor family into the
public CLI surface and appears in CI/docs.

**Decision**: Adopt Approach A and remove `vendor-espressif` entirely. Public and
accepted selectors become `check [core|host|vendor|all]` and
`build [core|host|vendor]`.

**Consequences**: The CLI becomes cleaner and future vendor-family expansion is
less constrained. Existing external scripts using `vendor-espressif` will fail
and must switch to `vendor`.
