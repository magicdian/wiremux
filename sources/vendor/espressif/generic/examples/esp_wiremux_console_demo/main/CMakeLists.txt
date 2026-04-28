idf_component_register(
    SRCS "esp_wiremux_console_demo_main.c"
    INCLUDE_DIRS "."
    REQUIRES
        console
        esp-wiremux
        esp_system
        freertos
        log
)
