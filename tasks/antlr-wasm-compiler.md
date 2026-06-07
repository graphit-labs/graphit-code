# ANTLR WASM Compiler — C++/WASI Migration

> ⚠️ **Superseded** — This C++ approach was replaced by a Go-based architecture
> using `GOOS=wasip1 GOARCH=wasm`. See `tasks/antlr-go-grammars.md`.

## Summary

Replaced the non-functional Java/TeaVM approach in `tools/antlr-wasm-compiler/` with
a working C++/WASI-SDK toolchain that compiles ANTLR4 grammars into standalone `.wasm`
binaries executable via wazero.

## Problem

The original plan used Java → TeaVM-WASI to compile ANTLR parsers to WASM. This was
abandoned because TeaVM-WASI had stability issues and added Java as a build dependency.
The user required a 100% plug-and-play solution like tree-sitter: drop a `.wasm` + `.yaml`.

## Solution

**C++ ANTLR4 runtime + wasi-sdk clang++ → WASI .wasm**

Three compatibility layers were needed because:
1. `wasip1` defines `_LIBCPP_HAS_NO_THREADS` — no `std::mutex`/`std::shared_mutex`
2. `wazero` does NOT support the WASM Exception Handling proposal
3. ANTLR4 C++ runtime depends on both threading primitives and C++ exceptions

### Files Created

| File | Purpose |
|------|---------|
| `CMakeLists.txt` | Generic parameterized build (GRAMMAR_NAME, GRAMMAR_DIR, DRIVER_SOURCE) |
| `wasi.cmake` | WASI-SDK toolchain — wasip1, 256MB/512MB memory, 8MB stack |
| `build.sh` | One-command build wrapper |
| `wasi_stubs/exception_stubs.cpp` | C++ EH ABI stubs (__cxa_throw → abort) |
| `wasi_stubs/threading_stubs.cpp` | pthread no-op weak symbol stubs |
| `runtime/json_serializer.h` | Parse tree → JSON serializer |
| `grammars/plsql/driver.cpp` | PL/SQL-specific driver (start rule: sql_script) |
| `grammars/plsql/*.cpp/*.h` | Generated ANTLR4 C++ parser/lexer for PL/SQL |
| `README.md` | Full documentation |
| `.gitignore` | Ignore build artifacts |

### Key Technical Decisions

- **8MB WASM stack** required — PL/SQL grammar init tables are huge (~12K lines of ATN data)
  and default 1MB stack caused `out of bounds memory access` in dlmalloc
- **Synchronization.h patched at CMake time** — injects `std::shared_mutex`/`unique_lock`/
  `shared_lock` aliases into `namespace std` so `Parser.cpp` compiles unchanged
- **Exception stubs abort on throw** — safe because ANTLR only throws on fatal errors
  (never during normal parsing)

## Validation

- ✅ Build succeeds from project directory: `./build.sh plsql grammars/plsql grammars/plsql/driver.cpp`
- ✅ Wasmtime: correct JSON parse tree for `SELECT * FROM employees WHERE salary > 50000 ORDER BY name;`
- ✅ Wazero: correct JSON parse tree, ~170ms parse time, 1.09s compile (cacheable)
- ✅ Binary size: 7.6MB
