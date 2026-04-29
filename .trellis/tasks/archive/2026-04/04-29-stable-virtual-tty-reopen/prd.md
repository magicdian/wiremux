# brainstorm: stable virtual tty names and reopen behavior

## Goal

Improve host-side virtual TTY behavior so `wiremux` works reliably with terminal clients such as `minicom`: virtual TTY paths should be predictable, reusable across `wiremux` restarts, and removed when the backing physical device disconnects.

## What I already know

* Current pain point: `wiremux` starts, creates a virtual port such as `/dev/ttys034`, `minicom` opens it, then after `wiremux` exits and restarts it cannot recreate/use the same virtual port path.
* Initial physical-device question: whether opening the original physical serial device should use a mechanism similar to `minicom`'s exclusive/open mode.
* Desired virtual-device behavior: virtual port names created by `wiremux` should remain usable as stable targets after `wiremux` restarts.
* Desired naming: expose virtual devices as `/dev/tty.wiremux-<simplified-original-device>-<channel>`.
* If a channel has a configured name such as `telemetry`, use that in the path suffix instead of `chX`.
* Simplified original device example: `/dev/tty.HUAWEIFreeClip2345` becomes `HUAWEIFreeClip2`, max 15 characters.
* Current `VirtualSerialConfig` already has `name_template = "wiremux-{device}-{channel}"`, but the Unix backend ignores the generated name when opening the PTY.
* Current Unix virtual serial backend calls `posix_openpt`, `grantpt`, `unlockpt`, and `ptsname`; the actual exposed path is the OS-assigned slave PTY path.
* Current TUI reconnect loop defaults to `--reconnect-delay-ms 500`; interactive serial reads use a short 5 ms timeout. Non-interactive CLI serial reads use a 100 ms timeout.
* Current `listen` reconnect loop also defaults to `--reconnect-delay-ms 500`, sleeping the full delay after open failures and after disconnect.
* Current interactive Unix backend prefers a `mio` event-driven serial backend; compat backend falls back to a read thread.
* The `serialport` crate's Unix `TTYPort::open` opens ports in exclusive mode by default and uses `TIOCEXCL` plus `flock`; `wiremux` currently uses `serialport::new(...).open()` / `.open_native()`, so physical serial exclusivity is likely already active unless the crate behavior is overridden.
* Minicom-style locking appears to rely on UUCP-style lock files in addition to normal serial open behavior; this coordinates with tools that honor lock files but cannot stop every raw opener.
* User clarified that a user directory, `/tmp`, or `/dev` alias can all be acceptable, with `/dev` preferred when practical.
* User does not want a daemon/background broker.
* The key user experience target is: when `wiremux` exits and starts again, the virtual serial endpoint should behave like a USB serial reconnect, so `minicom` should not need to quit and manually reconnect if that is technically possible.
* User provided observed behavior: `idf.py monitor` and `minicom` can survive an ESP32 reset where `/dev/tty.usbmodem41301` temporarily disappears, then automatically reopen the same path when it reappears.
* User accepts that `kill -9` does not run cleanup, so stale endpoint state may require closing/reopening `minicom`.
* User confirmed physical reconnect optimization is not needed for this task: current `wiremux` already reconnects after ESP32 reset.
* User-provided reset logs show virtual serial endpoints are recreated after disconnect/reconnect. That is acceptable, but endpoints must be removed when the physical device disconnects.
* User-provided reset logs also show repeated manifest handling can recreate PTYs again on the same connection. This churn should be avoided when the manifest has not materially changed.

## Assumptions (temporary)

* The host implementation is responsible for creating PTYs and exposing virtual serial endpoints.
* macOS-style `/dev/tty.*` paths matter for this task, though the code may also need Linux behavior to stay sane.
* Stable names likely require symlinks or filesystem aliases because PTY device numbers themselves are allocated by the OS and are not generally selectable.
* Creating symlinks directly under `/dev` may require privileges or may not be supported on all Unix systems; a user-writable runtime directory may be needed as fallback.
* Without a daemon keeping the PTY master alive, an already-open minicom file descriptor remains attached to the old slave PTY until hangup/error. The viable no-daemon model is to make the stable path disappear on normal `wiremux` exit and reappear on restart, so clients that already retry `open()` on the configured path can reconnect.

## Open Questions

* None remaining.

## Requirements (evolving)

* Design predictable virtual TTY names based on source device and channel name/index.
* Ensure `wiremux` restart behavior supports clients configured against stable virtual paths.
* Keep OS-assigned PTY paths visible in diagnostics because those remain the real slave devices.
* Do not introduce a daemon/background broker.
* Prefer `/dev/tty.wiremux-*` aliases when possible, but allow a user-writable alias directory when `/dev` cannot be written.
* On normal shutdown, remove stable virtual serial aliases so the configured path visibly disappears.
* On physical serial disconnect, drop virtual serial endpoints and remove stable aliases before waiting for device reconnect.
* On restart, recreate the same stable aliases, pointing to newly allocated PTY slave paths.
* Treat clients that retry `open()` on the configured stable path, such as the observed `minicom` behavior, as the target UX.
* Avoid recreating PTYs and aliases on duplicate/same manifest events when existing endpoints still match the manifest.

## Acceptance Criteria (evolving)

* [ ] A configured channel name such as `telemetry` appears in the stable virtual TTY path.
* [ ] A channel without a configured name uses `chX` in the stable virtual TTY path.
* [ ] Source device names are simplified and length-limited consistently.
* [ ] Restarting `wiremux` can re-establish the same stable virtual paths or cleanly report why it cannot.
* [ ] Physical serial open/reconnect behavior is left unchanged.
* [ ] Existing `/dev/cu.*` preference for physical macOS serial targets remains unchanged.
* [ ] The implementation documents the no-daemon limitation: existing open client FDs cannot be moved from the old PTY to the new PTY after `wiremux` restarts.
* [ ] During normal `wiremux` shutdown, stable aliases are removed.
* [ ] During physical serial disconnect, active virtual serial endpoints are dropped and stable aliases disappear.
* [ ] After restart, stable aliases are recreated with the same names when the same physical source/channel manifest is used.
* [ ] A client configured against the stable alias can reconnect without changing its configured port path, provided the client retries open after hangup/error.
* [ ] Existing physical serial exclusive-open behavior is preserved and documented; no compatibility regression for `/dev/tty.*` to `/dev/cu.*` candidate resolution.
* [ ] Receiving an unchanged manifest while still connected does not recreate PTYs or aliases.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI green.
* Docs/notes updated if behavior changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Replacing terminal clients such as `minicom`.
* Guaranteeing OS-assigned raw PTY node numbers such as `/dev/ttys034`.
* Adding a daemon/background broker that keeps PTY masters alive across `wiremux` frontend restarts.
* Guaranteeing automatic recovery after `kill -9`; best-effort stale alias replacement on next start is acceptable, but no graceful cleanup is required.
* Optimizing physical serial reconnect timing or adding `/dev` directory watchers.
* Adding UUCP/minicom-style lock files for physical serial devices.

## Technical Notes

## Research Notes

### What similar tools and APIs do

* `serialport` Unix `TTYPort::open` documents that ports are opened in exclusive mode by default and the implementation uses `TIOCEXCL` plus `flock`.
* `serialport` exposes `TTYPort::set_exclusive`, but current `wiremux` physical opens already go through the default builder path.
* `posix_openpt` allocates the next available pseudo-terminal master; the corresponding slave pathname is obtained via `ptsname`.
* Minicom source and distro behavior show traditional lock-file coordination for serial devices. This is useful for compatibility with tools that honor lock files, but it is advisory.

### Constraints from this repo

* Host config already has `[virtual_serial].name_template`, so fixed naming should extend existing config rather than invent a parallel setting.
* The current backend's `open_virtual_serial_endpoint(_name)` ignores `_name`; this is the narrowest place to attach stable aliases.
* TUI currently creates/syncs endpoints after the device manifest arrives, because channel names come from the manifest.
* Source physical port path is not currently passed into `VirtualSerialBroker`, so using original-device-derived names requires adding source context to broker construction or sync.
* Cross-platform behavior matters: Unix PTY backend exists; Windows virtual COM is planned/unsupported.

### Feasible approaches here

#### Virtual serial restart behavior

**Approach A: Stable symlink aliases for OS PTYs** (Recommended)

* How it works: keep using `posix_openpt` for real PTYs, then create/update a stable symlink from a sanitized configured alias to the OS-assigned slave path. Remove aliases on normal shutdown so the stable path disappears, then recreate them on restart.
* Pros: small change, preserves portable PTY allocation, works with terminal clients that retry opening a pathname after hangup/error.
* Cons: `/dev/tty.wiremux-*` may require privileges; stale symlink handling must be explicit; already-open client FDs remain attached to the old PTY until the client observes hangup/error.

**Approach B: User runtime directory aliases by default**

* How it works: create aliases under a user-writable directory such as `~/.local/state/wiremux/` or macOS Application Support, with optional `/dev` support later.
* Pros: no elevated privilege requirement, predictable cleanup ownership.
* Cons: paths are not exactly `/dev/tty.wiremux-*`, some users/tools expect `/dev`-style paths, and already-open client FDs remain attached to the old PTY after `wiremux` exits.

**Approach C: Persistent broker/daemon owns PTYs across CLI restarts**

* How it works: a background process keeps PTY masters alive and `wiremux` reconnects to the daemon.
* Pros: could keep clients attached through frontend process restarts.
* Cons: much larger architecture change, lifecycle/security/error handling becomes a separate product surface; explicitly rejected for this MVP.

### Recommended MVP decisions

* Virtual serial: implement Approach A, with normal-shutdown alias cleanup, physical-disconnect alias cleanup, and best-effort stale alias replacement on next start.
* Alias root priority: try `/dev/tty.wiremux-*` first; if unavailable, fall back to a user-writable alias directory.
* Fallback alias directory: `WIREMUX_VIRTUAL_SERIAL_DIR` when set, otherwise `/tmp/wiremux/tty`. The earlier macOS Application Support path was too long, contained spaces, and did not work well with `minicom`.
* macOS endpoint shutdown should best-effort `revoke(2)` the real PTY slave before alias removal so terminal clients with already-open descriptors can observe disconnect.
* Physical serial: preserve current behavior. No reconnect timing optimization, no OS watcher, and no lock-file changes in this task.
* Manifest sync: make endpoint reconciliation stable. Reuse endpoints whose channel id, channel alias name, input capability, and output mode still match; remove/create only changed endpoints.

## Technical Notes

* Likely impacted files:
  * `sources/host/wiremux/crates/interactive/src/lib.rs`: `VirtualSerialBroker`, naming, Unix PTY backend, serial open backend.
  * `sources/host/wiremux/crates/tui/src/lib.rs`: pass physical source info to virtual broker and display alias/real paths.
  * `docs/zh/host-tool.md`, `docs/product-architecture.md`, `docs/matrix/feature-support.md`: document stable virtual serial aliases if behavior changes.
* External references:
  * `serialport` source/docs for Unix exclusive open behavior: https://docs.rs/serialport/latest/src/serialport/posix/tty.rs.html
  * POSIX/OpenBSD `posix_openpt` manual: https://man.openbsd.org/posix_openpt.3
  * Minicom source/lock-file behavior reference: https://fossies.org/dox/minicom-2.10/main_8c_source.html
