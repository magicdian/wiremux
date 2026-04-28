# Directory Structure

> Current frontend status and future UI placement rules.

---

## Overview

There is no frontend application in this repository today. No React, Vite,
Next.js, browser UI, component tree, CSS system, or TypeScript frontend source is
committed.

The current user-facing surfaces are:

- Rust host CLI: current `sources/host/wiremux/crates/wiremux-cli/src/main.rs`, target
  `sources/host/wiremux/crates/wiremux-cli/src/main.rs`
- ESP-IDF demo application: current
  `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo/`, target
  `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo/`
- Documentation: `docs/zh/`

Do not create frontend directories or imply a frontend architecture unless the
task explicitly asks for a UI.

## Actual Directory Layout

This is the current pre-migration layout. New architecture work should use the
target source layout in `docs/source-layout-build.md`.

```text
docs/
└── zh/
    ├── channel-binding.md
    ├── esp-idf-console-integration.md
    ├── getting-started.md
    ├── host-tool.md
    └── troubleshooting.md
sources/
├── host/
│   ├── Cargo.toml
│   ├── proto/
│   └── src/
└── esp32/
    ├── components/esp-wiremux/
    └── examples/esp_wiremux_console_demo/
```

## Future UI Placement

If a future task adds a frontend, define the app boundary in the PRD before
creating files. The likely choices are:

- `sources/web/` for a browser-based host companion UI.
- `docs/` only for static documentation, not interactive application code.
- Keep generated build output out of git.

New frontend work must state how it talks to the existing host/ESP surfaces:

- Direct serial access is not available from ordinary browser code.
- A browser UI needs a host-side bridge, local service, native shell, or file
  import/export boundary.
- Protocol constants must be derived from or checked against the Rust/C contract
  documented in `.trellis/spec/backend/directory-structure.md`.

## Naming Conventions

No active frontend naming conventions exist.

Future conventions must be documented before implementation, including:

- Application root directory.
- Component directory structure.
- Test location.
- Asset location.
- Build and dev-server commands.

## Examples

Use these existing user-facing files as the baseline for terminology and product
behavior:

- `docs/zh/getting-started.md`
- `docs/zh/host-tool.md`
- `docs/zh/esp-idf-console-integration.md`
- `sources/host/wiremux/crates/wiremux-cli/src/main.rs`
- `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo/main/esp_wiremux_console_demo_main.c`

## Forbidden Patterns

- Do not add `src/components`, `app/`, or `pages/` at repository root.
- Do not add frontend dependencies to `sources/host/wiremux/crates/wiremux-cli/Cargo.toml`.
- Do not describe a React/Vite/Next architecture as existing until those files
  are committed.
- Do not build a UI that bypasses the `WMUX` frame and `MuxEnvelope` protocol
  contract.

## Common Mistakes

- Treating documentation pages as frontend application code.
- Assuming browser code can open `/dev/cu.*` or `/dev/tty.*` directly.
- Creating a polished UI before defining the host bridge and protocol test path.
