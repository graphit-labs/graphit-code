# ANTLR Go Grammars — Documentation & Comment Cleanup

## Summary

Cleaned up all documentation, comments, CI workflows, and memory files to reflect
the Go wasip1 ANTLR architecture (replacing the old C++ wasi-sdk approach).
Created a README template to make adding new grammars trivial.

## Changes

### Documentation
- `docs/specs/ast_module.md`: Rewrote ANTLR WASM Architecture section (C++ → Go wasip1)
- `docs/guides/user_manual.md`: Updated ANTLR compilation instruction

### CI Workflows
- `.github/workflows/ci.yml`: Removed dead `Set up wasi-sdk` step from build-check
- `.github/workflows/release.yml`: Replaced wasi-sdk job with Go setup for build-antlr-grammars

### Code Comments
- `internal/ast/wasmantlr/engine.go`: Removed redundant/obvious comments, kept protocol docs
- `tools/antlr-go-grammars/plsql/main.go`: Trimmed inline comments, kept package doc
- `tools/antlr-go-grammars/plsql/serializer.go`: Removed C++ serializer reference
- `tools/antlr-go-grammars/plsql/preprocessor.go`: Removed dead `PreprocessForDisplay`, trimmed comments

### Makefile
- Simplified ANTLR section header — removed C++ comparison comments

### New Files
- `tools/antlr-go-grammars/README.md`: Step-by-step guide for adding new grammars

### Legacy
- `tools/antlr-wasm-compiler/README.md`: Added deprecation banner
- `tasks/antlr-wasm-compiler.md`: Added superseded notice

### Memories Updated
- `ANTLR_v4_Grammar_System_Architecture.md`: Full rewrite (C++ → Go stack)
- `ANTLR_WASM_PL-SQL_Parser_-_Build_and_Performance.md`: Updated build section
- `ANTLR_two-stage_SLLLL_parsing_standard.md`: Updated to BailErrorStrategy + panic/recover

## Validation

- All CI workflows reference only Go toolchain for ANTLR builds
- No remaining C++/wasi-sdk references in active code or documentation
- `tools/antlr-wasm-compiler/` deprecated with clear redirect
