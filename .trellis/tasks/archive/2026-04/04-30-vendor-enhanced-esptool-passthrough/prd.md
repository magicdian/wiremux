# brainstorm: vendor enhanced esptool passthrough

## Goal

Design and implement the first practical vendor enhanced feature for ESP32:
when an ESP32 device connects with vendor enhanced enabled, the host exposes a
dedicated enhanced tty such as `/dev/tty.wiremux-esp-enhanced`. Normal terminal
tools can open it to observe channel data, while `idf.py flash` / esptool usage
can be detected by the host and routed through an ESP32 passthrough flashing
flow. This task focuses on passthrough serial flashing only; OTA is future work.

## What I already know

* The feature is ESP32-specific vendor enhanced behavior.
* When enabled, the host should still export configured device channels.
* The host should additionally export a special enhanced tty for ESP-specific
  diagnostics and flashing.
* Opening the enhanced tty with `screen` or `minicom` should show all channel
  data.
* Opening it with `idf.py flash` should let the enhanced tty take input
  ownership and run a flashing mode.
* Two future flashing modes are envisioned:
  * OTA channel: host sends firmware to ESP and ESP calls OTA APIs.
  * esptool passthrough channel: host forwards the flashing command stream while
    ESP enters bootloader mode.
* MVP scope for this round is esptool passthrough only.
* The design likely needs to separate generic protocol enhanced capabilities
  from host-only enhanced behavior.
* `docs/product-architecture.md` already defines the intended boundary:
  generic enhanced owns virtual TTY/broker/input ownership, while vendor
  enhanced owns ESP32 OTA/esptool compatibility bridges and device-aware update
  policy.
* `docs/product-architecture.md` names future profiles such as
  `esp32.ota.v1` and `esp32.esptool.v1`, but the current `DeviceManifest`
  proto does not yet expose profile declarations.
* The current generic enhanced API catalog has one frozen capability:
  `wiremux.generic.enhanced.virtual_serial`.
* The current virtual serial broker exports one PTY per manifest channel, writes
  channel output to the corresponding PTY, and forwards PTY input only when the
  virtual endpoint owns input.
* Unix PTY aliasing already supports stable names under `/dev/tty.wiremux-*`;
  Windows virtual COM support is still planned.
* The current CLI has an explicit `wiremux passthrough --channel <id>` mode, but
  that routes terminal keystrokes to a mux channel; it is not an esptool-facing
  raw serial bridge.
* The ESP-IDF component emits manifests from registered channels and does not
  currently expose profile/capability metadata or an ESP bootloader control
  channel.
* Proposed vendor enhanced host namespace shape:
  `wiremux.vendor_enhanced_host.espressif.*` (canonical vendor spelling in this
  repository is `espressif`).
* The ESP enhanced PTY should be created only after the current connection is
  identified as an Espressif/ESP device. It should not appear for generic or
  non-Espressif devices.
* Normal virtual serial input ownership remains unchanged: input channels are
  owned by TUI by default unless TUI manually transfers ownership to virtual
  serial. The ESP enhanced esptool flow is a vendor enhanced exception, not a
  change to generic virtual serial ownership.
* The ESP enhanced PTY can use a pre-claim input buffer: bytes read before the
  host decides whether the session is esptool flash are stored in order, then
  replayed unchanged into raw bridge if the flash trigger is accepted.

## Assumptions (temporary)

* MVP bootloader entry should first use the physical serial DTR/RTS reset/boot
  sequence. Add an ESP device control command only if the DTR/RTS path cannot be
  made reliable.
* OTA protocol and device OTA implementation are explicitly out of scope.
* The enhanced tty path naming and behavior should fit existing host-side
  channel export conventions.
* Existing recent work around generic enhanced host registry should be reused.

## Open Questions

* None for MVP requirements. Implementation may refine exact timing constants
  if tests/hardware runs show they need adjustment.

## Requirements (evolving)

* Add ESP32 vendor enhanced design that reserves future OTA without implementing
  it in this task.
* Implement or prepare host-side esptool passthrough behavior as the first
  practical enhanced feature.
* Treat esptool traffic recognition/parsing as ESP vendor enhanced host logic,
  not as core Wiremux protocol behavior and not as a generic enhanced API.
* MVP user flow target: `idf.py flash --port <TUI-shown-esp-enhanced-path>
  --baud 115200` can complete flashing through the Wiremux enhanced host
  session. macOS PTY aliases cannot accept pyserial's high-baud custom ioctl
  for ESP-IDF's default `460800` path; keeping the tty-shaped `--port` UX with
  default high baud is future native virtual serial/DriverKit work.
* First implementation only supports ESP enhanced PTY creation and flashing
  bridge while `wiremux tui` is running.
* Create `/dev/tty.wiremux-esp-enhanced` only for a connected Espressif/ESP
  device, based on available manifest/vendor identity heuristics until explicit
  manifest profiles exist.
* Preserve regular channel export behavior when vendor enhanced is enabled.
* Keep device-side changes minimal for passthrough flashing.
* Use DTR/RTS as the primary MVP bootloader entry mechanism; keep ESP-side
  control command as fallback work only if DTR/RTS is not viable.
* Do not automatically transfer normal input-channel ownership to virtual
  serial. Only the ESP enhanced flashing endpoint may auto-claim, and only for
  a complete esptool SYNC frame with expected sync payload, followed by flash
  intent observation where available.
* Buffer ESP enhanced PTY input while classifying a potential esptool session,
  then replay buffered bytes unchanged once raw bridge mode starts.
* MVP pending-input policy:
  * Buffer limit: 64 KiB.
  * Classification timeout: 1 second from first pending byte.
  * On accepted flash session: replay buffered bytes unchanged after DTR/RTS
    bootloader entry and before live PTY-to-serial forwarding.
  * On timeout, overflow, or non-flash classification: discard buffered bytes
    and write diagnostics; do not forward them to normal mux input.
* Do not implement local mock ESP ROM/stub responses in MVP.

## Acceptance Criteria (evolving)

* [x] The design distinguishes protocol-level enhanced capabilities from
  host-only enhanced integrations.
* [x] Host-side code can expose or route an ESP enhanced tty without disrupting
  normal configured channels.
* [x] The MVP flashing path supports esptool/idf.py passthrough semantics.
* [x] Running `idf.py flash --port <TUI-shown-esp-enhanced-path> --baud 115200`
  can flash an ESP32-S3 USB-Serial/JTAG target while `wiremux tui` is connected
  to the physical serial port, and TUI resumes mux output after reset.
* [x] `wiremux listen`, `wiremux send`, and `wiremux passthrough --channel` are
  not required to create or manage the ESP enhanced PTY in the first
  implementation.
* [x] The ESP enhanced PTY is absent for non-Espressif manifests/connections.
* [x] Generic channel virtual serial input still requires the existing TUI owner
  toggle; esptool flashing does not weaken the generic ownership gate.
* [x] Pre-trigger bytes from `tty.wiremux-esp-enhanced` are not dropped when
  esptool flash is accepted; they are replayed into the raw bridge in original
  order.
* [x] Pending enhanced PTY input is bounded to 64 KiB and times out after 1
  second when no flash session is accepted.
* [x] OTA is represented only as future-compatible design, not implemented.
* [x] Tests or executable checks cover the new host-side behavior where feasible.

## Validation Notes

* Hardware acceptance passed on ESP32-S3 USB-Serial/JTAG with:
  `idf.py flash --port /tmp/wiremux/tty/tty.wiremux-esp-enhanced --baud 115200`.
* TUI continued decoding mux output after firmware reset.
* Default ESP-IDF `-b 460800` currently fails on macOS PTY because pyserial uses
  the `IOSSIOSPEED` ioctl on the PTY slave; the host raw bridge cannot intercept
  or mock that ioctl from the PTY master. Native virtual serial/DriverKit support
  is the roadmap item for preserving the tty-shaped command and supporting
  default high-baud esptool behavior.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / build checks pass for touched packages.
* Docs/notes updated if behavior changes protocol or user-facing CLI behavior.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* ESP OTA firmware transfer and ESP OTA API integration.
* Full generic OTA channel protocol.
* Mocking ESP ROM/stub responses or implementing an esptool protocol emulator.
* Replacing normal configured channel export behavior.
* Creating the ESP enhanced PTY from `wiremux listen`, `wiremux send`, or a new
  standalone enhanced command in the first implementation.
* Supporting non-ESP vendors unless the generic abstraction naturally allows it.

## Technical Notes

* Relevant recent commits mention generic enhanced host registry and enhanced
  overlay API stability.
* Relevant files inspected:
  * `docs/product-architecture.md`: generic/vendor enhanced boundaries, profile
    discovery direction, firmware update boundary.
  * `docs/matrix/feature-support.md`: marks ESP32 OTA and esptool passthrough
    as planned vendor enhanced features.
  * `sources/api/proto/versions/current/wiremux.proto`: current manifest schema
    lacks profile declarations.
  * `sources/api/host/generic_enhanced/versions/current/generic_enhanced.proto`:
    host-side generic enhanced catalog schema.
  * `sources/host/wiremux/crates/generic-enhanced/src/lib.rs`: registry and
    virtual serial capability lookup.
  * `sources/host/wiremux/crates/interactive/src/lib.rs`: virtual serial broker,
    PTY backend, input ownership gate.
  * `sources/host/wiremux/crates/tui/src/lib.rs`: manifest sync creates vtty
    endpoints and polls virtual serial input.
  * `sources/host/wiremux/crates/cli/src/main.rs`: existing interactive
    passthrough command is channel-oriented, not raw esptool passthrough.
  * `sources/vendor/espressif/generic/components/esp-wiremux/src/esp_wiremux.c`:
    ESP manifest emission from registered channels.
  * `sources/host/wiremux/crates/interactive/src/lib.rs`: current
    `ConnectedInteractiveBackend` exposes serial reads/writes but no DTR/RTS
    control-line API. `CompatBackend` wraps `Box<dyn serialport::SerialPort>`;
    Unix mio backend wraps `serialport::TTYPort`.
* External protocol reference:
  * Espressif esptool serial protocol docs:
    `https://docs.espressif.com/projects/esptool/en/release-v4/esp32/advanced-topics/serial-protocol.html`

## Research Notes

### What similar tooling implies

* `idf.py flash` delegates to esptool and expects a normal serial port: it opens
  the port, may manipulate serial control lines for reset/boot mode, sends the
  ESP ROM bootloader sync protocol, then streams flashing commands.
* Espressif's esptool serial protocol documents `SYNC` (`0x08`) as the initial
  command after reset into UART bootloader mode. Flashing then uses
  `FLASH_BEGIN` (`0x02`), `FLASH_DATA` (`0x03`), and `FLASH_END` (`0x04`), with
  compressed flashing using related deflated-flash commands.
* Because flash commands come after successful bootloader sync, a serial-only
  bridge cannot wait for `FLASH_BEGIN` before performing DTR/RTS and starting
  raw bridging. The earliest reliable byte-stream trigger is esptool
  session/sync detection; flash intent can be confirmed later by passively
  observing flash commands in the bridged stream.
* A "mock responses until FLASH_BEGIN" design was considered. It would require
  the host to impersonate enough ESP ROM/stub-loader behavior for esptool to
  pass sync, chip detection, optional stub upload, and baud changes before
  flashing. If the host later replays those already-acknowledged bytes to the
  real ESP, the real ESP responses would be out of phase with esptool. This is
  closer to an esptool protocol proxy/emulator than a passthrough bridge and is
  too risky for MVP.
* A virtual PTY compatibility bridge must therefore preserve byte-stream
  semantics and, for a complete solution, account for reset/bootloader control
  line behavior as well as bytes.
* Once the ESP is in ROM bootloader mode on the same physical serial transport,
  the device application cannot speak Wiremux. The host must suspend mux
  decoding and bridge raw bytes between the esptool-facing PTY and the physical
  serial connection until flashing completes or the session is aborted.

### Constraints from this repo

* Generic enhanced virtual serial is a host-side API; it is not a core wire
  protocol feature.
* Vendor enhanced is already modeled as generic enhanced plus one concrete
  vendor adapter feature. The `cli` crate currently has an `esp32` feature that
  enables `generic-enhanced`, but there is no separate ESP32 adapter module yet.
* esptool compatibility is a host-facing tool bridge. The ESP application should
  not need to understand host-side esptool parsing; at most it may expose a
  vendor control/profile path that lets the host request bootloader entry before
  raw passthrough starts.
* Current manifest lacks profile declarations, so an implementation can either
  use temporary ESP heuristics (`sdk_name == "esp-idf"` / device name / selected
  vendor build) or extend the core protocol manifest.
* Existing virtual serial endpoints are channel-scoped; the requested enhanced
  tty is aggregate/session-scoped and needs a separate endpoint class.

### Feasible approaches here

**Approach A: Host-only ESP enhanced aggregate PTY first** (Recommended MVP)

* How it works:
  * Add an ESP vendor enhanced host module behind the existing `esp32` feature.
  * Reuse the Unix PTY backend and stable aliasing to expose a session-scoped
    endpoint such as `/dev/tty.wiremux-esp-enhanced`.
  * Mirror all mux channel output to that aggregate endpoint for terminal tools.
  * Detect esptool-like traffic on PTY input, claim input ownership, ask the ESP
    app to enter bootloader mode if a temporary control path exists, then switch
    host to raw bridge mode between PTY and physical serial.
  * Use manifest/device heuristics or selected vendor build to enable it until
    manifest profiles exist.
* Pros:
  * Smallest protocol blast radius.
  * Fits this round's "passthrough line flashing only" scope.
  * Keeps OTA and profile schema decisions reversible.
* Cons:
  * Discovery is weaker until manifest profiles exist.
  * Device-side bootloader entry still needs a concrete mechanism.
  * PTY modem-control behavior may need a follow-up for full esptool parity.

**Approach B: Add manifest profile declarations first**

* How it works:
  * Extend `DeviceManifest` with repeated profile identifiers, e.g.
    `esp32.esptool.v1`.
  * Update C core, Rust host session, ESP manifest encoder, and docs.
  * Vendor enhanced host activates the aggregate PTY only when the manifest
    declares the ESP esptool profile.
* Pros:
  * Clean capability discovery and future OTA path.
  * Aligns directly with product architecture.
* Cons:
  * Cross-layer protocol change before behavior lands.
  * More code-spec depth and compatibility work.
  * Slower path to a working `idf.py flash` MVP.

**Approach C: Treat esptool bridge as a generic enhanced API**

* How it works:
  * Add a new `wiremux.generic.enhanced.raw_bridge` or similar host API and let
    ESP vendor overlay bind it.
* Pros:
  * Could support other vendor flashing bridges later.
* Cons:
  * Premature generalization: reset/boot/signing policies are vendor-specific.
  * Risks polluting the generic enhanced catalog with a not-yet-proven API.

**Approach D: Mock esptool responses before raw bridge**

* How it works:
  * Buffer enhanced PTY input and respond to esptool locally until the host sees
    flash intent, then DTR/RTS and switch to raw bridge.
* Pros:
  * Could theoretically distinguish flash from monitor/non-flash before touching
    the physical ESP.
* Cons:
  * Requires emulating ESP ROM/stub protocol state, not just classifying bytes.
  * Esptool often performs sync, chip detection, optional stub upload, baud
    changes, and SPI setup before flash commands.
  * Replaying already-mocked bytes to the real ESP after switching raw would
    desynchronize responses.
  * Not suitable for MVP passthrough.

### Expansion sweep

* Future evolution:
  * OTA should eventually become a profile-driven device capability using
    generic transfer plus `esp32.ota.v1`.
  * The aggregate PTY should become one vendor endpoint among a family of
    session-scoped enhanced endpoints, not a special case inside per-channel
    virtual serial.
* Related scenarios:
  * The existing `wiremux passthrough --channel` command should remain a mux
    channel terminal mode and not be confused with esptool raw bridge mode.
  * TUI virtual serial toggles should remain generic; ESP enhanced can depend on
    generic PTY plumbing without changing generic channel endpoint behavior.
* Failure and edge cases:
  * If esptool opens the PTY while another terminal is attached, input ownership
    needs deterministic conflict handling.
  * If bootloader entry fails or esptool disconnects mid-flash, the host should
    exit raw bridge mode and restore normal mux listening when possible.
  * If the device resets into ROM bootloader, manifest and mux state are no
    longer valid until the app reboots after flashing.

### Minimal implementation for `idf.py flash`

The smallest practical implementation is a host-side bridge with four pieces:

1. **ESP enhanced PTY endpoint**
   * Add a session-scoped PTY endpoint behind the ESP vendor enhanced host
     feature.
   * Stable alias: `/dev/tty.wiremux-esp-enhanced` by default.
   * It is not tied to one mux channel; it belongs to the connected ESP session.

2. **Aggregate monitor mode before flashing**
   * While in normal mux mode, mirror decoded channel output from all channels to
     the enhanced PTY so `screen` / `minicom` can observe combined traffic.
   * Input from the enhanced PTY is ignored or handled only as a candidate
     flashing trigger until the bridge claims flashing ownership.

3. **Flashing trigger and bootloader entry**
   * Use a conservative two-stage trigger:
     * Stage 1 identifies a complete esptool SYNC frame with command `SYNC`
       (`0x08`) and expected sync payload prefix (`0x07 0x07 0x12 0x20`). Only
       then may the host claim the ESP enhanced PTY, perform DTR/RTS bootloader
       entry, and start raw bridging.
     * Stage 2 recognizes flash intent from esptool flashing commands such as
       flash-begin/data/end semantics and records that the automatic claim is a
       flash session, not generic terminal input.
   * Stage 1 classification should decode enough SLIP framing to avoid
     triggering on arbitrary terminal text, `idf.py monitor` traffic, or random
     binary input.
   * Stage 2 classification should passively recognize flash command IDs in the
     already-bridged stream, including at minimum `FLASH_BEGIN` (`0x02`),
     `FLASH_DATA` (`0x03`), and `FLASH_END` (`0x04`). Deflated flash commands
     can be included if they appear in the target esptool flow.
   * Maintain a bounded pending-input buffer for ESP enhanced PTY bytes while
     classification is undecided. If the session is accepted as esptool flash,
     replay the buffered bytes to the physical serial after DTR/RTS bootloader
     entry and before forwarding newly read PTY bytes.
   * If classification fails or times out, discard or keep the buffered bytes
     according to the endpoint's non-flash policy; do not forward them to normal
     mux channels implicitly.
   * MVP non-flash policy is discard-with-diagnostics. The aggregate endpoint is
     an output monitor unless the ESP flash bridge claims it.
   * Do not auto-claim on arbitrary PTY input. Ordinary `screen` / `minicom`
     typing should not take ownership from TUI.
   * On Stage 1 trigger, pause normal mux decoding, invalidate current manifest
     state, and enter `EspFlashBridge` mode.
   * Get the ESP into ROM bootloader using the physical serial DTR/RTS reset
     sequence through the host's physical serial handle. If that is not reliable
     with the existing backend after implementation/testing, add a tiny ESP
     control path in a follow-up that requests bootloader entry before raw
     bridge starts. Manual boot mode can be documented only as a fallback, not
     as the main acceptance path.

4. **Raw byte bridge until completion/disconnect**
   * Forward bytes from enhanced PTY master to the physical serial port
     unchanged.
   * Forward bytes from physical serial port to enhanced PTY master unchanged.
   * Do not parse full esptool commands in MVP; only classify the session and
     switch modes.
   * When the PTY client disconnects, the physical serial disconnects, or an
     idle/EOF condition is observed after flashing, leave raw bridge mode and
     resume normal Wiremux connect/manifest request flow.

Minimal code shape:

* `crates/interactive`: expose/reuse PTY endpoint IO in a way a session-scoped
  vendor endpoint can use, instead of keeping all PTY logic private to
  per-channel virtual serial.
* `crates/interactive`: extend `ConnectedInteractiveBackend` with an explicit
  serial control-line operation for ESP bootloader reset sequencing. The
  operation should be implemented by both compat and Unix mio backends where
  supported, and return a clear unsupported/error result otherwise.
* `crates/cli` or a new ESP adapter module/crate: register an ESP vendor
  enhanced host service behind the `esp32` feature.
* `crates/tui`: first runtime integration point. TUI owns the physical serial
  backend, manifest state, normal channel virtual serial broker, ESP enhanced
  aggregate endpoint, DTR/RTS bootloader sequence, and raw bridge transition.
* `wiremux.vendor_enhanced_host.espressif.*`: owns Espressif device matching,
  esptool session/flash-intent classification, pending PTY input buffering, and
  ESP flash bridge state.
* Docs/tests: add unit tests for endpoint naming, trigger classification, and
  bridge state transitions; hardware flashing remains a manual acceptance test.

## Decision (ADR-lite)

**Context**: The MVP must let `idf.py flash --port
/dev/tty.wiremux-esp-enhanced` complete flashing, and the ESP application should
not need to understand host-side esptool parsing.

**Decision**: Put esptool recognition and raw bridge behavior in
`wiremux.vendor_enhanced_host.espressif.*`. Use physical serial DTR/RTS as the
first bootloader entry mechanism. Do not add an ESP device control command
unless the DTR/RTS path cannot be made reliable. Limit the first runtime
integration to `wiremux tui`; other host commands will not create the ESP
enhanced PTY in this MVP. Create the ESP enhanced PTY only after matching the
connected device as Espressif/ESP. Preserve generic virtual serial input
ownership semantics; esptool flashing is the only automatic ownership exception.

**Consequences**: The MVP minimizes ESP-side changes and keeps esptool
compatibility host-only. The raw bridge must use a physical serial backend that
can manipulate control lines; if the current backend abstraction cannot expose
that cleanly, the first implementation must extend the backend API before adding
device-side fallback behavior. Restricting the first entry point to TUI reduces
duplication because TUI already owns long-running serial reconnect, manifest
sync, virtual serial endpoint lifecycle, and periodic polling. Triggering only
on a fully decoded flash command would be too late to enter bootloader because
esptool must first reset/sync with the ROM loader; therefore the implementation
needs early esptool-session detection plus flash-intent confirmation rather than
arbitrary-input triggering. A bounded pre-claim input buffer avoids dropping the
early bytes that were needed for classification: accepted flash sessions replay
the buffered bytes unchanged into raw bridge after bootloader entry.

**Pending-input policy**: Use a 64 KiB pending buffer and 1 second
classification timeout for MVP. Accepted flash sessions replay buffered bytes
unchanged; timeout, overflow, or non-flash classification discards the bytes with
diagnostics and preserves normal TUI input ownership.

## Technical Approach

Build the MVP as a TUI-only ESP vendor enhanced host service under
`wiremux.vendor_enhanced_host.espressif.*`.

The TUI remains the owner of the physical serial connection and normal Wiremux
channel input. After manifest sync, it matches Espressif devices using existing
manifest identity fields until explicit manifest profiles exist. Only matched
ESP sessions create `/dev/tty.wiremux-esp-enhanced`.

The ESP enhanced endpoint has two modes:

* Aggregate monitor mode mirrors all decoded channel output to the endpoint and
  buffers any input for classification.
* Flash bridge mode uses DTR/RTS to enter ROM bootloader, replays accepted
  pending input, and bridges bytes unchanged between the enhanced PTY and the
  physical serial backend.

Generic per-channel virtual serial ownership is unchanged. The ESP enhanced PTY
is a separate vendor endpoint and the only endpoint that may auto-claim on
esptool session detection.

Do not implement mock ESP ROM/stub responses for MVP. If the project later
needs strict "flash-only before raw bridge" detection, model it as a separate
esptool proxy/emulator feature, not as the minimal passthrough bridge.

## Implementation Plan

* PR1: Extract reusable PTY endpoint plumbing from the generic virtual serial
  broker so a session-scoped vendor endpoint can create stable Unix aliases.
* PR2: Add serial control-line support to `ConnectedInteractiveBackend` for the
  ESP DTR/RTS bootloader sequence, implemented for compat and Unix mio backends
  where the underlying serialport API supports it.
* PR3: Add `wiremux.vendor_enhanced_host.espressif.*` host module with device
  matching, aggregate endpoint naming, pending input buffer, SLIP/SYNC
  classifier, and bridge state machine tests.
* PR4: Integrate the ESP enhanced host service into `wiremux tui` behind the
  `esp32` / vendor enhanced feature path, including diagnostics and manual
  hardware acceptance instructions for `idf.py flash --port
  /dev/tty.wiremux-esp-enhanced`.

## Implementation Notes

Implemented in this task:

* `interactive` exposes reusable `VirtualSerialEndpointHandle` for session
  scoped PTY endpoints and adds DTR/RTS operations on
  `ConnectedInteractiveBackend`.
* `cli` feature `esp32` now enables `tui/esp32`.
* `tui/esp32` adds `esp_enhanced` runtime support:
  * creates `tty.wiremux-esp-enhanced` only for matched Espressif manifests;
  * mirrors decoded mux channel output to the ESP enhanced endpoint in aggregate
    monitor mode;
  * buffers PTY input up to 64 KiB with a 1 second classification timeout;
  * accepts only a complete esptool SLIP `SYNC` frame with the expected sync
    payload prefix before auto-claiming the endpoint;
  * uses DTR/RTS bootloader entry, replays buffered bytes, then bridges raw
    bytes between the enhanced PTY and physical serial backend;
  * preserves generic virtual serial input ownership semantics.
* README, README_CN, and feature matrix document the TUI-only MVP and hardware
  verification requirement.

Verification completed:

* `cargo fmt --check` in `sources/host/wiremux`.
* `cargo check` in `sources/host/wiremux`.
* `cargo test` in `sources/host/wiremux`.
* `cargo check --features generic`.
* `cargo check --features esp32`.
* `cargo check --features all-vendors`.
* `cargo check --features all-features`.
* `cargo test --features esp32`.
* `tools/wiremux-build check host`.

Manual hardware flashing with `idf.py flash --port
/dev/tty.wiremux-esp-enhanced` is intentionally left for user acceptance.
