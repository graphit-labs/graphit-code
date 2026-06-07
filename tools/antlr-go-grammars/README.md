# ANTLR v4 Go Grammars

ANTLR grammars compiled to WASM via `GOOS=wasip1 GOARCH=wasm`.

Each subdirectory is a standalone Go module that produces a `.wasm` file for a single grammar.

## Adding a New Grammar

### 1. Create the grammar directory

```bash
mkdir -p tools/antlr-go-grammars/<name>/parser
cd tools/antlr-go-grammars/<name>
go mod init github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/<name>
go get github.com/antlr4-go/antlr/v4@v4.13.1
```

### 2. Generate the Go parser

Download the `.g4` grammar files from [grammars-v4](https://github.com/antlr/grammars-v4), then:

```bash
java -jar antlr-4.13.2-complete.jar -Dlanguage=Go -package parser -o parser *.g4
```

If the grammar has base classes (e.g., `PlSqlParserBase.java`), port them to Go in `parser/`.

### 3. Create the driver files

Copy and adapt from `plsql/`:

- **`main.go`** — IPC driver. Change the parser import and start rule:
  ```go
  // Change these lines:
  import ".../parser"
  lexer := parser.NewYourLexer(input)
  p := parser.NewYourParser(tokens)
  tree = p.Your_start_rule()  // e.g., p.CompilationUnit()
  ```

- **`serializer.go`** — Copy verbatim (shared across all grammars).

- **`preprocessor.go`** — Optional. Only needed for language-specific source normalization (e.g., PL/SQL strips `EDITIONABLE` keywords, injects missing semicolons). For most grammars, skip this and pass source directly to the parser.

### 4. Register in the Makefile

Add one line to the `build-antlr-grammars` target:

```makefile
build-antlr-grammars:
    # ... existing grammars ...
    $(call build_antlr_go_wasm,<name>)
```

### 5. Create the language YAML

```yaml
language: <name>
parser: antlr4
start_rule: <start_rule>
grammar: antlr-<name>
extensions: [".ext"]
queries:
  - data_key: functions
    graph_label: Function
    pattern: "//your_xpath"
    name_capture: "name_node"
exports:
  strategy: none
```

### 6. Build and test

```bash
make build-antlr-grammars
graphit sync
```

## Protocol

All grammars use the same length-prefixed IPC protocol over stdin/stdout:

```
Request:  [4 bytes BE uint32 length][source bytes]
Response: [4 bytes BE uint32 length][JSON parse tree]
```

The WASM module runs as a persistent loop reading requests until stdin closes.

## Directory Structure

```
tools/antlr-go-grammars/
├── README.md          ← this file
├── plsql/
│   ├── go.mod
│   ├── main.go        ← IPC driver + SLL→LL parsing
│   ├── preprocessor.go ← PL/SQL-specific normalization
│   ├── serializer.go   ← parse tree → JSON
│   ├── parser/         ← ANTLR4-generated Go parser
│   └── grammar/        ← .g4 source files
└── <new-grammar>/
    ├── go.mod
    ├── main.go
    ├── serializer.go
    └── parser/
```
