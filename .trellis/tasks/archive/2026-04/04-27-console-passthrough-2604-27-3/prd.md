# brainstorm: console passthrough mode 2604.27.3

## Goal

Implement passthrough console capability now that the functional framework is usable, extending beyond the current `WIREMUX_CONSOLE_MODE_LINE` path. This is a cross-layer change involving the Rust host tool and ESP-IDF SDK/component, and the release/version should move to `2604.27.3` as part of the task.

## What I already know

* The current ESP console adapter exposes `ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH` in the public enum, mapped to the shared manifest value `WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH`.
* `esp_wiremux_bind_console()` currently returns `ESP_ERR_NOT_SUPPORTED` for `ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH`, so the advertised API is reserved but not implemented.
* The shared proto/core manifest already defines `ChannelInteractionMode.LINE = 1` and `PASSTHROUGH = 2`.
* Host manifest decoding already understands line and passthrough interaction modes, but host behavior does not yet switch input mode based on them.
* Host input currently has line-oriented paths only: `listen --line`, one-shot `send --line`, and TUI bottom-line `Enter` send.
* ESP input dispatch is generic and already passes raw payload bytes to the registered channel input handler after `WMUX` frame, CRC, envelope, direction, channel, and max payload validation.
* The ESP demo binds channel 1 as line-mode console and writes command output explicitly through `esp_wiremux_write_text(1, ...)`.
* Version declarations currently use `2604.27.2` in `VERSION`, host Cargo files, ESP component manifest, ESP public header, README badges, and release docs.

## Assumptions (temporary)

* Passthrough means raw bytes are still transported inside normal `WMUX` frames and `MuxEnvelope(direction=input)`, not as unframed serial bytes.
* Existing line mode must remain backward compatible and keep current tests/usage working.
* The ESP console passthrough MVP should not require changing the portable frame/envelope binary layout.
* The host should preserve single-serial-handle behavior; it must not require a second process opening the same port.

## Open Questions

* Final SDK passthrough submode type/function names should be finalized during implementation while preserving the agreed core/ESP alias boundary.

## Requirements (evolving)

* Add passthrough console capability across host and SDK.
* Preserve existing line-mode behavior.
* Keep passthrough payloads framed as `WMUX` + `MuxEnvelope(direction=input)` on the transport.
* Update version metadata to `2604.27.3`.
* Add proto API v2 passthrough policy metadata with `input_newline_policy`, `output_newline_policy`, `echo_policy`, and `control_key_policy`.

## Acceptance Criteria (evolving)

* [x] Host can send passthrough bytes to a selected channel without waiting for a full `--line` command.
* [x] SDK can bind a channel in passthrough mode and advertise `default_interaction_mode = PASSTHROUGH` in the manifest.
* [x] SDK passthrough input dispatch handles arbitrary payload chunks and preserves bytes within `max_payload_len`.
* [x] Manifest API v2 exposes passthrough policy metadata and API v1 devices remain usable with default policy behavior.
* [x] TUI passthrough input remains device/remote-echo owned, follows live output for the active passthrough channel, and treats split backspace/delete echo sequences as edits on the current channel stream line even when other channels interleave logs.
* [x] `Ctrl-]` exits both dedicated passthrough mode and TUI mode when the terminal reports a control key; dedicated passthrough also supports `Esc` then `x` as a terminal-independent exit sequence.
* [x] Existing line-mode behavior and tests continue to work.
* [x] Version metadata reports `2604.27.3` where package/release metadata expects it.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Host checks pass: `cargo fmt --check`, `cargo check`, and `cargo test` in `sources/host`.
* Portable C core checks run if core files change.
* ESP-IDF example build is run if ESP-IDF is available, or the limitation is reported.
* Docs/notes updated if CLI/API usage changes.
* Rollout/rollback considered for line-mode compatibility.

## Out of Scope (explicit)

* Replacing the existing line mode.
* Frontend UI work; there is no frontend application in this repository.
* Changing the `WMUX` frame layout or protobuf field numbers unless a later decision explicitly requires it.
* Multi-client broker/PTY service unless selected as a future extension, not MVP.

## Research Notes

### Repo constraints

* SDK public API already has the mode enum but no passthrough implementation.
* SDK channel registration already stores `interaction_mode` and emits it in `DeviceManifest`, so passthrough announcement is low risk once binding is supported.
* SDK dispatch already passes raw payload bytes to input handlers; passthrough can reuse this rather than adding a second parser.
* The hard part is console semantics: current console integration calls `esp_console_run(line)` and does not instantiate an ESP-IDF REPL or VFS-backed stdin/stdout loop.
* Host CLI parser currently accepts `listen`, `send`, and `tui`; no interactive raw/passthrough command exists.
* TUI currently edits a local line buffer and sends only on `Enter`, so key-by-key passthrough changes input UX and terminal handling.
* Existing docs already mention future passthrough/key-stream support in host and console integration docs.

### Comparable patterns

* Serial terminal passthrough normally uses raw terminal mode and forwards each byte/key event immediately; line editing happens on the device or remote endpoint.
* CLI tools often keep one-shot line send separate from interactive attach/passthrough to avoid surprising scripts.
* Muxed protocols generally keep payload bytes framed even for raw interactive sessions, so framing remains recoverable and channel-aware.
* REPL integrations usually choose one owner for line discipline: either host-side line editing sends complete commands, or device-side line editing receives raw keys and emits echo/prompt/output.

### Feasible approaches here

**Approach A: Add a dedicated host passthrough command plus SDK raw console handler** (Recommended)

* How it works: add `wiremux passthrough --port <path> --channel <id>` that opens terminal raw mode, sends stdin key bytes as framed input payloads, and listens on the same serial handle for output. SDK enables `ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH`, registers channel 1 with passthrough interaction mode, and lets the application choose a passthrough submode such as raw byte callback, SDK line discipline that executes `esp_console_run()` on CR/LF, or ESP-IDF/VFS REPL-style integration.
* Pros: clear CLI boundary, does not disrupt current `listen --line` or TUI UX, gives immediate interactive capability.
* Cons: more SDK config surface and validation than simply accepting raw chunks.

**Approach B: Manifest-driven TUI passthrough mode**

* How it works: TUI requests manifest, detects `default_interaction_mode = PASSTHROUGH`, and sends key events immediately instead of buffering until `Enter` for that channel. SDK passthrough handling is similar to Approach A.
* Pros: best integrated interactive UX and uses existing manifest control plane.
* Cons: more TUI state complexity; risks surprising users because TUI input behavior changes per channel; harder to test fully.

**Approach C: SDK-only passthrough primitive, host raw send remains minimal**

* How it works: SDK stops rejecting passthrough and exposes raw input callback semantics or a passthrough console hook, but host only adds low-level `send --raw`/`--bytes` style functionality and docs.
* Pros: smallest cross-layer code change.
* Cons: does not deliver practical interactive console passthrough; likely under-shoots the user goal.

## Expansion Sweep

### Future evolution

* Passthrough can later feed a broker/PTY so tools like `screen`, `minicom`, or IDE terminals can attach to a single mux channel.
* Manifest-declared interaction modes should remain the extension point for host UX decisions.

### Related scenarios

* `listen --line` and `send --line` should remain stable for script-friendly line-mode verification.
* TUI should eventually switch between line-mode and passthrough based on manifest, but that may be better after a dedicated passthrough command proves the byte contract.

### Failure and edge cases

* Raw terminal mode must restore terminal state on exit, Ctrl-C, and serial errors.
* Passthrough must handle backspace/delete, CR/LF/CRLF, empty commands, payload chunking at `max_payload_len`, reconnects, and channels that do not advertise input/passthrough.
* Host should surface decode/CRC errors in diagnostics without corrupting interactive output.

## Technical Notes

* Relevant specs read: backend directory structure, error handling, quality guidelines, logging guidelines, database/persistence boundary, cross-layer thinking guide, code reuse guide.
* Likely files: `sources/host/src/main.rs`, `sources/host/src/tui.rs` if Approach B is selected, `sources/esp32/components/esp-wiremux/include/esp_wiremux_console.h`, `sources/esp32/components/esp-wiremux/src/esp_wiremux_console.c`, `sources/esp32/examples/esp_wiremux_console_demo/main/esp_wiremux_console_demo_main.c`, `README*.md`, `docs/zh/*.md`, `VERSION`, `sources/host/Cargo.toml`, `sources/host/Cargo.lock`, `sources/esp32/components/esp-wiremux/idf_component.yml`, `sources/esp32/components/esp-wiremux/include/esp_wiremux.h`, `docs/esp-registry-release.md`.
* Code-spec depth triggers: cross-layer host/SDK input behavior, command/API signature changes, manifest interaction-mode contract, release version metadata.

## Decision (ADR-lite)

**Context**: Host and SDK both already carry interaction-mode metadata, and earlier docs indicate host behavior should be selected from the device manifest rather than user-specified mode flags.

**Decision**: Use a hybrid of Approach A and Approach B. The host must not require the user to manually specify line vs passthrough mode. Instead, the host consumes the device manifest, tracks each channel's declared `default_interaction_mode`, and enables passthrough behavior for channels that advertise `PASSTHROUGH`. The SDK side is responsible for registering passthrough-capable channels and emitting that capability in the manifest.

**Consequences**: This preserves the manifest as the single source of truth for channel interaction behavior. Host UX should remain stable for line-mode channels and automatically become key/raw-byte passthrough for passthrough channels. This likely means TUI needs manifest-driven input behavior, while CLI one-shot `listen --line`/`send --line` remain line-oriented compatibility paths unless a separate interactive path is added later.

## Requirements (decision update)

* Host interaction mode selection must be manifest-driven per channel.
* Users should not pass a manual `--mode line|passthrough` flag for normal operation.
* When a channel manifest declares `default_interaction_mode = PASSTHROUGH`, interactive host input for that channel sends raw/key bytes without waiting for `Enter` as a line command.
* When a channel manifest declares `LINE` or has no passthrough declaration, current line-mode behavior remains unchanged.
* SDK channel registration and console binding must advertise passthrough correctly in `DeviceManifest`.
* `2604.27.3` host MVP includes both a dedicated attach/passthrough command and TUI manifest-driven passthrough behavior.

## Passthrough Scope Exploration

Passthrough should not be console-only. The shared model already defines passthrough as a generic `ChannelInteractionMode`, so it should describe a channel interaction style rather than a specific ESP console binding.

### Working definition

Passthrough means the host treats the channel as an interactive byte stream:

* input bytes are sent promptly, usually per keypress or small chunks;
* output bytes are rendered promptly and without line-oriented command assumptions;
* the host should not require `Enter` to submit a complete logical command;
* framing remains `WMUX` + `MuxEnvelope`, so passthrough is raw-at-channel-level, not raw-at-transport-level.

### Not the same as binary payload

A channel can carry binary payloads without being passthrough. Binary payload means the payload bytes are not UTF-8 text. Passthrough means the UX and dispatch semantics are stream-like and interactive.

Examples:

* Binary non-passthrough: telemetry blob, protobuf record, sensor frame, file chunk.
* Passthrough text-like: shell, console, Lua/Python REPL, AT command terminal.
* Passthrough binary-like: tunneled SLIP/PPP-like session, custom peripheral byte stream, vendor protocol bridge.

### Candidate passthrough scenarios beyond ESP console

* Device shell or command interpreter that is not `esp_console`.
* Script/REPL runtimes such as MicroPython, Lua, JavaScript, or a device-specific debug shell.
* AT-command modem bridge where echo, prompts, partial responses, and control characters matter.
* Peripheral bridge to UART/I2C/SPI debug adapters where the mux channel represents an attached byte stream.
* Bootloader or ROM monitor interaction, where control characters and prompt timing matter.
* File-transfer or terminal protocols layered above a byte stream, such as XMODEM-like flows.
* Diagnostic escape hatch for application-specific raw control channels.

### Design implication

`esp_wiremux_bind_console()` can be one consumer of passthrough, but the core SDK should also allow applications to register passthrough channels independently through `esp_wiremux_register_channel()` plus `esp_wiremux_register_input_handler()`. Host behavior should depend on manifest interaction metadata, not on channel name `console` or channel id `1`.

### Open design boundary

The SDK console helper may offer convenience passthrough implementations, but passthrough itself should remain a generic channel capability in manifest and host UX. ESP-IDF console/REPL integration must be one selectable backend, not the definition of passthrough.

## User Preference Update

The preferred direction is a complete implementation, not only a generic primitive. The implementation should support manifest-driven host passthrough behavior and SDK-side passthrough registration/handling, while preserving line mode.

## Passthrough Taxonomy Exploration

The proto should not bind passthrough to console naming. The current proto already keeps this generic via `ChannelInteractionMode`, and SDK/host behavior should continue using channel manifest metadata rather than names such as `console`.

### Is a single PASSTHROUGH mode enough?

A single `PASSTHROUGH` mode can cover the common transport behavior for console, REPL, AT bridge, bootloader, and peripheral byte-stream scenarios if it means:

* host sends input bytes promptly instead of waiting for a complete line;
* host output rendering does not impose line-command assumptions;
* channel payloads remain framed as `WMUX` envelopes;
* application-specific protocol semantics live above the channel payload.

However, these scenarios differ in terminal policy rather than wire framing:

* local echo vs remote echo;
* CR, LF, and CRLF conversion;
* Backspace/Delete editing behavior;
* ANSI escape handling;
* binary-clean behavior vs text terminal behavior;
* whether Ctrl-C exits host mode or sends `0x03` to the device;
* whether prompt detection or command completion exists.

### Design recommendation

Keep proto `ChannelInteractionMode.PASSTHROUGH` as the generic, stable interaction mode for now. Do not introduce console-specific enum values. If finer control is needed, add it as generic channel capability metadata later, not as console-coupled mode names.

Possible future generic extensions:

* `terminal_profile`: raw, ansi-terminal, line-editor, at-command, binary-clean.
* `echo_policy`: local, remote, none.
* `newline_policy`: none, cr, lf, crlf, host-normalized.
* `control_key_policy`: host-handled, forwarded, configurable.
* `preferred_chunk_size` or input latency hints.

For `2604.27.3`, avoid expanding proto unless implementation proves a concrete required field. Use `PASSTHROUGH` as a broad mode and document that terminal policy defaults are host/tool behavior, not wire-protocol semantics.

## Newline Policy Note

CR/LF/CRLF handling should be modeled as a generic newline policy, comparable to Git's checkout/commit line-ending behavior:

* one side is a canonical representation boundary;
* checkout-like behavior transforms canonical data into the local/user-facing representation;
* commit-like behavior transforms local/user-facing input back into canonical data;
* binary-clean paths must opt out of newline transformation.

For Wiremux passthrough, the safe default remains no newline conversion because passthrough may carry binary-clean protocols. If conversion is needed for terminal-like channels, it should be declared as generic channel metadata rather than inferred from `PASSTHROUGH` itself.

Possible future policy shape:

* `newline_policy = none`: preserve bytes exactly; default for passthrough.
* `newline_policy = lf`: normalize submitted line endings to LF.
* `newline_policy = crlf`: normalize submitted line endings to CRLF.
* `newline_policy = cr`: normalize submitted line endings to CR.
* Split direction if needed: `host_input_newline_policy` and `host_output_newline_policy`.

Design implication for `2604.27.3`: implement passthrough in a way that does not hard-code CR/LF conversion into the protocol. Console helper code may choose a local default for Enter handling, but the protocol-level passthrough mode should stay byte-preserving until the manifest grows explicit newline metadata.

## Protocol API Compatibility Update

The core host consolidation and proto API snapshot work changes the risk profile for passthrough planning.

### New compatibility facts

* `sources/core/proto/api/current/wiremux.proto` is now the schema used by new device SDK builds.
* Frozen numbered snapshots under `sources/core/proto/api/<version>/` define the API versions host SDK builds support at compile time.
* Additive protobuf-compatible changes may update `current/`, but a release that ships a changed `current/` must freeze a new numbered snapshot.
* `WIREMUX_PROTOCOL_API_VERSION_CURRENT` must be bumped to the newest frozen snapshot, and host tools already have an unsupported-new diagnostic path for newer device APIs.
* Rust host paths now consume manifest/protocol data through the portable C host session API, so host-visible manifest additions must be exposed through C events and Rust FFI wrappers instead of direct Rust protobuf parsing.

### Impact on passthrough

This means proto changes are now viable for `2604.27.3` if they materially improve host behavior. The compatibility cost is bounded and testable:

* update `sources/core/proto/api/current/wiremux.proto`;
* add `sources/core/proto/api/2/wiremux.proto`;
* bump `WIREMUX_PROTOCOL_API_VERSION_CURRENT` from `1` to `2`;
* update portable C manifest encoder/host-session decoder events;
* update Rust FFI manifest structs;
* keep API v1 support and add snapshot/compatibility tests.

The remaining cost is implementation surface, not wire compatibility risk.

### Proto extension candidates

**Option A: No proto change, rely on existing `PASSTHROUGH`**

* How it works: host treats channels with `default_interaction_mode = PASSTHROUGH` as byte-stream interactive channels and applies fixed host defaults.
* Pros: fastest implementation; avoids C host-session event expansion.
* Cons: host still guesses newline, echo, and control-key policy; later behavior changes may become user-visible.

**Option B: Add a small generic passthrough/terminal policy to `ChannelDescriptor`** (Recommended if `2604.27.3` should optimize correctness over speed)

* How it works: add generic manifest metadata for passthrough terminal behavior, for example newline preservation/normalization, echo policy, and control-key forwarding. The field stays generic and channel-level, not console-specific.
* Pros: host no longer hard-codes terminal assumptions; console, REPL, AT bridge, and binary-clean channels can declare their desired behavior; still additive and compatible through API v2.
* Cons: requires portable C encoder/decoder events, Rust wrapper updates, and more tests.

**Option C: Add broader per-direction stream defaults**

* How it works: extend manifest metadata to distinguish input/output payload defaults and stream capabilities beyond terminal behavior.
* Pros: best long-term model for binary passthrough, file-transfer-like flows, and asymmetric channels.
* Cons: larger design surface; risks over-modeling before the first passthrough implementation proves the necessary contract.

### Updated recommendation

Keep `ChannelInteractionMode.PASSTHROUGH` as the generic interaction-mode switch. Do not add console-specific enum values.

For `2604.27.3`, consider a narrow API v2 only if the MVP includes host behavior that cannot be cleanly expressed by fixed defaults. The best candidate is a generic channel passthrough policy, not a console-only proto change. A minimal shape could be:

```proto
enum NewlinePolicy {
  NEWLINE_POLICY_UNSPECIFIED = 0;
  NEWLINE_POLICY_PRESERVE = 1;
  NEWLINE_POLICY_LF = 2;
  NEWLINE_POLICY_CR = 3;
  NEWLINE_POLICY_CRLF = 4;
}

enum EchoPolicy {
  ECHO_POLICY_UNSPECIFIED = 0;
  ECHO_POLICY_REMOTE = 1;
  ECHO_POLICY_LOCAL = 2;
  ECHO_POLICY_NONE = 3;
}

enum ControlKeyPolicy {
  CONTROL_KEY_POLICY_UNSPECIFIED = 0;
  CONTROL_KEY_POLICY_HOST_HANDLED = 1;
  CONTROL_KEY_POLICY_FORWARDED = 2;
}

message PassthroughPolicy {
  NewlinePolicy input_newline_policy = 1;
  NewlinePolicy output_newline_policy = 2;
  EchoPolicy echo_policy = 3;
  ControlKeyPolicy control_key_policy = 4;
}

message ChannelDescriptor {
  // existing fields 1..10
  PassthroughPolicy passthrough_policy = 11;
}
```

Default interpretation for old API v1 devices or omitted policy:

* passthrough input/output preserves bytes;
* echo is remote/device-owned;
* host reserves an escape sequence for exit and forwards ordinary control bytes when in passthrough mode.

This keeps old devices usable while letting API v2 devices be explicit.

## Decision Update: Proto API v2 Passthrough Policy

**Context**: The stable proto API snapshot architecture makes additive manifest changes safe and testable. Passthrough needs terminal policy metadata to avoid hard-coded host assumptions, but the design should remain a low-risk starting point for future broader stream metadata.

**Decision**: Use Option B for `2604.27.3`: add a small, generic `PassthroughPolicy` to `ChannelDescriptor` as proto API v2. Treat it as a subset of the future broader stream metadata model, not as a console-specific contract. The first version includes exactly four policy fields:

* `input_newline_policy`
* `output_newline_policy`
* `echo_policy`
* `control_key_policy`

**Consequences**:

* Future Option C-style stream metadata can evolve by appending fields to `PassthroughPolicy` or by adding adjacent generic channel metadata fields.
* The implementation must update `current/wiremux.proto`, freeze `api/2/wiremux.proto`, bump protocol API current version, and sync C/Rust manifest encode/decode/event surfaces.
* Old API v1 devices remain usable with default passthrough assumptions; newer API v2 devices can declare explicit policy.

## Decision Update: Host Interactive Surfaces

**Context**: Passthrough should be usable directly from the CLI and should also integrate with the existing manifest-driven TUI workflow.

**Decision**: `2604.27.3` includes both host interactive surfaces:

* a dedicated attach/passthrough command for focused terminal-style channel interaction;
* TUI manifest-driven passthrough so channels declaring `PASSTHROUGH` send key/input bytes immediately instead of using line-buffered `Enter` submission.

**Consequences**:

* The shared host passthrough input policy should be factored so the dedicated command and TUI do not duplicate newline/control-key behavior.
* TUI must preserve existing line-mode behavior for `LINE` or unspecified channels.
* Tests should cover parser/command behavior, manifest policy interpretation, and TUI input-mode state decisions where practical.

## Decision Update: SDK Passthrough Submodes

**Context**: Wiremux is a general-purpose mux framework. Passthrough channels may represent ESP-IDF console, another REPL, an AT bridge, a bootloader monitor, a peripheral byte stream, or an application-defined protocol. The SDK must not hard-bind passthrough semantics to ESP-IDF REPL behavior.

**Decision**: `2604.27.3` should support multiple SDK passthrough submodes selected during initialization/configuration. Both generic and ESP-IDF-oriented implementations are in scope:

* a raw passthrough input callback mode where the application owns byte-stream semantics;
* a lightweight SDK line-discipline mode for `esp_console_run()` compatibility;
* an ESP-IDF/VFS REPL-style mode where practical, as an adapter rather than the core contract.

**Consequences**:

* Public SDK config should separate the generic channel interaction mode (`LINE` vs `PASSTHROUGH`) from passthrough implementation/backend selection.
* Manifest still advertises generic channel behavior through `ChannelInteractionMode.PASSTHROUGH` plus `PassthroughPolicy`; it should not expose ESP-IDF-specific backend names.
* ESP-IDF-specific code should stay in the ESP console helper layer, while generic raw passthrough remains available through `esp_wiremux_register_channel()` and `esp_wiremux_register_input_handler()`.
* Validation must reject incompatible configs deterministically, for example a console helper passthrough backend with missing required callbacks/resources.
* Core/internal constants must stay platform-neutral. For example, core may define `WIREMUX_PASSTHROUGH_BACKEND_REPL`, while the ESP component may expose `ESP_WIREMUX_PASSTHROUGH_BACKEND_ESP_REPL` as an ESP-facing alias.
* In the ESP implementation, backend selection can live in `esp_wiremux_console_config_t` because line discipline and ESP-IDF REPL are ESP console adapter behaviors, not generic channel registration requirements.

### Candidate SDK config shape

```c
typedef enum {
    ESP_WIREMUX_PASSTHROUGH_BACKEND_RAW_CALLBACK = 1,
    ESP_WIREMUX_PASSTHROUGH_BACKEND_CONSOLE_LINE_DISCIPLINE = 2,
    ESP_WIREMUX_PASSTHROUGH_BACKEND_ESP_REPL = 3,
} esp_wiremux_passthrough_backend_t;

typedef struct {
    esp_wiremux_passthrough_backend_t backend;
    esp_wiremux_passthrough_policy_t policy;
    esp_wiremux_input_handler_t raw_handler;
    void *raw_user_ctx;
} esp_wiremux_passthrough_config_t;
```

Exact naming may change during implementation, but the boundary should remain: generic passthrough mode is channel metadata; backend/submode is SDK runtime configuration.

Core-level naming should avoid ESP-specific terms:

```c
typedef enum {
    WIREMUX_PASSTHROUGH_BACKEND_RAW_CALLBACK = 1,
    WIREMUX_PASSTHROUGH_BACKEND_LINE_DISCIPLINE = 2,
    WIREMUX_PASSTHROUGH_BACKEND_REPL = 3,
} wiremux_passthrough_backend_t;
```

ESP-facing names can alias those values:

```c
typedef enum {
    ESP_WIREMUX_PASSTHROUGH_BACKEND_RAW_CALLBACK = WIREMUX_PASSTHROUGH_BACKEND_RAW_CALLBACK,
    ESP_WIREMUX_PASSTHROUGH_BACKEND_CONSOLE_LINE_DISCIPLINE = WIREMUX_PASSTHROUGH_BACKEND_LINE_DISCIPLINE,
    ESP_WIREMUX_PASSTHROUGH_BACKEND_ESP_REPL = WIREMUX_PASSTHROUGH_BACKEND_REPL,
} esp_wiremux_passthrough_backend_t;
```

## Implementation Verification

* `sources/core/proto/wiremux.proto`, `api/current/wiremux.proto`, and `api/2/wiremux.proto` match; `api/1/wiremux.proto` remains frozen.
* Host command parsing, passthrough key mapping, and TUI manifest-driven passthrough behavior are covered by Rust unit tests.
* Host TUI tests cover split remote backspace echo handling, passthrough stream continuation across interleaved channel records, active-channel live-tail following, and `Ctrl-]` exit handling.
* Portable C manifest encoding, passthrough policy encoding, host-session manifest decode, and protocol API v2 snapshot checks are covered by GoogleTest.
* ESP demo exposes `mux_console_mode line|passthrough` so passthrough can be tested without rebuilding or rebooting the device; switching re-emits manifest for host/TUI mode discovery.
* ESP-IDF demo build could not be run in this environment because `idf.py` is not installed.
