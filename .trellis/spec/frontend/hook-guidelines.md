# Hook Guidelines

> Hook conventions for future frontend work.

---

## Overview

There are no React hooks or frontend data-fetching hooks in this repository.

The current stateful loops are not frontend hooks:

- `HostSession::feed()` and `HostSession::finish()` in
  `sources/host/wiremux/crates/host-session/src/lib.rs`.
- The host `listen()` loop in `sources/host/wiremux/crates/cli/src/main.rs`.
- FreeRTOS tasks `mux_task()` and `mux_input_task()` in
  `sources/vendor/espressif/generic/components/esp-wiremux/src/esp_wiremux.c`.

Do not add hook guidelines by copying generic React patterns. Add them only when
a frontend framework is introduced.

## Custom Hook Patterns

No custom hook patterns exist.

If a future React UI is added, hooks that wrap serial or mux state must keep a
clear boundary:

- UI hooks manage browser/UI state.
- Host-side serial access remains outside the browser unless a native shell or
  local bridge provides it.
- Protocol parsing should stay shared with, generated from, or validated against
  the Rust/C implementation.

## Data Fetching

No frontend data-fetching library is used.

Future UI data access must state the transport:

- Local HTTP/WebSocket bridge.
- Tauri/native command bridge.
- File import of captured frames.
- Mock fixture for docs/demo-only UI.

Each transport needs tests for disconnects, corrupt frames, and binary payloads.

## Naming Conventions

No hook naming conventions exist.

If React is adopted, hook names should use `use*` and describe mux concepts
directly, for example `useMuxFrames` or `useChannelFilter`. These names are
examples only; do not create them without a UI task.

## Forbidden Patterns

- Do not put serial-port ownership inside a browser-only hook without a real
  native/local bridge.
- Do not duplicate the `WMUX` scanner in UI code without byte-level tests against
  the backend contract.
- Do not hide reconnect or partial-frame behavior inside UI state without exposing
  errors to the user.
- Do not use hooks as global mutable stores for protocol constants.

## Common Mistakes

- Assuming Web Serial support is sufficient for all target environments.
- Forgetting that most physical serial devices cannot be reliably opened by
  separate send and listen processes at the same time.
- Treating a channel output filter and an input send channel as the same state.
