# Wiremux Product Architecture

Wiremux is organized as a stable protocol core with optional host and device
enhancements layered around it. The core should stay platform-neutral and avoid
embedding vendor-specific semantics such as ESP32 OTA, Raspberry Pi UF2, or
bootloader flashing flows.

The product model is:

- `wiremux-core` provides framing, multiplexing, channel metadata, manifest
  exchange, and future generic control or reliable transfer primitives.
- `wiremux-host generic enhanced` provides vendor-neutral Rust tooling such as
  TUI integration, broker services, virtual TTY/port bridges, transfer
  orchestration, diagnostics, capture/replay, and other overlays that can apply
  to any device family.
- `wiremux-host vendor enhanced` composes generic enhanced behavior with
  device-aware adapters such as ESP32 OTA/esptool or Raspberry Pi UF2/control.
- Device SDK adapters implement platform-specific profiles on top of the core.
- Profiles are the HAL-like boundary between host enhanced tooling and device
  implementations.

The target source layout and build orchestration contract are defined in
[`docs/source-layout-build.md`](source-layout-build.md). Follow-up migration PRs
move runtime code toward `sources/api`, `sources/core`, `sources/profiles`,
`sources/host/wiremux`, and `sources/vendor/espressif`; this architecture
document describes the product boundary independent of the temporary
pre-migration paths.

## Layered Architecture

```text
┌────────────────────────────────────────────────────────────────────┐
│                         USER FACING TOOLS                          │
│                                                                    │
│   wiremux tui      wiremux listen      wiremux send      broker     │
│   virtual tty      tcp bridge          capture/replay     ...       │
└────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│                    WIREMUX HOST GENERIC ENHANCED                   │
│                                                                    │
│   connection manager     manifest resolver     profile registry     │
│   transfer manager       virtual port manager  diagnostics          │
│   virtual serial broker  capture/replay       tcp bridge            │
└────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│                    WIREMUX HOST VENDOR ENHANCED                    │
│                                                                    │
│   Loaded according to selected build features and active manifest.  │
│                                                                    │
│   ┌────────────────┐   ┌────────────────┐   ┌────────────────┐     │
│   │ ESP32 Adapter  │   │ RPi Adapter    │   │ Vendor Adapter  │     │
│   │ OTA / esptool  │   │ UF2 / control  │   │ device control  │     │
│   └────────────────┘   └────────────────┘   └────────────────┘     │
└────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│                         WIREMUX CORE                               │
│                                                                    │
│   framing + crc     mux envelope     channel model     manifest     │
│   control plane     reliable transfer     payload_type routing      │
│                                                                    │
│   Core only knows generic protocol concepts.                        │
│   It does not know ESP32 OTA, Raspberry Pi UF2, or vendor flashing. │
└────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│                    PROFILE CONTRACT / HAL-LIKE ABI                  │
│                                                                    │
│   wiremux.transfer.v1      wiremux.pty.v1      wiremux.console.v1   │
│   esp32.ota.v1             esp32.esptool.v1    rpi.uf2.v1           │
│                                                                    │
│   Declared by DeviceManifest + profiles + capabilities.             │
└────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│                         TRANSPORT BACKENDS                         │
│                                                                    │
│            serial              TCP/IP              BLE              │
└────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│                         DEVICE SIDE SDK                            │
│                                                                    │
│   wiremux device core       manifest provider       channel registry │
│                                                                    │
│   ┌────────────────┐   ┌────────────────┐   ┌────────────────┐     │
│   │ ESP32 SDK      │   │ RPi SDK        │   │ Custom SDK      │     │
│   │ OTA / logs     │   │ UF2 / control  │   │ vendor profile  │     │
│   └────────────────┘   └────────────────┘   └────────────────┘     │
└────────────────────────────────────────────────────────────────────┘
```

## Treble-Inspired Mapping

Wiremux can borrow Android Treble's separation of stable framework interfaces
from vendor implementations without copying Android's exact runtime structure.

| Android Treble | Wiremux |
| --- | --- |
| System Apps | `wiremux tui`, `wiremux listen`, `wiremux send`, broker, virtual TTY tools |
| Android Framework | `wiremux-host enhanced` services and orchestration |
| Native Libraries / Runtime | `wiremux-core` implementation |
| HAL Interface | Wiremux profile contract |
| Vendor Implementation | Device SDK adapter and host profile adapter |
| VINTF Manifest | `DeviceManifest` plus profiles and capabilities |
| Linux Kernel / Drivers | Serial, TCP/IP, BLE, and other ordered byte transports |
| CTS / VTS | Core conformance and profile conformance tests |

The important boundary is the profile contract. The core protocol exposes
generic mechanisms, while device profiles define the product semantics.

## Core Responsibilities

`wiremux-core` owns protocol concepts that should remain useful across device
families and transport backends:

- frame encoding, decoding, version checks, and CRC validation
- `MuxEnvelope` routing by `channel_id`, direction, payload kind, and
  `payload_type`
- channel descriptors and device manifests
- system channel conventions
- payload batching and compression when supported
- future generic control request/response/event primitives
- future generic reliable object or byte-stream transfer primitives

The core should not define:

- ESP32 OTA partition semantics
- esptool bootloader command emulation
- Raspberry Pi UF2 policy
- vendor-specific reset, boot mode, signing, rollback, or reboot behavior
- host-only virtual TTY or pseudo-terminal implementation details

## Host Enhanced Responsibilities

Host enhanced is the official Rust tool layer for higher-level product
features. It may be larger and more opinionated than the core because it can
bundle common adapters and user workflows, but it is split into generic and
vendor overlays.

Generic enhanced responsibilities include:

- serial, TCP, BLE, and future connection management
- manifest resolution and profile discovery
- profile registry and adapter dispatch
- transfer progress, retry, cancel, and diagnostics UX
- virtual TTY, TCP bridge, broker, and capture/replay features
- generic virtual serial endpoints for every manifest channel
- input ownership and policy hooks for host vs virtual endpoint writes

Vendor enhanced responsibilities include:

- device-aware adapters such as ESP32 OTA or Raspberry Pi control workflows
- compatibility bridges such as an ESP-IDF/esptool-facing virtual port
- vendor policy for claiming a generic enhanced service, such as an ESP32
  flashing PTY taking input ownership while a flashing tool is attached

Users who only need the protocol can depend on `wiremux-core` and build their
own host or device integration without carrying the enhanced host feature set.

## Host Overlay Loading

Build modes control which host overlays are compiled:

- `generic`: core host protocol behavior only.
- `generic-enhanced`: generic host overlays such as virtual serial and broker
  services.
- `vendor-enhanced`: generic enhanced plus the selected vendor adapter feature.
- `all-features`: generic enhanced plus all compiled vendor adapter features.

Compiled code does not imply every service is active at runtime. A running host
instantiates generic enhanced services first, then activates only the vendor
adapter that matches the connected device manifest/profile. This keeps memory
bounded by active services and the connected device family instead of loading
every compiled vendor adapter.

The generic virtual serial broker is part of the generic enhanced overlay. A
generic host build cannot enable it, even if `[virtual_serial]` appears in the
host config. Generic enhanced, vendor enhanced, and all-feature builds enable it
by default when config omits the section, and then the config may explicitly
disable or enable it. When enabled, it exports all manifest channels by default.
Output-only channels are read-only virtual endpoints. Input-capable channels
accept writes only when the input-ownership gate grants ownership to the virtual
endpoint. For non-passthrough text channels, the broker maps mux record
boundaries to terminal line endings so tools such as `minicom` and `screen`
render each record as a separate line; passthrough channels preserve byte-stream
semantics. On Unix-style hosts, PTY numbers are still allocated by the OS, but
the broker exposes stable `tty.wiremux-*` aliases for terminal tools. These
aliases are removed when the backing serial device disconnects or the host exits
normally, then recreated with the same names after the next manifest sync. On
macOS, endpoint shutdown also best-effort revokes the real PTY slave so clients
with an already-open descriptor can observe disconnect.
Vendor enhanced adapters may later request ownership for special endpoints, for
example an ESP32 aggregate flashing PTY.

## Profile Discovery

Vendor or enhanced protocol selection should be driven by manifest-declared
profiles, not by a closed core enum. A profile identifier is an extensible
string, for example:

```text
wiremux.transfer.v1
wiremux.pty.v1
wiremux.console.v1
esp32.ota.v1
esp32.esptool.v1
rpi.uf2.v1
magicdian.device-control.v1
```

The core may define the schema for profile declarations, while official and
third-party adapters can maintain registries outside the protocol schema.
This keeps third-party extension possible without requiring a core proto release
for every new vendor or device family.

## Firmware Update Boundary

Firmware update is a device profile, not a core protocol concept.

The preferred product flow is:

1. The device manifest declares a generic transfer profile and a device-specific
   update profile, such as `wiremux.transfer.v1` plus `esp32.ota.v1`.
2. The host enhanced layer selects a file through CLI or TUI.
3. The host uses generic reliable transfer primitives to move bytes.
4. The ESP32 adapter validates image metadata, writes the OTA partition,
   verifies the result, commits, and optionally reboots.

The core only needs to understand reliable transfer state, chunks, acknowledgments,
errors, and completion. The ESP32 adapter owns image validation, partition choice,
secure boot policy, rollback, and reboot semantics.

An ESP-IDF or esptool bridge can coexist as a host enhanced compatibility layer.
That bridge may expose a virtual TTY or port to existing tools, but internally it
should still translate into Wiremux profiles and generic transfer mechanisms
instead of adding bootloader-specific behavior to core.

## Design Rules

- Keep core protocol fields generic and reusable.
- Prefer profile identifiers over vendor enums for extensibility.
- Treat `DeviceManifest` as the capability discovery source of truth.
- Keep virtual port and PTY behavior in host enhanced tooling.
- Keep device-specific update, reset, signing, and boot policies in adapters.
- Add conformance tests at both levels: core protocol compatibility and profile
  behavior compatibility.
- Treat `wiremux-build` as a product orchestrator over Cargo, CMake, `idf.py`,
  and release tooling, not as a replacement compiler or package manager.
