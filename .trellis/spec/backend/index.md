# Backend Development Guidelines

> Best practices for backend development in this project.

---

## Overview

Backend work in this repository covers the ESP-IDF C component, the ESP-IDF demo
application, and the Rust host CLI/library. There is no database layer and no web
frontend in the current codebase.

The source tree is migrating toward the product layout in
`docs/source-layout-build.md`: shared API definitions in `sources/api`,
platform-neutral core code in `sources/core`, profiles in `sources/profiles`,
host code in `sources/host/wiremux`, and Espressif code in
`sources/vendor/espressif`. Treat older `sources/host`, `sources/esp32`, and
`sources/api/proto` as the protocol schema path, and treat older
`sources/core/proto` references as historical pre-migration paths unless a task is
explicitly updating operational commands before those moves land.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | Active |
| [Database Guidelines](./database-guidelines.md) | Persistence boundary; currently no database | Active |
| [Error Handling](./error-handling.md) | Error types, handling strategies | Active |
| [Quality Guidelines](./quality-guidelines.md) | Code standards, forbidden patterns | Active |
| [Logging Guidelines](./logging-guidelines.md) | Structured logging, log levels | Active |

---

## Pre-Development Checklist

Before editing backend code, read:

- [Directory Structure](./directory-structure.md)
- [Error Handling](./error-handling.md)
- [Quality Guidelines](./quality-guidelines.md)
- [Logging Guidelines](./logging-guidelines.md) if touching ESP logging, mux
  transport, or host diagnostics
- [Database Guidelines](./database-guidelines.md) only if a task mentions
  persistence, captures, storage, migrations, manifests, or configuration files

For cross-language protocol changes, also read
`../guides/cross-layer-thinking-guide.md`.

---

**Language**: All documentation should be written in **English**.
