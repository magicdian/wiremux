# macOS DriverKit Serial POC

This directory is a feasibility spike for a future Wiremux macOS native virtual
serial backend. It is intentionally outside the Cargo workspace and does not
change the existing PTY virtual serial backend.

## Goal

Validate whether a narrow SerialDriverKit extension can expose an OS-visible
serial service that accepts the serial operations ESP-IDF/esptool expects:

- `/dev/tty.*` / `/dev/cu.*` style discovery
- host-side baud changes such as `460800`
- DTR/RTS modem-control changes
- byte read/write queues that can later bridge to the Rust Wiremux host

## Current Finding

The existing Wiremux virtual serial backend uses Unix PTYs. On macOS, pyserial
uses the `IOSSIOSPEED` ioctl for custom baud rates. PTYs reject that ioctl with
`ENOTTY`, which is why default ESP-IDF flashing can fail after esptool sends
`ESP_CHANGE_BAUDRATE`.

Run this reproducer:

```bash
./sources/poc/macos/driverkit-serial-poc/probes/pyserial-pty-baud.py
```

Expected result on macOS PTY backends:

```text
baud set failed: OSError: [Errno 25] Inappropriate ioctl for device
```

## Environment Probe

Run:

```bash
./sources/poc/macos/driverkit-serial-poc/probes/driverkit-env.sh
```

The probe checks:

- selected Xcode developer directory
- DriverKit SDK path
- `SerialDriverKit.framework`
- `IOUserSerial.iig`
- `IOUserUSBSerial.iig`
- DriverKit clang availability
- SIP status
- whether `systemextensionsctl developer` can be queried
- visible code-signing identities

This probe does not build or install a system extension. It is safe to run on a
normal development machine.

## Real DriverKit Build POC

This directory now includes a minimal Xcode project:

```text
xcode/WiremuxDriverKitSerialPOC.xcodeproj
  App/
    main.m                 # SystemExtension activation host
    App.entitlements
  Driver/
    WiremuxSerialDriver.iig
    WiremuxSerialDriver.cpp
    Driver.entitlements
```

Build it without signing:

```bash
./sources/poc/macos/driverkit-serial-poc/probes/build-driverkit-poc.sh
```

The unsigned build validates the important compile/package path:

- `iig` accepts the `IOUserSerial` subclass.
- `WiremuxSerialDriver.cpp` links against DriverKit and SerialDriverKit.
- The app embeds the dext at
  `Contents/Library/SystemExtensions/WiremuxSerialDriver.dext`.

The build output is intentionally ignored under `xcode/build/`.

To attempt a signed development build, provide your Apple development settings:

```bash
CODE_SIGNING_ALLOWED=YES \
DEVELOPMENT_TEAM=<team-id> \
PROVISIONING_PROFILE_SPECIFIER=<profile-name> \
./sources/poc/macos/driverkit-serial-poc/probes/build-driverkit-poc.sh
```

An ad-hoc signing attempt is expected to fail for the current entitlements:

```bash
CODE_SIGNING_ALLOWED=YES CODE_SIGN_IDENTITY=- \
./sources/poc/macos/driverkit-serial-poc/probes/build-driverkit-poc.sh
```

On this machine, that failed with:

```text
"WiremuxDriverKitSerialPOC" requires a provisioning profile.
"WiremuxSerialDriver" has entitlements that require signing with a development certificate.
```

That means the source/build side of the POC works, but loading still needs a
valid development identity, provisioning profile, and Apple-granted DriverKit
serial entitlement.

If you have a signed build and local system-extension development is enabled,
submit activation with:

```bash
./sources/poc/macos/driverkit-serial-poc/probes/activate-driverkit-poc.sh
```

The activation script checks signatures, submits the app's
`OSSystemExtensionManager` request, prints `systemextensionsctl list`, and looks
for `/dev/tty.wiremux*` / `/dev/cu.wiremux*`.

## Minimal Driver Shape

The first DriverKit implementation should be a small C++/IIG shim around
`IOUserSerial`, not a Rust dext:

```text
WiremuxSerialDriver : IOUserSerial
  HwProgramBaudRate(baud)
  HwProgramMCR(dtr, rts)
  HwGetModemStatus(...)
  HwProgramUART(...)
  HwResetFIFO(...)
  HwSendBreak(...)
  HwProgramFlowControl(...)
  ConnectQueues(...)
  TxDataAvailable()
  RxFreeSpaceAvailable()
```

The current C++ implementation is deliberately minimal: the baud, UART,
DTR/RTS, modem-status, break, FIFO, and flow-control hooks log their inputs and
return success. It does not yet move bytes to or from Rust.

The Rust Wiremux host should stay in user space and own:

- physical serial port ownership
- Wiremux host session parsing
- esptool bridge state
- physical baud and DTR/RTS actions
- future data/control IPC to the DriverKit shim

The boundary to validate in this POC is:

```text
pyserial/esptool
  -> macOS serial device node
  -> SerialDriverKit IOUserSerial hooks
  -> Wiremux driver client / IPC
  -> Rust Wiremux host
  -> physical serial device
```

## Signing And Loading Reality

DriverKit development is constrained by Apple platform security:

- The DriverKit extension is normally embedded inside an app bundle at
  `Contents/Library/SystemExtensions`.
- The app activates the extension through `OSSystemExtensionManager`.
- Normal distribution requires valid signing, provisioning, and Apple-granted
  DriverKit entitlements.
- Local development may require `systemextensionsctl developer on` and possibly
  reduced security settings, depending on macOS version and signing state.

If the environment probe shows SIP is enabled and `systemextensionsctl developer`
cannot be queried, local loading is expected to be blocked until those settings
are handled outside this repository.

## What This POC Should Prove Next

1. The local machine has a usable DriverKit SDK and SerialDriverKit headers.
2. A minimal `IOUserSerial` dext can be built with Xcode/xcodebuild.
3. The dext can be activated locally or fails with a concrete signing/security
   blocker.
4. If activated, pyserial baud, DTR, and RTS operations enter the DriverKit
   hooks instead of failing at the PTY ioctl layer.

## Non-Goals

- Shipping a signed/notarized package.
- Replacing the current PTY backend.
- Implementing the full esptool flashing bridge through DriverKit.
- Adding Windows virtual COM support.
- Treating Rust crates such as `apple-platforms` as DriverKit runtime bindings.

`apple-platforms` and `apple-sdk` may be useful later for build helper metadata
or SDK discovery, but the DriverKit runtime shim remains C++/IIG.
