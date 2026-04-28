# Wiremux Product Architecture

Wiremux is organized as a stable protocol core with optional host and device
enhancements layered around it. The core should stay platform-neutral and avoid
embedding vendor-specific semantics such as ESP32 OTA, Raspberry Pi UF2, or
bootloader flashing flows.

The product model is:

- `wiremux-core` provides framing, multiplexing, channel metadata, manifest
  exchange, and future generic control or reliable transfer primitives.
- `wiremux-host enhanced` provides batteries-included Rust tooling such as TUI,
  CLI, broker services, virtual TTY/port bridges, transfer orchestration, and
  device-aware adapters.
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
│                    WIREMUX HOST ENHANCED                           │
│                                                                    │
│   connection manager     manifest resolver     profile registry     │
│   transfer manager       virtual port manager  diagnostics          │
│                                                                    │
│   ┌────────────────┐   ┌────────────────┐   ┌────────────────┐     │
│   │ ESP32 Adapter  │   │ RPi Adapter    │   │ Generic Adapter │     │
│   │ OTA / esptool  │   │ UF2 / control  │   │ transfer / pty  │     │
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

`wiremux-host enhanced` is the official Rust tool layer for higher-level product
features. It may be larger and more opinionated than the core because it can
bundle common adapters and user workflows.

Host enhanced responsibilities include:

- serial, TCP, BLE, and future connection management
- manifest resolution and profile discovery
- profile registry and adapter dispatch
- transfer progress, retry, cancel, and diagnostics UX
- virtual TTY, TCP bridge, broker, and capture/replay features
- device-aware adapters such as ESP32 OTA or Raspberry Pi control workflows
- compatibility bridges such as an ESP-IDF/esptool-facing virtual port

Users who only need the protocol can depend on `wiremux-core` and build their
own host or device integration without carrying the enhanced host feature set.

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
