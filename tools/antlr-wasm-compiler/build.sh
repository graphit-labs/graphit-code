#!/usr/bin/env bash
# Build an ANTLR4 grammar into a WASI .wasm binary.
#
# Uses the official wasi-sdk CMake toolchain (wasi-sdk >= 33 required).
# Memory configuration is read from grammar.yaml in the grammar directory.
#
# Usage:
#   ./build.sh <grammar_name> <grammar_dir> <driver_source> [wasi_sdk_path]
#
# Example:
#   ./build.sh plsql grammars/plsql grammars/plsql/driver.cpp /opt/wasi-sdk
#
# Output: build/<grammar_name>/antlr-<grammar_name>  (WASM binary)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ $# -lt 3 ]; then
    echo "Usage: $0 <grammar_name> <grammar_dir> <driver_source> [wasi_sdk_path]"
    exit 1
fi

GRAMMAR_NAME="$1"
GRAMMAR_DIR="$(cd "$2" && pwd)"
DRIVER_SOURCE="$(cd "$(dirname "$3")" && pwd)/$(basename "$3")"
WASI_SDK="${4:-${WASI_SDK_PREFIX:-}}"

if [ -z "$WASI_SDK" ]; then
    echo "Error: WASI SDK path not provided. Set WASI_SDK_PREFIX or pass as 4th argument."
    exit 1
fi

TOOLCHAIN="${WASI_SDK}/share/cmake/wasi-sdk-p1.cmake"
if [ ! -f "$TOOLCHAIN" ]; then
    echo "Error: Official toolchain not found at ${TOOLCHAIN}"
    echo "       wasi-sdk >= 33 is required."
    exit 1
fi

# --- Per-grammar WASM memory configuration ---
# Read from grammar.yaml in the grammar directory. Values are in MB.
# Defaults are for simple grammars (JSON, YAML, small DSLs).
# Large grammars (PL/SQL, T-SQL) override these in their grammar.yaml.
GRAMMAR_CONFIG="${GRAMMAR_DIR}/grammar.yaml"

read_config() {
    local key="$1" default="$2"
    if [ -f "$GRAMMAR_CONFIG" ]; then
        local val
        val=$(grep "^  ${key}:" "$GRAMMAR_CONFIG" 2>/dev/null | awk '{print $2}' | head -1)
        echo "${val:-$default}"
    else
        echo "$default"
    fi
}

INITIAL_MEMORY_MB=$(read_config "initial_memory_mb" "4")
MAX_MEMORY_MB=$(read_config "max_memory_mb" "64")
STACK_SIZE_MB=$(read_config "stack_size_mb" "2")

INITIAL_MEMORY=$((INITIAL_MEMORY_MB * 1048576))
MAX_MEMORY=$((MAX_MEMORY_MB * 1048576))
STACK_SIZE=$((STACK_SIZE_MB * 1048576))

BUILD_DIR="${SCRIPT_DIR}/build/${GRAMMAR_NAME}"
mkdir -p "$BUILD_DIR"

echo "=== Building antlr-${GRAMMAR_NAME}.wasm ==="
echo "  Grammar dir:  $GRAMMAR_DIR"
echo "  Driver:       $DRIVER_SOURCE"
echo "  WASI SDK:     $WASI_SDK"
echo "  Toolchain:    $TOOLCHAIN"
echo "  Build dir:    $BUILD_DIR"
echo "  Memory:       initial=${INITIAL_MEMORY_MB}MB  max=${MAX_MEMORY_MB}MB  stack=${STACK_SIZE_MB}MB"
echo ""

cd "$BUILD_DIR"
cmake "$SCRIPT_DIR" \
    -DCMAKE_TOOLCHAIN_FILE="$TOOLCHAIN" \
    -DCMAKE_BUILD_TYPE=MinSizeRel \
    -DWASI_SDK_PREFIX="$WASI_SDK" \
    -DGRAMMAR_NAME="$GRAMMAR_NAME" \
    -DGRAMMAR_DIR="$GRAMMAR_DIR" \
    -DDRIVER_SOURCE="$DRIVER_SOURCE" \
    -DWASM_INITIAL_MEMORY="$INITIAL_MEMORY" \
    -DWASM_MAX_MEMORY="$MAX_MEMORY" \
    -DWASM_STACK_SIZE="$STACK_SIZE" \
    -Wno-dev \
    2>&1

NPROC=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
make -j"$NPROC" "antlr-${GRAMMAR_NAME}" 2>&1

OUTPUT="${BUILD_DIR}/antlr-${GRAMMAR_NAME}"
if [ -f "$OUTPUT" ]; then
    SIZE=$(du -h "$OUTPUT" | cut -f1)
    echo ""
    echo "=== Success: ${OUTPUT} (${SIZE}) ==="
else
    echo "Error: build failed — output not found at ${OUTPUT}"
    exit 1
fi
