#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint8_t channel_id;
    size_t max_line_len;
    uint32_t write_timeout_ms;
    bool tee_to_previous;
} esp_wiremux_log_config_t;

void esp_wiremux_log_config_init(esp_wiremux_log_config_t *config);

esp_err_t esp_wiremux_bind_esp_log(const esp_wiremux_log_config_t *config);

esp_err_t esp_wiremux_unbind_esp_log(void);

#ifdef __cplusplus
}
#endif
