---
title: Rust entity Source window selection
doc_type: technical
references:
  - target: {type: task, id: tsk-33fe}
    relation: implements
  - target: {type: task, id: tsk-db2c}
    relation: uses
  - target: {type: task, id: tsk-e11b}
    relation: reuses
  - target: {type: task, id: tsk-d02c}
    relation: derived_from
  - target: {type: task, id: tsk-2845}
    relation: implements
  - target: {type: task, id: tsk-db46}
    relation: implements
---

# Rust entity Source window selection

This library selects an already acquired raw UTF-8 Source window. Its seven functional tests
and strict affected Clippy passed on Linux. Core window selection is separately
qualified by db2c. Registered Native entity lookup and the Source MCP/client
composition use the captured imported graph API described below. Local PROJECT
acquisition uses its existing producer and resolver, qualified separately.

Implementation: [entity_source.rs](../../crates/graphit-ast/src/entity_source.rs).
Fixtures: [entity_source tests](../../crates/graphit-ast/tests/entity_source.rs).
Core semantics: [text selection](rust_text_selection.md).
Acquisition and retained caller ownership:
[authenticated Source handler](rust_ast_source_handler.md).

## API and ownership

```rust
pub fn select_entity_source(
    text: &str,
    window_start: usize,
    window_end: usize,
    selection: &graphit_core::text_slice::Request,
    original: &graphit_core::file_io::SnapshotContext,
) -> Result<
    graphit_core::text_slice::Result,
    graphit_ast::source_inspection::SourceInspectionError,
>;
```

The text is borrowed for the call and the Core result owns its output. Window
coordinates are selection inputs; they establish no entity identity, graph
association, physical capture or authorization. The function creates no new
request, queries no store and performs no physical fallback.

A consumer must first establish actual entity uniqueness and valid extent tied
to exactly the selected raw content. It retains its graph, IndexedSourceSnapshot
or physical acquisition owners and original request through selection and final
authorization. The authenticated Source handler's already formatted result does
not restore those dropped owners for a later entity lookup. The planning record
d02c retains those acquisition obligations; 2845 corrects the imported extent
consumer described below. The pure selector qualifies no acquisition route;
`ImportedGraph::entity_source` supplies the registered Native route below.

## Selector behavior

Core requires a strict 1-based inclusive window:
1 ≤ window_start ≤ window_end ≤ text.split('\n').count().
It rejects negative head/tail/start/end/before/after parameters and reversed
request ranges before clamping. Request start/end are relative to the window;
zero sentinels and positive overshoot follow the existing Core semantics.

The order is window, relative range, head, tail, then pattern. Pattern context
cannot escape the resulting selection. Returned total_lines describes the
original full file; start/end and match line numbers remain global. For a
10-line file a..j, window3..7, relative2..4, head2 and tail1 select line5 with
total_lines10.

Literal pattern matching is case-insensitive; explicit regex retains Core's
regex semantics and errors. Match markers and fragment separators come from
Core's shared formatter. Invalid metadata windows are errors, never clamped.
The caller must not substitute a hull of ambiguous or compound origin pieces.

UTF-8 is already guaranteed by &str. BOM, CR, NUL and multibyte characters are
preserved in the raw text selected by Core. Empty text has one empty logical
line; a trailing LF retains a final empty line. Normal formatted output follows
the existing Core rules, without source normalization or parsing.

## Original operation and errors

The function calls original.check before and after Core::apply_window.
Operation(io::Error) retains the real cancellation/deadline cause. Core errors
are retained as Selection(String). Failed checks return no partial selection.

These checks are cooperative. Regex execution and allocations are synchronous;
the library makes no hard preemption or allocation-recovery claim. Fixtures
cover pre-cancellation and an expired original deadline, not injected cancellation
inside the SDK or authenticated delivery.

## Qualification

Seven fixture groups passed on Linux (7/7). Strict affected Clippy and scoped
rustfmt passed; four local documentation links are valid. The final static
impact review found no functional fault.
These groups cover ES1–ES6: relative range/head/tail and global totals, defaults and
overshoot, bounded pattern context, regex/no-match/errors, invalid windows and
all six negative parameters, raw text/empty/trailingLF, and actual original
cancellation/deadline causes.

Affected commands:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test entity_source --jobs 1
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test entity_source --jobs 1 -- -D warnings
rustfmt --check --edition 2024 crates/graphit-ast/src/entity_source.rs crates/graphit-ast/tests/entity_source.rs
```

Root coordinates exports and the shared build target. This unit changes
no manifests or lockfiles. The executed platform is Linux; Windows/macOS remain
unrun. Existing ANTLR C++ overloaded-virtual warnings remain in the build log,
without grammar changes. Pure selector tests do not qualify stored entity
lookup, full indexing/queries or App transports. The affected qualification receipt is
`.build/rust-ast-entity-source/qualification.json`.

## Captured registered Native entity Source

[Imported entity acquisition](../../crates/graphit-ast/src/imported_graph/entity_source.rs)
adds `ImportedGraph::entity_source(runtime, path, entity, entity_type, selection,
options)`. Its receiver already retains the actual authenticated caller, registry
selection, artifact ID/concrete version, Native root, graph capture and Original.
The method captures the sibling `source.lance`, retains its actual SDK snapshot,
queries the same graph for uniqueness, acquires the indexed text only for a
valid candidate, then applies the Core window. It does not reopen another artifact,
clone authentication or obtain authority from cwd.
This associates the selected graph/raw pins in one Native payload; it promises
no atomic latest selection or global GC pin.

Eligible entity labels require STRING `name`/`path`, INT64 `start_line`/`end_line`,
BOOL `available` and STRING `extent_status` in the captured schema. Lookup only
considers rows with `available = true` and `extent_status = 'Exact'`. These are
the current producer fields; there is no old-format fallback. `line_number`
locates the entity's name and must not define the declaration's beginning.
Optional `entity_type` selects an
exact schema label. Unknown types and unsupported metadata are explicit errors.
Names and paths are query parameters; only schema-owned identifiers are quoted.
An exact name/path match across all eligible labels wins. Only zero exact matches
allow the native SDK's Unicode lowercase comparison. Two candidates, including
homonyms in different labels, are ambiguous; identical extents do not collapse
entity identities. No match is unavailable. Metadata must provide positive,
ordered, exactly represented integer coordinates within the retained text.

Available indexed raw text is required. Missing corpus, missing record or NULL
Source returns unavailable; corrupt storage/query data remains a failure. Empty
text remains available but still requires a valid metadata window. The method
never applies imported coordinates to unbound current workspace bytes. File-only
Source preserves its existing direct captured-disk fallback and all selectors.

Imported entity requests do not capture an unused explicit physical fallback
project. File-only fallback selection remains unchanged.

The Original, current caller and physical captures are checked before acquisition,
after SDK awaits/queries and before delivery. SDK/storage/query/selection causes
remain attached to `EntitySourceError`; public MCP diagnostics expose categories
without embedding source text or credentials. Native work uses the SourceServer's
existing owned blocking job and interruption/join lifecycle.

[SourceServer composition](../../crates/graphit-app/src/source_server/entity.rs)
uses that method when both `context` and `entity` are provided. The shipped CLI
arguments already transmit `--entity` and `--entity-type`; its `run_source` client
consumes the actual MCP result. The remote Native consumer uses the same extent
resolver with its retained graph and raw owners. Local PROJECT acquisition uses
the existing [PROJECT resolver](../../crates/graphit-ast/src/project_entity_source.rs),
with its producer's extent, availability and graph/raw association checks. Those
previously qualified producers and selectors are unchanged by the imported fix;
full local indexing is outside db46.

The new Native fixtures and causal qualification live in
[the existing graph fixture child](../../crates/graphit-app/tests/support/graph_server/entity_source.rs)
and `.build/rust-entity-source/`. The runner uses genuine Parquet graph metadata,
Lance raw records and the existing Native package publisher before registry
publication. Two distinct Native consumer/guard fixtures passed on Linux (runner06: 2/2,
0 ignored). They cover actual MCP and the CLI client, two registered versions,
exact-first and native Unicode lower matching, type filtering and ambiguity,
NULL/empty raw, invalid/null/reversed/negative/out-of-source metadata, safe query
parameters, relative selectors and global numbers, actual same-Original
cancellation/expiry, current runtime removal, Native root/graph replacement,
Local authentication, and ReadAst denial before malformed payload acquisition.
The file-only schema regression also passed; disk fallback and final scoped
Clippy results are recorded separately in the receipt. The compiled CLI process
and local project graph publisher are separate consumers. Prior pure selector
and file-only evidence does not substitute for the registered Native consumer.

## Imported extent correction (2845)

The current producer records the name's location in `line_number`, separately
from the declaration's inclusive `start_line`/`end_line`. For C source
`int\nmain(void){}`, the name is on line 2 and the Source window starts on line 1.
Both imported and remote Native lookup now use the latter coordinates and the
producer's availability/status gates. Invalid coordinates still fail rather
than being clamped or replaced with workspace text.

The existing Native MCP/CLI fixture above now publishes current Parquet fields and Lance raw records. It checks the complete two-line C declaration, unavailable and non-exact rows, while retaining homonyms, selectors and causal-error cases.
