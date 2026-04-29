# Transfer Profile

The transfer profile is reserved for a future HAL-like contract for file or
bulk-data movement over Wiremux channels.

Intended boundary:

- Uses the portable core for frame, envelope, manifest, batch, and compression
  primitives.
- Describes transfer semantics that host and vendor adapters can implement
  consistently.
- Avoids transport-specific details such as UART, USB, TCP, or SDK task models.

This directory is a tracked skeleton only. No runtime transfer protocol,
message schema, channel behavior, or adapter implementation is introduced here.
