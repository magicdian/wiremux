# Database Guidelines

> Database and persistence conventions for this project.

---

## Overview

This project currently has no database layer. There is no ORM, migration system,
server-side storage engine, or persistent application state in the committed
implementation.

The actual product surface is:

- ESP-IDF C component code under `sources/esp32/components/esp-wiremux/`.
- ESP-IDF demo application under `sources/esp32/examples/esp_wiremux_console_demo/`.
- Rust host CLI/library code under `sources/host/`.
- Protocol schema under `sources/core/proto/wiremux.proto`.
- Chinese user documentation under `docs/zh/`.

Do not introduce a database, embedded key-value store, or migration framework
unless a task explicitly requires persistent state.

## Current Persistence Model

State is in memory and bounded by configuration:

- ESP mux runtime state is held in the static `s_mux` context in
  `sources/esp32/components/esp-wiremux/src/esp_wiremux.c`.
- ESP channel metadata is registered at runtime with
  `esp_wiremux_register_channel()`.
- Host CLI state is process-local in `sources/host/src/main.rs`.
- Host protocol session state is process-local in the core C
  `wiremux_host_session_t`, reached from the Rust wrapper in
  `sources/host/src/host_session.rs`.
- Protocol compatibility is represented by constants and protobuf-compatible
  fields, not by stored schema migrations.

Real examples:

```c
static mux_context_t s_mux;
```

```c
typedef struct {
    wiremux_host_session_config_t config;
    size_t buffer_len;
    uint32_t last_device_api_version;
    uint32_t last_compatibility;
    uint8_t manifest_seen;
} wiremux_host_session_t;
```

```proto
message MuxEnvelope {
  uint32 channel_id = 1;
  Direction direction = 2;
  uint32 sequence = 3;
  uint64 timestamp_us = 4;
  PayloadKind kind = 5;
  string payload_type = 6;
  bytes payload = 7;
  uint32 flags = 8;
}
```

## Query Patterns

There are no query patterns because there are no database queries.

If future work needs persisted device manifests, captured mux traffic, or test
fixtures, keep persistence outside the transport-critical path first. Prefer an
explicit file or fixture format for developer tooling before adding a general
database dependency.

Before adding persistence, define:

- What data is persisted and why in the task PRD.
- Whether the data belongs on the ESP device, host CLI, or external tooling.
- The durability boundary: temporary capture, user config, or long-lived state.
- The compatibility strategy for frame/protobuf schema changes.
- The tests that prove old data is still readable.

## Migrations

No migrations exist.

If a future task introduces persistent schema, add a dedicated guideline update
in the same change. That update must document:

- Migration file location and naming.
- How migrations are run in development and release workflows.
- Rollback or forward-only policy.
- Good/base/bad test cases for schema evolution.
- How schema changes relate to `sources/core/proto/wiremux.proto`.

## Naming Conventions

There are no table or column naming conventions today.

Current schema-like names are protocol names:

- Protobuf package: `wiremux.v1`.
- Protobuf messages use PascalCase, for example `MuxEnvelope`.
- Protobuf fields use snake_case, for example `channel_id`.
- ESP public constants use the `ESP_WIREMUX_` prefix.
- Rust public constants use all-caps snake case, for example `SUPPORTED_VERSION`.

Keep protocol field numbers stable. Renaming a protobuf field in source is less
dangerous than changing its field number, but both require cross-language review.

## Forbidden Patterns

- Do not add an ORM or migration crate to `sources/host/Cargo.toml` for CLI-only
  state.
- Do not use ESP NVS, filesystem, or flash storage for mux runtime queues unless
  the task explicitly changes durability requirements.
- Do not persist raw frames as the only source of truth without recording the
  frame version and envelope schema version.
- Do not hide protocol compatibility changes inside a database migration.
- Do not add generated database artifacts under `sources/esp32/examples/*/build`
  or `sources/host/target`.

## Common Mistakes

- Treating protocol schema as database schema. The mux wire contract lives in
  constants plus protobuf-compatible field numbers; it must remain lightweight
  and stream-oriented.
- Adding persistence to solve test setup. Prefer deterministic unit tests in
  `sources/host/src/*.rs` and ESP-IDF build/demo verification.
- Assuming channel registration is durable. Channels are registered at runtime by
  application code such as
  `sources/esp32/examples/esp_wiremux_console_demo/main/esp_wiremux_console_demo_main.c`.
