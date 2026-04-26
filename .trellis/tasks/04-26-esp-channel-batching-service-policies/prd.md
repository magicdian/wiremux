# PR3: ESP Channel Batching Service Policies

## Goal

Make ESP channel send behavior configurable per channel and direction, with real
immediate or batched flush semantics.

## Requirements

* Add per-channel input/output policy config for send mode, compression, batch
  interval, and batch size.
* Keep console default immediate and uncompressed.
* Allow log/telemetry output to batch and compress.
* Flush when batch is full or interval expires; skip empty flushes.

## Acceptance Criteria

* [ ] Immediate channels still send one write as one frame.
* [ ] Batched channels can combine multiple channel records into one batch frame.
* [ ] Direction-specific compression settings are respected.
* [ ] Queue/backpressure failures remain non-fatal.

## Technical Notes

* ESP owns FreeRTOS task/timer integration; shared C core owns batch/codec format.
