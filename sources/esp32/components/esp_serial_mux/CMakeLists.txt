idf_component_register(
    SRCS
        "src/esp_serial_mux.c"
        "src/esp_serial_mux_console.c"
        "src/esp_serial_mux_frame.c"
        "src/esp_serial_mux_log.c"
    INCLUDE_DIRS "include"
    REQUIRES console esp_driver_usb_serial_jtag esp_system esp_timer log freertos
)
