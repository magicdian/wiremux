#include <stdio.h>

#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "wmux.h"

static void on_default_channel_rx(uint8_t channel_id,
                                  const uint8_t *data,
                                  size_t len,
                                  void *user_ctx)
{
    (void)channel_id;
    (void)user_ctx;

    (void)wmux_write_text("echo: ");
    (void)wmux_write(data, len);
    (void)wmux_write_text("\n");
}

void app_main(void)
{
    int err = wmux_begin();
    if (err < 0) {
        printf("wmux_begin failed: %s\n", wmux_strerror(err));
        return;
    }

    err = wmux_on_receive(WMUX_DEFAULT_CHANNEL, on_default_channel_rx, NULL);
    if (err < 0) {
        printf("wmux_on_receive failed: %s\n", wmux_strerror(err));
        return;
    }

    (void)wmux_write_text("beginner demo ready\n");

    uint32_t counter = 0;
    while (true) {
        char line[64];
        snprintf(line, sizeof(line), "beginner tick=%lu\n", (unsigned long)counter++);
        (void)wmux_write_text(line);
        vTaskDelay(pdMS_TO_TICKS(2000));
    }
}
