# Brainstorm: ESP Send Modes and Log Payload Strategy

## Goal

Clarify the current ESP Wiremux send behavior and decide whether future log/telemetry
handling should batch records, keep text payloads, or introduce compressed/binary
payload encoding for high-load production scenarios.

## What I Already Know

* The public ESP channel config defines `ESP_WIREMUX_FLUSH_IMMEDIATE`,
  `ESP_WIREMUX_FLUSH_PERIODIC`, and `ESP_WIREMUX_FLUSH_HIGH_WATERMARK`.
* The demo config labels channel 0 as immediate, channel 2 log as high-watermark,
  and channel 3 telemetry as periodic.
* Current runtime transmit behavior does not branch on `flush_policy`.
* Each `esp_wiremux_write*()` call allocates one `pending_item_t`, enqueues it,
  and the mux task encodes exactly one `MuxEnvelope` and one frame for that item.
* The ESP log adapter formats each `ESP_LOGx` call with `vsnprintf()` into bounded
  text and forwards that single formatted line as one text payload.
* The shared proto currently has one `MuxEnvelope` with `bytes payload`; log text is
  carried as payload bytes with `PayloadKind.TEXT`, not as a structured repeated log
  record message.
* There is no compression flag, compression payload kind, or compression feature flag
  in the current schema/runtime.

## Assumptions (Temporary)

* The user is asking for product/design confirmation first, not immediate code changes.
* High-load logging should prefer deterministic loss/backpressure behavior over
  preserving human-readable raw serial text.
* Backward compatibility with host decoding matters if protocol fields or flags change.

## Open Questions

* None currently. Requirements are ready for implementation planning.

## Requirements (Evolving)

* Answer whether immediate and service/periodic sends are currently implemented.
* Explain current log frame shape: one log call becomes one envelope/frame, not a
  multi-log batch.
* Evaluate generic batching for any channel payload, not a log-specific batch type.
* Evaluate lossless compression for text payload bytes in high-load scenarios.
* Keep Wiremux positioned as a channel multiplexing tool; avoid making core semantics
  depend on upper-layer log concepts.
* Implement a complete configurable feature, not metadata-only scaffolding.
* Allow batching and compression to be configured per channel.
* Allow compression to differ by direction: output-only, input-only, both, or neither.

## Acceptance Criteria (Evolving)

* [ ] The current ESP implementation behavior is documented with file references.
* [ ] Trade-offs are captured for raw text, generic batching, and compressed payloads.
* [ ] MVP scope is chosen before any implementation.
* [ ] Compression mode is configurable per channel and direction.
* [ ] Heatshrink and LZ4 are both implemented end-to-end as configurable codecs.
* [ ] ESP-side comparison data can be collected for representative payload streams.
* [ ] Benchmark output includes raw bytes, encoded bytes, compression ratio,
  encode time in microseconds, decode success, compression fallback count, and
  peak heap usage where measurable.

## Definition of Done (Team Quality Bar)

* Tests added/updated if implementation follows.
* Lint / typecheck / CI green if code changes are made.
* Docs/notes updated if behavior or product positioning changes.
* Rollout/rollback considered if protocol compatibility changes.

## Out of Scope (Explicit)

* No code changes during initial brainstorm unless the user confirms a target approach.
* No lossy compression.
* No encryption/security design in this task.

## Technical Notes

* `sources/esp32/components/esp-wiremux/include/esp_wiremux.h`: public channel
  config includes flush policies and backpressure policies.
* `sources/esp32/components/esp-wiremux/src/esp_wiremux.c`: `write_typed()`,
  `enqueue_item()`, and `mux_task()` implement the current per-item transmit path.
* `sources/esp32/components/esp-wiremux/src/esp_wiremux_log.c`: ESP log adapter
  formats one bounded line and calls `esp_wiremux_write()` once per log callback.
* `sources/core/proto/wiremux.proto`: current `MuxEnvelope` has one `bytes payload`
  field and no batch/compression metadata.
* `sources/core/c/src/wiremux_envelope.c`: encoder writes one payload field per
  envelope; it does not transform or compress payload bytes.
* `sources/host/src/envelope.rs`: host mirrors the same envelope schema and treats
  payload bytes directly.

## Research Notes

### Constraints From Current Repo

* ESP runtime uses one FreeRTOS queue of `pending_item_t *`, not per-channel buffers.
* Backpressure is implemented at enqueue time; flush policy is not implemented beyond
  configuration/manifest-facing intent.
* Current frame parser tolerates mixed terminal bytes and mux frames, but if logs are
  muxed and `tee_to_previous = false`, log visibility depends on the host decoder.
* Protocol changes cross C core, ESP adapter, Rust host, tests, docs, and manifest
  feature advertisement.

### Compression Algorithm Notes

* Heatshrink is designed for embedded/real-time systems, supports incremental bounded
  work, and can run with very small memory. It is a good fit for ESP-side online
  compression of serial payload batches.
* LZ4 is very fast and has a fast decoder, but the implementation and working memory
  trade-off are larger than heatshrink for the first ESP-focused implementation. It
  is still valuable in MVP so real ESP measurements can compare CPU/heap/ratio.
* Deflate/miniz gives common ecosystem compatibility and better ratios in many cases,
  but is heavier for ESP-side live compression and less attractive as the first
  runtime codec for small serial batches.

### Feasible Approaches Here

**Approach A: Document Current Semantics and Fix Naming Drift** (smallest)

* How it works: explicitly document that current `flush_policy` is declarative only,
  and every write/log callback is sent as a separate frame.
* Pros: no compatibility risk; clarifies product truth quickly.
* Cons: does not solve high-load overhead.

**Approach B: Implement Real Flush Policies With Per-Channel Aggregation**

* How it works: immediate writes bypass batching; periodic/high-watermark channels
  buffer multiple records and emit on timer or size threshold.
* Pros: makes current API names true; reduces frame overhead for telemetry/log streams.
* Cons: requires timers, per-channel buffers, edge-case policy, and stronger tests.

**Approach C: Add Generic Batch Payload**

* How it works: define a generic batch container that carries repeated Wiremux payload
  records, each preserving channel ID, direction, kind, payload type, flags, sequence,
  timestamp, and payload bytes.
* Pros: works for logs, telemetry, console/event streams, or future channel types;
  preserves record boundaries without making Wiremux understand log semantics.
* Cons: host/UI must decode the batch container before showing individual records;
  protocol surface grows and needs compatibility tests.

**Approach D: Add Optional Lossless Compression Layer**

* How it works: add feature/flag metadata that says payload bytes are compressed with a
  negotiated algorithm, then host inflates before interpreting payload type/kind.
* Pros: can save serial bytes for repetitive high-volume logs.
* Cons: adds RAM/CPU cost on ESP, failure modes, dictionary/window choices, and host
  compatibility requirements; best done after batching establishes record boundaries.

## Decision Notes

* Do not model the feature as `LogBatch`; batching must be generic to Wiremux.
* Compression should be represented in protocol metadata so host can decide whether
  to decode, display compressed payload metadata, or fail gracefully if unsupported.
* Batching should likely live in core, because the semantics are protocol-level:
  record boundaries, channel IDs, payload kinds, flags, compression metadata, and
  host decode behavior must match across platforms.
* ESP remains responsible for adapter concerns: FreeRTOS task/timer setup, baud-rate
  aware default configuration, memory caps, backpressure policy, and transport writes.
* Batch flush should be driven by both elapsed time and buffered size: flush when
  the encoded batch approaches max payload capacity or when the configured interval
  expires; skip flush cycles when no data is buffered.
* Channel config should express transmit mode and compression independently for
  input/output directions. Example product shape: console output immediate and
  uncompressed; log output batched and compressed; input uncompressed by default.

## Updated Target Scope

* Add protocol support for a generic batch container and compression metadata.
* Add shared core C batch encode/decode support and tests.
* Add Rust host support to decode batch containers and supported compressed payloads.
* Add ESP adapter config for per-channel batching and per-direction compression.
* Implement real periodic/high-watermark flush behavior using size and time triggers.
* Ship heatshrink and LZ4 end-to-end in the initial implementation.
* Add a small ESP/host benchmark or diagnostic path that reports per-codec compression
  ratio, encode time, decode success, and skipped-compression cases.

## Candidate Compression Config Shape

```c
typedef enum {
    ESP_WIREMUX_SEND_IMMEDIATE = 0,
    ESP_WIREMUX_SEND_BATCHED = 1,
} esp_wiremux_send_mode_t;

typedef enum {
    ESP_WIREMUX_COMPRESSION_NONE = 0,
    ESP_WIREMUX_COMPRESSION_HEATSHRINK = 1,
    ESP_WIREMUX_COMPRESSION_LZ4 = 2,
} esp_wiremux_compression_algorithm_t;

typedef struct {
    esp_wiremux_send_mode_t send_mode;
    esp_wiremux_compression_algorithm_t compression;
    uint32_t batch_interval_ms;
    size_t batch_max_bytes;
} esp_wiremux_direction_policy_t;

typedef struct {
    esp_wiremux_direction_policy_t input;
    esp_wiremux_direction_policy_t output;
} esp_wiremux_channel_policy_t;
```

Example policy:

* Console channel output: immediate, no compression.
* Console channel input: immediate, no compression.
* Log channel output: batched, heatshrink or LZ4 compression.
* Telemetry channel output: batched, compression configurable by user.

## MVP Codec Decision

* Include both heatshrink and LZ4 as configurable compression algorithms.
* Keep the protocol metadata algorithm-oriented, not implementation-oriented, so host
  can decode supported algorithms and report unsupported algorithms clearly.
* Compression should be attempted per flushed batch. If compressed output is not smaller
  than the uncompressed batch, the sender may fall back to uncompressed output unless
  user config explicitly forces compression.
* Benchmarking must use realistic ESP payload streams, because heatshrink and LZ4 are
  likely to trade memory, CPU, and compression ratio differently across log text,
  telemetry, and short console messages.

## Benchmark Output Decision

Minimum comparison metrics:

* `raw_bytes`
* `encoded_bytes`
* `ratio`
* `encode_us`
* `decode_ok`
* `fallback_count`
* `heap_peak`

These metrics should be emitted per codec and representative stream so heatshrink
and LZ4 can be compared on actual ESP hardware instead of desktop assumptions.

## Implementation Plan (Small PRs)

* PR1: Protocol and core batch model. Add generic batch/compression schema, C core
  encode/decode support, host decode support, and cross-language tests using
  uncompressed batches.
* PR2: Codec abstraction and algorithms. Add heatshrink and LZ4 support behind a
  shared codec interface in core/host/ESP, including fallback-to-uncompressed behavior.
* PR3: ESP batching service. Add per-channel input/output policy config, timer/size
  based flush behavior, and integration with the existing queue/backpressure path.
* PR4: Diagnostics and docs. Add benchmark/diagnostic output, update docs/specs, and
  capture recommended defaults for console, log, and telemetry channels.
