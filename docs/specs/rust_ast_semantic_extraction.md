---
title: Rust AST semantic extraction and publication
doc_type: technical
references:
  - target: {type: task, id: tsk-d284}
    relation: implements
  - target: {type: task, id: tsk-2a2e}
    relation: implements
  - target: {type: task, id: tsk-fb2c}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: implements
  - target: {type: task, id: tsk-4840}
    relation: implements
  - target: {type: task, id: tsk-f13b}
    relation: implements
  - target: {type: task, id: tsk-d82c}
    relation: implements
  - target: {type: task, id: tsk-d11c}
    relation: implements
  - target: {type: task, id: tsk-b557}
    relation: implements
  - target: {type: task, id: tsk-89ed}
    relation: implements
  - target: {type: task, id: tsk-0d94}
    relation: implements
  - target: {type: task, id: tsk-3bae}
    relation: implements
  - target: {type: task, id: tsk-3e9e}
    relation: implements
  - target: {type: task, id: tsk-1da6}
    relation: implements
  - target: {type: task, id: tsk-9969}
    relation: implements
  - target: {type: task, id: tsk-8621}
    relation: implements
  - target: {type: task, id: tsk-0dfe}
    relation: implements
  - target: {type: task, id: tsk-0f50}
    relation: implements
  - target: {type: task, id: tsk-ed57}
    relation: implements
  - target: {type: task, id: tsk-0eae}
    relation: supports
  - target: {type: task, id: tsk-5cb2}
    relation: supports
  - target: {type: task, id: tsk-04a9}
    relation: supports
  - target: {type: task, id: tsk-b0cb}
    relation: relates_to
  - target: {type: task, id: tsk-a4cd}
    relation: implements
  - target: {type: task, id: tsk-08df}
    relation: relates_to
  - target: {type: task, id: tsk-0953}
    relation: relates_to
  - target: {type: task, id: tsk-1d33}
    relation: relates_to
  - target: {type: task, id: tsk-072f}
    relation: reuses
  - target: {type: task, id: tsk-3f5d}
    relation: reuses
  - target: {type: task, id: tsk-8aee}
    relation: reuses
  - target: {type: task, id: tsk-b197}
    relation: reuses
  - target: {type: task, id: tsk-0395}
    relation: reuses
  - target: {type: task, id: tsk-e056}
    relation: reuses
  - target: {type: task, id: tsk-4176}
    relation: uses
  - target: {type: task, id: tsk-4c40}
    relation: uses
  - target: {type: task, id: tsk-2845}
    relation: relates_to
  - target: {type: task, id: tsk-a90a}
    relation: relates_to
  - target: {type: task, id: tsk-b847}
    relation: specifies
  - target: {type: task, id: tsk-5260}
    relation: derived_from
---

# Rust AST semantic extraction and publication

Audience: AST extractor, indexer and Source consumer maintainers. Project
graphit-code, ses-c98a. This is a partial semantic planning contract tracked by tsk-a4cd.
The limited first ANTLR query contract is tracked by tsk-b0cb, with executable
producer tsk-08df. It does not deliver an extractor, semantic publisher or stored
entity lookup. The independent
extent primitive 072f is implemented in Main and its seven functional fixtures
and strict affected Clippy passed on Linux. It has a saved,
read-back specification and static readiness review.

Preserve all active parser/profile/language families. Reuse the qualified
Tree-sitter and official ANTLR engines and actual origins. Semantic interpretation
of their nodes/captures is distinct from implementing another parser. Source spans come from the selected parser and remain in original coordinates.

## Delivered inputs

| Input | Actual evidence | Boundary |
|---|---|---|
| Typed profile/catalog | catalog.rs:934–1001,1488–1566; 3f5d | Declares queries, captures, context, exports and complexity. It does not extract entities. |
| Tree-sitter | tree_sitter_engine.rs:114–146,263–312; 8aee | Nodes and captures have complete half-open byte ranges; native trees/queries retain grammar owners. Full language release remains its own work. |
| ANTLR | antlr_engine.rs:176–220,549–557; b197 | Official engines supply spans/status/diagnostics. Recovery without original indexing stays explicit. Main Linux 18-fixture qualification is reused, not repeated here. |
| Prepared origins | prepared_input.rs:102–133,172,389; 0395 | Exact, Compound, Synthetic, Unmappable and Ambiguous remain different outcomes. |
| Identity | symbols.rs:240–359,515–527; 3f5d | IdentityDocument borrows original bytes/context. Its subject anchor is not full entity extent or a storage grant. |
| Original text | [source_ranges.rs](../../crates/graphit-ast/src/source_ranges.rs) | SourceText validates unchanged UTF-8 and SourceRange owns no text. |
| Indexed raw text | [indexed_source.rs](../../crates/graphit-ast/src/indexed_source.rs):80–127; 4176/4c40 | Retains actual Module/Branch Dataset and original Arc; version() is the SDK u64 generation; source(original) requires Arc::ptr_eq. No entity query or writer is supplied. |
| Selection | [entity Source selection](rust_ast_entity_source.md); 33fe | Selects a caller-provided inclusive line window with global coordinates. It does not discover or bind an entity. |

Reuse IndexedSourceSnapshot directly. A new raw-file wrapper would duplicate
its getters and retention without closing graph-to-content publication.

## Anchor, extent and metric subtree

Keep three meanings separate for each semantic occurrence:

- The name/subject anchor supplies logical identity through IdentityDocument.
- The entity extent covers the selected declaration/node in original bytes.
- The metric/export subtree supplies actual syntax nodes for those analyses.

Historical Tree-sitter behavior without span_capture starts at the name and
ends at its immediate parent. An explicit span capture replaces those bounds.
Historical ANTLR uses match.Node for extent and match.Parent, when present, for
metric/export text. Neither observation proves a universal full declaration.

A backend contract must choose the delimiter/node and state how it handles
missing configured captures, absent span_capture and malformed/recovered syntax.
Do not silently use an arbitrary ancestor, extend a name to a hull, or claim a
narrow anchor is complete. A typed diagnostic/unavailable semantic extent must
remain observable until that rule is resolved. These backend contracts are
pending; 072f does not choose a delimiter.

## Independent supplied-extent primitive: 072f

Main API with seven passing Linux fixtures and strict affected Clippy:

```rust
impl<'o> EntityExtent<'o> {
    pub fn new(
        source: SourceText<'o>,
        candidate: SourceRange<'o>,
        original: &SnapshotContext,
    ) -> Result<Self, EntityExtentError>;
    pub fn range(&self) -> SourceRange<'o>;
    pub fn start_line(&self) -> usize;
    pub fn end_line(&self) -> usize;
}
```

The constructor rejects an empty candidate, obtains the canonical range from
the supplied root SourceText, then compares pointer, length, offsets and byte
points. Equal content in a foreign allocation is insufficient. A subslice with
relative offsets/points that disagree with the root is rejected. It uses safe
Rust and the actual SourceText range factory, without manufacturing a Range DTO.

The resulting object validates only the supplied original span. It does not
prove semantic completeness, graph/raw association, physical authority or Auth.
Callers retain acquisition/prepared/native owners and the original request
through final validation and delivery.

Points are zero-based; byte ranges are half-open. start_line is start.row + 1.
If end.column is zero, end_line is end.row, excluding the next line; otherwise
end_line is end.row + 1. Arithmetic is checked. For raw `α\r\nb\n`,
range0..4 is line1 only, range0..6 is lines1..2, and range4..6 is line2.
The trailing empty third logical line is not included by a range ending at EOF.

Errors are EmptyRange, ForeignRange, Range(RangeError), Operation(io::Error)
and ArithmeticOverflow. Original checks run before and after validation,
including a failing validation result; no partial object is returned. IO and
range causes remain available. Range scanning and cancellation are cooperative.

Only actual Exact(SourceRange) origin projections can feed this factory.
Compound, Synthetic, Unmappable and Ambiguous outcomes stay in their typed
origin/diagnostic representation. They do not become a min/max line window.

Affected qualification commands used by 072f:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test entity_extent --jobs 1
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test entity_extent --jobs 1 -- -D warnings
rustfmt --check --edition 2024 crates/graphit-ast/src/entity_extent.rs crates/graphit-ast/tests/entity_extent.rs
```

Fixtures EE1–EE6 in the saved Task cover Unicode/CRLF/LF/EOF, empty spans,
foreign allocations and subslices, raw BOM/NUL, actual cancellation/deadline,
and composition with 33fe: original range4..14 in a..j gives window3..7;
range2..4/head2/tail1 selects global line5 with full-file total10.
These supplied-span fixtures passed on Linux (7/7). This qualification does not
prove backend delimiter selection or graph/content association.

## Required semantic output and unresolved schema

ParsedDocument/Entity/CallSite/Reference/GraphBatch remain proposed APIs.
The closed schema must preserve label/name/type/qualified context, anchor,
extent origin status, producing language/grammar, properties, documentation,
exports, metrics and relationship provenance. Logical syntax identities remain
separate from SCIP semantic identities; SCIP input types belong to its owner.

Duplicate occurrences can be grouped by actual identity and data_key only when
their information agrees or an explicit merge rule applies. Historical merge
keys combine label/name/context/line and fill the first empty fields; that is
not a conflict policy for Rust. Conflicting extents, values or owners require an
explicit result, not first-value-wins. Different paths, contexts, labels and
overloads must remain distinct.

Resolve owners from actual syntax containment and configured kinds. A logical
UID is not proof of containment. Calls/imports/inheritance/implementations,
field reads/writes, DML and custom profile relationships require configured
target labels/qualifiers and explicit zero/one/multiple candidate outcomes.
Cross-language target resolution is not silently introduced.

Comments/docstrings retain original source and attachment provenance.
Complexity/export inspect configured syntax subtrees and exclude nested
declarations. Data values retain raw spans and a decoded value from a mature,
qualified decoder; malformed decoding remains an error. Long or multiline
content is not silently dropped to make extraction appear successful.

These are obligations for subsequent schema/backend/resolution refinement.
This partial page does not claim their interfaces are closed or Source-ready.

## Per-file graph/raw association and incremental publication

Root's chosen direction is publication of a per-file association by the real
semantic publisher, carrying the raw SDK version actually observed and original
content/hash observed for that file. Raw path/source schema stays unchanged.
GraphGeneration's opaque 32-byte fingerprint is a different identity.

Dataset version is global. An update to another file can advance it while this
file's bytes remain unchanged. Permanent stale failure for every unchanged file
would regress incremental indexing and is not accepted as final behavior.

At publication, a90a/publisher must refresh associations of unchanged files
from the actual current raw capture, validating corresponding unchanged bytes
or their observed content hash. Changed bytes require extraction from that
capture. A stale association before refresh returns a typed stale result;
this is a temporary condition, not the completed incremental contract.

sha2 is already a dependency of graphit-ast. SHA-256 over exact original raw
bytes is a proposed mature hash option, not a delivered association/schema/API.
Hash metadata alone is not an authority grant. The actual publisher must pair
the observed raw file with its graph entities and retain actual snapshot,
namespace/branch/corpus and original request owners. The exact persisted graph
fields, writer handoff, publication fence and recovery are still pending with
a90a/Data/Root; this AST work does not alter their source.

Lookup2845 captures raw current, compares the published association, then retains
the corresponding graph/raw owners or returns typed stale/corrupt/unavailable.
It must never recreate a historical exact capture from a version/UID/path DTO.
Default branch observations, Module-without-branch and inherited branches remain
distinct. Available empty text differs from NULL/TextNotStored/FileAbsent.
index_source=false must preserve semantic graph capabilities and expose missing
stored Source honestly; it is not an empty available file.

## Required fixtures and current coverage

Native JS/TS/TSX model/Graph/NN are qualified; they are not pending interpreters. Python has an active stock profile. Its configured model/Graph/NN/foreground are now implemented and qualified for the scoped Linux baseline by tsk-9fa2; native/codec/model and affected AST strict proofs are qualified in their recorded scopes. The real App Python focal first failed at Graph capture with HTTP409. After the coordinator's causal Core ancestor-binding correction, the exact focal passed on App08 with actual NN/vectors/Graph families/ranges/CALLS/current Source association; the unavailable-Java proof was preserved. Affected App strict passed under the coordinator (App --lib --test foreground_ast_semantic --no-deps -D warnings, exit0/6.36s), with no Python/NN/Graph/CALLS/FILE replay. Oracle CALLS local is implemented by tsk-fb2c with 57 passing tests on the corrected AST17 model/Graph source and scoped strict also passing; global/C7–C9/publication remain separate requirements. Observed applications and collection classification alone do not establish Function/Procedure edges. The active matrix now maps E06–E08 to de12 fixtures, native Origin/Extent producers and the actual App host. XML preparation/projections and HTML child-parser/Graph proofs remain separate cases; the old nominal Step fixture has not been matched to a current case. Mature scalar decoding is required only by a concrete consumer; genuine literal_source/code is the existing value contract where specified.

The six broad a4cd obligations stay pending while concrete product gaps and traceability remain. Reuse the actual producer/Root receipts rather than repeating daemon/AST/semantic gates. Windows/macOS are unrun where only Linux was executed. No active parser/source capability is cancelled by this reconciliation.

## First independent native query delivery

Task tsk-08df implements `antlr_engine::parse_queries` and
`ParsedQueries { syntax, matches }`. Each match carries query and real node
ordinals in the same returned syntax, ordered by query then native preorder,
with duplicates removed only for the same query/node pair. The API is
implemented and qualified on Linux: nine query fixtures, the 18-test engine
regression and the final affected strict Clippy passed.
See the [native query API](rust_ast_antlr_queries.md) for the precise contract.

The implementation uses the official ANTLR 4.13.2 XPath splitter and path
elements while the parser/tree are alive. Each call owns its synthetic root,
avoiding the SDK `XPath::evaluate` shared mutable root. No native pointer escapes.
Spans, diagnostics, recovery and `syntax.project(actual PreparedInput)` keep the
delivered engine contracts. Empty paths and the unsupported nearest-capture operator `**` are rejected.
Other invalid paths return typed `QueryFailure` with the actual query index
and SDK cause; valid paths
with no matches succeed empty. Source text is unchanged.

This delivery supplies SDK `/`, `//`, tokens, rules, wildcard and inversion.
Profile guards `[KEYWORD]`/`[!KEYWORD]` and nearest-descendant `**` captures have
their own generated ANTLR DSL and native consumer in tsk-0953. The
[profile query API](rust_ast_antlr_profile_queries.md) preserves ordered capture
slots and real node ordinals; Source, all 36 affected ANTLR tests and strict
Clippy passed on Linux. Name sentinels/fallback, qualifier contexts, entity extraction and
publication remain required semantic work in tsk-a4cd. The native query
producers do not claim execution of every active profile or the full pipeline.

The next independent consumer is specified in tsk-1d33: effective profiles
bind to raw entity/relation occurrences, typed capture availability, original
projections and native scope observations. Its [implemented API and fixtures](rust_ast_antlr_semantics.md)
keep names, owner resolution and complete declaration extents explicit.
Source is implemented and qualification is pending; the broad tsk-a4cd obligations
remain required.

## PL/SQL invocation classification remains a semantic contract

The raw call-profile product `tsk-5cb2`, specified by refinement `tsk-0eae`, is
implemented and its raw fixture passed on Linux. It uses the delivered child-rule
guard and preserves local/raw
names, actual matched parts and ancestral statement/general-element wrappers.
It does not produce decoded full names, resolved CallSites, owners or edges.
Its quoted/Unicode policy changes only stock rejection [configuration](../guides/configuration.md), preserving
custom rejects and duplicate query observations.

Parenthesized syntax does not decide function versus constructor versus collection
access. The G4 also allows that shape on an assignment LHS. A model must preserve
an unresolved candidate until it can classify it from legitimate native and
symbol contracts; absence of resolution cannot become a fabricated CALLS edge.
Repeated application `fn()()` must keep actual argument groups before defining
semantic cardinality. Bare collection methods such as COUNT need coverage beyond
a parenthesized guard. Completed contract `tsk-04a9` preserves these necessary
`tsk-a4cd` semantic consumers. The earlier raw product and `tsk-1d33` consumer
are qualified in their recorded scopes; dedicated raw production is qualified on Linux
in the cohesive `tsk-b847` (four native tests, two affected inventory checks
and scoped strict Clippy).

Dedicated families require explicit branch anchors: CHR/TRIM/TO_DATE, numeric
aggregations, JSON functions and the named analytic/prediction wrappers differ
from cursor attributes and TIME literal alternatives in `other_function`.
JSON_TABLE/XMLTABLE retain tabular producer semantics and must not be duplicated
by containing wrappers. Neither generic `other_function` nor its first terminal
is an adequate universal callee capture. The
[Oracle collection contract](https://docs.oracle.com/en/database/oracle/oracle-database/21/lnpls/plsql-collections-and-records.html)
and [collection methods](https://docs.oracle.com/en/database/oracle/oracle-database/21/lnpls/collection-method.html)
support those semantic distinctions; they are reference evidence, not executed
Rust parser qualification.

See [raw occurrence contracts](rust_ast_antlr_semantics.md) and
[profile reconciliation](rust_ast_antlr_profile_reconciliation.md). Product
raw fixtures passed on Linux; the family refinement must register concrete producers
and integration proof before claiming fullCALLS, entity ranges or publication.

The next real consumer is the [Oracle callable model](rust_ast_antlr_model.md),
implemented in the existing a4cd packet: eight native Function/Procedure/Package
declaration kinds, canonical package headers and END references, native owners,
component-anchored qualified names, and existing Graph/NN adapters. All 13 affected Oracle model tests, the genuine Graph/NN focal, the existing
Oracle/DB2 Graph focal and scoped strict Clippy passed on Linux. This prerequisite does not classify every application or resolve CALLS.

The [Oracle declaration facts](rust_ast_antlr_model.md#oracle-native-parameter-and-type-observations)
from tsk-ef54 also consume actual Parameter and local Type subjects: modes/defaults,
native definition/element/index/bound facts, and separate collection/record/cursor
shapes. The affected 17 model and eight Graph tests and scoped strict Clippy passed
on Linux. These facts feed C8 without claiming lexical or catalog bindings.

Repeated application retains raw node/query identity and each actual function_argument group. Collection indexing (including LHS), constructors and functions need real declaration/type evidence; missing or ambiguous evidence cannot fabricate CALLS. Bare collection methods remain required. Preserve qualified/quoted/Unicode/dblink components and origins until an Oracle-qualified mature codec decodes them without repair. These C7–C9 consumers, semantic identity and actual Graph/publication integration remain necessary in tsk-a4cd; the raw packet does not make a universal model Source-ready.

## Native Rust projections for NN

Rust projections use the same genuine Tree-sitter model for configured Struct,
Field, Function and Method contributions. Prepare `PreparedTreeSitterContexts`
with the effective profile and retained grammar, then create a `PreparedInput`
that maps to the original allocation. `ParsedTreeSitterDocument::collect` and
`assemble` retain the native tree, genuine identity, original name anchors,
full extents, configured context owners, contributions and conflicts. The
[model contract](rust_tree_sitter_semantic_model.md) defines recovery and
unavailable observations; callers must select actual `Available` projections.
The foreground Tree-sitter and JSON NN consumers preserve those unavailable
entities and diagnostics in Graph while excluding them from inference. A file
with no eligible projections follows the existing empty-file publication path,
which removes its stale vectors. Configuration and operation errors still fail.

`NativeNnTsInput` and `encode_native_nn_ts` consume that borrowed semantic
owner directly. They do not parse again, reconstruct identity from row metadata
or create a separate Rust function document. Labels and data keys come from
configured queries; the stock profile labels both functions and methods
`Function`. Impl namespaces use the configured native type path, preserving
qualified/generic names. The profile's context labels remain explicit.

`NATIVE_ENTITY_PROJECTION_RECIPE` in `native_nn.rs` is
`ast-parser-entity-original-1`. Projections use exact original bytes and complete
native extents. ModelService applies its CodeDocument prefix once. Original,
physical captures and Raw observations remain live through encoding; serialized
rows do not restore parser ownership or authority.

The retired dedicated extractor and its preparation-only tests are superseded
by the genuine model and its qualified Function/Method producer. Publication,
query, Source association and codec controls remain covered by the active
NativeNN consumers. The consumer cutover is qualified by `tsk-4840`: 54 affected Linux cases and
scoped Clippy passed. This does not qualify the complete daemon indexing flow.
See [Tree-sitter NativeNN](rust_tree_sitter_native_nn.md) for the borrowed input
and Source association contract. Actual loader/callback publication, generation
retargeting and all-language integration remain required integration work.

## Arrow codec for native parser entities

The library in [`native_nn.rs`](../../crates/graphit-ast/src/native_nn.rs)
encodes and validates the shared Arrow representation for genuine Tree-sitter,
JSON and ANTLR semantic models. It receives observed vectors and never calls
ModelService or grants publication authority. Active consumers use the specific
parser input, preserving the same schema and source associations.

`NativeNnModel` carries an observed portable `vector_id` of 32 bytes, dimensions, model ID
and revision. The caller obtains these values from the same resolved model slot;
the input itself does not establish that binding. `native_nn_schema` preserves
all of that identity, format `ast-native-nn-4`, module `ast`, role `CodeDocument`
and recipe `ast-parser-entity-original-1`, including in a zero-row batch. Model
IDs and revisions retain their exact text. A different vector identity, revision, model,
recipe, format or width fails validation, even when vector widths agree.

`encode_native_nn_ts` receives actual `TreeSitterSemanticDocument` values, a typed
`NativeNnSource`, selected entity indices, ordered vectors and the same
`Arc<SnapshotContext>`.
`Indexed(&raw)` requires genuinely available indexed text, including available
empty text. `Physical { capture, raw }` requires an actual raw key whose retained
snapshot reports `MissingSourceReason::TextNotStored`; FileAbsent and available
text are rejected on that route. The caller must publish the actual raw key
before encoding, without inventing SDK generation zero or available empty text.

Both routes require the document's Original to be that context, the raw owner
to accept that exact Arc, and the source allocation, byte length and literal
logical path to match the document. The physical route borrows directly from
`NativeNnPhysicalSource`, checks its path/components and full content SHA against
the document, and revalidates the retained physical capture at return, including
failed encoding. The final Original check precedes propagation of the physical
revalidation result, preserving cancellation/deadline as the primary operation
cause. These associations are checked before the zero-entity shortcut.
Equal nonempty text copied into another allocation does not satisfy association;
empty buffers may share a dangling pointer and do not establish allocation
exclusivity. A real empty physical Source retains full schema/model metadata
without a model generation or dummy row.

The codec records the actual raw SDK version and branch observation of the NULL
text row, while the full-source digest covers the physical file bytes. Raw NULL
storage and physical Source have separate provenance. Neither observed values nor
matching path/Arc confer authorization or bind the raw corpus to the project.
The actual LocalSourceIndex caller must supply that binding and current version;
the primitive does not reobserve raw HEAD, infer a branch from physical paths,
or reconstruct arbitrary historical physical content.

The 21 columns carry UID, name, kind, path and path components, language and
grammar, name anchor, full extent, structured namespace owners, raw version and
branch observation, full-source SHA-256, original projection and its SHA-256,
nullable process-local model generation, vector and `data_key`. Ranges store half-open byte offsets
and zero-based byte points; extents add inclusive one-based lines. Owners remain
separate outer-to-inner frames containing their kind, name, anchor and extent.
Logical backslash components retain their identity on every platform; the
physical factory rejects platform separators and aliases before encoding. No
name joining, source rewriting or vector normalization is performed. Original
projection bytes may contain NUL; model metadata and identifiers have their own
validation rules.

Module observation is distinct from Default branch observation. `raw_origin`
is `module` for a missing branch observation and `branch` for a present one;
Default has both physical and inherited names absent. Named and inherited raw
captures retain their actual names. These columns do not materialize a branch
or authorize a corpus locator.

Vectors use a non-null fixed-size list of Float32 with a nullable child descriptor,
matching the physical SDK convention. Validation still rejects actual null
components or parents, nonfinite values, wrong dimensions and mismatched selected
entity counts. Each `NativeNnTsInput` supplies explicit `entity_indices: &[usize]` in vector
order. These refer to the genuine model, not source byte offsets. The codec
rejects unknown indices, unavailable projections and duplicate identity/data-key
tuples; it never clamps, reparses slices or infers a projection from rows.

Source/Original/allocation/path/fullSHA association is checked before selection,
including an empty selection in a nonempty document. Selected entities retain their
native UID, anchors, extents, namespace and projections in the full original
allocation; fullSourceSHA always covers that full file. Vectors must match the
selected count exactly. Each nonempty Local selection preserves its actual observed
model generation; remote selections store NULL. Disjoint selections of the same
retained document can therefore carry distinct generations returned by separate
Local RPC batches, without fabricating a document-wide generation or a remote
sentinel. Global `(uid, data_key)` checks reject overlapping selections.
An empty selection preserves complete schema/model metadata without a dummy row
or generation. Model generation, raw SDK version and NN dataset version remain
separate domains; the codec does not prove that observations came from M.

`validate_native_nn` checks schema/model equality and observable row invariants:
required values, unique `(uid, data_key)` tuples, path/components, branch discriminants, projection
digest/length, name bytes and points within the projection, inclusive lines and
strict namespace nesting with consistent byte-offset and byte-point ordering.
Persisted owner names and full-file digests cannot
prove original file bytes without the retained source. Validation restores no semantic owner, grant or native parser provenance.

IO cancellation/deadline, indexed-source, boxed physical-source, Arrow, range
and allocation causes remain available through `Error::source`. Integer and Arrow-offset conversions
are checked. The codec uses mature Arrow builders; their allocation behavior is
not a promise that every SDK allocation is fallible. Original checks surround
success and failure and cooperate within document, owner, hash and vector loops.

The [`native_nn` fixtures](../../crates/graphit-ast/tests/native_nn.rs)
exercise real Raw observations, genuine Tree-sitter projections, schema/model
identity, empty batches, corruption, Original errors and Module/Default/named/
inherited associations. A real SDK roundtrip and overwrite with a changed model
and width test the representation. These tests do not grant authorization or
qualify the entire daemon indexing pipeline. The model producer's qualified
Function/Method tests replace tests exclusively for the retired extractor API.

Data's Module overwrite accepts an explicit schema. Branch publication instead
uses the existing overwrite API with a first batch carrying that schema;
zero rows use `RecordBatch::new_empty(schema)`, not an empty batch vector.
The public Branch handoff belongs to its Data producer, separately from this
codec. The production locator is `installation/ast/<ProjectId>/native_nn.lance`,
a fixed sibling of `source.lance`, with the branch from the actual project capture.
Authorization and live caller ingress are separate prerequisites.

The `entity_selection` fixtures reuse the genuine Tree-sitter model, actual
indexed raw and physical PROJECT owners. Three disjoint selections preserve
the complete record payload with per-batch observed generations 7/8/9; actual
W publication and nearest-L2 read back all columns with ordered distances.
Other cases reject invalid/unavailable indices, vectors, overlapping
identity/data-key tuples and copied/foreign/mutated Source even with an empty selection. These are
controlled observations; G2 must split genuine RPC inputs by actual
`ModelServiceClient::limits()` and preserve each actual `EmbedOutput.generation`
and output identity/count/dimensions. No single-file/single-RPC restriction or
source reparse may replace that composition.

Current consumer cutover and affected checks are recorded in `tsk-4840`.
Existing Source/Raw, publication, query and selection oracles remain necessary;
unchanged model-producer results are reused. Run only consumers affected by a
change or concrete failure. Actual daemon publication/association refresh, full
semantic graph and all-language integration remain separate obligations.

## Retained AST corpus queries and vector publication

`query_corpus_nn(&branch, &model, &query, k, metric, namespace, &original)`
queries the retained `corpus.lance` branch. It validates the full model field
metadata before scanning, including when the selected cohort has no candidates.
The query is a finite Float32 vector of the exact model dimension and positive
`k`. Native L2, Cosine and Dot metrics retain their SDK distance semantics.

The scanner prefilters the existing graph namespace, compatible vector identity
and non-NULL vectors before nearest selection. Cosine uses the maintained
IVF_FLAT index with native coverage for new fragments; L2 and Dot use the SDK
flat scanner with the requested metric. Returned rows are validated transient
format-4 payloads plus finite native distances. Original, exact tuple identity,
Source and graph checks remain active in the composing consumer.

`corpus::prepare_embeddings` selects only current inferred identities and updates
extant rows whose fingerprints still match. It cannot insert a missing entity;
strict native CAS rejects an intervening corpus publication. The App records the
actual `finish_publication` receipt, recovery key and generation before handling
any late Original error. A confirmed effect is never reported as rolled back.
There is no separate NativeNN overwrite, FILE publisher or legacy query reader.

The [single-corpus contract](rust_tree_sitter_native_nn.md#single-ast-corpus-and-progressive-publication)
describes publication membership, nullable model evolution, checkpoints and
explicit filtered export. Affected tests use genuine parser graph rows with
`corpus::encode_file`, SDK publication and conditional embedding updates; they
do not reconstruct lexical observations from an NN payload. Linux integration
qualification of this cutover is recorded independently from historical codec
and model-service receipts. Windows/macOS remain unrun.

## Validated model identity for NativeNN

The App `ast_semantic::expected_native_nn_model(&ExpectedModel)` returns an actual
`NativeNnModel` borrowing the validated ExpectedModel's model and revision and
observing its portable vector ID and embedding width. It accepts no independent identity
strings, default width or arbitrary manifest carrier. The schema/codec retains
its existing width/representation validation. Actual rerank configurations,
whose observed width is None, return `NativeNnError::InvalidModel`.

An empty corpus still needs the complete current NativeNN format-4 schema and
model metadata from that validated owner. Calling the actual empty codec does not invent a model generation or need
a dummy RPC to discover metadata. A same-width revision/config change produces
a distinct schema; the old schema remains associated with its retained old owner.
The library bridge does not admit the operation or prove current lease/config,
Auth, runtime, actual CodeDocument/CodeQuery inference, product loader or Main.
Those real integration gates remain required in the Root caller/assembler.

The focused fixtures resolve actual captured embedding_rank2/rerank_rank1 model
manifests, retain the expectation after config drop, exercise a genuinely resolved
same-width changed revision and actual codec Original errors. They reuse the
qualified CPU ORT1.27.1 library explicitly through GRAPHIT_TEST_ORT_LIBRARY, without
loading a worker or executing inference. `cargo test --locked -p graphit-app
--test ast_semantic` passes three in 0.01 seconds (22.373 seconds including Cargo).
`cargo clippy --locked -p graphit-app --lib --test ast_semantic -- -D warnings`
passes in 169.228 seconds. Exact complete empty schema/model metadata, validated
same-width revision changes and Original error causes are covered. Evidence is
in `.build/ast-native-nn-model-identity/qualification.json`; Linux executed,
Windows/macOS UNRUN. This qualifies the metadata bridge, while the actual Root
caller/assembler and ModelService→NN product flow remain required.

## Physical PROJECT Source for NativeNN

`capture_native_nn_physical_source(project, relative_components, copy, retention,
read_limits, &original)` captures one selected current PROJECT file through the
existing Core native backing. It checks the real scope record/live capture,
PROJECT owner and VersionContext scope with the same operation Original. These
are data/physical observations; the caller still supplies actual Auth permission
and current logical version. The primitive does not inspect Git or reconstruct
bytes of an arbitrary historical version.

The private owner retains the ProjectCapture borrow, same Arc Original, literal
path/components, actual Core reader/file and one full retained range. It builds
RejectFileLinks and an empty RetainedFileSet internally, so capture retains no
second full body. `source(&same_original)` lends validated UTF-8 directly from that
full range after checking the original, scope and current captured content before
and after access. There is no constructor accepting fragments or restored owner,
body copy, raw Dataset getter or authorization carrier.

The SHA256 is the full file-content digest, compared with the captured root file
node, never the tree fingerprint. Empty files have their actual empty-content
SHA256. Names remain literal: Unix backslash names retain their identity; native
Windows separators/aliases reject rather than normalize. Malformed scope/path,
foreign Original, invalid UTF-8, observed file/ancestor mutations, links and
existing Core limit failures preserve their typed causes and return no partial
Source. Copy/retention/read bounds are the existing caller-selected producer
limits, without a new default, quota framework or work ledger.

`NativeNnPhysicalSource` is ready for direct IdentityDocumentInput and the actual
prepared Rust extractor. The typed `NativeNnSource::Physical` codec route above
now associates that authentic allocation with an actual raw TextNotStored key.
The capture producer itself does not fabricate FileAbsent as empty text, generate
embeddings or publish NN. Real Root ModelService/caller integration remains
required.
The affected fixtures use real PROJECT scopes/VersionContext, selected full UTF-8
and the prepared SDK extractor. `cargo test --locked -p graphit-ast --test
native_nn native_nn_physical_source` passes four on Linux in 0.02 seconds
(22.945 seconds including Cargo); `cargo clippy --locked -p graphit-ast --lib
--test native_nn -- -D warnings` passes in 99.103 seconds. The cases cover two
PROJECTs and literal UID/ranges/projection/source allocation/full SHA, empty files,
Unix backslash/Unicode names, invalid associations/UTF-8/Core limits/Original,
observable file/selected-ancestor mutations and strict links. These tests do not
qualify a raw TextNotStored association, M inference or the product NN flow.
Evidence is in `.build/ast-native-nn-physical-source/qualification.json`;
Windows/macOS remain UNRUN.

## Physical Source codec qualification

The own `physical_codec` fixtures nested in `native_nn_physical_source` reuse the
actual PROJECT capture and prepared Rust extractor. They create actual raw NULL
rows through the SDK and preserve Module/Default/named/inherited observations.
In the recorded qualification, actual NN overwrite and nearest-L2 consumed the
then-current 20-field payload and its model metadata; complete persisted/read
payloads and distances were compared. The current codec uses NativeNN format3
with21fields, including name_kind, as specified in [rust_tree_sitter_native_nn](rust_tree_sitter_native_nn.md).
The historical qualification receipt at .build/ast-native-nn-physical-codec/qualification.json retains its original scope. Named NN materialization is explicit in the fixture, outside the codec.
Vectors and generation 7 are controlled observations, without ModelService
inference or a product caller proof. The cases also exercise authentic empty
Source, literal Unix backslash/Unicode names, wrong routes/FileAbsent, foreign
Original/raw/capture, copied nonempty allocations with and without entities,
wrong document path, physical same-width content mutation and genuine control
causes. Prior indexed fixtures migrate only the required input enum field.

Affected commands are `cargo test --locked -p graphit-ast --test native_nn`,
`cargo test --locked -p graphit-ast --lib native_nn_query` and
`cargo clippy --locked -p graphit-ast --lib --test native_nn -- -D warnings`.
The current Linux target passes all 27 tests in 0.22 seconds (4.201 seconds
including Cargo); the three private query units pass in 0.04 seconds (6.306
seconds including Cargo). Strict affected Rust Clippy passes in 9.540 seconds.
Existing generated ANTLR C++ warnings remain separate from that Rust result.
The first attempt's empty-scan expectation failed: actual SDK EOF has no batches,
so the fixture now checks the retained exact schema and genuinely empty scan.
A later pre-runtime attempt failed with explicit ENOSPC while writing a shared
liblbug archive. Root recovered inactive generated cache before the exact retry;
no Source, models, data or producer history was cleaned by this worker. Both
failures and successful attempts remain immutable. Final Original control is
checked before propagating physical revalidation errors.

Evidence is `.build/ast-native-nn-physical-codec/qualification.json`; all earlier
producer counts above describe their own qualified slices. Required input
builders migrated without compatibility; W functional Source and the physical
capture producer remain byte exact, and Q changes only its private test builder.
The concurrent foreign App crossterm lock entries are outside the AST dependency
closure and were preserved. Windows/macOS remain UNRUN. Actual full-language
semantic publication, ModelService inference and authorized CLI/MCP/Main caller
integration remain required integration deliveries.

## Local AST corpus acquisition and retained branches

The locator in `native_nn_index_locator.rs` derives
`<global>/projects/<PROJECT_ID>/ast/corpus.lance` from genuine matching
Installation/PROJECT captures and the Core local-corpus selection for that
module. Every project version uses that same dataset; its retained
`BranchSnapshot` observes the exact native schema and branch at one selected URI, protected by native directory
guards. `corpus::validate_schema` rejects other formats;
there is no legacy path reader or implicit rebuild. `source.lance` retains the
complete per-FILE Source observations separately.

Named versions use native Lance branches. An inherited target remains a read
observation until the native `BranchSnapshot::materialize` producer confirms its
actual effect. The stable graph namespace is recorded as publication membership
on each corpus row; inherited Main rows cannot become this version's candidates
before foreground publication promotes their FILE. Their compatible vectors can
still be reused by that publication.

Initial creation is owned by App's `ProgressivePublication::open`: a private
empty corpus is committed and installed by exclusive rename. Concurrent versions
reopen and validate the actual winner; a rejected or failed birth does not
install an alternate schema or leave a retained private stage. The graph keeps
its existing cohort format. Actual FILE updates and embedding batches use this
same corpus, with no mandatory-vector sidecar or independent Main-creation API.

The affected locator fixtures and App's concurrent-version birth fixture
exercise the current layout. Their new results must be recorded for this cutover;
prior receipts for removed sidecar mechanisms do not qualify it.

## Genuine JSON NativeNN encoding

`encode_native_nn_json` accepts `NativeNnJsonInput` borrowing the actual
`JsonSemanticDocument` and ordered entity indices. It calls the retained model's
`code_projections()` once and selects only Available entries. Unknown indices,
non-embedding entities, conflicts or diagnostic-unavailable projections fail
before publication. The parser tree is not reparsed and DTOs cannot restore
native identities. The three crate-private parsed getters expose only the
existing source allocation, Original and identity document to this encoder.

Both native producers use recipe `ast-parser-entity-original-1` and the same
21-field NativeNN format-4 Arrow schema, including the explicit name_kind
discriminator. `data_key` is appended after vector, preserving active
column positions. Rust functions use `functions`; JSON retains its effective
profile query key. Row identity is `(uid, data_key)`: two real JSON entities can
share a Syntax UID while belonging to different keys. Exact tuple duplicates
fail within encoding, validation and across query/publication/Main batches.
Main validates this logical identity during SDK preparation before commit and persists both
columns. The current SDK create API has no primary-key constraint parameter;
these adapter checks do not claim a native persisted constraint.

JSON names and namespace names are decoded content, including empty strings
and escaped NUL. Their anchors remain the original quoted lexical ranges.
JSON scalar names preserve validated Number/Bool/Null lexemes without floating
point conversion. The validator uses mature `serde_json` decoding to compare a
name with its projection anchor; names need not have the anchor's byte length.
Namespace frames retain actual effective labels, decoded names and lexical
anchors/extents. Identifier guards still reject empty or NUL UID, path, grammar,
label and data key. Supported domains are explicitly `rust/tree-sitter-rust`
and `json/tree-sitter-json`; other languages need actual producers.

JSON association checks the same Original pointer, complete source allocation
and length, literal components/path and retained Raw observation before any
selection, including empty output. NULL raw text requires the genuine physical
capture and its full SHA; FileAbsent is rejected. Physical captures are
revalidated after success or failure. Vectors retain the current model metadata,
width, finite-value and model-generation checks. Metadata grants no authority.
MCP results include `data_key` alongside UID and existing small result fields;
Raw freshness still deduplicates requested paths and uses the current digest.

Current Linux qualification passes the three genuine JSON codec/SDK cases,
30 existing NativeNN integration cases, seven Main creation cases and 11
internal query/publication/Main guards. The physical JSON case uses actual
NULL Raw and a captured project file, rejecting missing/available Raw mismatch
and changed content. The JSON MCP case passes actual tiny CPU ORT CodeDocument
inference, genuine JSON assembly, SDK publication and HTTP CodeQuery delivery;
the two affected Rust MCP cases also pass, as do three model-schema fixtures.
It does not qualify all languages, index callbacks or embedding model quality.
The independent impact review found tuple/schema/Raw propagation consistent.
Exact commands, failures and current strict/format/link results are retained in
`.build/rust-native-nn-json-f13b/qualification-current.json`; Windows/macOS
remain unrun. No unchanged Main model gate or Auth suite was replayed.

## Local semantic Native/Web query through the existing model child

Task `tsk-d82c` composes `graphit_ast_search` in
[SourceServer](../../crates/graphit-app/src/source_server.rs) with the
[semantic route](../../crates/graphit-app/src/source_server/semantic.rs).
Both `mode: semantic` and `mode: vector` execute SDK NativeNN retrieval;
`metric` selects `cosine` (default), `l2` or `dot`, and `k` defaults to 10 within
the existing response cardinality limit. `query` is passed unchanged as
`CodeQuery` to ModelService. SDK recipes supply their prefix once.
`project_dir` is a [NativePath](../../crates/graphit-core/src/native_path.rs)
selector and `project_version` selects the captured foreground inventory.
This retrieval kernel reads the PROJECT NativeNN schema and configured model
identity. Imported semantic retrieval and FTS/hybrid remain separate required
integrations; the Foreground producer and its configured language profiles are
documented independently from this query route.

Foreground shares the existing `Arc<MemoryJobResources>` with SourceServer
before serving. It starts no second model process or pool. The HookPolicy's
fresh per-project module selection gates consumers: Embedding with
Memory, Ast or Knowledge can require the one model, while Memory-off projects
receive no Memory rescan callback or handle. Each request borrows the current
shared model-resource Arc and checks its actual Auth profile through the lease;
the query does not create a model, client or pool.

Each request authenticates its actual incoming Runtime/Local bearer, captures
the selected physical project/version, and admits `ProjectReadModule::Ast`
under the same Original. A project selector, valid credential or model identity
does not substitute for that PROJECT admission. The service captures Embedding
from that caller and uses its actual local lease with ModelServiceClient.
Source owns the request job and cancellation; shutdown joins it before model
and runtime-auth teardown. Waiting on the shared job mutex is cancellable
without creating another request or renewing the deadline.

The internal `SourceServer::ast_search_web` entry reuses that admitted kernel.
Its caller supplies the verified Web caller, fresh PROJECT ReadAst admission,
physical project/id/version tuple and the same Original. The kernel rejects
another module, Original, root, project ID or version context. Web input contains
only `query`, `mode`, `k` and `metric`; project selectors belong to the admitted
outer selection. It uses `capture_web_embedding_service` and the existing
LocalServiceLease at the actual inference boundary. The IPC RuntimeKey belongs
to the shared model resources and does not convert the Web principal into
Runtime or grant remote effects. Native and Web share Raw/pin/schema checks and
the existing result DTO. The HTTP owner retains cancel/join; forwarded errors
retain their typed cause in `io::Error::source`, including permission,
cancellation and deadline kinds. Browser HTTP/CPU consumer qualification is
tracked separately under tsk-9d92; this Source change alone is not that proof.

Imported AST uses `capture_imported_ast_embedding_service` with its borrowed
`ImportedAstReadAdmission`. That admission supplies the actual Web caller,
installation and Original; the factory rejects another issuer and rechecks
the admission at lease barriers. A matching Local rule retains the configured local
model. Explicit Broker AI rules may use only that Web read's own verified provider,
revision, endpoint, resource and bearer; arbitrary Direct Web execution is rejected.
Native imported reads require their actual RemoteRequest permission for Broker or
Direct inference. Optional reranking has its own capture using the same read proof.
The factories do not grant PROJECT access or accept a substitute caller or Original. The selected package's NN
model/[configuration](../guides/configuration.md)/dimensions must match the shared model's real identity;
this factory does not supply an NN package or imply imported search is complete.
Producer and consumer qualification remain tracked separately in `tsk-9d92`.

Full schema and model identity are checked before inference. `model_disabled`,
`model_unavailable`, `index_absent` and `index_empty` are distinct successful
outcomes. Captured NN absence is checked before model metadata or discovery;
hybrid search preserves its lexical results in this case. The empty-index probe reads only `uid` with a one-row
SDK limit; it does not load a vector, code projection or namespace merely to
decide whether the corpus is empty. Full row and distance validation applies
to the nearest results, including EOF of their returned stream.
Existing corrupt storage, mismatched model metadata
and transport errors are errors. A current Raw capture must exist for a
nonempty NN query. Every returned row must match its FTS FILE's actual Raw
version, physical/inherited branch observation and full Source SHA-256. Live
Source reads use that FILE's historical Raw pin; intact FILEs are not relabeled
when a different FILE advances Raw. Explicit exports retain real clones and
validate the FILE's Source hash within the captured clone. Only requested
paths are deduplicated for reading; inconsistent associations remain errors.
Raw NULL with `TextNotStored` uses the actual physical project file and the same
Original, with existing copy/read policies. An absent row or divergent FILE version,
branch or digest returns `stale_source`; errors never adopt older NN metadata or
trigger an error fallback. Raw and NN publication remain independent, and
these retained captures do not claim atomicity or continuous latest-HEAD checks.

Arrow JSON serializes only UID, data key, path, name, kind, language, grammar, name anchor,
extent, namespace and stored model generation, with SDK distance attached in
SDK row order. The response also reports model identity, current query-model
generation and observed NN/Raw SDK generations/branch. It excludes vectors and
the large code projection. Read admission, local lease, retained owners and
Original are rechecked before delivery, including after response construction.

The own [functional fixtures](../../crates/graphit-app/src/source_server/semantic/tests.rs) use the maintained tiny CPU GGUF model, real ModelChild/Auth/ModelService CodeDocument and CodeQuery, genuine Rust/JSON tree-sitter assembly, Arrow codec and temporary NativeNN/Raw SDK corpora. This tests the retrieval integration rather than EmbeddingGemma quality, all languages or foreground index callbacks. Affected commands select `source_server::semantic::tests`. The latter change preserves its closed-model-endpoint and cancel/join assertions; no product gate was weakened. The maintained model fixture is imported once by both test consumers.

## Oracle native application groups

The Oracle model exposes borrowed `oracle_applications()` and
`oracle_application(occurrence_index)` observations for native
`general_element_part`, `keyword_as_method` and `routine_name` CALLS
occurrences. Each retained occurrence keeps its own ordered groups and direct
arguments, optional native argument name, expression and KEEP clause. Repeated
groups remain distinct; nested argument lists belong to their own heads.
Origins are projected independently while PreparedInput is alive. Native
status/error/missing and nonexact origins remain observable; node presence
does not establish validity.

Boundaries are native ancestors in nearest-to-outer order, including bodies,
blocks, anonymous blocks, callable declarations, parameters and defaults.
A variable initializer can contain a `default_value_part`; the model retains
that actual ancestry. These observations do not assign a caller or classify
Function versus collection indexing. Other dedicated/dynamic/raw relations stay
retained without fabricated generic argument groups.

The Graph adapter adds optional `oracle_application` to the existing `observation_json` and FILE outcomes. At the C7 observation stage, it preserves ObservedOnly, no source index and no resolved edges. The local callable consumer below adds resolution only where native source and target proof both succeed. No NN/DTO/protocol/profile/G4 change is involved. Bindings, complete C7–C9, constructors, collection/LHS/bare methods, chains/@link resolution and publication remain necessary outcomes.

Boundary metadata contains only native status, ordinals and explicit Original ranges; it omits literal_source so callable/body text is not duplicated per outcome. Borrowed native Origins remain unchanged.

## Native collection statements and assignment context

The C8 extension adds three explicit raw selectors for native `collection_method_call` terminals DELETE, EXTEND and TRIM, using the current generic SDK compiler. The three PL/SQL stock copies keep all preceding queries in order; their query count becomes 172. The previous seven-family/84-descriptor dedicated-call fixture remains its own qualified scope.

`AntlrOracleApplicationKind::CollectionMethodStatement` retains the actual statement, first direct expression as receiver, subsequent direct expressions as indices and direct native delimiters. It does not fabricate `function_argument` groups. Native syntax does not prove the receiver's type or a Function endpoint. `assignment` records the nearest assignment statement, its direct Target/Value branch and actual intervening expressions. An application inside a target index retains that path; the location is not classified as a semantic write.

The collector independently projects these nodes while PreparedInput is alive. Graph adds optional `collection_statement`/`assignment` within the existing observation JSON, keeps ObservedOnly/no source index/no edges, and omits literal_source for structural statement/branch/path nodes. Receiver/index expressions retain their own native facts. No Source authority, NN schema, SDK/G4 or DTO protocol is changed.

## Oracle binding and prepared-source continuity

B1 collects native declarations and application references, projects required
Origins while PreparedInput lives, then resolves local receiver/head and nominal
type references during assembly. It uses the existing sqlparser codec and Type5
producer; it adds no parser, dependency or publication protocol.

Scope markers alone cannot prove lexical coexistence: a preparation can join two
original blocks by removing END/BEGIN while keeping selected anchors Exact.
Every visited scope therefore retains a real projection of its prepared prefix
through the reference. Resolution requires byte-identical Original pieces in
contiguous original order. Synthetic pieces, removed gaps, reordering and mapped
text changes make the lookup Indeterminate. Contiguous Compound pieces can pass;
the implementation does not coalesce or manufacture an Original hull. Markers,
ordering and anchor membership are checked separately.

This proof concerns each visited scope's prefix through the reference. Any
Synthetic source or gap inside that prefix makes lookup Indeterminate, even in
a nominally unrelated declaration or sibling. Source outside all visited
prefixes, including source after the reference, does not invalidate the lookup.
Cancellation/deadline and native recovery remain observable. Type detail
validation is shared with ef54 and does not depend on a Type entity being emitted.

## Collection member continuity

B2 collects the native member carrier and required Origins before PreparedInput
drops. Its private receiver lookup accepts only a complete simple native prefix;
it does not relax B1's ordinary reference checks or infer a receiver from an
earlier chain component. Child positions are collected once for member lookup.

Scope-prefix evidence alone does not establish continuity after the receiver.
Each member therefore retains `member_continuity`: its real native syntax and
Original projection with a verified result. The entire member must be nonempty,
recovery-free, contiguous, ordered and byte-identical to Original. Required
name/separator/group/argument/expression fields remain independently usable.
Present with an empty span is insufficient.

A contiguous Compound member can pass. Removing text between receiver and
separator, Synthetic name/separator and changed MappedAtom projections cannot
grant a method. An identical whole-span MappedAtom may prove that span while its
partial name/reference projections still block binding. No hull, fabricated
projection or text repair replaces these causes.

## Local Oracle CALLS consumer

The [local model contract](rust_ast_antlr_model.md#local-oracle-callable-references)
adds borrowed caller/body and lexical target outcomes without changing raw C7
occurrences, B1 lookups or B2 classifications. Only a Bound native callable source
and Resolved permitted local Function/Procedure entity produce CALLS between real
Graph node keys. Observed, ambiguous, unsupported and indeterminate cases retain
their actual causes; Package/global/qualified/database-link/compiler overload
resolution and full C7–C9 publication are outside this leaf. An uncertain charset
comparison cannot be discarded as a proven noncallable.

## Gemma BF16 AST consumer composition

The productive AST caller already sends CodeDocument through the authenticated ModelService with effective RPC limits; native search sends CodeQuery through its own genuine admission and finite Original. NativeNN reuse compares the full expected schema, including `vector_id`: matching dimensions alone do not permit reuse.

## Generic NativeSource format-3 cutover

The shared schema authority is now `ast-native-nn-4`, with the existing 21 fields
and nullable `model_generation` for remote embeddings. Older schemas require
ordinary derived reindexing; see the [provider and rebuild contract](rust_tree_sitter_native_nn.md#embedding-providers-and-optional-query-reranking).
The [generic consumer](rust_tree_sitter_generic_model.md) adds raw Original
NativeSource names and complete native namespace/Entity frames while preserving
specialized representations. Deserialized metadata does not grant provenance or
authority. Tag 2 is rejected and derived data reindexes normally; no reader or
metadata repair is provided. Earlier qualification receipts remain historical
evidence for their recorded schemas. C now has 15 passing Linux focal cases,
including the registered real CPU Main NN scenario with increment/reuse and
model change. See [current scoped qualification](rust_tree_sitter_native_nn.md#generic-format-3-qualification);
its quality/platform limits remain explicit and this does not close the parent
migration.

## Oracle configured object targets — a4cd qualification

The original gap was the stock PL/SQL object TargetRules path, where the ANTLR
Graph adapter only resolved local callable targets. The object consumer below
is implemented and genuine App publication passed Root36a. SCIP
Definition joins and ordinary publication alone do not resolve Oracle references.

The reduced API lives in semantic_graph::oracle and leaves generic batch/parts
and other-language constructors unchanged:

- collect_oracle_graph_rows(document, batch, Original) copies compact owned
  AstOracleGraphRows while the genuine native document is alive. It checks the
  document context, Source digest/components and actual entity UID/data_key/label.
- resolve_oracle_graph_reference(reference, candidates, Original) resolves over
  eligible actual endpoints from the same authorized job. Root retains final
  CurrentProject/Raw/physical pins.
- apply_oracle_graph_resolution(FILE, resolution, Original) changes only the unique
  matching deferred occurrence in the genuine observational FILE. Path,
  components, language/grammar, raw version, Source hash and branch association
  must match. A mismatched or already consumed observation rejects atomically.

The first supported object form is a native tableview_name reference inside a
proved Function/Procedure body, with a genuine Table endpoint. Schema components
and quoted names come from the retained grammar nodes and the existing mature
SQL word codec; no dot splitting, schema removal or guessed Source offset is
used. Quoted names remain exact; the established ASCII unquoted equivalence does
not assert an unknown database character set. Unsupported names/scopes,
database links, normalization origins and recovery preserve explicit causes.
A SELECT with a native with_clause is an unsupported scope barrier: a CTE with
the same spelling cannot resolve to a global Table.

Allowed labels union the configured relationship rule and query target_labels
for that same relationship type. An explicitly empty language-rule list admits
the graph/value/parent labels declared by that language. Only a unique available
eligible endpoint produces an edge. Zero candidates yields NotFound; multiple
genuine endpoints yield Ambiguous; unavailable source/target and unsupported
scope/origin/codec yield an explicit unresolved cause. Legacy fallback metadata
does not authorize a fabricated placeholder or edge. Other native observations
and the qualified local CALLS consumer remain intact.

The App ANTLR branch collects rows before dropping the model and passes them to
graph::append_oracle. graph/oracle stages endpoint payloads and deferred FILE
observations in the existing writer SQLite database. At finish it joins candidate
keys to actual same-job entities, resolves each occurrence, updates FILE outcomes,
deduplicates identical occurrence edge identities and completes the existing
publication flow. It retains neither a corpus of parser trees nor Source bodies,
and does not add a new writer, compiler service or SCIP interpretation.

Focal oracles and actual qualification:

- semantic_graph_antlr::oracle_target_rules_* passed five actual Raw/model/Graph
  focals in Root34f (18.78s); this covers
  cross-file Table/Procedure, zero/two targets, allowed-label policy and query
  union, qualified quoted names, owned serde/determinism, wrong FILE associations,
  recovered unavailable target, synthetic origin, cancellation/deadline and a
  homonymous native CTE.
- foreground_source::ast_source_tests::oracle_target_rules::
  foreground_oracle_target_rules_cross_file_and_ambiguous uses the actual
  configured foreground, ReadAst admission, Source branch and Graph publication.
  It adds/removes a second real Table and expects unique/ambiguous/unique edges
  and matching observational FILE outcomes. The same captured publication now
  supplies its validated relationship schema, FILE query and required typed
  SELECTS query in each unique cycle. Unique cycles require one actual
  Procedure→Table member and one queried edge. The ambiguous cycle requires no
  SELECTS member and Ambiguous FILE outcomes; the writer has no relation table
  when it emits no members. This is a genuine schema observation, without
  catching a binder error or fabricating query rows. Cycle/generation/member/FILE
  diagnostics distinguish an absent unique edge from legitimate ambiguity.
