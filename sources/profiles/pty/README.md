# PTY Profile

The PTY profile is reserved for a future HAL-like contract for terminal
passthrough behavior over Wiremux channels.

Intended boundary:

- Uses the portable core for frame, envelope, and manifest primitives.
- Describes terminal stream semantics above core and below host or vendor
  adapters.
- Leaves terminal mode handling, OS PTY integration, and transport IO to adapter
  layers.

This directory is a tracked skeleton only. No runtime PTY protocol change,
terminal implementation, host command, or device adapter is introduced here.
