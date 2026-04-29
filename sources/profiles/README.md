# Wiremux Profile Contracts

This directory reserves the profile layer for Wiremux behavior contracts.
Profiles sit above the portable core protocol and below host, vendor, and
platform adapters.

The portable core owns framing, envelope encoding, manifests, batching,
compression, and compatibility checks. Profiles define HAL-like contracts for
common channel behaviors without binding those behaviors to a specific runtime,
transport, SDK, or user interface.

Current skeletons:

- `transfer/`: file or bulk-data transfer profile contract.
- `console/`: command-line console profile contract.
- `pty/`: terminal passthrough profile contract.

This PR adds documentation-only skeleton directories. It does not add runtime
profile protocol messages, frame fields, host commands, ESP handlers, or adapter
implementations.
