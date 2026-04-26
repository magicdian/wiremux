# PR1: Generic Batch Protocol and Core

## Goal

Add a generic Wiremux batch container so multiple channel records can be carried
inside one mux frame without making Wiremux aware of upper-layer payload types.

## Requirements

* Extend `sources/core/proto/wiremux.proto` with generic batch/compression schema.
* Add portable C batch encode/decode APIs and tests.
* Add Rust host batch encode/decode support and tests.
* Preserve existing single-record `MuxEnvelope` compatibility.

## Acceptance Criteria

* [ ] C core can encode/decode an uncompressed batch with multiple records.
* [ ] Rust host can decode the same batch shape.
* [ ] Existing frame/envelope/manifest tests still pass.

## Technical Notes

* Parent task: `04-26-esp-send-modes-log-payload-strategy`.
* Protocol fields must remain protobuf-compatible and stable once introduced.
