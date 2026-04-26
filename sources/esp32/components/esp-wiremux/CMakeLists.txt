idf_component_register(
    SRCS
        "../../../core/c/src/wiremux_batch.c"
        "../../../core/c/src/wiremux_compression.c"
        "../../../core/c/src/wiremux_frame.c"
        "../../../core/c/src/wiremux_envelope.c"
        "../../../core/c/src/wiremux_manifest.c"
        "src/esp_wiremux.c"
        "src/esp_wiremux_console.c"
        "src/esp_wiremux_frame.c"
        "src/esp_wiremux_log.c"
    INCLUDE_DIRS
        "include"
        "../../../core/c/include"
    REQUIRES console esp_driver_usb_serial_jtag esp_system esp_timer log freertos
)
