---
title: "Rust AST and Parser Contracts"
description: "Source ranges and the AST producer boundaries."
content-type: reference
audience: developers
references:
  - target: {type: task, id: tsk-24b6}
    relation: implements
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: task, id: tsk-0006}
    relation: derived_from
  - target: {type: task, id: tsk-e056}
    relation: derived_from
  - target: {type: task, id: tsk-08ae}
    relation: derived_from
  - target: {type: task, id: tsk-3eb5}
    relation: derived_from
  - target: {type: task, id: tsk-04ab}
    relation: derived_from
  - target: {type: task, id: tsk-50f7}
    relation: derived_from
  - target: {type: task, id: tsk-b4df}
    relation: derived_from
  - target: {type: task, id: tsk-2cf8}
    relation: derived_from
  - target: {type: task, id: tsk-c847}
    relation: derived_from
  - target: {type: task, id: tsk-13ae}
    relation: derived_from
  - target: {type: task, id: tsk-c10c}
    relation: derived_from
  - target: {type: task, id: tsk-ba35}
    relation: derived_from
  - target: {type: task, id: tsk-9d9d}
    relation: derived_from
  - target: {type: task, id: tsk-b0cb}
    relation: derived_from
  - target: {type: task, id: tsk-a90a}
    relation: derived_from
  - target: {type: task, id: tsk-a183}
    relation: derived_from
  - target: {type: task, id: tsk-e11b}
    relation: derived_from
  - target: {type: task, id: tsk-bcf6}
    relation: derived_from
  - target: {type: task, id: tsk-c775}
    relation: validated_by
  - target: {type: task, id: tsk-0395}
    relation: validated_by
  - target: {type: task, id: tsk-e4a9}
    relation: validated_by
  - target: {type: task, id: tsk-8aee}
    relation: implements
  - target: {type: task, id: tsk-113b}
    relation: implements
  - target: {type: task, id: tsk-f664}
    relation: derived_from
  - target: {type: task, id: tsk-fb73}
    relation: implements
  - target: {type: task, id: tsk-b197}
    relation: validated_by
  - target: {type: task, id: tsk-afac}
    relation: implements
  - target: {type: task, id: tsk-3f5d}
    relation: derived_from
---

# Rust AST and Parser Contracts

This page describes AST producer APIs and their current integration boundaries
in `graphit-code`, session `ses-c98a`, delivery `tsk-7858`. Rust owns the migrated
product APIs. Parser libraries, grammars and bindings are reused directly;
Each supported language has a selected parser.

See the [AST specification](ast_module.md),
[grammar guide](../guides/ast_extensibility.md),
[embedded parsing specification](embedded_language_parsing.md), and
[Tree-sitter engine](rust_ast_tree_sitter.md).

## Ownership and integration

AST owns its source, parser fixtures, tests and these contracts. The workspace
coordinator applies shared Cargo manifests, lockfiles, exports and lint policy
serially. Core, FS, Runtime, Auth, Data/Lance, AI, Knowledge and SCIP retain their
own implementations; AST consumes their delivered APIs.

The parser contract uses ordinary ownership, RAII, input validation, typed
errors, cancellation and deadlines. Linked native grammars use the documented
ABI and retained SDK lifetimes. Hard native resource proofs,
universal grants, reservations, quotas and mandatory process isolation are not
parser prerequisites. They were cancelled in the current session direction.
Historical private qualification receipts are not current acceptance gates.

## Language coverage

Preserve **44 top-level languages and 45 profiles: 40 Tree-sitter and 5 ANTLR**.
The 41 Tree-sitter grammars include opt-in Markdown without a shipped profile.
Automatic discovery covers 39 Tree-sitter profiles and COBOL85. Plain `.sql`
uses Tree-sitter SQL; Oracle PL/SQL, PostgreSQL, DB2 and T-SQL require explicit
grammar selection. The exclusive `plpgsql` profile supports PostgreSQL embedded
routine bodies.

The five ANTLR starts remain PL/SQL `sql_script`, PostgreSQL `root`, DB2
`db2_file`, TSQL `tsql_file`, and COBOL85 `startRule`. Generated recognizers must
use the real grammar rules, semantic predicates and actions. Parsing preserves
source bytes and recovery diagnostics; strict validation is explicit.
COBOL Fixed, Variable and Tandem preprocessing stages 1–4 and the DB2 procedure
control-flow grammar remain active requirements. COPY/REPLACE expansion is
outside the active preprocessing pipeline.

## Builtin native grammar acquisition

`tree_sitter_engine::builtin(id)` accepts the literal effective grammar name
from `CatalogDraft`, such as `tree-sitter-rust`, `tree-sitter-c_sharp` or
`tree-sitter-plpgsql`. `BUILTIN_GRAMMAR_NAMES` lists the 41 keys in the executable.
C# additionally accepts the explicit `tree-sitter-c-sharp` alias; selection does
not normalize case, underscores or paths. Unknown keys return
`BuiltinGrammarError::Unsupported`; `Native(EngineError)` preserves an SDK
validation error and its cause.

`tree_sitter_build.rs` has one closed descriptor list. Cargo uses mature `cc`
1.6.0 with C11 to build 39 vendored parser/scanner bundles under
`crates/graphit-ast/grammars/tree_sitter/`, preserving original headers and
license/notice bytes. Tree-sitter Rust 0.24.2 and JSON 0.24.8 use their pinned
SDK crates, avoiding duplicate C exports. The helper derives Rust dispatch,
extern declarations, literal keys and native ABI fixture expectations from
that same list. Build inputs are repository sources and pinned Cargo packages. Their provenance and explicit local patches are documented in
[grammar provenance](rust_ast_grammar_provenance.md).

The distributed runtime artifact is the Graphit executable with all 41
Tree-sitter grammars and the existing five custom ANTLR C++ dialects linked
statically. Intermediate native archives stay in Cargo `OUT_DIR`. Builtin
acquisition takes no filesystem path, native export name, pointer or YAML
library location. Its narrow unsafe boundary calls only actual generated
Tree-sitter exports, then `GrammarLanguage::from_language` validates the SDK
language. Custom parsers run in Docker images selected by extension through
`ast.scip.parsers`; see [parser resources](rust_ast_parser_resources.md).

Native builtin code has the lifetime of the executable. Tree and Query retain
ordinary language owners after the acquiring provider is dropped. Reloading
profiles/queries can preserve an existing borrower; it does not unload or
replace builtin machine code. Consumer factories prepare the operation's
current profile/configuration and retain grammar/query owners, without caching
Original requests, Auth, readers or source bodies. Cooperative cancellation
and deadlines preserve their original causes and do not claim a hard RSS or
preemptive CPU bound.

`tests/tree_sitter_builtin.rs` defines 41 positive native samples, including
opt-in Markdown, and prepares the 40 actual Tree-sitter profiles from
`crates/graphit-ast/profiles/queries/`. It checks native query/capture ranges, UTF-8 and CRLF,
Original identity projections, retained Tree/Query owners and causal failures.
The private builtin tests compare actual SDK ABIs with the source catalog and
exercise literal selection. Native execution exposed three invalid shipped
anonymous-function kind names: Julia now selects `arrow_function_expression`,
Lua `function_definition` (the anonymous expression, retaining named
`function_declaration`), and Objective-C `block_literal`. These are actual named,
visible kinds in the distributed grammars; Lua `_function_body` is hidden and
is not a replacement. Other queries, data keys and context policies are
preserved. The focused fixtures include calls inside real closures and verify
that their native trigger participates in `ContextSource::Anonymous`; the old
invalid names must still produce `UnknownAnonymousKind`.

The original compact Kotlin fixture with semicolons produced native recovery (`has_syntax_errors=true`) without an explicit error/missing node in the flattened view. This observed limitation remains recorded; no parser bytes or error flags were changed. The positive fixture now uses ordinary multiline Kotlin. ANTLR generation, preprocessing and native consumer results reuse their delivered producers; complete semantic models and daemon consumer integration remain their own deliveries.

## Producer APIs

| Module | Contract and integration boundary |
|---|---|
| `source_ranges` | Borrowed UTF-8 text, half-open byte ranges and byte points; implemented contract below. |
| `profiles`, `catalog` | Effective parser/grammar/start-rule and extraction configuration, inherited query patterns, capture names and relation metadata. |
| `prepared_input` | Original/prepared coordinates and origin projection; reuse its mapping model for preprocessing. |
| `symbols` | Document/entity identity construction used by extraction. |
| `tree_sitter_engine` | Actual SDK parsing, syntax nodes, compiled queries and capture results with retained language ownership. |
| `antlr_engine` | Five generated C++/cxx recognizers, typed semantic hooks and token/tree conversion qualified in Main; full extraction/index dispatch remains separate. |

### Profile, catalog and identity APIs

`tsk-3f5d` reconciles these pure producers. MAIN affected tests passed;
Clippy and scoped formatting passed; all seven durable checks passed after
final documentation review.

- ProfileDecoder::decode(input, &profile_limits, &original_context).
- CatalogDraft::build(layers, &catalog_limits, &original_context).
- SelectionPolicy::new(allow, deny, &catalog_limits, &original_context).
- Embedded::mapped_language(captured, &catalog_limits, &original_context).
- CatalogDraft::resolve_file(selector, &policy, &original_context).
- IdentityDocument::new(input, identity_limits, &original_context).
- document.symbol(label, subject, owners), using its retained original context.

WireUsage retains per-profile input/representation validation; catalog limits
bound only supplied policies and selectors. CatalogUsage and aggregate catalog
quotas are removed. serde_saphyr options budget bounds SDK structure. The operation ledger and
pointer-budget binding have been removed. Typed YAML, presence/null, ordered
overlays, provenance, selection, opt-in and embedded behavior remain.
Identity preserves UID wire, document/owner borrows, original ranges, checked
u64 offsets and BoundedWriter count/cap/first_error with real fallible allocation.
Normal context checks preserve cancellation and deadline.
MAIN construction uses SnapshotContext::new(deadline,Cancellation) -> Self,
as published by Core2.

Continuous operations may use Core's `SnapshotContext::service(cancellation)`
with no deadline. AST scope checks use `ScopeContext::from_snapshot`, preserving
the same cancellation and optional deadline. Native Cypher maps no deadline to
the SDK's documented timeout-disabled value; finite positive durations remain
positive and expired originals remain errors. Its observer always responds to
cancellation or completion and joins before native connection/IO destruction.
This context carries no authorization; caller admission still precedes reads
and is revalidated before delivery.

See [decoder](rust_ast_profiles.md), [catalog](rust_ast_profile_catalog.md) and
[identity](rust_ast_symbols.md) for full contracts and affected evidence.

## Borrowed source ranges

This implemented library uses only the standard library and performs no filesystem
access or authority construction. The text is borrowed; a lazy LF index is owned
by `SourceText` and shared by its cheap `Clone` views. `SourceRange` stays `Copy`
and borrows only the original text, independently of the view and its index.

```text
SourceText<'a>::new(bytes: &'a [u8], max_bytes: usize)
    -> Result<SourceText<'a>, RangeError>
BytePoint { row: usize, column: usize }
SourceText::point_at(byte: usize) -> Result<BytePoint, RangeError>
SourceText::byte_at(point: BytePoint) -> Result<usize, RangeError>
SourceText::scalar_byte(index: usize) -> Result<usize, RangeError>
SourceText::range(start: usize, end: usize) -> Result<SourceRange<'a>, RangeError>
SourceRange::text() -> &'a str
SourceRange::start()/end() -> usize
SourceRange::start_point()/end_point() -> BytePoint
```

`SourceText` privately borrows the original `&str`. Construction checks a positive
limit, then input length, then `std::str::from_utf8`. It preserves BOM, CRLF and
lone CR; invalid bytes are rejected, never replaced.

Points use zero-based rows and **byte** columns. LF advances the row; CR is an
ordinary byte. `point_at` checks bounds before UTF-8 boundaries and accepts EOF.
`byte_at` checks that the row exists and that its column does not cross LF or EOF,
then checks the UTF-8 boundary. EOF following LF belongs to a real empty final row.

`scalar_byte` maps a Unicode scalar ordinal in the **original document** to a byte
offset. The ordinal equal to the scalar count is EOF. It is neither a grapheme
index, a UTF-16 index nor automatically an ANTLR runtime index.

Ranges are half-open `[start, end)`. Validation checks reversed endpoints, bounds,
then UTF-8 boundaries. Empty ranges, including EOF, are valid. `SourceRange`
privately stores the borrowed subslice, global byte offsets and validated global
points. Its getters are infallible. It can outlive the wrapper but not the original
bytes, and cannot be applied to another source.

| Operation | Error precedence |
|---|---|
| `new` | `InvalidLimit`, `InputTooLarge`, `InvalidUtf8 { valid_up_to: usize, error_len: Option<usize> }` |
| `point_at` | `OutOfBounds`, `NotCharBoundary` |
| `byte_at` | `InvalidPoint` for missing row, excessive column or overflow; then `NotCharBoundary` |
| `range` | `ReversedRange`, `OutOfBounds`, `NotCharBoundary` |
| `scalar_byte` | `OutOfBounds` beyond the scalar count |

`RangeError` implements `Display` and `Error` without raw source text or paths.
Invalid inputs must not panic, clamp offsets or hide overflow. UTF-8 validation
is O(n). The first point conversion scans the source once to build the LF index;
subsequent `point_at` calls are O(log lines), and `byte_at` uses direct row lookup.
`scalar_byte` still scans Unicode scalars up to the requested ordinal. Consumers
should reuse a validated `SourceText` within an operation instead of validating
the same bytes for every node. Cancellation and deadlines belong to that operation.

### Range fixtures

F01–F12 are required outcomes exercised by the focused Linux test run below.

| Fixture | Input, operations and expected output |
|---|---|
| F01 | Empty bytes, limit 1: point 0 is `(0,0)`, scalar 0 is 0, range `0..0` is empty. |
| F02 | `a\nb`, length 3: points 1/2/3 are `(0,1)/(1,0)/(1,1)`; point conversion is invertible; `0..2` is exactly `a\n`. |
| F03 | `a\r\nb\r`, length 5: points 2/3/5 are `(0,2)/(1,0)/(1,2)`; CR bytes remain intact. |
| F04 | `é中🦀e\u{0301}`, length 12: boundaries `0,2,5,9,10,12`. BOM plus `a`: boundaries `0,3,4`. |
| F05 | `[C3]` produces UTF-8 error `{0,None}`; `[80]` gives `{0,Some(1)}`; `[61,FF]` gives `{1,Some(1)}`; `[ED,A0,80]` gives `{0,Some(1)}`. |
| F06 | `éx`: point 1 is `NotCharBoundary`, point 4 is `OutOfBounds`; point `(0,1)` splits UTF-8, `(0,4)/(1,0)` are invalid. Range `(2,1)` is reversed, `0..4` is out of bounds, `1..2` splits UTF-8, `3..3` is empty. |
| F07 | F04 scalar ordinals `0..=5` map to `0,2,5,9,10,12`; 6 is out of bounds. BOM plus `a` maps to `0,3,4`. `🦀` scalar ordinal 1 is EOF byte 4. |
| F08 | All valid character boundaries round-trip; one-scalar slices match original text, interior bytes reject. |
| F09 | `usize::MAX` byte/scalar indices and range end are out of bounds; extreme points are invalid; no panic. |
| F10 | `é` with limit 2 succeeds; limit 1 is too small; limit 0 is invalid. `[FF]` with limit 0 rejects the limit before encoding. |
| F11 | Borrowed pointer equality; range survives wrapper lifetime while bytes live. Compile-fail mutates original bytes and uses the range afterwards so NLL cannot shorten the borrow first. |
| F12 | `x\n` range `0..2` preserves the LF, end point is `(1,0)`, EOF range `2..2` is empty. |

The ranges producer has recorded Linux evidence for 12 focused integration
cases, its normal and compile-fail documentation examples, and Clippy. The
current Tree-sitter engine is integrated in the workspace with nine functional
tests and affected Clippy passing against the current Core implementation.
Dependencies, exports and the narrow unsafe lint boundary were integrated by
the workspace coordinator. Windows and macOS native execution have not been run.

## Tree-sitter

The implementation uses Tree-sitter 0.27.0 and tree-sitter-language 0.1.8.
`GrammarLanguage::from_language` accepts a statically linked grammar SDK;
`builtin(id)` supplies the linked catalogue. `Parser::set_language` validates
compatibility. Trees and queries retain an Arc-owned language after its provider
drops.

`parse(&str, ParseControl)` creates a fresh SDK parser. Syntax errors/missing
nodes remain inspectable; cancellation, deadline, ABI and invalid-range
failures are errors. `ParsedTree::nodes` returns kind, parent, field, named/error/
missing flags and exact byte ranges/points. `CompiledQuery` and `matches` use
SDK compilation and predicate evaluation. Active `eq?`, `match?` and
`not-match?` patterns retain their semantics. Unsupported external semantic
predicates and property conditions are rejected explicitly; `set!` returns
metadata. Captures keep their names and exact original text. Query overflow
returns an error rather than truncated success.

Nodes and capture iteration use the retained original UTF-8 without revalidating
the whole document per item. `source_range` is an explicit checked conversion
through the ranges producer. Catalog extraction consumes these records; the
low-level engine does not publish to Data.

### Native ownership and operation controls

`GrammarLanguage` retains its statically linked SDK language in an Arc owner.
Queries require the exact retained language generation of their tree. Each
parse creates a fresh SDK parser. Native grammar code has executable lifetime.

ANTLR's input, lexer, token stream, parser and tree pointers remain local to the
C++ call. The generated `cxx` boundary returns owned records and translates C++
exceptions through `Result`; Rust preserves the native cause and original control
error. Stopped operations return no partial records. Recovery remains inspectable
until the caller explicitly requests strict validation.

`parse_with_context` checks the original Core request in Tree-sitter's progress
callback; ANTLR carries that same context through its operation control.
`ParseControl` checks, native callbacks and query limits are cooperative controls.
They do not establish hard native memory/CPU caps or native crash recovery.
Consumers must retain their observations and original authorization/context
through their own final delivery and publication checks. Whole extraction/indexing
and CLI/MCP integrations require their own evidence.

## ANTLR and prepared origins

The five recognizers use the project's customized local G4 inputs, generated
C++ and the official ANTLR4.13.2 runtime through a small `cxx` boundary. Upstream
bases provide references without replacing local grammar rules. Generation is a
maintenance step. Source qualification in `tsk-113b` passed18 Linux fixtures for
all five families, actions/predicates, origins and the real PL/pgSQL splice;
[the ANTLR contract](rust_ast_antlr.md) records APIs, provenance and limits.
Root publication `tsk-fb73` and MAIN qualification `tsk-b197` remain separate.

`PreparedInput` relates unchanged original bytes to prepared bytes using an
`OriginPlan`; `project_prepared` returns exact, compound,
synthetic, ambiguous or unmappable projections. A parser adapter must convert
SDK scalar/token coordinates with checked arithmetic and preserve inclusive
stops, EOF, BOM, CRLF and synthetic spans. A line-offset approximation is not
an origin map. The official C++ SDK strips exactly one initial BOM; the Rust
adapter converts its scalar endpoints to physical prepared UTF-8 bytes and then
uses `project_prepared`. EOF and unindexed recovery observations remain explicit.
Obsolete SDK-specific mapping types were retired in `tsk-afac`; the current
consumer binds its syntax to the actual prepared bytes and preserves every origin
piece or alternative. Preprocessing emits actual transformations and origins,
without normalization that conceals invalid source.

`PreparedInput::new(original_bytes, prepared_bytes, plan, &context)` borrows its
sources, plan and actual `graphit_core::file_io::SnapshotContext`. Construction
validates UTF-8, complete prepared/original coverage, direct byte equality,
atoms, removed spans and anchors. Projections use `&self`; there is no mutable
mapping ledger. Empty sources remain valid. Input-derived `len().max(1)` bounds
satisfy the ranges producer's positive bound without adding an operation quota.
The context supplies ordinary cancellation and deadlines.

PostgreSQL `AS` dollar-quoted `LANGUAGE plpgsql` bodies feed the actual supplied
Tree-sitter grammar. Concrete C++ base actions/predicates inspect real host rules
and tokens; body observations retain their host-parent index and scalar bounds.
The Rust splice preserves native trees/queries, their grammar owners,
original byte ranges and points. `parse_with_context` checks the original Core
operation in the Tree-sitter progress callback alongside caller ParseControl.
Single/E'/U&' literals, SET dollar values and other languages remain host syntax.
Tree-sitter's existing C bindings remain appropriate for its C API.

## Extraction, indexing and AST services

`tsk-b0cb` integrates capture configuration, deterministic identities, entities,
relationships, exports, values, documentation, complexity and embedded origins.
Calls/imports/inheritance, implementations, field access, DML, custom relations
and ambiguity rules remain functional requirements. Embedded/preprocessed
results retain original bytes, source ranges and host diagnostics. SCIP owns
its own symbol handoff.

`tsk-a90a` integrates full/incremental/watch indexing and AST source/query/schema/
search/sample/neighborhood services. Reuse FS ignore/re-inclusion and captured
source APIs. Preserve changed/deleted inputs, indexed generations, source
freshness, partial outcomes and atomic publication with the Data writer.
Cypher, typed parameters, null ordering, traversal, cursors, FTS/semantic/hybrid
search and sample/neighborhood operations use the delivered Data/AI engines.
Those producer interfaces remain real integration prerequisites.

Source inspection retains the selected indexed generation, including imported
contexts. Requirement N1 adds physical reading only when the caller explicitly
classifies indexed text as missing. Available indexed text, including an empty
file, takes precedence over current filesystem text. Database, authorization,
corruption and stale-generation errors must propagate rather than become missing
text. Physical reading uses a capture of the requested relative file under the
physical instance already resolved by Runtime, with the original request and
authorization. An imported context without a resolved, authorized local instance
has no physical fallback; it cannot substitute the current workspace.

The pure Source producer in `tsk-e11b` is implemented in
[`source_inspection.rs`](../../crates/graphit-ast/src/source_inspection.rs).
`inspect_source` accepts `IndexedSource::Available(&str)` or explicit `Missing`,
an optional borrowed `PhysicalSource`, Core's `text_slice::Request`, and the
original `SnapshotContext`. It returns `SourceInspection` with `SourceOrigin`
and Core's selection result. Available indexed text never examines the physical
capture. On a miss, the reader must belong to the same original request; its path
is relative to the capture, with the empty path selecting a captured file.
The producer retains the bounded Core read through strict UTF-8 and selection,
then revalidates the physical content and request before delivery. It preserves
BOM, CRLF and UTF-8 NUL. Any extension, including an unknown extension or no
extension, is readable without a parser or reindexing. A capture is not an Auth grant:
MCP/CLI integration must establish authorization and physical project/version
binding through the actual owning APIs before providing it.

`Missing` without a capture returns `PhysicalUnavailable`; it never searches the
cwd. `Operation` and `PhysicalRead` retain the original IO error and cause;
`InvalidUtf8` retains the UTF-8 error, `Selection` retains Core's error text,
and `UnrepresentableLength` rejects a file length that cannot fit `usize`.
For example, invalid UTF-8 fails instead of returning sanitized or partial text;
an indexed acquisition error must be returned by the caller instead of being
classified as `Missing`. Original request checks bracket selection. Core's
selector allocations and regex execution do not provide recoverable OOM or
hard cancellation inside that helper; IO allocation errors propagate when the
actual reader returns them.

Selector composition and literal/regex filtering reuse Core's text-slicing
implementation: line range, head, tail, then pattern. Its current contract rejects
negative selectors and reversed ranges and explicitly clamps positive line
ranges to the available text. Source byte coordinates remain strictly validated.
Entity selection still requires the actual symbol/query producer and a range
bound to the selected content; a missing graph cannot be replaced by parsing the
physical file. MCP/CLI acquisition and entity integration remain separate pending
work; this library does not qualify either user-facing command. Entity requests
must reach the actual owning query producer, which must reject absent or ambiguous
symbols rather than invent an entity window from the physical file.

Main Linux functional qualification passed all 12 Source fixtures in
`.build/rust-source-inspection/functional.log`. Strict affected Clippy passed in
`clippy-qualified.log` after the coordinator resolved the shared lockfile.
The initial lock rejection before compilation is preserved in
`clippy-lock-failure.log`. Scoped formatting and local documentation links passed.
Windows/macOS and the caller integrations above remain unqualified.

The new fixtures exercise cancellation before the call. During/last-read
cancellation and deadline behavior reuse the qualified Core2 reader fixtures
recorded by `tsk-10bd`, and the Source checks before return are inspected in the
implementation. This evidence does not claim a new Source timing injection or
preemption inside Core's selector. The qualification receipt preserves these
distinct scopes in `.build/rust-source-inspection/qualification.json`.

Affected library qualification uses these commands:

```text
cargo xtask cargo test -p graphit-ast --test source_inspection --jobs 1
cargo xtask cargo clippy -p graphit-ast --lib --test source_inspection --jobs 1 -- -D warnings
rustfmt --edition 2024 --check crates/graphit-ast/src/source_inspection.rs crates/graphit-ast/tests/source_inspection.rs
```

### Indexed raw Source snapshots

The new [indexed_source.rs](../../crates/graphit-ast/src/indexed_source.rs) reader
queries an already acquired `graphit_data::lance_module_store::ModuleSnapshot`.
`read_branch_source(&BranchSnapshot, path, &original).await` reads the exact
captured local branch through the same schema, scanner and row decoder. It
does not reopen latest, materialize an inherited target or resolve versions.
The private snapshot owner retains either actual SDK type.
The caller selects and authorizes that corpus; this reader does not resolve
projects, versions, imported contexts or filesystem locators. Its Main export
and seven Linux SDK fixtures are qualified; caller integration is pending.

`source_schema()` defines the complete new Arrow schema: `path: Utf8` is
non-nullable and `source: Utf8` is nullable, in that order. Metadata is
`graphit.module=ast-source` and `graphit.ast.source.format=1`. Different fields,
types, nullability or metadata fail. Raw UTF-8 preserves BOM, CRLF, NUL and
empty text. No legacy BM25 document is decoded. The producer must publish this
format when storing indexed Source.

`read_indexed_source(&snapshot, path, &original).await` returns an owned
`IndexedSourceSnapshot`, retaining the actual Dataset and original
`Arc<SnapshotContext>`. Its `path()` and `version()` identify the key and observed
SDK generation. `branch_observation()` is absent for a Module snapshot, whose
API does not expose branch identity. A Branch snapshot returns the SDK's
`physical_branch` and `inherited_target`: Default has both absent, an inherited
target has only `inherited_target`, and a named branch has `physical_branch`.
These observations grant no authority. `source(&same_original)` checks Arc identity and cancellation/
deadline, then borrows `IndexedSource` for `inspect_source`. The caller retains
this owner through selection and revalidates its scope/admission before delivery.

An empty string is `Available("")`. Zero rows are `Missing` with
`MissingSourceReason::FileAbsent`; a matching NULL is `Missing` with
`TextNotStored`. NULL records unavailable stored text in this generation,
including source storage disabled at publication. Corpus absence is outside
this reader. SDK/schema/row failures propagate and do not permit fallback.
Duplicate paths fail, including duplicate NULL rows. Empty or NUL-containing
input keys fail as `InvalidPath`. Other keys remain exact, without trimming,
normalization or SQL interpolation.

The query uses Lance's literal expression filter and at most two rows. Success
for zero or one row requires EOF; a late stream failure cannot become partial
success. `Operation` and `Sdk` preserve their underlying causes. Request checks
surround SDK awaits and row processing; they do not promise hard preemption of
SDK work or allocations. Dataset retention is not a GC pin, authorization
grant or binding to a graph generation.

The [SDK fixtures](../../crates/graphit-ast/tests/indexed_source.rs) cover raw
text/reopen, NULL/absence, literal keys/duplicates, schema mismatches, publication
generations, two corpora with the same path, original request checks and real
storage disappearance. All seven Main Linux SDK fixtures passed, including
removal of a referenced data file while keeping the corpus manifest present.
The private overlay references these same physical Source/test files and the
Main Lance Foundation Source. Strict affected Clippy passed in both Main and
the overlay. Four additional branch fixtures passed in Main, for eleven total:
Default/inherited reads without branch creation, materialized content and old
captures across publication, shared raw/schema/absence handling and original
request checks. Their setup explicitly materializes and publishes through the
SDK; the reader performs neither operation. Strict Clippy after the branch
extension passed. Scoped acquisition, imported
resolution, entity integration, MCP/CLI and Windows/macOS require their own evidence.

Affected qualification uses:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test indexed_source --jobs 1
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test indexed_source --jobs 1 -- -D warnings
rustfmt --check --edition 2024 crates/graphit-ast/src/indexed_source.rs crates/graphit-ast/tests/indexed_source.rs
```

[Local Source index acquisition](rust_ast_source_index.md) specifies the scoped
Installation/ProjectId locator and resolved branch acquisition. Its six affected
SDK fixtures and strict affected Clippy passed in Main on Linux.

AST preserves real permissions, authentication, source authority and transaction
integrity through their owning APIs. Shared safety policy is not duplicated in
parser code.

## Affected validation

```text
cargo xtask cargo test -p graphit-ast --test source_ranges
cargo xtask cargo test -p graphit-ast --test tree_sitter_engine
cargo xtask cargo clippy -p graphit-ast --lib --test tree_sitter_engine -- -D warnings
rustfmt --edition 2024 --check crates/graphit-ast/src/tree_sitter_engine.rs crates/graphit-ast/tests/tree_sitter_engine.rs
```

The Tree-sitter tests use the linked official JSON grammar and verify unchanged
UTF-8/CRLF, syntax diagnostics, captures, active predicates, properties, control
errors and generation retention. Context and PostgreSQL body tests use the
linked HCL/TOML/JavaScript and PLpgSQL grammars.

The executable `ast_generic_nn_executable::generic_main_native_nn` retains the
Main/Raw/Graph/FTS/NN journey with a custom extraction profile over linked TOML.
Its registered `graphit-app/ast-generic-main-native-nn` gate supplies the real
CPU model server and EmbeddingGemma2 model. It checks exact Source associations,
semantic/hybrid results, incremental projection/vector reuse, model identity
changes and empty-file publication.

Use focused functional checks for the changed leaf and reuse build caches.
Do not infer coverage for other platforms or unfinished parser/indexing
integration from low-level library tests.
