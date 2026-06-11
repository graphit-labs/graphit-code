# COBOL 85 ANTLR Grammar Integration

## Summary

Added COBOL 85 language support to the ANTLR grammar system, enabling AST indexing
of COBOL programs (.cob, .cbl, .cpy, .cobol files).

## Changes

### Grammar Files
- Downloaded `Cobol85.g4` from [antlr/grammars-v4](https://github.com/antlr/grammars-v4/tree/master/cobol85)
- Downloaded `Cobol85Preprocessor.g4` from the same repository
- Generated Go parser code using ANTLR 4.13.2 (Go target)
  - Main grammar: `internal/ast/antlr/cobol85/` package
  - Preprocessor grammar: `internal/ast/antlr/cobol85/preprocessor/` subpackage

### Preprocessor (faithful Go port of proleap Java reference)
- `preprocessor.go` — Implements the full 4-stage pipeline from the Java reference:
  1. **LineReader** — parses raw text into CobolLine structs using FIXED-format regex
  2. **LineIndicatorProcessor** — processes indicator column (continuation, comments, debug)
  3. **CommentEntriesMarker** — handles multi-line comment entries (AUTHOR., etc.)
  4. **LineWriter** — serializes processed lines back to text
- COPY/REPLACE expansion (stage 5) intentionally skipped (requires copybook file resolution)

### Driver & Parser
- `driver.go` — GrammarDriver implementation
- `parser_sll_ll.go` — Two-stage SLL→LL parser

### Registration
- Registered `antlr-cobol85` driver in `antlr_adapter.go`
- Created `internal/ast/queries/cobol85.yaml` with comprehensive entity extraction
- Updated Makefile `vet` exclusions

## Supported Entities
- Programs (PROGRAM-ID)
- Sections (Procedure Division sections)
- Paragraphs
- Data items / variables
- File descriptions (FD/SD)
- Condition names (88-level)

## Supported Relations
- PERFORM → paragraph/section calls (CALLS)
- CALL → external program references (CALLS)
- GO TO → paragraph references (CALLS)
- READ/WRITE/REWRITE/DELETE → file I/O (SELECTS/INSERTS/UPDATES/DELETES)
- OPEN/CLOSE → file references (REFERENCES)
- COPY → copybook references (REFERENCES)
