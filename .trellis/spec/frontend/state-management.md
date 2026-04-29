# State Management

> State management conventions for current and future user-facing code.

---

## Overview

There is no frontend state-management library in this project.

Current state is managed explicitly in backend/runtime code:

- ESP mux singleton state: `static mux_context_t s_mux` in
  `sources/vendor/espressif/generic/components/esp-wiremux/src/esp_wiremux.c`.
- ESP adapter state: `s_console_config`, `s_console_bound`, `s_log_config`, and
  `s_log_bound` in component adapter files.
- Host CLI command state: `CliCommand`, `ListenArgs`, and `SendArgs` in
  `sources/host/wiremux/crates/cli/src/args.rs`.
- Host protocol session state: C `wiremux_host_session_t` wrapped by
  `sources/host/wiremux/crates/host-session/src/lib.rs`.

Target paths are `sources/vendor/espressif/generic/components/esp-wiremux/src/`
for ESP adapter state and `sources/host/wiremux/crates/{host-session,interactive,tui,cli}/src/`
for host state.

## State Categories

Current state categories:

| State | Owner | Lifetime |
|-------|-------|----------|
| mux config and channel registry | ESP component | process/device runtime |
| outbound queue | ESP FreeRTOS queue | until sent, dropped, or stopped |
| RX frame buffer | ESP component | runtime, bounded by max payload |
| host scanner buffer | Rust host process | CLI process lifetime |
| CLI args | Rust host process | parsed once at startup |

## When To Use Global State

On ESP, the current component uses static module state because it is a singleton
service around one configured transport. Keep that assumption explicit.

Do not add additional global state unless:

- It is bounded.
- It is protected by the existing mutex or another documented synchronization
  primitive.
- Start/stop/reset behavior is defined.
- Error behavior is documented in `.trellis/spec/backend/error-handling.md`.

For future frontend code, do not introduce global state until at least two
independent UI areas need the same data. Prefer local state for filters and form
inputs.

## Server State

There is no server state today.

If a future frontend talks to a host bridge, treat the bridge as the owner of
serial state. The UI should subscribe to decoded events and send explicit
commands; it should not assume it owns the serial device directly.

## Derived State

Current derived state examples:

- Host CLI derives `send_channel` from `--send-channel`, `--channel`, or default
  channel 1 when `--line` is provided.
- Host printable payload rendering derives escaped UTF-8 or hex from raw bytes.
- ESP log adapter derives bounded log lines from `vprintf` input.

Keep derived state recomputable. Do not store both raw bytes and formatted text as
separate authoritative values.

## Forbidden Patterns

- Do not add a frontend state library to solve CLI argument parsing.
- Do not store unbounded frame history in ESP RAM.
- Do not let UI state mutate protocol constants.
- Do not conflate output filtering with input routing.

## Common Mistakes

- Forgetting to clear or bound stream buffers on corrupt input.
- Introducing state that survives `esp_wiremux_stop()` without documenting
  restart behavior.
- Treating formatted payload text as the original payload.
