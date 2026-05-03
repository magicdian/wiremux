idf_component_register(
    SRCS "esp_wiremux_beginner_demo_main.c"
    INCLUDE_DIRS "."
    REQUIRES
        esp-wiremux
        esp_system
        freertos
)
