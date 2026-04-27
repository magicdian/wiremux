#pragma once

#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"
#include "esp_wiremux.h"
#include "wiremux_manifest.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    ESP_WIREMUX_CONSOLE_MODE_DISABLED = WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
    ESP_WIREMUX_CONSOLE_MODE_LINE = WIREMUX_CHANNEL_INTERACTION_LINE,
    ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH = WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH,
} esp_wiremux_console_mode_t;

typedef enum {
    ESP_WIREMUX_PASSTHROUGH_BACKEND_RAW_CALLBACK = WIREMUX_PASSTHROUGH_BACKEND_RAW_CALLBACK,
    ESP_WIREMUX_PASSTHROUGH_BACKEND_CONSOLE_LINE_DISCIPLINE = WIREMUX_PASSTHROUGH_BACKEND_LINE_DISCIPLINE,
    ESP_WIREMUX_PASSTHROUGH_BACKEND_ESP_REPL = WIREMUX_PASSTHROUGH_BACKEND_REPL,
} esp_wiremux_passthrough_backend_t;

typedef struct {
    uint8_t channel_id;
    esp_wiremux_console_mode_t mode;
    esp_wiremux_passthrough_backend_t passthrough_backend;
    esp_wiremux_passthrough_policy_t passthrough_policy;
    esp_wiremux_input_handler_t passthrough_raw_handler;
    void *passthrough_raw_user_ctx;
    const char *name;
    const char *prompt;
    size_t input_queue_size;
    size_t output_queue_size;
    uint32_t write_timeout_ms;
} esp_wiremux_console_config_t;

void esp_wiremux_console_config_init(esp_wiremux_console_config_t *config);

esp_err_t esp_wiremux_bind_console(const esp_wiremux_console_config_t *config);

esp_err_t esp_wiremux_console_run_line(const char *line, int *command_ret);

#ifdef __cplusplus
}
#endif
