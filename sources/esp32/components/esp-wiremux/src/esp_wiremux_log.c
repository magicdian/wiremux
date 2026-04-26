#include "esp_wiremux_log.h"

#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "esp_log.h"
#include "esp_wiremux.h"

static esp_wiremux_log_config_t s_log_config;
static vprintf_like_t s_previous_vprintf;
static bool s_log_bound;
static volatile bool s_in_mux_log_vprintf;

static int mux_log_vprintf(const char *fmt, va_list args)
{
    int previous_result = 0;
    if (s_log_config.tee_to_previous && s_previous_vprintf != NULL) {
        va_list previous_args;
        va_copy(previous_args, args);
        previous_result = s_previous_vprintf(fmt, previous_args);
        va_end(previous_args);
    }

    if (s_in_mux_log_vprintf) {
        return previous_result;
    }

    s_in_mux_log_vprintf = true;

    size_t max_line_len = s_log_config.max_line_len > 0 ? s_log_config.max_line_len : 256;
    char *line = malloc(max_line_len);
    if (line == NULL) {
        s_in_mux_log_vprintf = false;
        return previous_result;
    }

    va_list format_args;
    va_copy(format_args, args);
    int formatted = vsnprintf(line, max_line_len, fmt, format_args);
    va_end(format_args);

    if (formatted > 0) {
        size_t len = (size_t)formatted;
        if (len >= max_line_len) {
            len = max_line_len - 1;
            line[len] = '\0';
        }
        (void)esp_wiremux_write(s_log_config.channel_id,
                                   ESP_WIREMUX_DIRECTION_OUTPUT,
                                   ESP_WIREMUX_PAYLOAD_KIND_TEXT,
                                   0,
                                   (const uint8_t *)line,
                                   len,
                                   s_log_config.write_timeout_ms);
    }

    free(line);
    s_in_mux_log_vprintf = false;
    return previous_result != 0 ? previous_result : formatted;
}

void esp_wiremux_log_config_init(esp_wiremux_log_config_t *config)
{
    if (config == NULL) {
        return;
    }

    memset(config, 0, sizeof(*config));
    config->channel_id = 2;
    config->max_line_len = 256;
    config->write_timeout_ms = 0;
    config->tee_to_previous = true;
}

esp_err_t esp_wiremux_bind_esp_log(const esp_wiremux_log_config_t *config)
{
    if (config == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    const esp_wiremux_channel_config_t channel = {
        .channel_id = config->channel_id,
        .name = "log",
        .description = "ESP-IDF log adapter",
        .directions = ESP_WIREMUX_DIRECTION_OUTPUT,
        .default_payload_kind = ESP_WIREMUX_PAYLOAD_KIND_TEXT,
        .flush_policy = ESP_WIREMUX_FLUSH_HIGH_WATERMARK,
        .backpressure_policy = ESP_WIREMUX_BACKPRESSURE_DROP_OLDEST,
        .output_policy = {
            .send_mode = ESP_WIREMUX_SEND_BATCHED,
            .compression = ESP_WIREMUX_COMPRESSION_HEATSHRINK,
            .batch_interval_ms = 100,
            .batch_max_bytes = 384,
        },
    };

    esp_err_t err = esp_wiremux_register_channel(&channel);
    if (err != ESP_OK) {
        return err;
    }

    s_log_config = *config;
    s_previous_vprintf = esp_log_set_vprintf(mux_log_vprintf);
    s_log_bound = true;
    return ESP_OK;
}

esp_err_t esp_wiremux_unbind_esp_log(void)
{
    if (!s_log_bound) {
        return ESP_ERR_INVALID_STATE;
    }

    (void)esp_log_set_vprintf(s_previous_vprintf);
    s_previous_vprintf = NULL;
    s_log_bound = false;
    return ESP_OK;
}
