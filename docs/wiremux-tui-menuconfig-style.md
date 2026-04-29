# Wiremux TUI Menuconfig Style

This document adapts the menuconfig style matrix from
`bridging-io/docs/matrix/MENUCONFIG_STYLE_MATRIX.md` for Wiremux TUI settings.
It applies to local, trusted Wiremux TUI settings panels such as the serial
profile editor.

## Scope

- `wiremux tui` settings panels.
- Physical serial profile editing:
  - serial device path
  - baud rate
  - data bits
  - stop bits
  - parity
  - flow control

It does not define virtual TTY, broker, channel QoS, file transfer, modem, or
capture/replay UI behavior.

## Principles

- Use a single-column, step-by-step settings panel.
- Keep the content area centered and row text left-aligned.
- Use semantic row templates instead of handcrafted row strings.
- `>` marks current focus on actionable rows only.
- `Esc` closes the active popup first, backs out of settings next, and exits the
  settings panel from the root when there are no unsaved changes.
- Dirty state is derived from current draft settings compared with the settings
  baseline, not from edit history.
- Below `80x24`, show a resize-required overlay instead of rendering a broken
  settings panel.

## Row Templates

| Template ID | Intent | Focusable | Grammar | Key Contract |
| --- | --- | --- | --- | --- |
| `info-row` | Read-only status or hint | no | `--- Label` or `--- Label = Value` | none |
| `field-entry-row` | Editable field | yes | `Label (value) --->` | `Enter` opens editor popup |
| `action-row` | Managed action | yes | `Label --->` | `Enter` runs action or opens confirm |

Wiremux serial settings currently use:

| Field | Template | Editor |
| --- | --- | --- |
| Serial Device | `field-entry-row` | `text-input-modal` |
| Baud Rate | `field-entry-row` | `text-input-modal` |
| Data Bits | `field-entry-row` | `choice-list-modal` |
| Stop Bits | `field-entry-row` | `choice-list-modal` |
| Parity | `field-entry-row` | `choice-list-modal` |
| Flow Control | `field-entry-row` | `choice-list-modal` |
| Apply And Reconnect | `action-row` | immediate action |
| Save As Defaults | `action-row` | save result modal/message |
| Discard And Close | `action-row` | immediate action |

## Popup Templates

| Template ID | Intent | Key Contract |
| --- | --- | --- |
| `text-input-modal` | One-line text editor | `Left/Right` move caret, `Backspace` deletes, `Enter` commits, `Esc` cancels |
| `choice-list-modal` | One-of-many picker | `Up/Down` move, `Enter` or `Space` confirms, `Esc` cancels |
| `confirm-modal` | Save/discard/cancel decision | `Left/Right` move button focus, `Enter` confirms, `Esc` cancels |
| `message-modal` | Short acknowledgement or error | `Enter` or `Esc` closes |
| `blocking-overlay` | Viewport too small | normal settings navigation disabled |

## Serial Profile Contract

The settings panel edits only the physical serial profile:

```toml
[serial]
port = "/dev/cu.usbserial-0001"
baud = 115200
data_bits = 8
stop_bits = 1
parity = "none"
flow_control = "none"
```

Defaults:

| Field | Default | Allowed Values |
| --- | --- | --- |
| `baud` | `115200` | positive integer supported by the OS/device |
| `data_bits` | `8` | `5`, `6`, `7`, `8` |
| `stop_bits` | `1` | `1`, `2` |
| `parity` | `"none"` | `"none"`, `"odd"`, `"even"` |
| `flow_control` | `"none"` | `"none"`, `"software"`, `"hardware"` |

Virtual channel baud is intentionally out of scope. Future virtual TTY endpoints
may expose termios compatibility metadata, but that must not be confused with
the real physical transport serial profile.

## Dirty Tracking

- The settings title uses a dirty suffix only when the draft differs from the
  baseline profile.
- Reverting a field to its baseline value clears that field's dirty state.
- Leaving settings with unsaved changes opens a save/discard/cancel confirm.
- Applying settings updates the active runtime profile and triggers reconnect.
- Saving defaults writes the draft profile to the global config file through an
  explicit user action.

## Navigation

| Context | `Up/Down` | `Left/Right` | `Space` | `Enter` | `Esc` |
| --- | --- | --- | --- | --- | --- |
| Settings list | move focus across actionable rows | none | no action | edit field or run action | close or confirm unsaved changes |
| Text input popup | none | move caret | insert space | commit | cancel |
| Choice popup | move selected option | none | confirm option | confirm option | cancel |
| Confirm popup | none | move button focus | no action | confirm selected button | cancel |

## Layout

- Minimum viewport: `80x24`.
- Main settings panel is centered over the existing TUI.
- Rows remain single-line.
- Long values should be truncated before they can overlap the `--->` suffix.
- Popup overlays are centered and freeze background settings navigation.

## Maintenance

Any Wiremux TUI settings feature that changes row grammar, popup behavior,
dirty tracking, key semantics, or minimum viewport behavior must update this
document in the same change.
