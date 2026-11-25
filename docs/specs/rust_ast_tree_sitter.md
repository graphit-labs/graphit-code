---
title: Rust Tree-sitter engine
references:
  - target: {type: task, id: tsk-8121}
    relation: implements
  - target: {type: task, id: tsk-8aee}
    relation: implements
  - target: {type: task, id: tsk-50f7}
    relation: reuses
  - target: {type: task, id: tsk-e056}
    relation: reuses
---

# Rust Tree-sitter engine

`graphit_ast::tree_sitter_engine` parses unchanged UTF-8 with Tree-sitter 0.27.
`GrammarLanguage::from_language` accepts a statically linked grammar SDK;
`builtin(id)` acquires one of the 41 linked grammars. Construction validates
language compatibility with `Parser::set_language`.

`ParsedTree` and `CompiledQuery` each retain the same Arc-owned SDK language.
Queries require the exact language generation of their tree. Custom parsers
run through Docker images configured by extension in `ast.scip.parsers`; see
[parser resources](rust_ast_parser_resources.md).

```rust
use graphit_ast::tree_sitter_engine::{GrammarLanguage, ParseControl};

let language = GrammarLanguage::from_language(tree_sitter_json::LANGUAGE.into())?;
let document = language.parse("[1, 2]", ParseControl::default())?;
let query = language.compile_query("(number) @value")?;
let matches = document.matches(&query, ParseControl::default())?;
# Ok::<(), graphit_ast::tree_sitter_engine::EngineError>(())
```

Parsing creates a fresh SDK parser. `ParseControl` accepts an optional shared
atomic cancellation flag and deadline. Checks run before and after parsing,
during SDK progress, node traversal and query iteration. A cancellation or
elapsed deadline returns an error and discards the private result. These are
ordinary cooperative controls, without a claim of hard native resource bounds.

`GrammarLanguage::parse_with_context`, `ParsedTree::nodes_with_context` and
`ParsedTree::matches_with_context` accept the caller's original Core
`SnapshotContext` alongside `ParseControl`. Each method uses the same private
implementation as its context-free counterpart. The original context is
checked before optional control, during SDK progress, traversal, captures and
property collection, and before delivery (including empty queries). An original
operation failure is `EngineError::Operation(io::Error)` with its original
`Interrupted`/`TimedOut` kind and error cause; optional controls retain
`Cancelled`/`TimedOut`. Results assembled before failure are discarded. No new
context is manufactured. Generation mismatch and SDK match-limit errors retain
their existing meanings.

Syntax errors and missing nodes remain visible through `has_syntax_errors` and
the flattened `SyntaxNode` records. They are distinct from ABI, query
and control failures. Nodes include parent indexes, field names, named/error/
missing flags and half-open byte ranges. `SyntaxNode.node_id` and
`Capture.node_id` come directly from SDK `Node.id()`. They join a capture to
its native node in the **same retained tree**; they are not preorder ordinals,
persisted symbol UIDs or identities across parses. `SyntaxNode.parent` remains
a preorder index in that tree. Repeated queries on the retained tree preserve
these IDs and capture multiplicity. Capture text comes from the retained
original source. BOM, NUL and CRLF are not normalized. `source_range` validates
coordinates through the existing `SourceText`/`SourceRange` implementation.

`CompiledQuery` uses SDK compilation and text predicates, including `eq?`,
`match?` and `not-match?`. Captures retain names, exact text, byte ranges and byte
points; `set!` annotations are returned as property metadata. General predicates
and property conditions that require an external semantic handler are rejected
explicitly rather than treated as true. Queries cannot be applied to another
language generation. SDK query match overflow is an error, never truncated
success.

Catalog extraction and indexing consume the engine's parse/query records;
the low-level engine itself does not publish records into Data.

Focused validation uses the linked official JSON grammar for functional parsing,
UTF-8/CRLF, syntax diagnostics, predicates, capture metadata, retained generation,
cancellation/deadline and fresh parsing. The context suite uses linked HCL,
TOML and JavaScript; PostgreSQL body fixtures use linked PLpgSQL. Historical
native execution evidence retains its original platform and scope; current
changes require running the affected suites.

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test tree_sitter_engine --test tree_sitter_context --test antlr_engine --test antlr_semantic --test postgresql_semantic --jobs 1
cargo xtask cargo test --locked --offline -p graphit-ast --lib existing_host_is_reused_for_plpgsql_bodies --jobs 1
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test tree_sitter_engine --test tree_sitter_context --test antlr_engine --test antlr_semantic --test postgresql_semantic --jobs 1 -- -D warnings
```

For host/body projection and reuse preconditions, see
[ANTLR queries and embedded bodies](rust_ast_antlr_queries.md).
