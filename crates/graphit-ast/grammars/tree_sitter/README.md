# Builtin Tree-sitter source bundles

These 39 generated parser/scanner bundles are compiled by `tree_sitter_build.rs`.
Rust 0.24.2 and JSON 0.24.8 use their pinned SDK crates and have no duplicated
bundle here. Cargo consumes only these repository sources and the SDK crates;
source preparation reused the already qualified bytes from tsk-0260.

Original C, scanner, support header, license and notice bytes are preserved.
External bundles retain their upstream source paths and relative includes;
local `.inc` filenames are compiled through generated C include wrappers.
Clojure retains upstream CC0 `COPYING.txt` and the divergent local MIT text.
Objective-C retains the exact upstream license alongside its local attribution
edit. Elixir retains its mixed-license NOTICE. Kotlin's four exit-to-abort edits,
Objective-C's include directive, and PL/pgSQL's three metadata edits remain
explicit source customizations; no normalization hides those changes.

See `docs/specs/rust_ast_grammar_provenance.md` for immutable bases, generation recipes and source limitations, and `docs/specs/rust_ast_parsers.md` for the literal builtin API and parser contracts. Operational test receipts are kept in Graphit Task rather than the grammar source tree.
