#include <stdio.h>

#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "wmux.h"

static wmux_channel_handle_t s_control_channel;
static wmux_channel_handle_t s_data_channel;

static void control_reader_task(void *arg)
{
    (void)arg;
    uint8_t buffer[128];

    while (true) {
        int len = wmux_channel_read(s_control_channel, buffer, sizeof(buffer), 1000);
        if (len > 0) {
            (void)wmux_channel_write_text(s_control_channel, "control echo: ");
            (void)wmux_channel_write(s_control_channel, buffer, (size_t)len);
            (void)wmux_channel_write_text(s_control_channel, "\n");
        }
    }
}

void app_main(void)
{
    wmux_config_t mux_config;
    wmux_config_init(&mux_config);
    mux_config.queue_depth = 16;
    mux_config.max_payload_len = 512;
    mux_config.default_timeout_ms = 20;
    mux_config.auto_manifest = 0;

    int err = wmux_init(&mux_config);
    if (err < 0) {
        printf("wmux_init failed: %s\n", wmux_strerror(err));
        return;
    }

    wmux_channel_config_t control_config;
    wmux_channel_config_init(&control_config);
    control_config.channel_id = 1;
    control_config.name = "control";
    control_config.description = "Advanced simple control channel";
    control_config.queue_depth = 8;
    control_config.mode = WMUX_CHANNEL_MODE_LINE;

    err = wmux_channel_open_with_config(&control_config, &s_control_channel);
    if (err < 0) {
        printf("control open failed: %s\n", wmux_strerror(err));
        return;
    }

    wmux_channel_config_t data_config;
    wmux_channel_config_init(&data_config);
    data_config.channel_id = 3;
    data_config.name = "data";
    data_config.description = "Advanced simple data tick channel";
    data_config.mode = WMUX_CHANNEL_MODE_STREAM;

    err = wmux_channel_open_with_config(&data_config, &s_data_channel);
    if (err < 0) {
        printf("data open failed: %s\n", wmux_strerror(err));
        return;
    }

    err = wmux_start();
    if (err < 0) {
        printf("wmux_start failed: %s\n", wmux_strerror(err));
        return;
    }
    (void)wmux_emit_manifest();

    if (xTaskCreate(control_reader_task,
                    "wmux_control_reader",
                    3072,
                    NULL,
                    4,
                    NULL) != pdPASS) {
        printf("control reader task create failed\n");
        return;
    }

    uint32_t counter = 0;
    while (true) {
        char line[80];
        snprintf(line, sizeof(line), "advanced data tick=%lu\n", (unsigned long)counter++);
        (void)wmux_channel_write_text(s_data_channel, line);
        vTaskDelay(pdMS_TO_TICKS(2000));
    }
}
