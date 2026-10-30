cmake_minimum_required(VERSION 3.22.1)

project(playintegrityfix)

find_package(cxx REQUIRED CONFIG)

link_libraries(cxx::cxx)

enable_language(C CXX ASM)

set(PIF_VENDOR_DIR "${CMAKE_CURRENT_SOURCE_DIR}" CACHE PATH "Pinned engine dependencies")
set(shadowhook_SOURCE_DIR "${PIF_VENDOR_DIR}/shadowhook")
include_directories(${PIF_VENDOR_DIR})

file(GLOB SHADOWHOOK_SRC
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/*.c
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/common/*.c
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/third_party/xdl/*.c
)

if(CMAKE_ANDROID_ARCH_ABI STREQUAL "armeabi-v7a")
    file(GLOB SHADOWHOOK_ARCH_SRC ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/arch/arm/*.c ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/arch/arm/*.S)
    include_directories(${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/arch/arm)
elseif(CMAKE_ANDROID_ARCH_ABI STREQUAL "arm64-v8a")
    file(GLOB SHADOWHOOK_ARCH_SRC ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/arch/arm64/*.c ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/arch/arm64/*.S)
    include_directories(${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/arch/arm64)
endif()

include_directories(
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/common
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/include
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/third_party/bsd
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/third_party/lss
    ${shadowhook_SOURCE_DIR}/shadowhook/src/main/cpp/third_party/xdl
)

add_library(${CMAKE_PROJECT_NAME} SHARED main.cpp companion.cpp config.cpp property_hook.cpp java_injection.cpp observation.cpp ${PIF_VENDOR_DIR}/local_cxa_atexit_finalize_impl/atexit.cpp ${SHADOWHOOK_SRC} ${SHADOWHOOK_ARCH_SRC})

target_link_libraries(${CMAKE_PROJECT_NAME} PRIVATE log)

set(PIF_MODULE_DIR "/data/adb/modules/duck-toolbox/play-integrity" CACHE STRING "Engine payload directory")
set(PIF_DATA_DIR "/data/adb/play-integrity" CACHE STRING "Engine state directory")
target_compile_definitions(${CMAKE_PROJECT_NAME} PRIVATE
    PIF_MODULE_DIR="${PIF_MODULE_DIR}"
    PIF_DATA_DIR="${PIF_DATA_DIR}"
)

set(DUCK_RUNTIME_DIR "" CACHE PATH "Embedded Zygisk composition source")
set(DUCK_FAKER_LIBRARY "" CACHE FILEPATH "ARM64 Device Faker static library")
if(DUCK_RUNTIME_DIR)
    target_sources(${CMAKE_PROJECT_NAME} PRIVATE
        ${DUCK_RUNTIME_DIR}/entry.cpp ${DUCK_RUNTIME_DIR}/engine.cpp ${DUCK_RUNTIME_DIR}/channel.cpp)
    target_include_directories(${CMAKE_PROJECT_NAME} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
    target_compile_definitions(${CMAKE_PROJECT_NAME} PRIVATE DUCK_EMBEDDED_ENGINES=1)
    if(CMAKE_ANDROID_ARCH_ABI STREQUAL "arm64-v8a")
        if(NOT EXISTS "${DUCK_FAKER_LIBRARY}")
            message(FATAL_ERROR "Build the embedded Device Faker static library first")
        endif()
        target_compile_definitions(${CMAKE_PROJECT_NAME} PRIVATE DUCK_HAS_DEVICE_FAKER=1)
        target_link_libraries(${CMAKE_PROJECT_NAME} PRIVATE "${DUCK_FAKER_LIBRARY}" dl m)
        target_link_options(${CMAKE_PROJECT_NAME} PRIVATE -Wl,--exclude-libs,ALL)
    endif()
endif()
