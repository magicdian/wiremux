# Console Profile

The console profile is reserved for a future HAL-like contract for command-line
console behavior over Wiremux channels.

Intended boundary:

- Uses the portable core for frame, envelope, and manifest primitives.
- Describes console semantics above core and below host or vendor adapters.
- Leaves platform-specific console binding, buffering, and shell integration to
  adapter layers.

This directory is a tracked skeleton only. No runtime console protocol change,
host command, ESP handler, or adapter implementation is introduced here.
