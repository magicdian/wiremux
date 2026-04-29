# Wiremux TUI Shortcut Matrix

This matrix is the source of truth for Wiremux TUI keyboard and pointer
contracts.

| Context | Shortcut/Input | Effect | Notes |
| --- | --- | --- | --- |
| Global | `Ctrl-C` | Quit TUI | Immediate exit. |
| Global | `Ctrl-]` | Quit TUI | Terminal support varies. |
| Global | `Esc`, then `x` | Quit TUI | Portable exit sequence. |
| Global | `Ctrl-B`, then `0` | Show all channels | Unfiltered view is read-only. |
| Global | `Ctrl-B`, then `1..9` | Filter to channel 1 through 9 | Input targets the filtered channel only when manifest allows input. |
| Global | `Ctrl-B`, then `s` | Open settings | Settings follow `docs/wiremux-tui-menuconfig-style.md`. |
| Global | `Ctrl-B`, then `v` | Toggle virtual serial for this session | Creates or closes generic enhanced virtual endpoints when supported. |
| Global | `Ctrl-B`, then `o` | Toggle active channel input owner | Requires a filtered input-capable channel; switches between host and virtual serial ownership. |
| Output pane | Mouse wheel up | Scroll older output | Pauses live-follow. |
| Output pane | Mouse wheel down | Scroll toward live output | Reaches live-follow at the bottom. |
| Output pane | Scrollbar up button | Jump to oldest visible output | Button action is immediate. |
| Output pane | Scrollbar down button | Return to live-follow | Button action is immediate. |
| Output pane | Drag scrollbar track | Move through scrollback | Drag target may animate across frames. |
| Output/status selection | Mouse drag | Select application-rendered text | Selection is app-managed because mouse capture blocks terminal-native selection. |
| Output/status selection | `y` | Copy selection | Uses the app copy path. |
| Output/status selection | `Ctrl-Shift-C` | Copy selection | Terminal support varies. |
| Output/status selection | `Command-C` | Copy selection on macOS terminals | Terminal support varies. |
| Output/status selection | `Esc` | Clear selection | Selection is cleared before `Esc` is interpreted as the exit prefix. |
| Read-only input | Text keys | Drop input and show read-only status | Unfiltered mode and output-only channels are read-only. |
| Read-only input | Empty `Enter` twice | Restore live-follow | Does not send input. |
| Line input | Text keys | Edit bottom input buffer | Only for input-capable non-passthrough channels. |
| Line input | `Backspace` | Delete one character | Bottom input buffer only. |
| Line input | `Enter` | Send complete line | Sends a `WMUX` input frame to the active channel. |
| Line input | `Esc` | Clear input | Does not send an `Esc` byte in line mode. |
| Passthrough input | Text/control/navigation keys | Send key payload promptly | Uses manifest passthrough policy. |
| Passthrough input | `Esc` timeout | Send `Esc` byte | If not followed by `x`, `Esc` is forwarded after the timeout. |
| Settings list | `Up`/`Down` | Move focus | Focus moves across actionable rows. |
| Settings list | `Enter` | Edit field or run action | Depends on selected row. |
| Settings list | `Esc` | Close or confirm unsaved changes | Dirty settings open a confirm popup. |
| Settings text popup | `Left`/`Right` | Move caret | Text fields only. |
| Settings text popup | `Backspace` | Delete one character | Text fields only. |
| Settings text popup | `Enter` | Commit value | Validates before applying. |
| Settings text popup | `Esc` | Cancel edit | Does not change draft value. |
| Settings choice popup | `Up`/`Down` | Move selected option | Choice fields only. |
| Settings choice popup | `Space` or `Enter` | Confirm option | Updates draft value. |
| Settings choice popup | `Esc` | Cancel choice | Does not change draft value. |
| Settings confirm popup | `Left`/`Right` | Move button focus | Used for unsaved settings. |
| Settings confirm popup | `Enter` | Confirm selected action | Save/apply, discard, or cancel. |
| Settings message popup | `Enter` or `Esc` | Close message | Acknowledgement only. |

## Maintenance

Any TUI change that adds, removes, or changes a shortcut, pointer action,
settings key, virtual serial key, or input ownership behavior must update this
matrix in the same change.
