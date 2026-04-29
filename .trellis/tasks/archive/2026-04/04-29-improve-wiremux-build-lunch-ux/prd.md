# brainstorm: improve wiremux-build lunch UX

## Goal

Improve the `wiremux-build lunch` experience so users can run `lunch` without
remembering both the vendor/device build target and host preset. When no
arguments are supplied, the tool should guide the user through a constrained
interactive selection.

## What I already know

* Current command fails with `error: lunch requires <device> <host-preset>` when
  invoked as `./tools/wiremux-build lunch`.
* Desired behavior is similar to Android's `lunch`: show choices and prompt the
  user to choose.
* The vendor/host choice data should not be hard-coded inside `wiremux-build` or
  `wiremux-build-helper` source code.
* A direction-key controlled selector is preferred if it fits the tool and
  environment.
* `tools/wiremux-build` is a Python bootstrap that runs the Rust helper via
  Cargo.
* `tools/wiremux-build-helper/src/main.rs` owns `cmd_lunch`, reads
  `build/wiremux-build.toml`, and writes `.wiremux/build/selected.toml`.
* `build/wiremux-build.toml` already stores product defaults, devices,
  host presets, and tool policy.
* Project docs/specs define build config files as TOML and define selected state
  priority as `CLI args > selected.toml > product defaults`.
* The desired model is two independent TOML-maintained dimensions:
  * Vendor side: built-in choices for all, skip, or one concrete model.
  * Host side: generic, vendor enhanced, or all features.
* Vendor/host constraints:
  * vendor `all` allows host `generic` or `all features`.
  * vendor `skip` allows host `generic` or `all features`.
  * vendor single model allows host `generic`, that model's vendor-enhanced
    host, or `all features`.
* Current vendor tree has `sources/vendor/espressif/generic` implemented, with
  `s3` and `p4` as reserved placeholders.
* ESP32-S3 build docs currently use the generic ESP-IDF example with:
  `idf.py set-target esp32s3` followed by `idf.py build`.

## Assumptions (temporary)

* Existing explicit usage should remain supported for scripts and advanced users,
  though the argument shape may evolve from `<device> <host-preset>` to explicit
  vendor/host selectors.
* The lists should be repository-maintained TOML config data, not generated from
  network or external services.
* Non-interactive environments should fail clearly or support a deterministic
  text fallback.

## Open Questions

* None.

## Requirements (evolving)

* `wiremux-build lunch` with no extra arguments should be usable.
* Vendor choices should be maintained outside Rust/shell source code.
* Host choices should be maintained outside Rust/shell source code.
* Interactive no-arg `lunch` should select vendor first, then host choices
  filtered by the selected vendor scope.
* Interactive no-arg `lunch` is the primary intended user workflow.
* Explicit lunch behavior should remain scriptable via flags:
  `wiremux-build lunch --vendor <vendor-scope-or-model> --host <host-mode>`.
* The old positional `lunch <device> <host-preset>` form should not remain
  supported because the build system is still in development and can take a
  breaking CLI change.
* If users pass positional lunch arguments, the command should fail with a clear
  message pointing to `--vendor` and `--host`.
* Selected state should preserve enough information for later build/env commands
  to know vendor range and host mode.
* `wiremux-build build/check` should respect the selected vendor scope and host
  mode.
* Build/check target dispatch and validation should be fully wired for ESP32-S3
  first.
* Other concrete vendor models may exist as TOML/menu placeholders in this task,
  but build/check should fail clearly if selected before support is implemented.

## Acceptance Criteria (evolving)

* [x] Running `./tools/wiremux-build lunch` without arguments presents vendor
  choices instead of immediately erroring.
* [x] After vendor selection, host choices are filtered according to the vendor
  scope rules.
* [x] Vendor choices live in repository TOML, not Rust/Python source.
* [x] Host choices live in repository TOML, not Rust/Python source.
* [x] Invalid vendor/host combinations fail with deterministic validation
  errors.
* [x] Explicit lunch selection remains possible for non-interactive scripts with
  `--vendor` and `--host`.
* [x] Positional `lunch <device> <host-preset>` arguments are rejected with a
  clear migration message.
* [x] `wiremux-build build vendor-*` / `check vendor-*` behavior respects the
  selected vendor scope for ESP32-S3.
* [x] Placeholder vendor models fail with a clear "not implemented yet" style
  error when build/check would need to execute them.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI-equivalent checks pass.
* Docs/usage notes updated if command behavior changes.
* Non-interactive behavior is considered so CI/scripts do not hang.

## Out of Scope (explicit)

* Auto-detecting devices from connected hardware.
* Downloading or discovering lunch choices from remote services.
* Changing unrelated build commands.
* Implementing full build/check support for every placeholder vendor model.

## Technical Notes

* Likely files:
  * `tools/wiremux-build-helper/src/main.rs`
  * `tools/wiremux-build-helper/Cargo.toml`
  * `build/wiremux-build.toml` or new vendor/host TOML files under `build/`
  * `docs/source-layout-build.md`
  * `.trellis/spec/backend/quality-guidelines.md` if the contract changes
* `dialoguer` provides Rust CLI select prompts with item slices and returns the
  selected index. It supports optional interaction cancellation via
  `interact_opt`.
* `inquire` also provides interactive CLI `Select`, but `dialoguer` is smaller
  and matches the simple prompt need here.
* ESP32-S3 dispatch should run in
  `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo` and call
  `idf.py set-target esp32s3` before `idf.py build`.

## Research Notes

### What similar tools do

* Android `lunch` shows common combinations and accepts either a numeric menu
  choice or a typed custom product variant.
* Rust CLI prompt libraries commonly render interactive prompts on stderr and
  return an index/value to the program.

### Constraints from our repo/project

* Build configuration files use TOML.
* `wiremux-build` should remain an orchestrator, not a replacement for Cargo,
  CMake, or `idf.py`.
* The source of truth after selection remains `.wiremux/build/selected.toml`.
* Explicit CLI arguments must keep priority for scriptability.

### Feasible approaches here

**Approach A: Add explicit lunch choices inside `build/wiremux-build.toml`**

* How it works: add a `[[lunch.choices]]` list with `name`, `device`, and
  `host_preset`; no-arg `lunch` reads that list and presents it interactively.
* Pros: one TOML file, one parse path, easy validation against existing devices
  and host presets.
* Cons: the main build config can become long if there are many product choices.

**Approach B: Add separate `build/wiremux-lunch.toml`** (Recommended for growth)

* How it works: keep devices/host presets in `wiremux-build.toml`, keep visible
  lunch combinations in a separate TOML file, and validate references across
  both.
* Pros: menu can grow without cluttering tool policy/defaults; still structured
  and reviewable.
* Cons: two files must be loaded and validated together.

**Approach C: Add a plain line-based text file**

* How it works: e.g. `build/wiremux-lunch.txt` with lines like
  `generic-only generic-only` or `label device host-preset`.
* Pros: very easy to edit.
* Cons: weaker schema, more custom parser code, harder comments/metadata, less
  consistent with the repository's TOML build config direction.

### Updated feasible approach after requirement refinement

**Approach D: Split vendor and host menu definitions into separate TOML files**
(Recommended)

* How it works:
  * `build/wiremux-vendors.toml` defines vendor scopes such as `skip`, `all`,
    and concrete model entries.
  * `build/wiremux-hosts.toml` defines host modes such as `generic`,
    `vendor-enhanced`, and `all-features`.
  * The helper validates host modes against the selected vendor scope before
    writing selected state.
* Pros:
  * Models the user's mental flow directly.
  * Vendor/model growth does not clutter host preset policy.
  * Constraints become data-driven and testable.
* Cons:
  * Requires a small schema migration from current `device + host_preset`.
  * Existing docs/specs mention `core-only + device-only`; those contracts need
    updating to the new vendor-scope model.

## Decision (ADR-lite)

**Context**: The existing `lunch <device> <host-preset>` model makes users know
two low-level names up front and does not distinguish vendor build scope from
host capability selection.

**Decision**: Use two TOML-maintained dimensions, vendor side and host side, and
make no-arg `lunch` perform a constrained two-step interactive selection.
Implement build/check integration in the same task, but only fully dispatch
ESP32-S3 initially. Keep other vendor model entries as placeholders that validate
selection constraints but fail clearly when execution is requested. For
non-interactive use, prefer explicit flags:
`wiremux-build lunch --vendor <vendor-scope-or-model> --host <host-mode>`.
Do not preserve the old positional `lunch <device> <host-preset>` form.

**Consequences**: The selected state schema and build docs/specs need to evolve
from `device + host_preset` toward `vendor scope + host mode`. This gives a
better user model and a cleaner path for adding more vendor targets later, at
the cost of touching command behavior beyond `lunch`.
