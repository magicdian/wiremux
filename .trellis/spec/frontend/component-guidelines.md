# Component Guidelines

> Component conventions for future frontend work.

---

## Overview

There are no frontend components in the current codebase. This file intentionally
does not prescribe a component framework because the project has not chosen one.

Current interactive behavior is implemented as:

- Rust CLI argument parsing and serial I/O in `sources/host/src/main.rs`.
- ESP-IDF console commands in
  `sources/esp32/examples/console_mux_demo/main/console_mux_demo_main.c`.
- ESP mux adapters in `sources/esp32/components/esp_serial_mux/src/`.

## Component Structure

No component file structure exists yet.

If a future task adds UI components, the task must first document:

- Framework and build tool.
- Component file naming.
- Props typing strategy.
- Styling strategy.
- Test strategy.
- How the UI obtains mux data from the host side.

## Props Conventions

No props conventions exist today.

Future UI props should mirror stable protocol concepts rather than ad-hoc labels:

- `channelId` maps to `channel_id`.
- `direction` maps to input/output direction.
- `sequence` maps to mux sequence number.
- `timestampUs` maps to `timestamp_us`.
- `payloadKind` maps to `PayloadKind`.

Keep conversion code explicit at the host/UI boundary. Do not silently reinterpret
binary payloads as text.

## Styling Patterns

No frontend styling system exists.

Future UI work must choose and document one styling approach in the same task.
Do not mix global CSS, CSS modules, Tailwind, and inline style systems without an
explicit reason.

## Accessibility

No browser UI exists, so there are no current accessibility patterns.

If a UI is added, controls for serial ports, channels, filters, and send actions
must be keyboard operable and labeled. Channel output must remain inspectable as
text because this is a debugging/diagnostics tool.

## Real Examples To Preserve

Current UI-like command surfaces:

```bash
esp-serial-mux listen --port <path> [--channel id]
esp-serial-mux listen --port <path> [--channel output_id] [--send-channel input_id] --line <text>
esp-serial-mux send --port <path> --channel <id> --line <text>
```

ESP console command examples live in
`sources/esp32/examples/console_mux_demo/main/console_mux_demo_main.c`:

- `help`
- `hello`
- `mux_manifest`
- `mux_hello`
- `mux_log`

## Forbidden Patterns

- Do not invent reusable components for a frontend that does not exist.
- Do not hard-code local serial paths such as `/dev/cu.usbmodem2101` into UI
  components.
- Do not display decoded mux frames without preserving channel, direction,
  sequence, timestamp, kind, flags, and payload.
- Do not create a component API that conflicts with the existing protocol field
  names without documenting the mapping.

## Common Mistakes

- Designing visual components before deciding the host bridge.
- Treating all payloads as UTF-8 text; binary payloads are valid.
- Making channel filtering a display-only feature while send-channel behavior
  still needs a separate input target.
