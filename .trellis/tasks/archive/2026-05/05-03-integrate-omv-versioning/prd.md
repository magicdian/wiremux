# brainstorm: integrate OMV versioning

## Goal

Integrate OMV as the source of truth for wiremux release versions so release
surfaces can be checked and synchronized from `.omv/state.toml` instead of being
edited by hand.

## What I already know

* The user wants to connect wiremux to OMV, a local date-based version manager
  from `https://github.com/magicdian/oh-my-versioning`.
* `.omv/` already exists in this project and currently reports managed version
  `2605.3.1`.
* Current wiremux release surfaces still contain `2604.30.3`.
* The test fixture
  `/Users/magicdian/documents_local/MyProjects/oh-my-versioning/tests/external_scenarios/wiremux-2604.30.3/omv/targets.toml`
  contains a wiremux-specific target list that maps closely to this repository.
* `omv integrate status --json` reports Codex and Trellis capabilities installed.
* `omv plan --json` and `omv sync --check --json` currently fail target checks
  because the initialized `.omv/targets.toml` points at generic root-level
  manifests that do not exist in wiremux.

## Assumptions (temporary)

* This task should configure OMV targets for the existing version surfaces, not
  redesign the release process.
* The OMV managed version should remain `2605.3.1` unless the user explicitly
  asks for a bump.
* Historical release notes such as older entries in `docs/esp-registry-release.md`
  should remain historical and should not be mass-rewritten.

## Open Questions

* None for MVP implementation.

## Requirements (evolving)

* Replace the generic initialized `.omv/targets.toml` entries with wiremux
  project-specific targets.
* Configure the complete target set needed by wiremux, including `VERSION`,
  README badges, ESP component metadata/header, host Rust workspace member
  package versions, lockfile refresh behavior, and the current-release doc line.
* Preserve OMV as authority for version truth in `.omv/state.toml`.
* Keep host adapter files and Trellis projections treated as derived OMV outputs.
* Validate the configuration with `omv plan --json` and
  `omv sync --check --json`.
* Do not run `omv sync --json` in this task; leave target drift visible so
  `$finish-work` / OMV integration behavior can be observed separately.

## Acceptance Criteria (evolving)

* [x] `omv plan --json` no longer reports missing generic root manifest targets.
* [x] All required wiremux version surfaces are represented by targets.
* [x] `omv sync --check --json` reports only expected drift from current project
      files to `.omv/state.toml`, not missing or unsupported targets.
* [x] No native manifest version is hand-edited outside OMV target sync behavior.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI-relevant checks green for touched areas.
* Docs/notes updated if workflow behavior changes.
* Rollout/rollback considered if risky.

## Research Notes

### Version surfaces found in this repo

* `VERSION`: whole-file version scalar.
* `README.md` and `README_CN.md`: shields.io version badge.
* `sources/vendor/espressif/generic/components/esp-wiremux/idf_component.yml`:
  ESP component registry version.
* `sources/vendor/espressif/generic/components/esp-wiremux/include/esp_wiremux.h`:
  `ESP_WIREMUX_VERSION` macro used by the ESP component implementation.
* `sources/host/wiremux/crates/*/Cargo.toml`: seven Rust workspace member
  package versions.
* `sources/host/wiremux/Cargo.lock`: lockfile contains the same workspace
  versions and should be refreshed by the Cargo target when versions change.
* `docs/esp-registry-release.md`: `Current release: ...` should track the
  current version. Historical release entries and tag examples should remain
  unchanged unless release docs policy changes.

### Constraints from OMV instructions

* Version truth lives in `.omv/state.toml`.
* Use `omv current --json`, `omv plan --json`, and `omv sync --check --json`
  around version-sensitive changes.
* Do not hand-edit native manifest versions directly.
* Runtime export files and host adapter files are derived projections.
* At finalize boundaries, use the finalize-boundary helper from
  `.omv/ai/contract.json` with an explicit `change_type`.

### Feasible approaches here

**Approach A: Configure targets only** (Recommended for first step)

* How it works: replace initialized generic targets with wiremux-specific
  targets, then run `omv plan --json` and `omv sync --check --json`.
* Pros: separates configuration correctness from version mutation; low risk.
* Cons: current project files will likely still drift from `.omv/state.toml`
  until a sync is intentionally run.

**Approach B: Configure targets and sync current version**

* How it works: configure targets, run `omv sync --json`, and update all managed
  version surfaces to `2605.3.1`.
* Pros: ends with the project fully aligned to OMV version truth.
* Cons: updates release-facing files immediately; may be premature if `2605.3.1`
  is only an integration/test version.

**Approach C: Configure minimal non-release targets only**

* How it works: manage only `VERSION` and Rust/ESP manifests for now, leaving
  README badges and release docs outside OMV.
* Pros: smallest set of managed outputs.
* Cons: leaves obvious release surfaces manual and undermines the value of OMV.

## Out of Scope (explicit)

* Changing OMV itself in the `oh-my-versioning` repository.
* Reworking the wiremux release workflow beyond target configuration.
* Rewriting historical release note entries.
* Running `omv sync --json` to mutate current version surfaces.

## Decision (ADR-lite)

**Context**: The initialized OMV targets are generic templates and do not match
wiremux's actual layout, while the fixture from OMV's external scenario test has
project-specific targets for this repository.

**Decision**: Use Approach A. Replace `.omv/targets.toml` with the complete
wiremux-specific target set and validate with read-only OMV commands. Do not
sync target files in this task.

**Consequences**: The configuration should become correct, but project files may
continue to drift from `.omv/state.toml` until a later explicit OMV sync or
finish-work/finalize flow mutates version surfaces.

## Technical Notes

* Current `.omv/targets.toml` contains generic `workspace-c-family`,
  `workspace-rust`, and `workspace-python` enabled targets against root-level
  manifests that do not exist in wiremux.
* The fixture target list contains concrete `kind`-based targets for the
  relevant wiremux paths.
* `omv current --json` reports `enabled_targets: 3` and `total_targets: 5`
  before target migration.
* `omv integrate status --json` shows installed Codex capabilities
  `project-instructions` and `host-skill`, plus installed Trellis capabilities
  `spec-guide`, `spec-index-snippet`, and `finalize-boundary`.
* After target replacement, `omv plan --json` reports 7 drift targets and 0
  missing, 0 unsupported, 0 errors, 0 skipped.
* After target replacement, `omv sync --check --json` exits non-zero only
  because required target drift exists from current files (`2604.30.3`) to OMV
  truth (`2605.3.1`).
