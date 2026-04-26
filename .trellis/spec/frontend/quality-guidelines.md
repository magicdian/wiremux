# Quality Guidelines

> Quality standards for future frontend work.

---

## Overview

No frontend build, lint, or test pipeline exists today. Frontend work must not be
treated as established until the repository contains an actual app and its
commands are documented here.

Current quality gates are backend/runtime gates:

```bash
cd sources/host
cargo fmt --check
cargo check
cargo test
```

ESP-IDF changes should be built from
`sources/esp32/examples/console_mux_demo` with `idf.py build` when ESP-IDF is
available.

## Forbidden Patterns

- Do not add UI code without adding its build/test commands to this file.
- Do not add a mock-only UI that claims hardware behavior without documenting the
  bridge or fixture source.
- Do not duplicate protocol decoding in UI code without cross-language tests.
- Do not hide binary payloads or corrupt-frame diagnostics from a diagnostics UI.
- Do not commit generated frontend build artifacts.

## Required Patterns For Future UI

Any future frontend task must define:

- Framework and package manager.
- Dev command and build command.
- Unit/component test command.
- Browser or native-shell manual verification steps.
- How the UI obtains mux frames and sends input frames.
- How protocol constants are synchronized or checked.

Diagnostics UI must preserve these fields when displaying decoded frames:

- channel ID
- direction
- sequence
- timestamp
- payload kind
- flags
- payload bytes or escaped text/hex rendering

## Testing Requirements

Until a frontend exists, there are no frontend tests to run.

When a frontend is added, minimum tests must cover:

- Channel filtering.
- Send-channel selection independent from output filter.
- UTF-8 payload rendering.
- Binary payload rendering.
- Corrupt frame/error display.
- Serial bridge disconnect/reconnect state.

## Code Review Checklist

- Does the change create a real frontend app, or only docs?
- Are build/test commands documented and runnable?
- Does UI state match the backend protocol model?
- Are protocol constants checked against Rust/C definitions?
- Are serial permissions, disconnects, and exclusive-port behavior handled?
- Are generated artifacts excluded from git?

## Common Mistakes

- Adding generic frontend boilerplate before defining the serial bridge.
- Testing only mocked happy-path text payloads.
- Letting display filters change what channel receives host input.
