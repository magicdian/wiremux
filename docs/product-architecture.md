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

## Generic Enhanced API Stability

Generic enhanced services expose a host-side API contract for vendor-neutral
features that vendor enhanced overlays can depend on. These APIs are separate
from the core device/host protocol: a core-only Wiremux integration can ignore
them, while enhanced hosts and overlay providers can use them as a stable base.
The proto contract is a capability catalog between host core/session behavior
and enhanced implementations; it does not replace the runtime implementation of
each feature.

Generic enhanced API schemas live under
`sources/api/host/generic_enhanced/versions`. The `current/` directory is the
latest development schema. Numbered directories are frozen snapshots that
released overlays may target. A host may expose the current schema and multiple
older frozen snapshots concurrently, so an overlay targeting frozen version 1 can
continue to run after the host current API moves to a later version.

Generic enhanced API states are explicit:

- development APIs may change before they are published as stable;
- stable APIs preserve compatibility within their declared version;
- frozen APIs are immutable numbered snapshots for released overlays.

Each declared API uses a stable string name in the
`wiremux.generic.enhanced.*` namespace plus a single `frozen_version` value.
Version ranges are resolved by the host or overlay manager, not declared by the
device manifest. Generic enhanced v1 contains only
`wiremux.generic.enhanced.virtual_serial`; it derives endpoint behavior from the
existing manifest channel descriptors and does not define a dedicated virtual
serial config message. The v1 schema still carries an optional typed config
extension point so later APIs can add typed configuration without changing the
meaning of existing fields.

The intended host flow is:

```text
host core/session state -> generic enhanced API catalog -> implementation registry
                         -> virtual serial provider
```

The catalog answers which generic enhanced APIs the host supports. The
implementation registry maps a supported `api_name` and `frozen_version` to a
provider such as the built-in virtual serial broker. Future vendor enhanced
overlays can declare a dependency on
`wiremux.generic.enhanced.virtual_serial` and let the host resolver find the
matching implementation instead of importing private virtual serial internals.
In the Rust host workspace, this contract is owned by the
`crates/generic-enhanced` crate; concrete providers such as virtual serial stay
in their implementation crates and register through that shared boundary.

Future overlay package identity, package trust metadata, and TUI contribution
contracts should be added additively after the overlay package/runtime format is
designed. The preferred runtime direction for closed-source overlays is an
out-of-process provider that communicates with the host through a stable local
protocol. In-process dynamic libraries are a higher-risk optional mode and are
not part of the stable generic enhanced ABI commitment.

## Future Overlay Package Identity

Future vendor overlay activation should be based on explicit package identity
rather than only compile-time host features. Official Wiremux overlays reserve
the `wiremux.*` package namespace, for example
`wiremux.espressif.esp32`. Third-party overlays must not use the `wiremux`
prefix; they should use a publisher-owned namespace such as reverse-DNS or an
account-scoped identifier.

An overlay package is the install, update, compatibility, and trust unit. A
runtime executable, WASM module, or shared library inside that package is only
the execution unit. The host overlay resolver should eventually read installed
package manifests, validate namespace and signature metadata, compare generic
enhanced API compatibility, then activate a matching built-in or installed
provider when the connected device manifest requests it.

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
