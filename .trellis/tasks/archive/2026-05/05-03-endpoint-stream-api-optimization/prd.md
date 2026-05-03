# brainstorm: endpoint stream api optimization

## Goal

Evolve the ESP32 component from a channel registry plus callback model into a robust per-channel endpoint/stream model, while keeping the existing advanced protocol API available and adding a beginner-friendly simple API with a distinct naming prefix.

## What I already know

* The current core already has channel slots and per-channel input handlers.
* In `esp_wiremux.c`, `channel_state_t` stores `registered`, copied channel config, `input_handler`, handler context, `next_sequence`, and `dropped_count`.
* RX dispatch currently decodes frame/envelope/batch, validates `direction == INPUT`, checks `channel_id`, then calls the registered handler outside the mux lock.
* The model does not yet expose endpoint handles, per-channel RX queues, or per-channel session state.
* The console adapter is currently treated as a global singleton, which does not safely model multiple console-like endpoints.
* `esp_wiremux_console.c` has singleton state: `s_console_config`, `s_console_bound`, `s_passthrough_line`, `s_passthrough_line_len`, and `s_passthrough_last_was_cr`.
* The log adapter is also singleton by nature because ESP-IDF `esp_log_set_vprintf()` is process-global, but it can still write through an endpoint/channel boundary.
* The current demo uses advanced APIs directly and can remain as a compatibility demo while a beginner example is added.
* Host TUI already routes display/input behavior by manifest channel metadata and `channel_id`; it does not require collapsing device input into global stdin.
* Host input must remain routed through physical transport, frame parser, envelope decode, channel id, then endpoint RX queue or endpoint consumer.
* Host input must not be collapsed into global stdin.
* Each channel needs isolated line, passthrough, newline, echo, and control-key state.
* Existing advanced APIs must remain available: `esp_wiremux_init`, `esp_wiremux_register_channel`, `esp_wiremux_register_input_handler`, `esp_wiremux_write`, and `esp_wiremux_emit_manifest`.
* The simple API should not reuse the full `esp_wiremux_*` prefix. Current preferred direction is to keep `esp_wiremux_*` for advanced/core APIs and use a distinct simple prefix such as `wmux_*`.

## Assumptions (temporary)

* This task targets the ESP-IDF component first; host/TUI work may be limited to compatibility checks unless the endpoint boundary requires host contract updates.
* The first implementation should preserve current demos or provide a clear migration path.
* The new endpoint model should be internal first unless public handle APIs are explicitly included in MVP.

## Open Questions

* What exact limits should the global `wmux_*` quick-start API have?

## Requirements (evolving)

* Keep `esp_wiremux_*` as the advanced/core API namespace.
* Use `wmux_*` as the beginner-facing simple API prefix.
* Treat `wmux_*` as a portable simple API prefix that can also be used by future non-ESP platforms such as Raspberry Pi.
* MVP includes endpoint internals plus the full beginner-facing simple API.
* Provide two simple API layers:
  * Global quick-start API for the smallest beginner examples.
  * Explicit channel-object API for users who need clearer per-channel state and a path toward advanced features.
* The global quick-start API is intentionally limited and does not need to expose every feature.
* The explicit channel-object API uses an opaque handle, not a public stack-allocated endpoint struct.
* Simple `wmux_*` APIs should avoid platform-specific error return types such as `esp_err_t`.
* Simple read/write-style APIs should return byte counts on success and negative error codes on failure.
* Provide `wmux_strerror()` so users can convert simple API error codes into readable messages, including platform-specific detail when available.
* Provide two simple lifecycle entry styles:
  * `wmux_begin()` for beginner examples; internally wraps `wmux_init()` plus `wmux_start()`.
  * `wmux_config_t`, `wmux_config_init()`, `wmux_init(const wmux_config_t *config)`, and `wmux_start()` for explicit lifecycle control in the advanced simple demo.
* `wmux_begin()` should automatically emit the manifest after start.
* `wmux_config_t` should include `auto_manifest` so explicit simple users can control whether `wmux_start()` emits a manifest.
* Provide two channel-object open styles:
  * `wmux_channel_open(channel_id, &handle)` for default text/binary stream behavior.
  * `wmux_channel_open_with_config(&config, &handle)` for explicit per-channel simple configuration.
* Do not add public simple console adapter APIs in this task.
* `wmux_*` simple APIs should be implemented as a cohesive convenience layer over `esp_wiremux_*` advanced/core APIs rather than bypassing the core API contract.
* Do not migrate the existing `esp_wiremux_console_demo` to the simple API.
* Add two separate simple API example projects:
  * `esp_wiremux_beginner_demo` for the limited global quick-start API.
  * `esp_wiremux_advanced_demo` for explicit channel-object usage.
* The two simple API example projects should remain separate because the global API is intentionally constrained and may not grow with the channel-object API.
* Rename the existing advanced protocol demo to `esp_wiremux_professional_demo`.
* The global quick-start API should lazily auto-register channel 1 if the user has not explicitly registered/configured any channel. If the user manually registers/configures a channel, global APIs must not overwrite that configuration.
* Global simple API should include both default-channel helpers and explicit-channel helpers:
  * Default-channel helpers target channel 1 for the smallest beginner examples.
  * Explicit-channel helpers remain part of the `wmux_*` simple API family and are useful before users adopt opaque channel handles.
* Beginner and advanced example projects are primarily teaching boundaries, not separate namespaces.
* Use a one-consumer-per-channel input model:
  * A channel uses either a read queue or a callback consumer.
  * The default simple API behavior is read queue when no callback is registered.
  * If a callback is registered, that channel uses callback-only delivery.
  * The advanced `esp_wiremux_*` API must expose explicit registration/configuration for the selected consumer behavior.
* Public stream handle APIs are not the main implementation goal for this task; reserve a clean boundary for the next VFS task and add only minimal simple interfaces if they are needed to avoid painting the VFS work into a corner.
* Introduce an endpoint abstraction with channel id, config, RX queue, TX policy, interaction policy, session state, and consumer type.
* Preserve host input routing by decoded `channel_id` into endpoint-specific queues or consumers.
* Avoid any design that sends all host input into global stdin.
* Isolate line, passthrough, newline, echo, and control-key state per channel.
* Leave a stable boundary for future VFS, UART-like, and multi-console-like endpoints.

## Acceptance Criteria (evolving)

* [ ] Different channels have isolated input, output, line state, passthrough state, and interaction policy.
* [ ] Beginner users can initialize and send/receive with a 3-5 line example.
* [ ] Users can move from global quick-start API to explicit channel-object API without switching directly to advanced protocol concepts.
* [ ] Channel-object API exposes an opaque handle so endpoint internals can evolve without ABI churn.
* [ ] Simple API signatures are portable and do not require users to handle ESP-IDF-specific error types.
* [ ] Simple API errors can be rendered through `wmux_strerror()`.
* [ ] `wmux_begin()` provides one-call init/start behavior.
* [ ] `wmux_init()` accepts an explicit portable simple config.
* [ ] `wmux_start()` is available for explicit simple lifecycle examples.
* [ ] Beginner lifecycle automatically emits a manifest.
* [ ] Explicit simple lifecycle can enable/disable auto manifest emission.
* [ ] Channel objects can be opened with defaults or explicit config.
* [ ] Advanced users can still access the complete `esp_wiremux_*` protocol-oriented API.
* [ ] Existing demo behavior is preserved through `esp_wiremux_professional_demo`.
* [ ] `esp_wiremux_beginner_demo` demonstrates the limited global quick-start API.
* [ ] `esp_wiremux_advanced_demo` demonstrates explicit channel-object simple API usage.
* [ ] Global simple API includes both default channel helpers and explicit channel helpers.
* [ ] Per-channel input uses exactly one active consumer: read queue or callback.
* [ ] Future VFS, UART-like, and multi-console-like adapters can be built on endpoint/stream boundaries instead of directly coupling to VFS fd state.

## Definition of Done (team quality bar)

* Tests added/updated where practical for endpoint routing and per-channel state.
* Lint/typecheck/build checks pass for affected packages.
* Public API and migration notes are documented if behavior or headers change.
* Rollout/rollback risk is considered because this changes core component API shape.

## Out of Scope (explicit)

* Implementing the full future VFS adapter unless selected as MVP.
* Implementing a complete UART-like compatibility layer unless selected as MVP.
* Replacing the host TUI interaction model unless required to preserve routing contracts.
* Migrating the existing advanced console demo to the simple API.
* Fully implementing public stream handle APIs unless the final simple API decision explicitly includes them.
* Public `wmux_console_*` or other simple console adapter APIs.

## Technical Notes

* Task created for requirements discovery before implementation.
* Naming decision: reserve `esp_wiremux_*` for advanced APIs and use `wmux_*` for the portable simple API.
* Likely primary files: `sources/vendor/espressif/generic/components/esp-wiremux/include/esp_wiremux.h`, new simple header such as `include/wmux.h`, `src/esp_wiremux.c`, `include/esp_wiremux_console.h`, `src/esp_wiremux_console.c`, component `CMakeLists.txt`, and the ESP-IDF demo.
* Existing CMake component includes `esp_wiremux.c`, `esp_wiremux_console.c`, `esp_wiremux_frame.c`, and `esp_wiremux_log.c` plus portable core C sources.
* RX path today is already protocol-safe at the frame/envelope level; the missing part is endpoint-local buffering/session/consumer state after `channel_id` resolution.
* TUI side has tests around manifest-driven input modes and passthrough stream behavior; future host changes should preserve those semantics.

## Decision Log

### MVP Scope

**Context**: The endpoint model is needed now so beginner APIs and future VFS/UART-like adapters share a stable per-channel boundary.

**Decision**: Implement endpoint internals and the full `wmux_*` simple API in this task. Keep the existing advanced demo unchanged and add a new `esp_wiremux_console_simple_demo`.

**Consequences**: The task is larger than an internal-only refactor, but it gives users an immediate beginner-facing API. Public stream handle design should stay minimal this round so the next VFS task can shape the lower-level stream handle contract with real requirements.

### Simple API Prefix

**Context**: The simple API should be distinct from ESP-IDF advanced/core APIs and should remain available to future non-ESP platforms.

**Decision**: Use `wmux_*` for the simple API.

**Consequences**: The simple API should avoid ESP-IDF-only naming assumptions where practical. ESP-IDF-specific implementation can live in the ESP component, but the public naming should remain portable.

### Simple API Layers

**Context**: Beginner users need a tiny API surface, while ordinary users need a path to per-channel state before dropping down to the full advanced protocol API.

**Decision**: Provide both a limited global `wmux_*` quick-start API and an explicit channel-object API.

**Consequences**: The global API should be documented as a constrained entry point. Channel-object APIs become the recommended simple layer for per-channel configuration, receive queues, callbacks, and future stream-oriented adapters.

### Simple Demo Shape

**Context**: The global quick-start API has intentionally limited capability, while the explicit channel-object model is the user's next step for richer behavior.

**Decision**: Use separate example projects instead of putting both styles in one example project:

* `esp_wiremux_beginner_demo`: limited global quick-start API.
* `esp_wiremux_advanced_demo`: explicit channel-object simple API.
* `esp_wiremux_professional_demo`: renamed existing advanced protocol demo.

**Consequences**: The examples will be clearer for beginners and easier to evolve independently. The global API example should not imply feature parity with the channel-object API.

### Global Quick-Start Channel Registration

**Context**: A 3-5 line beginner API needs a usable default channel, but advanced/simple-channel users must keep control over their channel configuration.

**Decision**: Use lazy default registration. If no explicit channel has been registered/configured, global `wmux_*` operations auto-register channel 1 with default text stream behavior. If the user explicitly registers/configures channels first, global APIs do not auto-create or overwrite channel configuration.

**Consequences**: Quick-start remains tiny while explicit configuration takes precedence. Implementation must track whether a channel came from auto-registration or user configuration.

### Global Simple API Channel Arguments

**Context**: The simple API family should remain cohesive. Beginner and advanced demos teach different usage levels, but both are still `wmux_*` convenience APIs.

**Decision**: Provide both default-channel global helpers and explicit-channel global helpers. Default helpers operate on channel 1. Explicit-channel helpers allow calls such as `wmux_write_text_ch(channel, text)` without requiring users to move immediately to opaque channel handles.

**Consequences**: The API is larger, but it preserves a smooth learning curve: default channel helpers, explicit channel helpers, opaque channel handles, then advanced `esp_wiremux_*`.

### Endpoint Input Consumer Model

**Context**: Per-channel endpoint state needs a clear rule for whether incoming data goes to a blocking/readable queue, a callback, or both.

**Decision**: Use one active consumer per channel. Default simple API channels use a read queue. Registering a callback switches that channel to callback-only delivery. Advanced `esp_wiremux_*` APIs should make the selected consumer behavior explicit during registration/configuration.

**Consequences**: The behavior is easy to reason about and avoids duplicate consumption. Users who need both queue reads and event notification will need a future explicit fan-out feature rather than relying on implicit double delivery.

### Channel Object Handle Shape

**Context**: The channel-object API should help users move toward endpoint/stream concepts without exposing internal endpoint/session fields.

**Decision**: Use an opaque handle for `wmux_channel_*` APIs.

**Consequences**: Endpoint internals can change for VFS/UART-like work without breaking ABI. The API will likely look like `wmux_channel_handle_t ch` plus open/close/read/write functions, where the handle is owned by the component runtime.

### Simple API Return Style

**Context**: The simple API should be portable across ESP32 and future platforms such as Raspberry Pi, so callers should not need platform-specific error structures in the common path.

**Decision**: Use POSIX/Arduino-like return values for simple APIs. Read/write functions return a non-negative byte count on success and a negative `wmux` error code on failure. Lifecycle/configuration functions should also avoid `esp_err_t`; they can return `0` on success and a negative `wmux` error code on failure.

**Consequences**: Beginner API calls remain portable and easy to reason about. ESP-IDF-specific failures need to be mapped internally to `wmux` error codes, with detailed text available through `wmux_strerror()`.

### Simple Lifecycle API

**Context**: Beginners should be able to start with one call, while users learning the explicit channel API need a clear init/configure/start lifecycle.

**Decision**: Provide both lifecycle styles. `wmux_begin()` is the beginner API and is equivalent to initializing default config, calling `wmux_init(&config)`, then `wmux_start()`. The `esp_wiremux_advanced_demo` uses `wmux_config_init()`, `wmux_init(&config)`, and `wmux_start()` to show explicit setup.

**Consequences**: Beginner examples stay tiny, and advanced simple examples have a clear configuration window before starting the mux runtime.

### Simple Manifest Emission

**Context**: Beginner users benefit when host tools can discover channels immediately, while explicit simple users may need control over when manifest metadata is emitted.

**Decision**: `wmux_begin()` automatically emits the manifest after start. `wmux_config_t` includes `auto_manifest`; when enabled, `wmux_start()` emits the manifest for explicit simple lifecycle users.

**Consequences**: Beginner examples work with host discovery by default. Advanced simple users can disable auto emission and call the advanced manifest API or a simple manifest helper explicitly if they need precise timing.

### Simple Config Shape

**Context**: Global beginner users should not need to call `wmux_init()` directly. Users who do call `wmux_init()` are already in the explicit simple API path and can handle a config object.

**Decision**: Make `wmux_init()` take a `wmux_config_t` pointer. Provide `wmux_config_init()` for defaults. Do not add a no-argument `wmux_init()` overload in C.

**Consequences**: The lifecycle API remains explicit without duplicating init variants. `wmux_begin()` remains the no-argument beginner entry point.

### Channel Open Config Shape

**Context**: Users need both a low-friction way to get a channel handle and a path to configure per-channel details without switching to the advanced protocol API.

**Decision**: Provide both default and configured channel opens: `wmux_channel_open(channel_id, &handle)` and `wmux_channel_open_with_config(&config, &handle)`.

**Consequences**: The advanced simple demo can show explicit channel config, while common code can still open a channel in one line. The configured form should expose simple concepts only, such as channel id, name, queue depth, mode, and default timeout, while leaving protocol direction/payload-kind details hidden.

### Console Adapter Boundary

**Context**: Console support is important, but adding a public simple console API now would mix ESP-IDF console concepts into the portable `wmux_*` namespace.

**Decision**: Do not include a public simple console adapter API in this task. The beginner demo should avoid console-specific behavior. The advanced demo may use a small number of `esp_wiremux_*` / `esp_wiremux_console_*` calls if needed to demonstrate console integration alongside simple channel usage.

**Consequences**: `wmux_*` stays focused on portable stream/channel convenience. Console endpoint/session instance work can happen internally or in the existing `esp_wiremux_console_*` namespace without committing to a portable console API prematurely.

### Simple Layer Coupling Rule

**Context**: The project should preserve high cohesion and low coupling between beginner convenience APIs and the advanced protocol implementation.

**Decision**: Implement `wmux_*` as a convenience layer on top of the `esp_wiremux_*` core/advanced API contract. Endpoint internals may support both layers, but public simple APIs should not create a separate parallel protocol path.

**Consequences**: Advanced and simple APIs share one behavior model, one routing path, and one manifest/protocol implementation. This reduces drift while keeping the user-facing simple API portable.
