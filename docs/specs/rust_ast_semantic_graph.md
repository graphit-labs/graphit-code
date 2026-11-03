---
title: "Native AST Graph producer"
doc_type: technical
description: "Owned per-file Graph observations from genuine semantic models and Source, independent of NN inference."
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-fb2c}
    relation: implements
  - target: {type: task, id: tsk-9fa2}
    relation: implements
  - target: {type: task, id: tsk-a4cd}
    relation: implements
  - target: {type: task, id: tsk-50e1}
    relation: reuses
  - target: {type: task, id: tsk-d29d}
    relation: reuses
  - target: {type: task, id: tsk-9d92}
    relation: consumed_by
  - target: {type: task, id: tsk-db46}
    relation: uses
  - target: {type: task, id: tsk-6866}
    relation: reuses
  - target: {type: task, id: tsk-f13b}
    relation: reuses
  - target: {type: task, id: tsk-77c6}
    relation: implements
  - target: {type: task, id: tsk-6eae}
    relation: implements
  - target: {type: task, id: 01M5C6SF19TSS3NE06D71HSM7N}
    relation: implements
  - target: {type: task, id: 01M5C9XXR0ZE2Q2GX2MMHA6ZBD}
    relation: implements
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
---

# Native AST Graph producer

`semantic_graph::assemble_graph_ts(AstGraphTsInput { document, source }, original)`
assembles an owned `AstSemanticGraphBatch` from the retained
`TreeSitterSemanticDocument`. It uses the actual model, identity document,
Original allocation and Source binding once per FILE. The caller supplies
`NativeNnSource::Indexed` or the existing genuine physical Source owner;
the shared Source type does not introduce an NN model or vector dependency.

The adapter checks the same Original pointer before the exact
`native_nn::associated_source` helper, which validates path/components,
full digest, actual Raw and allocation. Original operation checks bracket the
entire assembly, including failed results. The helper's signature/algorithm
remain unchanged; only crate visibility permits this consumer. Empty models
use their genuine IdentityDocument and still produce a checked file association.

The batch, error and rows are Send. Node/edge/association/outcome structures
contain owned observations. Batch.into_parts() returns public AstGraphParts
fields file_observation, association, nodes, edges, outcomes, diagnostics_json and
capabilities_json.
Node.into_parts() moves (label, key, properties); edge.into_parts() moves
(kind, source_key, target_key, uid). App maps owned values without cloning rows. Serialization cannot recreate
SyntaxIdentity, parser ownership, Source authority, filesystem permission or
freshness. Root owns Data writer, App callback, publication and captured
cross-file catalogue integration.

## Writer boundary

AstGraphValue has String, Int64 and Bool variants. App maps these to the concrete
Data writer variants. AST does not import the Data Graph DTO or mix Arrow/Parquet
versions. Optional properties are absent; diagnostic and structured metadata are
explicit JSON strings.

| Output | Actual contract |
| --- | --- |
| Node key | Mature serde_json serialization of [SyntaxUID, data_key]; uid/data_key remain separate properties. |
| Common properties | uid, data_key, name, raw_name, path, **lang**, grammar, Raw version, full source digest and line_number. GraphNodeSummary reads lang and line_number. |
| Raw version | Association retains actual u64; node property raw_version is its lossless decimal String. GraphGeneration/NodeId are different identities. |
| Name anchor | name_start/end_byte, name_start/end_row/column, name_start/end_line; bytes/points are Original, rows/columns zero based, lines one based. |
| Full extent | start/end_byte, start/end_row/column and start/end_line only for Exact extents; missing/nonexact/conflict status is explicit with no invented coordinates. |
| line_number | Original name-anchor row plus one, checked into Int64. |
| Availability | Bool from actual entity diagnostics/conflicts/extents/namespaces; independent of NN embed_labels. Import/Parameter therefore remain Graph rows. |
| Source observation | IndexedStored {empty} versus PhysicalTextNotStored {empty}; source_stored/source_empty Bool properties. Stored empty is available and distinct from Raw NULL, including a genuine empty physical file. |
| Metadata | Native declaration/type/signature/value observations, actual contribution/query/match/name-capture slot, capture names and SDK properties, namespace/context/parent-capture provenance, exports/comments/complexity/issues and Original origins. |
| Batch observation | capabilities_json and diagnostics_json preserve query capability/policy and actual model issues. |

Full Source is borrowed for association, not copied into a file-body property.
Native syntax and attachment snippets retain literal_source and actual ranges.
Go/Rust raw text is not labelled decoded_value. Large numeric/string literals retain
authored bytes instead of rounding, unquoting or evaluating expressions.

OriginalProjection serializes its exact/compound/synthetic/unmappable/ambiguous
variants and constituent ranges explicitly; no SourceRange Debug serialization
duplicates the complete file. Query capture ordinals and properties stay linked
to the actual subject occurrence. SDK/node indices are observations in this
retained parse, not stable Graph or symbol identity.

Metrics, exports and declared types remain in metadata_json. The inspected
GraphNodeSummary/imported_graph/icebug_query consumers do not require additional
primitive properties for them; this does not create independent Cypher columns.

## One file observation for the writer

The batch.file_observation() getter borrows one owned AstGraphNode, and
AstGraphParts.file_observation moves it. The caller moves semantic nodes and
this observation separately into the same writer: nodes.into_iter().chain(
std::iter::once(file_observation)). nodes() continues to contain semantic rows.
The label is AstFileObservation, a data observation rather than a syntax entity.
The internal record has is_file_observation: Bool(true). Semantic rows omit this
flag and retain their uid/data_key tuple2, even when a configured graph_label
also equals AstFileObservation. Consumers distinguish the record by the separate
field/flag and key shape; they must not hide the entire label or reserve it silently.
Its canonical JSON key is ["file_observation", actual_path, [language, grammar]],
with three members distinct from the two-member semantic [uid, data_key] key.
It carries no uid, data_key, symbol anchor or fabricated FileSyntaxUID.

Properties retain path/name, lang/grammar, relative_components_json, actual Raw
version as decimal String, full digest, optional observed branch/inherited
association and source_stored/source_empty. diagnostics_json, capabilities_json
and resolution_outcomes_json occur once in this file record. Outcomes preserve
relation/source/capture/evidence indices, typed target state and actual resolved
or ambiguous keys, explicit unresolved reason, and the full raw observation
including effective TargetRule. This avoids losing globals when App moves only
Graph nodes/edges to Data. No full-file body is added.

A successfully associated empty Source or a model with zero semantic entities
still emits this record; diagnostics from unsupported/recovery observations
remain available. Physical Raw NULL and stored empty remain distinct. This
serialization is read-only evidence and cannot recreate Source/parser authority,
ReadGrant, Original ownership, publication freshness or filesystem access.
Root owns the Data writer append, publication and CLI/MCP query integration.

SDK process/parser addresses remain runtime-only identity evidence. The persisted
Graph contribution and raw relation wires omit sdk_name_address and sdk_node_id;
actual capture names/slots, stable collection indices and Original ranges retain
provenance. For unchanged Source and effective profile, independently retained
parses must produce equal complete nodes/properties, FILE observations, edges and
outcomes without comparator normalization. Node/edge keys and persistent digests
cannot acquire runtime pointer identity. NativeNN has no such pointer column.

## Selected profile with unavailable semantic model

observe_unavailable_graph_file(AstGraphUnavailableFileInput { profile, source,
reason, identity_limits }, original) returns only a genuine AstGraphNode file
observation. The typed reason is NoNativeSemanticModel or
UnsupportedNativeContext. The caller uses UnsupportedNativeContext only for a
confirmed unsupported semantic context; invalid configuration, ABI, permission,
integrity and Source failures remain typed errors and are not converted to this
observation. It does not construct a ParsedDocument or a zero-row
batch that could imply an executed semantic model. Root can move this one
record into the writer and continue supported files while retaining required
model gaps in the implementation backlog.

The Source is borrowed from the actual Raw SDK record or retained physical
capture. Raw missing/NULL through Indexed is rejected; physical Source requires
its real NULL-text association. A private pure IdentityDocument is created only
from those actual bytes, path components, selected effective profile and
Original to reuse the existing association kernel. No symbol is constructed,
restored or exposed. Pre/post operation checks and physical revalidation also
apply to failures.

The observation retains the same key3/marker/source association and explicitly
sets model_available=false, model_unavailable_reason and file diagnostics.
Each configured query is NotEvaluated with its policy and reason; no fake zero
match count is recorded. Empty raw outcomes are not interpreted as zero target
candidates. A successfully executed model with zero entities has
model_available=true and no unavailable reason. For this unavailable observation,
lang/grammar are the actual selected profile's configured values; they do not
assert that a parser or semantic model ran. Metadata cannot grant Source access
or substitute for the missing model.

## Active JSON model

assemble_graph_json(AstGraphJsonInput { document, source }, original) consumes the same retained JsonSemanticDocument as native JSON NN. It does not collect, parse or reconstruct observations again. It returns the same owned batch with the actual Raw/Original/Source association checks.

Every assembled entity becomes a Graph node, including Value and custom labels outside embed_labels. name_representation = JsonName follows the producer: strings use mature serde_json decoded names, including empty, NUL and escaped names, while scalar names preserve their lexical representation. raw_name and its exact Original anchor remain separate.

Contribution metadata retains name_slot/value_slot, query/match indices, actual capture names and query policy/properties, Original origins, extent and declared-value status. Native namespace frames remain metadata and do not create ownership edges. NotConfigured, Missing, Unavailable and Present remain distinct from conflicts. Present literals record literal_source and Original range; String/Bool/Null record genuinely decoded values. Number retains mature RawValue validated_number_lexeme as a String, preserving exponent, large precision and distinct lexemes such as 1 and 1.0 without arithmetic conversion.

Model issues and optional Original origins remain diagnostics. Query capabilities record the actual adapter/query kind and match count. Configured relation queries retain raw captures/properties, data_key/relation_type and the effective global TargetRule labels/fallback (or explicit absence) as ObservedOnly alongside UnsupportedQueryKind diagnostics. This producer has no resolved JSON target API and emits no guessed CALLS or HAS_* edge.

## Relations

Only LOCAL Resolved targets and genuine Bound sources create edges.
Endpoint keys reference actual emitted entity tuples. Edge UID serializes kind,
source key, target key, relation data_key and exact Original capture byte bounds.
Changing query order does not change that identity. Concordant repeated
observations coalesce by this UID; every raw occurrence remains in outcomes.

Outcomes preserve query/captures/properties/TargetRule/source evidence and
Resolved, NotFound, Ambiguous, Unresolved or ObservedOnly states. Go import,
dynamic interface, shadow, anonymous and unsupported/nonexact observations do
not receive guessed endpoints. Configured namespace frames/receiver references
do not become invented vertices. Raw relations remain ObservedOnly without supporting semantic evidence;
the common SCIP join resolves only targets justified by associated definitions.

Cross-file catalogue resolution, productive writer/publication/update/delete/
rename/refresh/query/EntitySource integration and all-language semantic coverage
remain required parent/Root work. The AST adapter does not assert their completion.

## Qualification

The affected [semantic_graph_go.rs](../../crates/graphit-ast/tests/semantic_graph_go.rs)
uses native Go/Rust factories and actual Raw Lance stores, query-order and
tuple candidates, all ten supported Go families, Original ranges, local
read/write/call edges, wrong Original/allocation/path and actual cancellation.
It verifies stored-empty versus real Scope/Project physical Raw NULL, physical
empty and changed-file rejection. A retained model can drop while the owned
batch crosses a thread.

The affected [semantic_graph_json.rs](../../crates/graphit-ast/tests/semantic_graph_json.rs)
uses the genuine JSON model/Raw SDK for stock14 entities, Original ranges, owned
move/thread handoff, empty/NUL/escaped names, exact large/exponent number lexemes,
UID/data_key separation, raw relation diagnostics, conflicts, foreign Source/
Original/allocation/path and cancellation. Three cases qualified by composition;
strict lib+JSON Clippy passed. This does not prove Root publication.

The [file observation fixtures](../../crates/graphit-ast/tests/semantic_graph_file_observation.rs)
qualify native Go target states, stored-empty/zero entities, JSON raw-only issues
and custom-label collision. The affected physical Raw NULL case qualifies the
record for empty/nonempty captures and changed-file rejection. Strict affected
Clippy and fmt/diff passed; Root writer/query integration remains separate.

Run:

```text
cargo xtask cargo test -p graphit-ast --test semantic_graph_go --locked --offline -j4
cargo xtask cargo test -p graphit-ast --test semantic_graph_json --locked --offline -j4
cargo xtask cargo test -p graphit-ast --test semantic_graph_file_observation --locked --offline -j4
cargo xtask cargo clippy -p graphit-ast --lib --test semantic_graph_go --test semantic_graph_json --locked --offline -j4 -- -D warnings
```

## Configured Tree-sitter value rows

The retained model emits a separate node for a proven configured
`value_label`, preserving the query's `data_key`, SDK properties and actual
value capture Original anchor/extent. The declaration's value metadata remains
present. Names retain literal Source, including quotes and escapes; they are
not IdentifierName or evaluated scalar values.

Contribution JSON records `derived_value` with actual declaration
`owner_uid`/`owner_label`, native declaration/name indices,
`owner_name_capture_index`, `value_capture_index` and binding. The contribution
name-capture index points to the actual value capture. Native namespace frames include the real
declaration owner with selection `DerivedDeclarationIdentifier`; context and
parent-capture slots remain null for this derived frame. These indices are
retained model ordinals, never native process addresses. No HAS_VALUE or
CONTAINS edge is invented, and a value containing an anonymous function does
not become a callable source through container equality.

All such nodes remain in Graph independently of embedding policy. Genuine Raw allocation/path/hash/version and Original cancellation checks apply to the same owned batch and its separate FILE observation. NativeNN format 2 distinguishes literal representations explicitly, preserving exact names and normal symbols when a custom embedding policy selects both.

## Retained ANTLR model rows

`semantic_graph::assemble_graph_antlr(AstGraphAntlrInput { document, source }, original)`
consumes the genuine `AntlrSemanticDocument` for Oracle `plsql/antlr-plsql` and
DB2 `db2/antlr-db2`. The exported input borrows the retained document and the
existing `NativeNnSource`; the returned `AstSemanticGraphBatch` owns its rows.
Callers keep the acquisition's exact Original and Source allocations. The adapter
checks Original pointer identity and reuses `associated_source` for path,
components, digest, Raw version and physical/inherited branch observations.
Foreign Original, copied source or mismatched path fail through typed errors;
actual cancellation/deadline bracket assembly. `AstGraphError::Antlr` preserves
an `AntlrModelError` cause through Display, Error::source and From.

Every actual model subject emits its configured label and `[uid,data_key]` key,
including Parameter, Alias and Variable outside embedding policy. Display names
use the mature identifier decoder; DB2 qualified anchors preserve the complete
raw name with native component ordinals, ranges, decoded components and quote
styles. Oracle callable/package names instead anchor their final native component;
the full ordered QName stays in metadata with separate component ranges, including
a package schema component. Function, Procedure and Package headers use the same
existing adapter as other model entities. Optional END package names remain typed
reference observations without another entity. Native ancestry supplies namespaces and owner-link indices. A qualifier
such as APP does not itself create a Schema vertex or classify a datatype.

For actual Oracle Parameter/Type subjects, optional oracle_declaration metadata
retains the [typed native declaration facts](rust_ast_antlr_model.md#oracle-native-parameter-and-type-observations):
mode order, default/bound expressions, five Type shapes, element/index/definition
observations and direct-parent modifiers. Presence and Original projection status
are explicit; empty required native spans produce model diagnostics and unavailable
code. Other subjects omit this key. No binding, CALLS edge, DTO or NN schema is
created by these facts.

Entity metadata records declaration role/ordinal, qualified name, contribution
captures and policy provenance, extent/type/value observations, all typed roles
in native order, namespaces, diagnostics and conflicts. Source ranges remain
Original; prepared capture positions are explicitly separate. NonExact, Missing
and Conflict extents have status without invented coordinates. Availability
requires exact extent and namespace observations without entity conflicts or
diagnostics. Failed group capture marks each affected genuine subject unavailable;
an individual name policy exclusion does not contaminate another group subject.

Value statuses NotConfigured, Missing, NonExact, Scalar and Expression remain
distinct. Scalars retain literal Source/range plus mature token data: Number
keeps the exact validated lexeme without floating conversion; quoted and national
strings keep decoded values; TRUE/FALSE/NULL retain their actual decoded value.
Expression text is retained without evaluation. Capabilities preserve each
configured query, provenance and actual match count; native syntax and model
diagnostics remain distinct observations.

All retained ANTLR relation occurrences become `ObservedOnly` outcomes with
query policy, captures, contexts, evidence ordinals and configured TargetRule or
explicit absence. The model has no resolved-target API, so the adapter emits no
edges. Names and ranges do not guess CONTAINS, HAS_VALUE or call targets. The
existing `AstFileObservation` helper emits an owned FILE record even for an empty
model; indexed stored-empty remains distinct from Raw NULL.

The implementation is in
[semantic_graph/antlr.rs](../../crates/graphit-ast/src/semantic_graph/antlr.rs).

## SCIP-default producers and the common join

SCIP is the normal index format; `ast.scip.enabled` is removed rather than
reinterpreted as a hidden Docker selector. The family orchestrator attempts the
ten maintained complete producers and preserves their pinned/custom behavior.
An admitted file without a usable compiler document receives official
syntax-derived SCIP from its native model. See the
[family and fallback contract](../guides/ast_extensibility.md#scip-as-the-standard-index-format).

The syntax producer is
`produce_scip_syntax_document(AstScipSyntaxInput { graph, source, project_id,
project_version }, original)`. It uses official SCIP 0.10/protobuf 3.7.2 and the
official symbol formatter, validates source SHA/ranges and logical paths, and
borrows genuine `AstGraphParts`. Equal entity spellings must not collide. Its
SCIP occurrences/symbols carry only evidence available to that model; local
references do not imply compiler types, import/overload or cross-file resolution.

An entity with `Missing`, `NonExact`, `Conflict` or `Unavailable` extent still
contributes a definition at its proven Original name range, without an
`enclosing_range`. Its native observations remain in Graph. Unknown status values
and invalid name/source associations remain errors.

Tree-sitter typed/generic/JSON and all registered ANTLR paths retain their real
models and fields. Oracle/DB2 remain; generic PostgreSQL/TSQL/COBOL observations
complete the other dialects. Configured labels, relations, source/ranges,
namespace/owner frames, calculated complexity and embedding fields survive in
the native graph even when standard SCIP has no corresponding field. They are
not smuggled into documentation/diagnostics or a private wire protocol.

COBOL's Raw Compound observation remains intact. Its Original `programUnit` body
can carry `SequentialOriginalCoverage`, admitting only the seven actual spaces
of generated line-start trivia from the maintained preprocessor. The provenance
remains distinct from Exact; Source allocation, ordered Original pieces and
proven removed gaps still bound the physical extent.

A valid compiler document can also supply definitions for admitted source with
no native syntax grammar. `assemble_scip_source` uses the real Raw/Source and the
honest `scip-compiler` grammar identifier for VB from the maintained .NET
producer, which supports C#/VB. Native-only admission requires `NoSelector`
and empty grammar filters, without reviving replaced stock, exclusive or
explicitly overridden routes. No synthetic Tree-sitter document, computed
metric or NN vector is created to disguise missing native support.

The stock TypeScript profile selects `.ts`, `.mts` and `.cts`, including module
declarations such as `.d.mts`; JavaScript selects `.js`, `.jsx`, `.mjs` and
`.cjs`. These files retain their existing native syntax fallback when a compiler
document is absent. They are not compiler-only routes. TSX keeps its separate
profile, and explicit profile/grammar overrides retain their authority.

Compiler-only Source identity admission preserves the existing closed
`IdentityError` through the ordinary operation-error boundary, including its
code, field and optional index. An invalid language and a per-file Source limit
therefore remain distinguishable without retaining Source text or paths in the
diagnostic. The surrounding Original checks still return cancellation, deadline
and authorization errors; invalid input is never repaired or accepted silently.

Compiler symbols are evidence for semantic binding; native identifiers, ranges
and Source authority remain observed data. Reference resolution requires the
actual scoped symbol and a valid associated definition, not equal display names.
Definitions supplied only by the compiler retain official documentation and
signatures in `scip_symbols_json`. They omit parser `metadata_json` rather than
inventing syntax contributions; the entity-search consumer accepts these nodes
with their associated Source extent.
Ambiguous, external and unbound targets retain explicit outcomes. Compiler
position encodings are preserved for interpretation against the associated
Source; the syntax emitter explicitly uses original UTF-8 positions. Byte,
UTF-16 and wire coordinates are not interchangeable by relabeling metadata.

## Family operations, inputs and errors

The App consumes the maintained Docker producer contract: `/source` read-only,
fresh `tmpfs` `/overlay`, writable merged `/workspace`, `/output/index.scip`,
family caches and real output ownership. Unix UID/GID and Windows
`output-mount` ownership are alternative modes. Positive integer
`GRAPHIT_SCIP_TIMEOUT_MS` comes from the finite compiler operation, not a fabricated principal or legacy image mount.
Version/tag and pull policy retain their actual meanings; there is no opt-in.

Input capture distinguishes admitted Raw files from build metadata. It retains
logical paths, physical identities and digests, then checks them around compiler
execution, association and publication. An indexer's dependencies or generated
files do not enlarge the admitted source set. Metadata/path declarations alone
cannot prove association. Native paths/cache keys retain their platform units;
family/project/root/version isolate targets and output.

Missing Docker, compiler unavailability after confirmed cleanup and missing
eligible documents permit per-file syntax SCIP. The SDK visitor rejects malformed
compiler output; the pipeline can explicitly fall back after rolling back that
compiler import and rechecking the parent Source Original, authority,
cancellation and deadline. Partial compiler indexes are never published.

A completed compiler with a nonzero exit reports its family, exit status,
captured stderr and the first 64 KiB of captured stdout in the local daemon
diagnostic. Some indexers, including scip-dotnet, write their failure reason to
stdout. This diagnostic does not change fallback eligibility or expose the
compiler transcript through search responses.

A timeout of the compiler's derived operation can become compiler unavailability
(`Ok(None)`) only for a typed Process timeout with `Cleanup::Complete`, completed
Process join and container removal, and no remaining cleanup owner. The parent
Original must still be live and the current authority check must succeed. The
diagnostic exposes only family, reason, status and cleanup metadata; the existing
native syntax path handles the absent compiler result. The API remains
`Result<Option<_>>`, and the background embedding RPC budget is not substituted
for the compiler's finite policy.

Parent Original cancellation or expiry, Source/build-input integrity failures,
authorization failures and incomplete cleanup remain terminal causal errors.
Process/container/output owners are retained and joined; cleanup has its own
finite context and is not abandoned when the caller cancels. Earlier
Source/Docker proofs retain their own scope and do not by themselves qualify
the timeout classification.

## Canonical per-file SCIP staging and publication

`foreground_ast_sources/graph/scip_index.rs` is the common consumer. The former
App `graph/scip.rs` Rust-only join is not a parallel path. Oracle's existing
resolver is preserved alongside the common pipeline.

The official streaming reader visits metadata/documents/external symbols
without retaining a whole protobuf corpus. Each admitted compiler document is
staged with its family and observed source digest; nonadmitted documents do not
become graph authority. A file without a compiler document uses the syntax
producer. Private SQLite staging holds official compiler documents and the
original parser batches needed for unfinished compiler enrichment. Resolution
facts belong to the active native FILE store; there is no second portable
resolution table or end-of-corpus writer. Staging is not a public protocol or a
reconstruction of Source/Tree authority.

The common join consumes actual definitions/references and their Source
association, keeping native keys and original ranges. An explicit export streams
the current official documents into `index.scip`
alongside its captured Graph. Full indexing does not automatically rebuild a
portable graph or clone Raw/NN after inference. NN publication remains separate,
with its own model/schema/source checks and causal readiness.

### NN progress and public Graph visibility

The local daemon publishes each processed FILE's native graph and FTS before
corpus EOF, independently of embeddings. Its model-independent projections are
persisted for background inference; every confirmed NativeNN batch becomes
queryable through a coherent, authorized capture. A project can therefore have
structural and lexical results with no semantic coverage yet, then an increasing
set of semantic results as batches commit.

The incremental path selects changed FILEs only. It replaces their owned graph
rows and FTS, invalidates incompatible vectors, and resolves only affected
cross-file symbol relationships in the native graph. Intact source/projection
observations and vectors retain their real Raw pins. No per-FILE portable
Parquet/CSR rebuild, whole-corpus clone or global Raw-version rewrite is needed.
Source/rerank lookups use the associated FILE's historical Raw version on the
same physical branch. Configuration changes are not mistaken for identical
source: each supplied FILE replacement gets a new native structural revision.

The [FILE checkpoint contract](rust_tree_sitter_native_nn.md#single-ast-corpus-and-progressive-publication)
retains upsert-only intermediate batches and complete-FILE final replacement
across all profiles/embedded languages. Durable pending projections permit
retry without keeping parser owners or rediscovering every FILE. Inference-only
failure preserves the already available structure and is reported explicitly.
Authorization, Original, source/model integrity, cancellation and CAS remain
required; partial coverage never authorizes stale vectors.

Immutable [Graph/Raw/NN artifacts](rust_native_publication.md#captured-files-and-snapshots)
remain separately captured exports. Their completion does not gate active local
Cypher, FTS or progressive NN. The local storage and reader contracts are in
[local graph](rust_ast_live_graph.md) and [local search](rust_ast_live_search.md).
Source implementation, focused producer qualification, daemon integration and
installed activation are distinct evidence boundaries.

## Qualification boundary

The maintained Rust dispatcher/entrypoint sources and local TypeScript/Rust
images have been built. TypeScript passed a local smoke and emoji UTF-16 position
fixture. The real Rust Docker Source journey passed in 34.98 seconds with App
artifact `cc66436b6a5f3d1b`, local image `842d/rust-src`, version
`scip-default-local` and explicit `pull=missing`. It validates the real compiler
through Raw/Source and Graph, cross-file CALLS, FILE target association,
physical-change rejection and cleanup. The retained evidence is
`validation/scip-default-pipeline-20261020/native-rust-source-final-repair.log`.
The other eight images, registry publication and additional platforms remain
unqualified here.
Earlier Rust-only Docker/SCIP fixtures qualify their recorded component scope.
They do not prove the new default, the complete family routing or publication
of images containing the maintained Rust entrypoint. Configured release/CI
matrices are not executed platform or registry evidence. Keep existing unchanged
producer proofs with their limits; distinguish focused process fixtures from
real Docker execution and Linux from native Windows/macOS/arm64 coverage.

The integrated Source consumer passed all Compiler/Absent/Failed scenarios in
51.47 seconds. It validates published official SCIP for Rust, TSQL and JS, finite
NativeNN with Raw/hash/UID/data_key matching Graph, exact Source and CLI-to-MCP
delivery. Evidence is retained in
`validation/scip-default-pipeline-20261020/source-consumer-final-attempt-04.log`.
This integrated fixture is distinct from the real Rust Docker journey above.
Final strict library Clippy passed in 36.58 seconds for `graphit-ai`,
`graphit-daemon`, `graphit-ast` and `graphit-app`, using the canonical local target,
`CARGO_INCREMENTAL=0`, the maintained Ladybug SDK and
`cargo clippy --locked --offline -j2 -p graphit-ai -p graphit-daemon -p graphit-ast -p graphit-app --lib -- -D warnings`.
Evidence is retained in
`validation/scip-default-pipeline-20261020/final-clippy-repair.log`.
Runtime installation and registry publication are qualified separately from
these fixture receipts. These results do not qualify the
other eight images or additional platforms, or complete the end-to-end validation.

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

## Oracle local-binding metadata

B1 adds optional `oracle_binding` beside `oracle_application` in the existing
outcome JSON and FILE observations. It carries lookup states, actual native
candidates and entity indices, scope marker/continuity evidence, Type5 field
evidence, classification and typed issues. Scope continuity preserves the
actual prepared bounds and OriginalProjection returned by PreparedInput.

All these structural nodes omit literal_source before allocation. No declaration
or body corpus is copied per outcome. Owned rows remain deterministic across
independent acquisitions and survive the parsed owners.

A proven collection operation is a local semantic discrimination. Relation outcomes continue to preserve TargetRule, ObservedOnly, no source_index and no resolved edge. Type declarations never become Function endpoints. There is no NN, UID or shared DTO change.

## Collection member facts in Graph

B2 adds optional `oracle_application.member`,
`oracle_binding.collection_method` and `oracle_binding.member_continuity`.
The carrier retains native receiver/name/PERIOD/delimiters; continuity retains
the actual syntax/Original projection and verified result. All new structural
nodes omit literal_source before allocation.

The genuine Raw Source fixture keeps two parsed owners alive simultaneously,
assembles each Graph, compares complete nodes/FILE/outcomes and then moves owned
output across a thread after the parsed owners drop. It covers six proven
accessors and genuine duplicate query provenance.

These facts discriminate local collection operations. Outcomes retain ObservedOnly, source_indexNone, empty edges and existing TargetRule/UID/NN/Source authority. No Function endpoint or shared DTO/protocol is created.

## Python retained-model Graph consumer

The exact Python native model flows through the existing owned Graph adapter. Python contributions append `python_declaration` and Python callables append `python_signature` only when present. These fragments retain native kinds, node indices, prepared/Original range observations, policies and issue metadata. Structural signature/leading-string fragments do not duplicate literal Source. Other-language wire fragments remain unchanged.

Only proved LOCAL targets produce edges. Decorator/header calls and class-private/implicit-class names with unsupported environments remain explicit outcomes, with no false global-homonym edge. FILE remains a separate `AstFileObservation`, even when a configured entity label collides with that spelling. [Python tests](../../crates/graphit-ast/tests/tree_sitter_python_model.rs) compare full nodes, edges, outcomes and FILE across simultaneous parser owners, then retain a Send owned result after the owners drop.

## Local Oracle CALLS consumer

The [local model contract](rust_ast_antlr_model.md#local-oracle-callable-references)
adds borrowed caller/body and lexical target outcomes without changing raw C7
occurrences, B1 lookups or B2 classifications. Only a Bound native callable source
and Resolved permitted local Function/Procedure entity produce CALLS between real
Graph node keys. Observed, ambiguous, unsupported and indeterminate cases retain
their actual causes; Package/global/qualified/database-link/compiler overload
resolution and full C7–C9 publication are outside this leaf. An uncertain charset
comparison cannot be discarded as a proven noncallable.
