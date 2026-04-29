# Document Wiremux Product Architecture

## Goal

Record the product architecture decision that separates Wiremux core protocol
responsibilities from host enhanced tooling and device/vendor profile adapters.

## Requirements

- Capture the Treble-inspired architecture mapping discussed during design.
- Document that virtual TTY/port bridging is a host enhanced feature, not a
  wiremux-core responsibility.
- Document that firmware update semantics belong to device profiles/adapters,
  while core may provide generic transfer and control primitives.
- Add a standalone architecture document with a square, layered text diagram.
- Keep the documentation in English and avoid runtime code changes.

## Acceptance Criteria

- [x] A standalone architecture document exists under `docs/`.
- [x] The document includes a layered architecture diagram.
- [x] The document defines the boundaries between user tools, host enhanced,
  wiremux-core, profile contracts, transports, and device SDK adapters.
- [x] The document includes a Treble mapping table.
- [x] The document records the firmware update/product profile boundary.

## Technical Notes

- This is a documentation-only task.
- No protocol schema or runtime implementation changes are required.
