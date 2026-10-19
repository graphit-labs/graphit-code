---
title: Rust Tree-sitter effective profile matches
doc_type: technical
references:
  - target: {type: task, id: tsk-de12}
    relation: implements
  - target: {type: task, id: tsk-82b9}
    relation: implements
  - target: {type: task, id: tsk-7890}
    relation: implements
  - target: {type: task, id: tsk-8121}
    relation: uses
  - target: {type: task, id: tsk-3f5d}
    relation: uses
  - target: {type: task, id: tsk-afac}
    relation: uses
---

# Rust Tree-sitter effective profile matches

The [collector](../../crates/graphit-ast/src/tree_sitter_profile.rs) prepares the
queries of an existing `EffectiveProfile` and returns raw observations for an
actual `PreparedInput`. It composes the existing [engine](rust_ast_tree_sitter.md),
catalog and origin mapping. It does not publish entities or resolve their owners,
generic context, UID, typed value, export, metric or stored Raw association.
The PostgreSQL-specific body route below composes embedded observations.
Those contracts remain in [semantic extraction](rust_ast_semantic_extraction.md).

## Preparation and collection

```rust
let prepared = PreparedTreeSitterProfile::prepare(profile, language, original)?;
let observations = prepared.collect_matches(input)?;
```

Preparation requires a Tree-sitter profile. The caller supplies the grammar
selected by its real loader; `GrammarLanguage` exposes no public grammar-name
proof. Each descriptor compiles once through the SDK and retains its compiled
query in an `Arc`. Duplicate descriptors remain distinct and ordered. Preparation
borrows the effective profile, its queries and provenance, without copying their
metadata or retaining the preparation Original.

Collection uses the input's Original, parses its prepared text once, walks nodes
once and matches every query against that same native tree/generation. A new
document uses its own Original. Normal cancellation/expiry are checked around
SDK operations and inside collection loops. Service contexts without a deadline
remain valid. Operation, parser mismatch, SDK and origin errors remain typed;
failure discards the complete private document result.

The result contains the borrowed profile, retained `ParsedTree`, `SyntaxNode`
vector and one `TreeSitterQueryGroup` per descriptor, including empty groups.
Each group holds its descriptor/provenance, configured roles, all matches and a
private shared compiled query. Its `capture_names()` returns actual SDK metadata.
Queries and trees retain the grammar after the preparation object is dropped.

## Captures and bindings

Each match conserves the SDK `pattern_index`, property settings and all captures
in SDK order, including repeated/quantified captures. Properties retain their
optional SDK `capture_id`; use the group's capture names to interpret that ID.
Native captures retain text, node identity, byte range and prepared byte points.

`QueryCaptureRole` exposes the effective role name and its actual optional SDK
capture index. An empty configured name means `@name`; an empty optional
value/span/declaration/parent/qualifier means that role is not configured. Names are never
interpreted as literals or trimmed. Aliases between roles remain valid.

| Binding | Meaning |
| --- | --- |
| `NotConfigured` | An optional role is empty in the profile. |
| `Missing { declared: false }` | The effective role has no capture declaration in this query. |
| `Missing { declared: true }` | The capture is declared but absent in this match. |
| `Present(Vec<usize>)` | All matching slots in this match's captures, in SDK order. |

Present indices address that match's vector. They are distinct from SDK capture
IDs and native node IDs. There is no first-wins selection, deduplication, Cartesian
product or fallback to other captures. `name_is_data` classifies captured text for
later interpretation; it does not change binding. Quotes, blank text, escaped
newlines and long text remain intact. Kind, labels, data keys, target rules,
native context/name queries, normalizers and embedding declarations remain borrowed metadata.
The existing decoder validates Entity labels and Relation types separately, and
requires paired value/parent capture and label fields. Fixtures use that schema.

## Coordinates and recovery

`TreeSitterProfileCapture` moves the native capture and adds `prepared:
SourceRange` and `original: OriginalProjection`. Both come from the supplied
input's factories, with checked offset conversion. Prepared ranges borrow the
actual prepared buffer; original ranges borrow the actual original buffer.
Neither uses the engine's copied `Arc<str>` as origin evidence.

Exact, Compound, Synthetic, Unmappable and Ambiguous projections remain distinct.
No clamp, normalization, hull or assumed declaration extent is applied. The
origin plan validates coordinate relations, without proving a transform or
filesystem authority. Explicit span roles select their exact captures; absent
span configuration remains `NotConfigured`. The separate `declaration_capture`
role records the configured native declaration unit for semantic policies; it
does not infer a nearest ancestor or reuse the span role. The separate `context_capture` role retains explicitly declared scope captures for
semantic consumers. The raw collector does not infer an owner from it. All roles retain all
SDK slots, including aliases and absent optional captures. An omitted optional
string stays unconfigured, an empty string is explicit empty text, and YAML
`null` is rejected by the existing typed string decoder.

Syntax recovery remains observable through `tree.has_syntax_errors()` and node
error/missing flags. The SDK permits querying zero-width missing tokens with
[`MISSING`](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html#the-missing-node);
their empty spans can project to ambiguous original anchors. These observations
are not a semantic publication success. `SyntaxNode.parent` is an index in the
returned nodes vector. `node_id` joins captures and nodes within this retained
tree, without supplying a persistent UID or identity across parses.

SDK text predicates such as `eq?`, `match?` and `not-match?` and `set!` property
metadata retain the existing engine behavior. Unsupported general/property
predicates return its explicit error. This adapter adds no predicate evaluator,
context path parser or silent-ignore policy.

## Focused validation

The [fixtures](../../crates/graphit-ast/tests/tree_sitter_profile.rs) use the real
catalog decoder, existing structural input limits, JSON/Rust SDK languages and
validated origin plans. They exercise stock Rust Entity/Relation metadata and
source allocation identity, duplicate descriptors, multiple patterns, missing
roles, aliases, raw data text, quantified order and property IDs, transformed
origins, missing-token recovery, query/parser errors and genuine Original control.
The collector introduces no new input caps or operation ledger.

Commands:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test tree_sitter_profile
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test tree_sitter_profile -- -D warnings
```

The seven native cases pass on Linux in 1.01 seconds (3.448 seconds including
Cargo); strict affected Rust Clippy passes in 0.610 seconds. Earlier fixture
failures and their bounded native SDK diagnostics remain under
`.build/ast-tree-sitter-profile/`, alongside the passing `test04` and `clippy01`
results. Windows/macOS remain UNRUN. Existing producers
are reused, without replaying broad engine/catalog/ANTLR suites. The current
stock catalog has 40 Tree-sitter and five ANTLR profiles; that inventory does not
reduce the migration obligation of 41 Tree-sitter grammars and five ANTLR families.
This collector supplies observations for subsequent semantic work, without a
claim of complete language indexing, inference, Auth or App/Main integration.


## Generic semantic consumer

The [generic semantic model](rust_tree_sitter_generic_model.md) consumes these
retained observations outside the reserved specialized language routes. It
requires explicit native span/declaration evidence, resolves actual entity and
namespace contexts, and feeds the existing Graph/FTS publication path. Its
`context_requires_entity` profile policy belongs to semantic context resolution;
the raw collector continues to preserve every native context observation.
An Exact original projection denotes one original interval, including a complete
MappedAtom; byte identity requires Direct origin-plan evidence. Consumers retain
the origin plan instead of inferring byte identity from Exact.

Collector qualification above is independent of the generic consumer acceptance.
Consumer run 29c passed 36/36 Generic cases, profiles 14/14, catalog 18/18,
raw roles 7/7 and both affected exact filters. Two additional Generic regressions
passed in run 29l. Main Graph/FTS acceptance passed in run 29m, including
incremental publication, empty FILE and invalid-library rejection. See the linked
consumer page for the current evidence and the separate NativeNN delivery.

## Prepared queries over retained PostgreSQL bodies

`PreparedTreeSitterProfile::collect_plpgsql_bodies(host, input, control)` uses its
retained PL/pgSQL language and compiled queries. It invokes the existing host-reuse
helper once, then executes each query against each actual retained body. No host
reparse, query recompilation or public internal-field getter is required.

`PlPgSqlProfileMatches` retains `profile`, `body`, projected `nodes` and query
`groups`. Native IDs and parents stay local to the body's tree; byte ranges and
points are in prepared host coordinates. Captures are additionally projected with
the actual input's origin plan. Query provenance, metadata, pattern indexes,
properties, multiplicity and declared/missing/not-configured bindings use the same
materialization as the ordinary whole-input collector.

The input must bind the same prepared buffer as the PostgreSQL host, parsed under
that original operation. The exact buffer pointer and length are checked before
allocation; operation identity remains the producer/caller precondition. A non-PL/pgSQL
profile fails with `EmbeddedProfileMismatch`, including when the host has no bodies.
Original and optional controls are checked through collection and before delivery.
`Embedded` preserves the existing host/Tree-sitter/projection error cause.

The [specific semantic consumer](rust_ast_antlr_semantics.md#postgresql-host-and-plpgsql-body-composition)
adds native parent-chain scope addresses and raw decision observations without
changing host ANTLR ordinals or inventing graph publication.
