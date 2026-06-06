# ANTLR4 → WASI WASM Compiler

Compiles ANTLR4 v4 C++ parsers into standalone WASI `.wasm` binaries that run in
wazero (pure Go) with zero external dependencies.

## Architecture

```
.g4 files ──► antlr4 tool ──► C++ Lexer/Parser ──► wasi-sdk clang++ ──► .wasm
                                                         │
                                    ANTLR4 C++ Runtime ──┘
                                    (fetched at build time)
```

Each grammar produces a self-contained `.wasm` binary that:
- Reads source code from **stdin**
- Outputs a JSON parse tree to **stdout**
- Requires no runtime dependencies (no Java, no shared libs)

## Prerequisites

- **wasi-sdk** ≥ 33.0 — [Download](https://github.com/WebAssembly/wasi-sdk/releases)
- **CMake** ≥ 3.14
- **Make** or Ninja
- **wazero** ≥ 1.12.0 (Go runtime)

Set `WASI_SDK_PREFIX` to your wasi-sdk installation path.

## Quick Start

```bash
# Build the PL/SQL grammar
export WASI_SDK_PREFIX=/path/to/wasi-sdk
./build.sh plsql grammars/plsql grammars/plsql/driver.cpp

# Test with wasmtime (or any WASI runtime)
echo 'SELECT 1 FROM dual;' | wasmtime run build/plsql/antlr-plsql
```

## Adding a New Grammar

1. Generate C++ sources from `.g4` files using the ANTLR4 tool:
   ```bash
   java -jar antlr-4.13.2-complete.jar -Dlanguage=Cpp -visitor -listener YourGrammar.g4
   ```

2. Create a grammar directory with all generated + auxiliary files:
   ```
   grammars/yourgrammar/
   ├── YourGrammarLexer.cpp
   ├── YourGrammarLexer.h
   ├── YourGrammarParser.cpp
   ├── YourGrammarParser.h
   ├── YourGrammarLexer.g4       # optional, for reference
   ├── YourGrammarParser.g4      # optional, for reference
   └── driver.cpp                # grammar-specific driver
   ```

3. Create `driver.cpp` for your grammar:
   ```cpp
   #include <iostream>
   #include <sstream>
   #include "antlr4-runtime.h"
   #include "YourGrammarLexer.h"
   #include "YourGrammarParser.h"
   #include "json_serializer.h"

   int main() {
       std::ostringstream ss;
       ss << std::cin.rdbuf();
       antlr4::ANTLRInputStream input(ss.str());
       YourGrammarLexer lexer(&input);
       lexer.removeErrorListeners();
       antlr4::CommonTokenStream tokens(&lexer);
       YourGrammarParser parser(&tokens);
       parser.removeErrorListeners();
       parser.setBuildParseTree(true);
       auto *tree = parser.startRule();  // your grammar's entry rule
       graphit::treeToJSON(std::cout, tree, parser.getRuleNames(), parser.getVocabulary());
       std::cout << std::endl;
       return 0;
   }
   ```

4. Build:
   ```bash
   ./build.sh yourgrammar grammars/yourgrammar grammars/yourgrammar/driver.cpp
   ```

## JSON Parse Tree Format

```json
{
  "rule": "compilationUnit",
  "start": [1, 0],
  "end": [10, 1],
  "children": [
    {
      "rule": "statement",
      "start": [1, 0],
      "end": [1, 20],
      "children": [
        {"token": "SELECT", "text": "SELECT", "start": [1, 0], "end": [1, 5]},
        ...
      ]
    }
  ]
}
```

- Positions: `[line, column]` — line is 1-indexed, column is 0-indexed
- Rule nodes have `rule`, `start`, `end`, `children`
- Terminal nodes have `token` (vocabulary name), `text`, `start`, `end`
- EOF tokens are excluded

## WASI Compatibility

The build uses the **official wasi-sdk toolchain** (`wasi-sdk-p1.cmake`) with one
compatibility layer:

| Layer | File | Purpose |
|-------|------|---------|
| Exceptions | `wasi_stubs/exception_stubs.cpp` | C++ EH ABI stubs (`__cxa_throw` → `abort()`) |

ANTLR's `DefaultErrorStrategy` handles parse errors via `ErrorListener` without
throwing exceptions. The stubs satisfy the linker for EH symbols that libc++ references
but are never invoked during normal parsing. If one IS called (e.g. `BailErrorStrategy`
or OOM), it aborts cleanly.

**Threading and synchronization** are handled natively by wasi-sdk 33's libc (pthread
stubs) and libc++ (`std::mutex`, `std::shared_mutex`).

Two CMake patches are applied at build time:
1. Treat WASI like Emscripten for ANTLR's C++17 detection
2. Stub the `Threads::Threads` CMake target (CMake's `find_package(Threads)` doesn't
   recognize WASI's threading model)

## Performance

Built with LTO (`-flto`) and `-Os` for optimized size and execution:

| wazero Mode | Compile | Run | Total | Notes |
|---|---|---|---|---|
| Compiler (JIT) | 3.0s | **490ms** | 3.5s | Compilation cached on disk |
| Interpreter | 158ms | 16.4s | 16.5s | — |

Compiler mode is **33x faster** at runtime. The 3s compilation cost is paid once
and cached by wazero's `CompilationCache`.

## Directory Structure

```
├── CMakeLists.txt         # Generic build — parameterized by GRAMMAR_NAME/DIR/DRIVER
├── build.sh               # One-command build wrapper (uses official wasi-sdk toolchain)
├── wasi_stubs/
│   └── exception_stubs.cpp
├── runtime/
│   └── json_serializer.h  # Parse tree → JSON serializer
├── grammars/
│   └── plsql/             # PL/SQL grammar (reference implementation)
│       ├── driver.cpp
│       ├── PlSqlLexer.*
│       └── PlSqlParser.*
└── build/                 # Build output (gitignored)
```
