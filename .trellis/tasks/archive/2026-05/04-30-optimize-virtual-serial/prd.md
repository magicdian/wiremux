# macOS DriverKit virtual serial POC

## Goal

Validate whether a native macOS DriverKit virtual serial backend can give
Wiremux a real OS-visible serial endpoint that external tools, especially
ESP-IDF `idf.py flash` / esptool, can treat like a physical serial device.

## What I already know

* Current virtual serial alias creation under `/dev/` fails and falls back to a
  `/tmp/wiremux/tty/tty.wiremux-esp-enhanced` path.
* ESP-IDF/esptool can connect through the fallback path and begin flashing.
* The fallback path is not recognized as a normal COM or `/dev/` serial port by
  PID identification logic.
* RTS/DTR reset control is not supported for the fallback port path, so esptool
  warns that the chip was not reset.
* Flashing reaches stub upload and then fails when changing baud rate to
  460800 with: `Failed to set baud rate 460800. The driver may not support this
  rate.`
* A possible long-term direction is a platform-specific virtual serial adapter,
  such as macOS DriverKit or another virtual serial device mechanism, so the OS
  treats the endpoint as a real serial device.
* Existing docs already mark the current macOS high-baud failure as a known PTY
  limitation and recommend `idf.py flash --port <path> --baud 115200` for the
  current MVP.
* The current Unix virtual serial backend creates a PTY with `posix_openpt`,
  exposes a stable `tty.<name>` symlink under `/dev` when possible, then falls
  back to `WIREMUX_VIRTUAL_SERIAL_DIR` or `/tmp/wiremux/tty`.
* The ESP enhanced path creates a fixed `wiremux-esp-enhanced` endpoint,
  detects esptool SYNC, uses DTR/RTS on the physical serial port to enter the
  bootloader, raw-bridges bytes, detects esptool `CHANGE_BAUDRATE`, and applies
  that baud to the physical port after a device response.
* Local reproduction with a plain macOS PTY symlink shows pyserial can open at
  115200, but setting baud to 460800 fails with `OSError(25, 'Inappropriate
  ioctl for device')` from the macOS `IOSSIOSPEED` path. This matches esptool's
  fatal error text.
* Esptool opens ports through `serial.serial_for_url`, so URL transports like
  RFC2217 are technically available, but its PID detection only runs for COM
  and `/dev/` paths and reset behavior still depends on RTS/DTR support.
* Apple SerialDriverKit exposes `IOUserSerial` / `IOUserUSBSerial` hooks for
  baud, modem status, UART config, RX/TX queues, and DriverKit runs as a
  user-space system extension packaged with an app.

## Assumptions (temporary)

* The current implementation uses a pseudo-terminal plus symlink alias rather
  than a kernel/DriverKit serial device.
* The baud-rate failure is caused by pyserial attempting the macOS
  `IOSSIOSPEED` ioctl on a PTY, which returns `ENOTTY`, before Wiremux can treat
  the requested virtual baud as metadata.
* Creating arbitrary named nodes directly in `/dev` on modern macOS is probably
  constrained by system policy and may require a driver, launch service, or
  accepted platform device mechanism rather than ordinary filesystem writes.

## Requirements (evolving)

* Scope this round as a DriverKit POC / feasibility spike, not a production
  flashing backend.
* Validate whether a minimal DriverKit/SerialDriverKit extension can expose an
  OS-visible serial service suitable for `/dev/tty.*` / `/dev/cu.*` discovery.
* Validate whether pyserial/esptool baud-rate and modem-control operations can
  reach DriverKit serial hooks instead of failing at the PTY `IOSSIOSPEED`
  boundary.
* Keep the DriverKit extension as a narrow C++/IIG shim; keep Rust responsible
  for Wiremux host behavior and future physical serial bridging.
* Document local development prerequisites and blockers, especially Xcode,
  DriverKit SDK, SIP/developer mode, signing, and entitlement requirements.
* Evaluate whether `apple-platforms` / `apple-sdk` are useful for build helper
  environment detection, without treating them as DriverKit runtime bindings.

## Acceptance Criteria (evolving)

* [x] POC structure is added or documented with a minimal macOS DriverKit serial
      extension target/shim plan.
* [x] A local build/probe command verifies whether the current machine has the
      required DriverKit SDK/Xcode tooling.
* [x] A real Xcode/xcodebuild app+dext POC builds a minimal `IOUserSerial`
      subclass against DriverKit/SerialDriverKit.
* [x] The POC documents exactly what can be tested without Apple-granted
      DriverKit signing entitlements.
* [ ] If local loading is possible, macOS exposes a serial-like service and
      pyserial baud/DTR/RTS probes are attempted against it.
* [x] If local loading is blocked, the blocker is concrete and actionable
      (missing entitlement, signing, SIP/security mode, SDK, or code issue).
* [x] The POC result recommends whether to proceed to a production DriverKit
      backend, switch to RFC2217/esptool adapter, or keep PTY-only support.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI green for touched code.
* Docs/notes updated if behavior or platform support changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Production-grade flashing support through DriverKit.
* Signed/notarized public distribution package.
* Full Rust build-system integration for DriverKit.
* Windows virtual COM backend.
* Replacing the current PTY virtual serial backend for Linux/macOS generic
  virtual channels.

## Technical Notes

* User-provided failing flow: `idf.py flash` using
  `/tmp/wiremux/tty/tty.wiremux-esp-enhanced`, reaching stub upload, then
  failing on baud change to 460800.
* Repo files inspected:
  * `README.md`: documents ESP enhanced endpoint and current `--baud 115200`
    workaround.
  * `docs/matrix/feature-support.md`: marks ESP32 esptool passthrough as
    partial and native macOS DriverKit virtual serial backend as planned.
  * `docs/product-architecture.md`: defines generic enhanced virtual serial
    broker semantics.
  * `sources/host/wiremux/crates/interactive/src/lib.rs`: PTY creation, stable
    alias fallback, virtual serial broker, physical serial baud setters.
  * `sources/host/wiremux/crates/tui/src/esp_enhanced.rs`: ESP enhanced
    esptool bridge and baud-change detection.
  * Local ESP-IDF env esptool/pyserial sources: esptool fatal error text,
    PID/path detection, reset warning, and pyserial macOS `IOSSIOSPEED` path.
* Local repro command created an OS PTY, symlinked it under `/tmp`, opened it
  with pyserial at 115200, then failed on `s.baudrate = 460800` with
  `OSError(25, 'Inappropriate ioctl for device')`.
* External references:
  * Apple `IOUserSerial`: serial drivers can implement baud, modem status,
    UART, RX/TX behavior.
  * Apple DriverKit security: DriverKit drivers run in user space, are packaged
    as app extensions, and are installed/upgraded through System Extensions.

## Research Notes

### What similar tools and platform APIs do

* Pyserial on macOS uses normal termios baud constants when available; for
  non-standard/custom rates it uses the macOS `IOSSIOSPEED` ioctl. PTYs reject
  that ioctl with `ENOTTY`.
* Esptool sends `ESP_CHANGE_BAUDRATE`, prints `Changed.`, then sets the host
  serial object's baudrate. The failure happens after the device-side command
  succeeds, when the host-side pyserial setter fails.
* Esptool reset flows expect RTS/DTR support; PTYs do not expose modem-control
  behavior, so esptool warns and Wiremux compensates by driving reset on the
  physical serial port after detecting SYNC.
* DriverKit/SerialDriverKit is the Apple-supported route for a tty-like serial
  service that can respond to baud and modem-control requests as a device
  driver instead of as a PTY.
* Apple's DriverKit flow is app/system-extension based. Xcode provides a
  DriverKit Driver target template, and the extension must be delivered inside
  the app bundle under `Contents/Library/SystemExtensions`.
* The DriverKit template starts from C++ source plus an IOKit Interface
  Generator (`.iig`) header. SerialDriverKit's public serial surface is C++,
  with `IOUserSerial` hooks such as `HwProgramBaudRate`, modem control, UART,
  and RX/TX queue methods.
* Rust can remain the main Wiremux host implementation, but the DriverKit
  extension should probably be a narrow C++/IIG shim. The shim can communicate
  with a Rust user-space process/client over a DriverKit user-client/control
  channel or another app-mediated IPC path.
* Non-DriverKit options on macOS do not appear to provide the same "real
  serial `/dev` device" result:
  * PTY/openpty/socat can create real `/dev/ttysNN` nodes and symlink aliases,
    but not a custom serial driver with reliable high-baud/modem-control ioctl
    behavior.
  * Legacy kernel extensions can create device nodes, but Apple positions
    DriverKit/System Extensions as the preferred replacement for most low-level
    services and kexts carry much heavier security/distribution risk.
  * External hardware or USB device emulation can expose a real serial device,
    but that changes the product requirement from host-only virtual serial to
    hardware-backed serial.
  * Filesystem tricks such as symlinks or FUSE/macFUSE cannot make pyserial see
    a character device that implements serial ioctls.

### Constraints from this repo/project

* The current TUI host already owns the physical serial port and successfully
  raw-bridges esptool bytes after SYNC.
* The high-baud failure is not a bridge payload problem; it occurs inside
  pyserial's host-side port reconfiguration on the virtual endpoint.
* The existing enhanced capability registry can host platform-specific virtual
  serial providers without changing the Wiremux device protocol.
* A DriverKit backend is a distribution/security project, not just a Rust crate
  patch: it needs app extension packaging, entitlements, installation UX, and a
  user-space control channel to the Wiremux host.

### Feasible approaches here

**Approach A: Stabilize the current PTY backend and formalize the workaround**

* How it works: keep PTY/symlink backend, improve diagnostics/docs/tests, make
  the TUI explicitly show that default ESP-IDF high-baud flashing is unsupported
  on macOS PTY and suggest `--baud 115200` / `ESPBAUD=115200`.
* Pros: small, low-risk, fits current architecture, preserves Linux/macOS Unix
  behavior.
* Cons: does not solve default `idf.py flash` high-baud behavior or `/dev`
  naming on locked-down macOS.

**Approach B: Add a Wiremux esptool adapter/shim MVP**

* How it works: provide a Wiremux-owned wrapper or pyserial URL/RFC2217-style
  adapter that accepts baud/RTS/DTR requests as metadata and forwards them to
  the TUI/physical serial bridge.
* Pros: can support high-baud intent sooner without DriverKit packaging; avoids
  pretending PTYs can satisfy unsupported ioctls.
* Cons: less transparent than a `/dev/tty.*` device; may not integrate cleanly
  with `idf.py flash` unless users configure a wrapper, environment variable, or
  custom port URL.

**Approach C: Native macOS DriverKit virtual serial backend**

* How it works: implement a macOS system extension using SerialDriverKit
  (`IOUserSerial` or related), expose a real `/dev/tty.*` / `/dev/cu.*` style
  serial service, and connect it to the Wiremux host through a user-space
  control/data channel.
* Pros: best long-term UX; likely satisfies pyserial/esptool baud and modem
  control expectations through real serial-driver callbacks.
* Cons: largest scope; requires signing/entitlements/installer UX and careful
  architecture outside the current Rust workspace.

### DriverKit/Rust architecture sketch

* `wiremux-macos-serial-dext`: small C++/IIG DriverKit extension subclassing
  `IOUserSerial` (or USB-specific serial class only if matching real USB
  providers becomes necessary).
* `wiremux-host`: existing Rust binary remains responsible for physical serial
  ownership, esptool bridge state, Wiremux frames, and physical baud/RTS/DTR
  actions.
* `wiremux-driver-client`: app/helper control plane activates the system
  extension and brokers data/control between the dext and Rust host.
* Contract to define before implementation: virtual serial naming, one or many
  ports, data queues, baud change request/ack, DTR/RTS event propagation,
  disconnect semantics, and failure behavior when Rust host is not running.

### Packaging/signing research

* Xcode project files are not inherently the runtime contract. The runtime
  contract is an app bundle containing the DriverKit system extension at
  `Contents/Library/SystemExtensions`, plus a host app/helper that calls
  `OSSystemExtensionManager` activation APIs. In practice, Xcode/xcodebuild
  remains the most reliable path because DriverKit uses C++/IIG, entitlements,
  provisioning profiles, app-extension embedding, and codesigning.
* Apple recommends shipping system extensions inside the app bundle and
  activating them from the app, not installing the dext as an unmanaged global
  file. The system validates bundle location, signatures, entitlements, team
  grants, and bundle IDs during activation.
* Development can temporarily relax some checks:
  `systemextensionsctl developer on` permits loading from non-Applications
  locations, and disabling SIP can bypass notarization checks. Apple explicitly
  says this is local-development-only and must be reenabled before shipping.
* DriverKit requires Apple-granted entitlements tied to the developer team for
  normal activation/distribution. DriverKit development provisioning profiles
  are only available for App IDs with DriverKit entitlements enabled.
* Without a paid/enrolled developer account and granted DriverKit entitlements,
  a local spike may still explore code structure and possibly local loading with
  relaxed security, but a user-installable/distributable driver is not feasible.
* Karabiner-DriverKit-VirtualHIDDevice is a useful distribution and IPC example:
  it ships a manager app containing the DriverKit driver, a daemon, and client
  IPC. Its README states proper signing is required and that general developer
  accounts lack the required DriverKit signing permissions until Apple grants
  them. It uses Xcode/XcodeGen and produces a signed/notarized pkg.
* Karabiner's dext is HID-specific (`VirtualHIDDevice`), not serial-specific.
  It can inform architecture, entitlement flow, activation, daemon/client IPC,
  and packaging, but cannot directly expose `/dev/tty.*` or satisfy serial
  baud/modem-control ioctls for Wiremux.
* `karabiner-driverkit` / psych3r wrapper is a Rust wrapper around Karabiner's
  VirtualHIDDevice for kanata-style HID use. It is not a general Apple
  SerialDriverKit binding.
* The docs.rs `driverkit` crate is a generic Rust device-driver framework with
  PCI/network-oriented modules and is not Apple DriverKit/SerialDriverKit.
* `apple-platforms` is relevant only as build metadata glue. It models Apple
  platforms including DriverKit, maps Rust target triples to Apple/Clang target
  strings, and resolves SDK names for `xcrun --sdk`. It does not provide
  DriverKit C++/IIG bindings, `IOUserSerial` wrappers, system-extension
  activation, codesigning, entitlements, or serial driver runtime behavior.
* `apple-sdk` may be more directly useful than `apple-platforms` for a custom
  build helper because it can discover Xcode developer directories, platform
  directories, and SDK paths, including a `DriverKit` platform variant.
* Swift Package Manager also recognizes `.driverKit` as a platform, but that is
  useful only if the POC chooses SwiftPM for helper/app pieces; it does not
  replace DriverKit C++/IIG work.

## POC Artifacts

* `sources/poc/macos/driverkit-serial-poc/README.md`
  documents the POC scope, DriverKit/Rust boundary, signing/loading constraints,
  and next proof points.
* `sources/poc/macos/driverkit-serial-poc/probes/driverkit-env.sh`
  checks local Xcode/DriverKit SDK availability, SerialDriverKit headers,
  SIP/system-extension developer status, and visible signing identities.
* `sources/poc/macos/driverkit-serial-poc/probes/pyserial-pty-baud.py`
  reproduces the current PTY custom-baud limitation using pyserial.
* `sources/poc/macos/driverkit-serial-poc/xcode/WiremuxDriverKitSerialPOC.xcodeproj`
  is a minimal app plus DriverKit dext project. The app contains an
  `OSSystemExtensionManager` activation path; the dext subclasses
  `IOUserSerial`.
* `sources/poc/macos/driverkit-serial-poc/probes/build-driverkit-poc.sh`
  builds the app+dext through `xcodebuild`, defaulting to
  `CODE_SIGNING_ALLOWED=NO` so source/IIG/link/package validation can run
  without DriverKit signing credentials.
* `sources/poc/macos/driverkit-serial-poc/probes/activate-driverkit-poc.sh`
  checks signatures, submits the activation request through the host app, prints
  `systemextensionsctl list`, and looks for `/dev/tty.wiremux*` /
  `/dev/cu.wiremux*`. It is intended only after a signed build exists.

## Local Probe Results

* `driverkit-env.sh` result on 2026-04-30:
  * Xcode developer directory exists at
    `/Applications/Xcode.app/Contents/Developer`.
  * DriverKit SDK exists at
    `/Applications/Xcode.app/Contents/Developer/Platforms/DriverKit.platform/Developer/SDKs/DriverKit25.2.sdk`.
  * `SerialDriverKit.framework`, `IOUserSerial.iig`,
    `USBSerialDriverKit.framework`, and `IOUserUSBSerial.iig` are present.
  * DriverKit clang is available through `xcrun --sdk driverkit clang`.
  * SIP is enabled.
  * `systemextensionsctl developer` is blocked while SIP is enabled, so local
    dext loading remains blocked until system-extension development settings are
    handled outside the repo.
  * One codesigning identity is visible, but DriverKit entitlement availability
    is not proven by this probe.
* `pyserial-pty-baud.py` result on 2026-04-30:
  * A PTY alias opens at 115200.
  * Setting pyserial baud to 460800 fails with `OSError: [Errno 25]
    Inappropriate ioctl for device`.
  * This confirms the existing PTY backend cannot satisfy the macOS custom-baud
    ioctl path used by esptool.
* `build-driverkit-poc.sh` unsigned build result on 2026-04-30:
  * `xcodebuild -list` recognizes the project, targets, and scheme.
  * The DriverKit target runs `iig` against `WiremuxSerialDriver.iig`.
  * The dext compiles and links against `DriverKit.framework` and
    `SerialDriverKit.framework`.
  * The app embeds the dext at
    `Contents/Library/SystemExtensions/WiremuxSerialDriver.dext`.
  * The dext `Info.plist` expands `IOTTYBaseName = wiremux` and
    `IOTTYSuffix = esp-enhanced`, which is the intended candidate for
    `/dev/tty.wiremux*` / `/dev/cu.wiremux*` naming if activation succeeds.
  * The unsigned build output is suitable for compile/package validation only;
    `codesign` reports the dext is not signed.
* Ad-hoc signing result on 2026-04-30:
  * `CODE_SIGNING_ALLOWED=YES CODE_SIGN_IDENTITY=- build-driverkit-poc.sh`
    fails before compilation with Xcode signing errors.
  * The app requires a provisioning profile.
  * The dext has entitlements that require signing with a development
    certificate.
  * This confirms the current blocker is signing/provisioning/DriverKit
    entitlement availability, not the C++/IIG source shape.

## POC Recommendation

The source/build side of DriverKit is now proven enough for this POC: a minimal
`IOUserSerial` dext builds and is embedded in a host app. Continue DriverKit
runtime exploration only after setting up local system-extension development
conditions and a development provisioning profile with DriverKit serial
entitlements. The next concrete milestone is activation, `/dev` node discovery,
and pyserial baud/DTR/RTS probes against the generated device. If entitlement
or loading requirements cannot be met locally, switch the next implementation
task to the RFC2217/esptool adapter path while keeping DriverKit as a longer-term
distribution project.

## Decision Signals

* User prefers Approach C if feasible, but wants to understand DriverKit/Xcode/Rust
  integration constraints and whether any non-DriverKit route can create a real
  macOS `/dev` serial device.
* User agrees to make this round mainly a POC.
* User agrees that the first POC success metric is: macOS sees a minimal serial
  device, and pyserial baud/DTR/RTS calls can be observed entering the DriverKit
  layer or failing at a concrete platform/signing boundary.

## Decision (ADR-lite)

**Context**: The existing Wiremux virtual serial implementation uses Unix PTYs.
On macOS, pyserial/esptool high-baud changes fail because PTYs reject the
`IOSSIOSPEED` ioctl with `ENOTTY`, and `/dev` stable aliases are unreliable for
ordinary user-space symlinks.

**Decision**: Run a macOS DriverKit virtual serial feasibility POC. The POC will
prefer a small C++/IIG SerialDriverKit shim and keep Rust as the host/control
plane. The goal is to validate platform feasibility before committing to a full
production backend.

**Consequences**: This introduces macOS-specific tooling and signing risk. The
POC may be blocked by Apple-granted DriverKit entitlements; that result is still
valuable if documented clearly. If DriverKit is not feasible locally, the next
fallback direction is an RFC2217/esptool adapter rather than trying to force
PTYs to satisfy unsupported serial ioctls.
