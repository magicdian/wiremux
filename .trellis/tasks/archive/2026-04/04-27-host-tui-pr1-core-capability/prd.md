# PR1 core proto capability model

## Goal

Move channel input/interaction mode into the core protocol so host and device
can share line-mode and passthrough/key-stream semantics.

## Requirements

* Add core/proto interaction mode values for unspecified, line, and passthrough.
* Add `DeviceManifestRequest` to the proto schema.
* Extend `ChannelDescriptor` with interaction mode information.
* Add matching C core constants/types.
* Update manifest encoder and C core tests.

## Acceptance Criteria

* [x] Manifest encoding includes interaction mode when declared.
* [x] C core tests cover interaction mode encoding.
* [x] Proto changes are additive/backward compatible.

## Technical Notes

Parent task: `.trellis/tasks/04-27-host-ratatui-tui`.
