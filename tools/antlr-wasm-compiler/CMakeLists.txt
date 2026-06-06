cmake_minimum_required(VERSION 3.14)
project(antlr-wasm-compiler LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# --- Parameters ---
# GRAMMAR_NAME  : e.g. "plsql" — used for output binary name (antlr-<name>)
# GRAMMAR_DIR   : absolute path to directory containing generated .cpp/.h files
# DRIVER_SOURCE : path to the grammar-specific driver.cpp
if(NOT DEFINED GRAMMAR_NAME)
    message(FATAL_ERROR "GRAMMAR_NAME is required (e.g. -DGRAMMAR_NAME=plsql)")
endif()
if(NOT DEFINED GRAMMAR_DIR)
    message(FATAL_ERROR "GRAMMAR_DIR is required (path to generated parser sources)")
endif()
if(NOT DEFINED DRIVER_SOURCE)
    message(FATAL_ERROR "DRIVER_SOURCE is required (path to grammar driver.cpp)")
endif()

# --- Fetch ANTLR4 C++ Runtime ---
include(FetchContent)
FetchContent_Declare(
    antlr4-runtime
    URL https://github.com/antlr/antlr4/archive/refs/tags/4.13.2.tar.gz
    SOURCE_SUBDIR runtime/Cpp
)

FetchContent_GetProperties(antlr4-runtime)
if(NOT antlr4-runtime_POPULATED)
    FetchContent_Populate(antlr4-runtime)

    # Patch: treat WASI like Emscripten for C++17 detection
    file(READ "${antlr4-runtime_SOURCE_DIR}/runtime/Cpp/CMakeLists.txt" _antlr_cmake)
    string(REPLACE
        "CMAKE_SYSTEM_NAME MATCHES \"Emscripten\")"
        "CMAKE_SYSTEM_NAME MATCHES \"Emscripten\" OR CMAKE_SYSTEM_NAME MATCHES \"WASI\")"
        _antlr_cmake "${_antlr_cmake}")
    file(WRITE "${antlr4-runtime_SOURCE_DIR}/runtime/Cpp/CMakeLists.txt" "${_antlr_cmake}")

    # Patch: stub Threads::Threads CMake target.
    # wasi-sdk 33 provides real pthread symbols in libc, but CMake's find_package(Threads)
    # doesn't know about WASI's threading model.
    file(READ "${antlr4-runtime_SOURCE_DIR}/runtime/Cpp/runtime/CMakeLists.txt" _rt_cmake)
    string(REPLACE
        "find_package(Threads REQUIRED)"
        "if(NOT TARGET Threads::Threads)\n  add_library(Threads::Threads INTERFACE IMPORTED)\nendif()"
        _rt_cmake "${_rt_cmake}")
    file(WRITE "${antlr4-runtime_SOURCE_DIR}/runtime/Cpp/runtime/CMakeLists.txt" "${_rt_cmake}")

    set(ANTLR4_INSTALL OFF CACHE BOOL "" FORCE)
    set(ANTLR_BUILD_CPP_TESTS OFF CACHE BOOL "" FORCE)
    set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)

    add_subdirectory("${antlr4-runtime_SOURCE_DIR}/runtime/Cpp" "${antlr4-runtime_BINARY_DIR}")
endif()

# --- Grammar sources ---
file(GLOB GRAMMAR_SOURCES "${GRAMMAR_DIR}/*.cpp")

# --- Exception stubs (WASI only) ---
# WASM EH (-fwasm-exceptions) is not stable with LTO in wasi-sdk 33 (see
# WebAssembly/wasi-sdk#565). Instead, we compile with -fno-exceptions and
# provide link-time stubs for __cxa_throw etc. The NoThrowErrorStrategy in the
# driver prevents 99%+ of throws; any remaining ones (deep ATN simulator
# errors on extremely malformed SQL) will abort() and the Go engine restarts.
if(CMAKE_SYSTEM_NAME STREQUAL "WASI")
    set(WASI_STUBS
        ${CMAKE_CURRENT_SOURCE_DIR}/wasi_stubs/exception_stubs.cpp
    )
endif()

# --- Build target ---
set(OUTPUT_NAME "antlr-${GRAMMAR_NAME}")

add_executable(${OUTPUT_NAME}
    ${DRIVER_SOURCE}
    ${GRAMMAR_SOURCES}
    ${WASI_STUBS}
)

target_include_directories(${OUTPUT_NAME} PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/runtime
    ${GRAMMAR_DIR}
    ${antlr4-runtime_SOURCE_DIR}/runtime/Cpp/runtime/src
)

target_link_libraries(${OUTPUT_NAME} PRIVATE antlr4_static)

# Per-grammar WASM memory limits — passed from build.sh via grammar.yaml config.
# Defaults are for simple grammars. Large grammars (PL/SQL) override in grammar.yaml.
if(NOT DEFINED WASM_INITIAL_MEMORY)
    set(WASM_INITIAL_MEMORY 4194304)  # 4MB default
endif()
if(NOT DEFINED WASM_MAX_MEMORY)
    set(WASM_MAX_MEMORY 67108864)     # 64MB default
endif()
if(NOT DEFINED WASM_STACK_SIZE)
    set(WASM_STACK_SIZE 2097152)      # 2MB default
endif()

# LTO + size optimization for WASI release builds
if(CMAKE_SYSTEM_NAME STREQUAL "WASI")
    target_compile_options(${OUTPUT_NAME} PRIVATE -Os -flto)
    target_link_options(${OUTPUT_NAME} PRIVATE
        -flto -Wl,--gc-sections
        -Wl,--initial-memory=${WASM_INITIAL_MEMORY}
        -Wl,--max-memory=${WASM_MAX_MEMORY}
        -Wl,-z,stack-size=${WASM_STACK_SIZE}
    )
endif()

