# brainstorm: optimize host output

## Goal

Improve the host-side tool output so direct CLI usage is more product-like and less noisy, while preserving complete diagnostics in a temporary cache file for debugging.

## What I already know

* Current host output prints batch diagnostics and full per-record metadata to the terminal.
* The desired direct-run output should primarily show raw payload, optionally prefixed with a channel marker such as `ch1> payload`.
* Full details should be written under the system temporary cache path in a `wiremux` directory, with filenames composed from date/time and port information.
* The current output appears to add line breaks between records; the desired behavior is to preserve the original payload formatting instead of forcing line endings.
* Console-like channels may need special handling so output changes do not break interactive console semantics.
* Repo inspection confirms host CLI output is currently concentrated in `sources/host/src/main.rs`.
* `write_event()`, `write_batch_event()`, and `write_envelope_line()` currently write user-facing data and diagnostics to the same stdout writer.
* Current `write_envelope_line()` uses `writeln!` with a leading `\n`, which explains the extra blank-looking spacing around mux records.
* Existing parser tests live in `sources/host/src/main.rs`; adding output-mode tests can stay in the same file for this slice.
* Current docs in `docs/zh/host-tool.md` describe decoded mux frame summaries, so docs should be updated if stdout changes.

## Assumptions (temporary)

* This is a host-side backend task in the Rust CLI/library portion of the repo.
* The terminal default should become concise, while a log/cache file keeps the current detailed diagnostic information.
* Existing explicit verbose/debug flags, if present, should keep enough observability for developers.
* The implementation can use `std::env::temp_dir()` for the system temporary root and create a `wiremux` subdirectory.
* Keeping no new dependency is possible if filenames use a UNIX timestamp; human date/time filenames likely need either a small time-formatting dependency or a local formatter.

## Open Questions

* None currently.

## Requirements (evolving)

* Reduce default terminal noise from host-side receive output.
* Preserve full detailed diagnostics in a system temporary `wiremux` path.
* Preserve original payload formatting rather than unconditionally appending newlines.
* Consider channel labeling without breaking console output behavior.
* Keep parser behavior unchanged: valid mux frames are decoded, ordinary terminal bytes remain preserved in unfiltered mode, and CRC/decode errors do not terminate listening.
* Keep `listen --line` single-handle console workflow working.
* Use output approach A: when `--channel N` is set, print only raw payload bytes for channel N with no prefix and no forced newline; when no channel filter is set, preserve ordinary terminal bytes and print decoded mux records with a `chN> ` prefix.
* Treat text payload newlines as display control characters, not escaped text: CRLF, CR, and LF should be rendered as their original line breaks on stdout instead of `\r` / `\n` escape sequences.
* In unfiltered mixed-channel mode, write the `chN> ` prefix once at the beginning of each decoded record, not before every embedded payload line.
* Preserve serial decode order as much as possible and avoid host-side display interleaving where one channel appears to start in the middle of another channel's visible line.
* For batched records, it is acceptable to add a small host-side display buffer if that trades slight display latency for stable ordered output.
* Use display buffering strategy 1: in unfiltered mixed-channel output, if a decoded record starts for a different channel while the previous channel's visible line is still partial, the host may end the current display line before printing the next channel record.
* When the host inserts such a display-only line break, print a concise marker so users understand the line break was added by the host for readability, not received from the device payload.
* The host-inserted marker must occupy its own line.

## Acceptance Criteria (evolving)

* [x] Default direct-run output no longer prints batch summary lines to stdout.
* [x] Default direct-run output no longer prints full per-record metadata to stdout.
* [x] Detailed records are written to a temp `wiremux` log/cache file with date/time and port in the filename.
* [x] Payload output preserves original formatting and does not force extra blank lines.
* [x] Text payloads containing CRLF, CR, or LF render as line breaks on stdout, not as literal escape text.
* [x] Unfiltered mixed-channel output does not place a new channel prefix in the middle of a previous channel's visible line when buffering can avoid it.
* [x] Host-inserted line breaks for partial-line channel switches are visible through a concise `wiremux>` marker on its own line.
* [x] Console-style output remains usable.
* [x] `cargo test`, `cargo check`, and `cargo fmt --check` pass in `sources/host`.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI-equivalent checks pass.
* Docs/notes updated if behavior changes user-facing CLI output.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Changing ESP-side wire protocol.
* Changing payload encoding or compression behavior.
* Adding a full UI or dashboard.

## Technical Notes

## Relevant Specs

* `.trellis/spec/backend/directory-structure.md`: host Rust layout, CLI contract, batch decode/filter behavior, and mixed-stream requirements.
* `.trellis/spec/backend/error-handling.md`: CRC and decode diagnostics must remain deterministic and suppressed for filtered channels where appropriate.
* `.trellis/spec/backend/quality-guidelines.md`: host protocol tests and console/full-duplex behavior must not regress.
* `.trellis/spec/backend/logging-guidelines.md`: log adapter channels remain observable through host filtering.

## Code Patterns Found

* `sources/host/src/main.rs`: synchronous CLI flow, stdout locking, serial reconnect loop, argument parser, and unit tests.
* `sources/host/src/frame.rs`: mixed-stream scanner already preserves terminal bytes and emits structured frame/error events.
* `docs/zh/host-tool.md`: user-facing host CLI behavior documentation.

## Files Likely to Modify

* `sources/host/src/main.rs`: split concise display output from diagnostics logging; add temp log file creation and output tests.
* `sources/host/Cargo.toml` / `Cargo.lock`: only if choosing a date/time formatting dependency.
* `docs/zh/host-tool.md`: document concise stdout and diagnostics file path.

## Research Notes

### What similar tools do

* CLI stream tools commonly keep stdout machine/user-consumable and route extra diagnostics elsewhere. The Unix `tee` convention demonstrates a useful pattern: show the primary stream while copying detail to a file.
* Rust's standard library provides `std::env::temp_dir()` for the platform temporary directory, which fits the requested system temporary cache path.
* Rust `OpenOptions` can create append/write files directly with the standard library, so the diagnostics sink does not require an async logger.

Sources:

* https://doc.rust-lang.org/std/env/fn.temp_dir.html
* https://doc.rust-lang.org/std/fs/struct.OpenOptions.html
* https://www.gnu.org/software/coreutils/manual/html_node/tee-invocation.html

### Constraints from this repo/project

* No current verbose/debug CLI option exists.
* `sources/host/Cargo.toml` currently has only `serialport`; avoiding a new dependency keeps the host binary simple.
* Current host code does not decode channel descriptors from manifest, so it cannot reliably know "console type" for arbitrary devices yet.
* Existing convention treats channel 1 as console in docs and examples, but this is a convention rather than a discovered runtime type.

### Feasible approaches here

**Approach A: Raw payload for filtered output, channel-prefixed payload for unfiltered mux records** (Chosen)

* How it works: `--channel N` prints only raw payload bytes for channel N with no prefix and no forced newline. Without `--channel`, ordinary terminal bytes pass through; mux records print as `chN> ` plus raw payload, but full metadata goes to the temp diagnostics file.
* Pros: Best console behavior when users filter to console channel; still understandable when viewing multiple channels together.
* Cons: Unfiltered console-channel output is not perfectly raw because it may get `ch1> ` prefixes.

**Approach B: Always raw payload, no channel prefix by default**

* How it works: stdout only writes payload bytes for decoded records and terminal bytes; all channel identity lives in diagnostics.
* Pros: Preserves payload formatting most strictly.
* Cons: Mixed-channel output becomes ambiguous and harder to debug without opening the diagnostics file.

**Approach C: Explicit output mode flag**

* How it works: add something like `--output raw|prefixed|diagnostic`, defaulting to `prefixed` or `raw`.
* Pros: Flexible and future-proof.
* Cons: Larger CLI surface before the product behavior is settled; more docs/tests now.

## Decision (ADR-lite)

**Context**: The current host tool prints verbose batch and record metadata to stdout, which makes direct CLI use noisy and makes console-like payloads harder to read.

**Decision**: Use approach A for the MVP. Filtered channel output is raw payload bytes with no prefix and no forced newline. Unfiltered output keeps ordinary terminal bytes and adds a lightweight `chN> ` marker before decoded mux record payloads. Full metadata and decode diagnostics move to a temp `wiremux` diagnostics file.

**Consequences**: Console workflows should use `--channel 1` for the most terminal-like output. Unfiltered mode remains useful for observing multiple channels but may prefix console payload records. Newline bytes in payloads must be rendered as actual line breaks rather than escaped strings. The chosen prefix behavior is once per decoded record, not once per embedded line. To keep mixed-channel output readable, the host may insert a display-only line break before switching channels from a partial visible line, and should mark that insertion with a dedicated marker line.

## Display Marker Wording

Recommended marker:

```text
wiremux> continued after partial ch1 line
```

The marker must be printed on its own line between the interrupted channel line and the next channel record.

Rationale:

* `wiremux>` makes it clear this line is generated by the host tool, not by a device channel.
* `partial ch1 line` names the channel whose visible line was interrupted.
* Avoids `decode`, because the protocol decode succeeded; the marker describes display normalization only.

Alternatives considered:

* `wiremux> ch1 partial line break inserted` - accurate but more mechanical.
* `wiremux> ch1 partial decode` - rejected because it implies a decode problem.
