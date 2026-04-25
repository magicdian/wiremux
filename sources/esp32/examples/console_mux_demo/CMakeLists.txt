cmake_minimum_required(VERSION 3.16)

set(EXTRA_COMPONENT_DIRS
    "${CMAKE_CURRENT_LIST_DIR}/../../components/esp_serial_mux"
)

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(console_mux_demo)
