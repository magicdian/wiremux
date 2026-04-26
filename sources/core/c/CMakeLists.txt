cmake_minimum_required(VERSION 3.20)

project(wiremux_core_c LANGUAGES C CXX)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_library(wiremux_core_c STATIC
    src/wiremux_batch.c
    src/wiremux_compression.c
    src/wiremux_envelope.c
    src/wiremux_frame.c
    src/wiremux_manifest.c
)

target_include_directories(wiremux_core_c
    PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}/include
)

if(CMAKE_C_COMPILER_ID MATCHES "Clang|GNU")
    target_compile_options(wiremux_core_c PRIVATE -Wall -Wextra -Werror)
endif()

include(FetchContent)

set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
set(BUILD_GMOCK ON CACHE BOOL "" FORCE)

FetchContent_Declare(
    googletest
    GIT_REPOSITORY https://github.com/google/googletest.git
    GIT_TAG v1.17.0
)

FetchContent_MakeAvailable(googletest)

enable_testing()

add_executable(wiremux_core_tests
    tests/wiremux_core_test.cpp
)

target_link_libraries(wiremux_core_tests
    PRIVATE
        wiremux_core_c
        GTest::gmock_main
)

if(CMAKE_CXX_COMPILER_ID MATCHES "Clang|GNU")
    target_compile_options(wiremux_core_tests PRIVATE -Wall -Wextra -Werror)
endif()

include(GoogleTest)
gtest_discover_tests(wiremux_core_tests)
