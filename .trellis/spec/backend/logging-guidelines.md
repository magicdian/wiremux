# Logging Guidelines

> How logging is done in this project.

---

## Overview

ESP log integration is provided through `esp_log_set_vprintf()`. The adapter forwards formatted log text into a mux channel while optionally teeing to the previous ESP log backend.

## ESP Log Adapter Contract

Public API:

```c
void esp_wiremux_log_config_init(esp_wiremux_log_config_t *config);
esp_err_t esp_wiremux_bind_esp_log(const esp_wiremux_log_config_t *config);
esp_err_t esp_wiremux_unbind_esp_log(void);
```

Default config:

- `channel_id = 2`
- `max_line_len = 256`
- `write_timeout_ms = 0`
- `tee_to_previous = true`

## Required Behavior

- The vprintf callback must be re-entrant enough for ESP-IDF logging contexts.
- The adapter must guard against recursive entry after `esp_log_set_vprintf()` is installed. If mux transport or queue code indirectly triggers ESP logging, the callback must return without trying to enqueue another mux log frame.
- Use `va_copy()` before formatting or forwarding `va_list`.
- Do not call `ESP_LOGx` inside mux logging code.
- If line formatting exceeds `max_line_len`, truncate and send the bounded line.
- Queue failures must not crash the logging caller.

## Wrong vs Correct

Wrong:

```c
static int mux_log_vprintf(const char *fmt, va_list args)
{
    ESP_LOGI("mux", "forwarding log"); // recursive
    return vprintf(fmt, args);
}
```

Correct:

```c
va_list copy;
va_copy(copy, args);
int len = vsnprintf(buffer, sizeof(buffer), fmt, copy);
va_end(copy);
(void)esp_wiremux_write(...);
```

Correct recursion guard:

```c
static volatile bool s_in_mux_log_vprintf;

static int mux_log_vprintf(const char *fmt, va_list args)
{
    if (s_in_mux_log_vprintf) {
        return 0;
    }

    s_in_mux_log_vprintf = true;
    /* format and enqueue bounded log text */
    s_in_mux_log_vprintf = false;
    return formatted_len;
}
```

## Demo Expectations

The console demo should emit both telemetry and ESP log messages periodically. This keeps host-side filtering testable:

- `--channel 2` should show log adapter output.
- `--channel 3` should show telemetry output.
- No filter should show ordinary terminal output plus concise `chN> ` decoded
  mux record payloads; full frame metadata and batch summaries belong in the
  host diagnostics file.
