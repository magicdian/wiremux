# PR2: Configurable Heatshrink and LZ4 Codecs

## Goal

Implement configurable lossless compression for batch payloads with both
heatshrink and LZ4 available for ESP/host comparison.

## Requirements

* Add shared codec IDs for `NONE`, `HEATSHRINK`, and `LZ4`.
* Add C core codec APIs for compression/decompression.
* Add Rust host codec APIs for decompression and tests.
* If compressed bytes are not smaller, allow fallback to uncompressed output.

## Acceptance Criteria

* [ ] Heatshrink round-trip tests pass in C core and Rust.
* [ ] LZ4 round-trip tests pass in C core and Rust.
* [ ] Unknown/unsupported codec paths fail deterministically.

## Technical Notes

* Initial codecs should be isolated behind a small interface so algorithm details
  do not leak into batch protocol code.
