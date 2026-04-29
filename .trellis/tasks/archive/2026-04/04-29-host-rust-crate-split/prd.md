# brainstorm: split host rust crate structure

## Goal

Refactor the host Rust workspace/crate structure so implementation files are split into clearer functional modules instead of keeping unrelated responsibilities mixed together. Each resulting framework/module area should have self-tests that validate its behavior and protect the refactor.

## What I already know

* The current request targets the host workspace Rust code.
* A previous refactor moved directories into the current rough layout, but implementation files were not deeply split.
* The desired outcome is a more reasonable framework structure with self-test code for each framework/module area.
* Current host workspace root is `sources/host/wiremux`.
* The only workspace member today is `sources/host/wiremux/crates/wiremux-cli`, package/bin/lib name `wiremux`.
* Current Rust source size is concentrated in a few files: `main.rs` 1864 lines, `tui.rs` 3747 lines, `host_session.rs` 697 lines, `interactive.rs` 512 lines, `lib.rs` 1 line.
* `cargo test` currently passes with 97 tests.

## Assumptions (temporary)

* The refactor should preserve existing CLI/runtime behavior.
* The task is primarily backend/host Rust work, not ESP-IDF firmware or frontend work.
* Tests should use the repository's existing Rust test style where possible.
* This iteration should avoid changing the C core ABI or protocol behavior.

## Open Questions

* None currently blocking.

## Requirements (evolving)

* Split mixed host Rust implementation files into clearer functional modules.
* Add or update self-tests for each major split module/framework area.
* Preserve existing behavior unless an explicit design decision changes it.
* Keep protocol parsing and host session behavior unit-testable outside serial-device dependent command paths.
* Split the host workspace into multiple Cargo crates now, to establish future framework boundaries.
* Use simple crate names such as `cli` and `tui`; do not prefix every host crate with `wiremux-`.
* Keep the public user-facing binary command name `wiremux`.
* Keep shared serial/terminal interactive backend code in an independent `interactive` crate.

## Acceptance Criteria (evolving)

* [x] Host Rust code is organized into clearly named modules with separated responsibilities.
* [x] Each major module/framework area has focused tests.
* [x] Existing host Rust tests continue to pass.
* [x] The refactor does not introduce unrelated firmware/frontend changes.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Rust checks/tests pass for the affected workspace/crate.
* Relevant Trellis specs are followed.
* Docs/spec notes updated if the host framework boundary changes.

## Out of Scope (explicit)

* Firmware protocol behavior changes unless required to keep host tests compiling.
* Frontend work.
* Feature additions unrelated to structural refactoring.

## Technical Notes

### Relevant Specs

* `.trellis/spec/backend/directory-structure.md`: host Rust code belongs under `sources/host/wiremux`; parser/state machines must stay unit-testable and out of CLI-only entrypoints.
* `.trellis/spec/backend/quality-guidelines.md`: required host gate is `cargo test`, `cargo check`, and `cargo fmt --check`; preserve host protocol and interactive console contracts.
* `.trellis/spec/backend/error-handling.md`: host stream decode and interactive I/O error behavior must remain deterministic.
* `.trellis/spec/guides/code-reuse-thinking-guide.md`: this refactor should extract shared logic rather than duplicate it across modules.
* `docs/source-layout-build.md`: target host path is already `sources/host/wiremux` workspace root with member crate `crates/wiremux-cli`.

### Current Code Map

* `crates/wiremux-cli/src/main.rs`: CLI args, command dispatch, listen/send/passthrough loops, serial port candidate/opening helpers, diagnostics file naming, display output formatting, event diagnostics, frame building helpers, passthrough key policy helpers, and many unit tests.
* `crates/wiremux-cli/src/tui.rs`: TUI app state, event loop, terminal input handling, mouse/selection handling, stream event handling, rendering, output wrapping, clipboard/base64 helpers, layout helpers, and many unit tests.
* `crates/wiremux-cli/src/interactive.rs`: interactive backend mode, compat backend, Unix mio backend wrapper, terminal polling helpers, and small retry tests.
* `crates/wiremux-cli/src/host_session.rs`: Rust wrapper around the C host session FFI, event data models, frame builders, callback aggregation, C layout structs, and extern bindings.
* `crates/wiremux-cli/src/lib.rs`: currently only exports `host_session`.

### Baseline

* `cd sources/host/wiremux && cargo test` passes: 97 tests.

### Implementation Notes

* Host workspace is now split into `crates/host-session`, `crates/interactive`, `crates/tui`, and `crates/cli`.
* The public binary remains `wiremux` from the `cli` package.
* `host-session` owns C core linking, host event/data models, input frame builders, and host-session diagnostics helpers.
* `interactive` owns terminal/serial interactive backends, serial candidate path helpers, passthrough key mapping, and interrupted-operation retry helpers.
* `tui` owns ratatui behavior and now has `args` and `clipboard` submodules.
* `cli` owns argument parsing, diagnostics file creation, listen display formatting, serial opening, and the `listen`/`send`/`passthrough` command loops.
* Updated docs/spec references from the old `crates/wiremux-cli` path to the new crate layout.

### Validation

* `cd sources/host/wiremux && cargo fmt --check`
* `cd sources/host/wiremux && cargo check`
* `cd sources/host/wiremux && cargo test`
* `cd sources/host/wiremux && cargo check --features generic`
* `cd sources/host/wiremux && cargo check --features esp32`
* `cd sources/host/wiremux && cargo check --features all-vendors`
* `cd sources/host/wiremux && cargo check --features all-features`
* `tools/wiremux-build check host`

## Research Notes

### Constraints from our repo/project

* Keep one host workspace root at `sources/host/wiremux`.
* Existing crate name is `wiremux`; changing crate/package names would increase churn and may affect commands/docs.
* C host session API already owns protocol parsing, manifest, batch, compression, and compatibility behavior. Rust should wrap and present it, not reimplement protocol parsers.
* Current tests are valuable but are colocated with oversized files; splitting should move tests with their modules so each area self-tests.

### Feasible approaches here

**Approach A: Internal module decomposition inside the existing crate** (Recommended)

* How it works: keep the current workspace and `wiremux` crate, split `main.rs` and `tui.rs` into focused modules such as `cli`, `commands`, `serial`, `diagnostics`, `display`, `input_frame`, `passthrough`, and `tui::{app,event,render,selection,scroll,clipboard}`.
* Pros: lowest risk, preserves crate/package/CLI identity, lets tests move with behavior, and fits this refactor-only goal.
* Cons: does not create independent crates for future reuse yet.

**Approach B: Split library domains into multiple workspace crates**

* How it works: create crates such as `host-session`, `cli`, and `tui`, then make the binary compose them while preserving the `wiremux` command name.
* Pros: stronger API boundaries and clearer long-term reuse story.
* Cons: higher Cargo/config/doc churn and public-ish internal API decisions become part of this task.

**Approach C: Hybrid staged split**

* How it works: do Approach A now, but create module boundaries and visibility that can later become separate crates without large rewrites.
* Pros: keeps current risk low while avoiding module shapes that trap future crate extraction.
* Cons: requires discipline on visibility and module naming; still leaves all code in one package for now.

### Prior Productization Context

* `.trellis/tasks/archive/2026-04/04-29-source-layout-build-productization/prd.md` recorded that the previous productization task should move host to a Cargo workspace skeleton but avoid deep host code refactoring in that first pass.
* `.trellis/tasks/archive/2026-04/04-29-pr5-host-workspace-skeleton/prd.md` explicitly says the workspace should later host crates such as host core, CLI, TUI, and adapters.
* `.trellis/workspace/magicdian/journal-1.md` session 27 records the follow-up note that future host behavior refactoring can happen inside `sources/host/wiremux/crates/`.

## Decision (ADR-lite, evolving)

**Context**: The existing workspace skeleton was intentionally shallow; current code still mixes CLI, passthrough, TUI, host-session FFI, serial handling, diagnostics, display formatting, and tests in a small number of large files.

**Decision**: Use a multi-crate workspace split for this task, with simple host-local crate names. Keep shared terminal/serial interactive backend code in a separate `interactive` crate. Keep the user-facing binary named `wiremux`.

**Consequences**: This creates stronger boundaries now and supports future host framework work, but the implementation must carefully preserve current command behavior and build feature behavior.

## Candidate Crate Shape

* `crates/host-session`: Rust wrapper around portable C host session, host event/data models, frame builders, and FFI tests. Package can use hyphenated name; Rust crate name becomes `host_session`.
* `crates/interactive`: shared serial/terminal interactive backend used by both passthrough and TUI, including compat/mio backends and `Interrupted` retry tests. This is confirmed in scope.
* `crates/tui`: ratatui app state, rendering, selection/clipboard, stream event handling, TUI input behavior, and focused TUI tests.
* `crates/cli`: argument parsing, command dispatch, listen/send/passthrough command flows, serial port discovery/opening, diagnostics, display output formatting, and CLI tests. Exposes the `wiremux` binary.

This keeps names simple while avoiding a crate literally named `core`, because `core` is already a Rust standard library crate name and the repo already has `sources/core/c` for the portable C core.
