# Source Layout and Build Orchestration

This document defines the target repository shape for the Wiremux productization
work. It is the migration target for follow-up PRs; PR1 documents the contract
only and does not move runtime source files.

## Target Source Layout

```text
sources/
+-- api/
|   `-- proto/
+-- core/
|   `-- c/
+-- profiles/
+-- host/
|   `-- wiremux/
`-- vendor/
    `-- espressif/
        +-- generic/
        |   +-- components/
        |   `-- examples/
        +-- s3/
        |   `-- README.md
        `-- p4/
            `-- README.md
build/
tools/
+-- wiremux-build
`-- wiremux-build-helper/
.wiremux/
`-- build/
    `-- selected.toml
```

Top-level responsibilities:

- `sources/api`: stable API definitions shared across host, core, profiles, and
  vendor integrations. Protobuf files migrate here from `sources/core/proto`.
- `sources/core`: platform-neutral Wiremux protocol implementation. The C core
  remains under `sources/core/c`.
- `sources/profiles`: profile contracts and reusable profile implementations.
  PR1 does not edit PR6-owned profile skeleton files.
- `sources/host/wiremux`: Rust host CLI/library workspace root after migration
  from `sources/host`.
- `sources/vendor/espressif`: Espressif-owned integration surface. The generic
  ESP-IDF component and examples move under
  `sources/vendor/espressif/generic/{components,examples}`. S3 and P4 start as
  README placeholders until platform-specific content exists.
- `build`: checked-in build product metadata, templates, and scripts that belong
  to the product. Generated output must not be committed here.
- `tools`: developer and release tooling, including the future build
  orchestrator bootstrap and helper.
- `.wiremux`: local generated selection/cache state. This directory is ignored
  and must not be treated as source.

## Current-To-Target Migration

The current repository still uses the pre-migration paths:

| Current path | Target path | Owner PR |
| --- | --- | --- |
| `sources/core/proto` | `sources/api/proto` | PR2 |
| `sources/esp32/components/esp-wiremux` | `sources/vendor/espressif/generic/components/esp-wiremux` | PR3 |
| `sources/esp32/examples/esp_wiremux_console_demo` | `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo` | PR3 |
| `sources/host` | `sources/host/wiremux` | PR4 |
| host workspace skeleton | `sources/host/wiremux` workspace root with member crate at `sources/host/wiremux/crates/wiremux-cli` | PR5 |
| profile skeleton docs | `sources/profiles` | PR6 |
| build orchestrator | `tools/wiremux-build`, `tools/wiremux-build-helper` | PR7 |
| CI/release validation | migrated layout and generated outputs | PR8 |

Until each owner PR lands, operational commands in user docs may continue to use
current paths. New architecture and spec text should name both the current path
and the target path when the distinction matters.

## wiremux-build Boundary

`wiremux-build` is the long-term product build orchestrator. It is not a
compiler, package manager, or replacement for Cargo, CMake, or `idf.py`.

The planned implementation shape is:

- `tools/wiremux-build`: Python bootstrap and command entrypoint.
- `tools/wiremux-build-helper`: Rust helper for product-specific validation,
  metadata, and operations that benefit from compiled code.
- TOML configuration files for product defaults, presets, selected state, and
  tool policy.

The orchestrator may select products, validate tool availability, derive
environment exports, call underlying tools, and collect metadata. It must leave
actual compilation to the existing ecosystem tools for each layer.

## Lunch Model

The selected build state is stored in:

```text
.wiremux/build/selected.toml
```

That file is the source of truth after a lunch/select command succeeds. Optional
environment exports are derived state, produced by a command such as:

```bash
tools/wiremux-build env --shell bash
tools/wiremux-build env --shell zsh
```

Configuration priority is:

```text
CLI args > .wiremux/build/selected.toml > product defaults
```

Environment variables do not normally override the selected configuration. They
may be used for tool discovery or explicit debugging only when a command
documents that behavior.

Valid host presets are:

- `all-features`
- `generic-only`
- `device-only`

The combination `core-only + device-only` is invalid because `device-only`
requires a concrete device/vendor target, and `core-only` has none.

## Reproducibility Policy

CI and local development intentionally have different strictness:

- CI runs in strict mode per tool, with configurable policy for required tool
  versions, missing tools, dirty worktrees, and generated output drift.
- Local builds are tolerant by default. They should warn on dirty or deviated
  versions and write build metadata so a produced artifact can be diagnosed
  later.

Dirty or deviated inputs should be recorded as metadata rather than silently
discarded. Release workflows can promote those warnings to failures.

Future ignored generated paths:

```text
/.wiremux/
/build/out/
/tools/wiremux-build-helper/target/
```

## Staged PR Plan

1. PR1: finalize this documentation and Trellis spec target.
2. PR2: migrate the proto API boundary from `sources/core/proto` to
   `sources/api/proto`.
3. PR3: migrate Espressif source layout to
   `sources/vendor/espressif/generic/{components,examples}` and add S3/P4 README
   placeholders.
4. PR4: move the host Wiremux crate to `sources/host/wiremux`.
5. PR5: add the host workspace skeleton.
6. PR6: add profile skeleton docs under `sources/profiles`.
7. PR7: add the `wiremux-build` MVP orchestration surface.
8. PR8: update CI and release validation for the migrated layout.
