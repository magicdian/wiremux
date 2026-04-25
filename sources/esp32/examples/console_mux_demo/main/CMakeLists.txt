idf_component_register(
    SRCS "console_mux_demo_main.c"
    INCLUDE_DIRS "."
    REQUIRES
        console
        esp_serial_mux
        esp_system
        freertos
        log
)
