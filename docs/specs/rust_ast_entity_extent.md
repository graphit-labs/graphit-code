---
title: Rust original entity extent
doc_type: technical
references:
  - target: {type: task, id: tsk-072f}
    relation: implements
  - target: {type: task, id: tsk-e056}
    relation: reuses
  - target: {type: task, id: tsk-33fe}
    relation: uses
  - target: {type: task, id: tsk-b0cb}
    relation: derived_from
---

# Rust original entity extent

The library validates a supplied nonempty original span and derives its
inclusive Source line window. Seven functional fixtures and strict affected Clippy passed on Linux. Selecting a complete entity node, graph/raw
association, acquisition authorization and transport delivery remain caller
contracts.

Implementation: [entity_extent.rs](../../crates/graphit-ast/src/entity_extent.rs).
Fixtures: [entity extent tests](../../crates/graphit-ast/tests/entity_extent.rs).
Ranges: [source_ranges.rs](../../crates/graphit-ast/src/source_ranges.rs).
Consumers: [entity Source selection](rust_ast_entity_source.md) and the
[semantic extraction contract](rust_ast_semantic_extraction.md).

## API and ownership

```rust
impl<'o> EntityExtent<'o> {
    pub fn new(
        source: &SourceText<'o>,
        candidate: SourceRange<'o>,
        original: &SnapshotContext,
    ) -> Result<Self, EntityExtentError>;
    pub fn range(&self) -> SourceRange<'o>;
    pub fn start_line(&self) -> usize;
    pub fn end_line(&self) -> usize;
}
```

SourceText and SourceRange come from the existing original UTF-8 factories.
The result borrows the original bytes and has private fields. It owns no text,
native parser, capture or authority. The caller must supply the root original
SourceText and an extent selected by the actual backend delimiter contract.

EntityExtent validates only that supplied span. It does not prove the span
covers an entire declaration. A name anchor is not a substitute for full extent;
the semantic producer keeps anchor, extent and metric subtree separate.

Retain actual raw/physical/native/prepared owners and the original request
through semantic validation, selection, final authorization and delivery.
The numeric line window is a selector input, never a graph/content binding or
grant. Getters do not re-capture Source or restore an owner from metadata.

## Validation and line conversion

The function checks the original operation first, then rejects start >= end.
It obtains canonical SourceText.range(start,end), preserving its typed error.
The candidate and canonical range must have equal text pointer/length, byte
offsets and byte points. Equal bytes in another allocation are insufficient.
A subslice with incompatible relative offsets/points is rejected. A range in
the same allocation with exact root offsets/points is valid.

The function uses safe Rust, without pointer arithmetic, fabricated range
structs, source copying or normalization. BOM, CR, NUL and Unicode bytes remain
unchanged. SourceText's own factory handles UTF-8 boundaries and bounds.

Points use zero-based rows/byte columns. start_line is start.row + 1.
end_line is end.row if end.column is zero, otherwise end.row + 1.
Checked arithmetic also validates end_line >= start_line before returning.

For six-byte raw text `α\r\nb\n`:

| Byte range | Inclusive lines | Original text |
|---|---|---|
| 0..4 | 1..1 | α + CRLF |
| 0..5 | 1..2 | α + CRLF + b |
| 0..6 | 1..2 | Complete raw text; trailing empty third logical line excluded |
| 4..6 | 2..2 | b + LF |
| 3..4 | 1..1 | LF only |

A nonempty newline-only span can cover an empty logical line. An empty span,
including one in an empty file, cannot describe an entity extent.

Only actual Exact(SourceRange) projections may be supplied. Compound,
Synthetic, Unmappable and Ambiguous origin outcomes remain typed; callers must
not synthesize a hull or approximate line offset.

## Errors and original operation

EntityExtentError is EmptyRange, ForeignRange, Range(RangeError),
Operation(io::Error) or ArithmeticOverflow. Error::source preserves the actual
range/IO cause. Display reports the category/cause without printing source text.

The original context is checked before and after validation, including a failing
validation result. A failed check returns no partial extent. Range scans and
allocations inside SDKs remain synchronous/cooperative; this library claims no
hard preemption. The fixtures test pre-cancellation and an expired deadline,
not cancellation injected during the scan or authenticated transport delivery.

## Qualification

Seven fixture groups covering EE1–EE6 passed on Linux (7/7); strict affected
Clippy and scoped rustfmt passed. Five local links are valid. They cover
Unicode/CRLF/LF/EOF, empty versus newline-only spans, equal foreign allocations,
relative subslices, an actual UTF-8 range error, BOM/NUL bytes, actual
original cancellation/deadline causes, and composition with 33fe.

The composition uses a..j, original bytes4..14 -> window3..7. Core relative
range2..4/head2/tail1 selects global line5 and total_lines10.

Affected commands:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test entity_extent --jobs 1
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test entity_extent --jobs 1 -- -D warnings
rustfmt --check --edition 2024 crates/graphit-ast/src/entity_extent.rs crates/graphit-ast/tests/entity_extent.rs
```

Root coordinates the module export, warm target and ordinary Cargo queue.
This unit changes no manifests or lockfiles. The executed platform is Linux;
results are recorded in `.build/rust-ast-entity-extent/qualification.json`.
Full semantic extraction, publisher/lookup, Auth/App/e2e and
Windows/macOS remain unrun. Existing ANTLR C++ overloaded-virtual warnings
are preserved in the log, without grammar changes. The initial fixture compile
failure and corrected successful run remain separate in the literal
`qualification.json` file.
