# PR2: ESP-IDF Component Skeleton

## Goal

Create the ESP32 firmware-side component under `sources/esp32` with static channel capacity, mux service task structure, frame encoder boundary, and basic output API.

## Requirements

* Source code lives under `sources/esp32`.
* Provide ESP-IDF component structure.
* Define init/start/stop APIs.
* Define static channel registry and manifest model.
* Add service task, queues, flush policy, and frame encoder skeleton.
* Add an ESP-IDF example demo project for testing and demonstration.

## Acceptance Criteria

* [x] ESP-IDF component skeleton exists under `sources/esp32`.
* [x] Public C headers define init/config/channel APIs.
* [x] Component can be built in an ESP-IDF project once dependencies are available.
* [x] Example demo project exists under `sources/esp32/examples`.

## Out of Scope

* Console/log adapters.
* Host TUI.
