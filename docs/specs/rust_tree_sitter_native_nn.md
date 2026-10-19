---
title: Rust Tree-sitter entities in native NN
doc_type: technical
description: Genuine Rust and stock Go original projections, Source associations and observed embeddings in native NN.
references:
  - target: {type: task, id: tsk-d284}
    relation: implements
  - target: {type: task, id: tsk-9fa2}
    relation: implements
  - target: {type: task, id: tsk-75af}
    relation: implements
  - target: {type: task, id: tsk-50e1}
    relation: implements
  - target: {type: task, id: tsk-d29d}
    relation: implements
  - target: {type: task, id: tsk-107e}
    relation: implements
  - target: {type: task, id: tsk-4840}
    relation: implements
  - target: {type: task, id: tsk-6d5b}
    relation: uses
  - target: {type: task, id: tsk-a82f}
    relation: uses
  - target: {type: task, id: tsk-cafe}
    relation: uses
  - target: {type: task, id: tsk-77c6}
    relation: implements
  - target: {type: task, id: 01M5C6SF19TSS3NE06D71HSM7N}
    relation: implements
  - target: {type: task, id: 01M5C9XXR0ZE2Q2GX2MMHA6ZBD}
    relation: implements
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
---

# Rust Tree-sitter semantic entities in native NN

`NativeNnTsInput` and `encode_native_nn_ts` consume the borrowed
[`TreeSitterSemanticDocument`](rust_tree_sitter_semantic_model.md). Each input
supplies an actual Raw observation, entity indices, observed vectors in that
order and the observed model generation. The codec selects only `Available`
projections from the retained native model; unavailable or nonembedding
selections are errors. Empty selection needs neither vectors nor generation.

The encoder verifies the same Original and Source allocation, relative path,
full-source digest, current Raw association, dimensions and finite vectors.
Physical Source is accepted only with an actual NULL-text Raw observation and
its retained project file capture. It revalidates that capture before returning,
including after a codec error. Missing Raw records and stored text from another
allocation are rejected. Physical reading uses the project version's captured
path; it does not resolve Source through Git.

Parser embedding payloads use the 21-field Arrow shape in memory, including name_kind, with
NativeNN format 4 metadata and
`ast-parser-entity-original-1` projection recipe. UID and `data_key` form the
unique tuple: the same native identity may have two configured data keys, but a
repeated tuple is rejected. Configured Struct, Field and Function labels, namespace labels,
full extents, exact name anchors and branch/generation observations survive the
same parser codecs. The active persisted representation is the single corpus described below.

Rust identifier display names use Syn's identifier parser and `unraw`, while
anchors retain their original bytes. A configured `impl` type namespace retains
the complete native type expression already validated by the genuine model with
Syn. The encoder does not parse the file again or retain a second projection
lookup map. Function and Method contributions use this same model and codec; labels/data
keys come from the effective profile. There is no dedicated function extractor
or second Rust encoder. The stock profile uses the Function label for methods.

Persisted validation checks schema/model identity, tuple uniqueness, coordinates,
projection names, digests, vectors and namespace containment. It accepts
configured labels/data keys without a hardcoded family allowlist. A namespace
anchor may be outside its child projection. Its serialized raw_name is checked
against its declared name kind and coordinates; those bytes cannot prove their
association with a native parser owner or current Original. Serialized observations grant
no filesystem authority, freshness or native parser ownership.

Run only the affected targets:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test native_nn_tree_sitter --test native_nn --test native_nn_json --test native_nn_antlr
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test native_nn_tree_sitter --test native_nn --test native_nn_json --test native_nn_antlr -- -D warnings
```

The fixtures use genuine parser factories and native storage: configured data
keys/namespaces, raw identifiers, qualified `impl` types, Main/publication/query,
NULL physical Source, missing/changed files, foreign allocation/path/Original,
invalid vectors/model identity, duplicate tuples, conflicting spans in both
orders, nonexact origins and nonembedding selections. These checks qualify the
codec. Model inference, full AST families and the daemon's extraction/indexing
consumer remain separate required integrations.

The Function consumer cutover qualifies 54 distinct affected Linux cases:
NativeNN codec/query/publication/Source, Main/materialization/locator, borrowed
Tree-sitter projections and the actual App ModelService/MCP consumers. Eight
tests exclusively for the retired preparation/extractor API were removed; the
genuine model producer's qualified Function/Method coverage is reused. Scoped
Rust Clippy and formatting pass. The current receipt is
`.build/rust-native-nn-genuine-cutover/qualification.json`; failed build and
empty-fixture attempts remain recorded. The full daemon NN publisher/association
and all-language integration remain separate required work.

## Go projections in the same codec

NativeNnTsInput/encode_native_nn_ts consume genuine complete original projections
from the Go model. Stock Interface, Type, Constant and Variable now join the
original Function/Method/Struct/Field embedding families. Import/Parameter remain
nonembedding under the stock policy; callers must select actual Available code
projections rather than every assembled entity index.

The codec and row validator accept only the exact go/tree-sitter-go pair.
Display names for NN equal native identifier lexemes; mature Unicode letter/
decimal-digit regex validates integrity without Syn, case folding or repaired
text. A literal-source Import name is distinct and cannot be passed through this
identifier domain. The model/tree remains parser authority; rows cannot create it.

Go values are typed literal_source observations and original full code text,
not decoded scalar values. No number rounding or string unquote is added for
NN. Namespace frames retain actual origins and context/capture provenance; the
encoder still consumes genuine owner name/anchor/extent, not restored DTOs.

Source allocation, same Original, relative path, actual Raw observation, full
digest, extent coordinates, dimensions/finiteness and UID/data_key tuple
uniqueness retain existing checks. Invalid/unavailable/nonembedding/duplicate
selections and foreign Source/Original fail before publication. The associated
source helper validates Raw/path/hash/allocation; the same-Original pointer
check is explicitly in the caller. Rust, JSON and Oracle retain their algorithms.

The Go fixture uses a native builtin/stock profile, genuine retained model,
actual Raw SDK store and Interface/Type/Constant/Variable projections. The NN
batch is stored/published and queried through the mature native NN APIs; rows
are compared by UID/data_key and full row content. An explicitly embedding
Import profile encodes a genuine LiteralSource row under the explicit name_kind
protocol; its available native extent and exact quoted name remain literal.
Explicit controlled vectors qualify codec integrity, not inference. Root owns shared M
and App publication in crates/graphit-app/tests/foreground_ast_semantic.rs.
Receipts/checks are under tsk-d29d and tsk-50e1; new producer qualification and
Root runtime evidence remain distinct. Graph assembly/publication is independent
of NN model/vector availability and uses its own concrete writer contract.

The affected Go commands are:

```text
cargo xtask cargo test -p graphit-ast --test tree_sitter_go_model --locked --offline -j4
cargo xtask cargo clippy -p graphit-ast --lib --test tree_sitter_go_model --locked --offline -j4 -- -D warnings
```

The concrete [AST Graph producer](rust_ast_semantic_graph.md) reuses only the
existing Source binding/helper. Its input contains no NN model or vectors, and
its Graph rows include nonembedding Import/Parameter entities. Root composes the
owned DTO with the Data writer and native publication independently of M availability.

### Explicit name representations (NN format 3)

These representations were introduced in format 3; the current schema metadata
graphit.ast.nn.format is ast-native-nn-4. name_kind is a
required Utf8 field appended after data_key. Identifier, LiteralSource and
JsonName remain specialized entity representations; NativeSource is allowed
only in the explicit Generic domain. RustType is allowed only in a
genuine Rust impl-type namespace frame. A name kind is supplied by the native
producer, never inferred from the spelling or graph label.

The name column keeps the producer meaning: mature identifier/JSON decoding
where applicable, or the exact Original lexeme for LiteralSource/NativeSource.
Generic NativeSource retains quotes, Unicode spelling and original mapped atoms;
it does not decode an identifier or Graph display. Literal values
include authored quotes, escapes and bare identifier expressions. They do not
pass through the identifier decoder, and a configured Value embedding no longer
requires dropping that entity or the valid symbols in its file.

Each namespace item appends name_kind and raw_name to its existing kind/name/
anchor/extent fields. raw_name is the actual owner-anchor lexeme, not a cloned
body. The encoder checks its mature name-domain recipe and actual Source/
Original ownership. The row validator checks kind/domain compatibility, spelling,
byte length, UTF-8 points and existing range containment. This representation
does not restore native AST or request authority from deserialized metadata.

Rows from a different or incomplete schema return SchemaMismatch. Unknown kinds,
wrong kind/domain combinations or spelling/range inconsistencies return
InvalidRow. Arrow itself refuses NULL values in these nonnullable fields at batch
construction. No old-row migration, tag inference, normalizing fallback or silent
skip is provided. UID/data_key/projection/model reuse keys remain unchanged.

The actual Go mixed fixture selects all eight Available projections (four normal Variable rows and four LiteralSource Value rows), encodes and validates them, publishes/queries through the SDK and compares all columns by UID/data_key. This codec proof uses controlled vectors and does not prove M inference.

## Python native observations

The codec admits only the exact Python producing pair. Identifier rows use the same mature XID/underscore and NFKC recipe as the model; raw names, ranges and source digests remain authored. Native literal-source names, including dotted imports and derived values, retain their complete spelling. There is no unescaping, case folding or parser reconstruction from stored rows.

Python uses the current format-4 schema, projection recipe, selection/vector guards, Source association and publication/query APIs. The [Python focused tests](../../crates/graphit-ast/tests/tree_sitter_python_model.rs) add actual Raw → model → projections → controlled-vector NN → SDK publication/query and row-corruption checks. Controlled vectors prove the codec; the [foreground focal](../../crates/graphit-app/tests/foreground_ast_semantic.rs) exercises the model service.

## Generic native owners and async consumer

The [generic model contract](rust_tree_sitter_generic_model.md) describes
`NativeNnGenericInput`, complete mixed namespace/Entity owner frames, original
MappedAtom names, and the lending async input scope. Ineligible configured
projections retain their reasons and do not produce fabricated rows/vectors.
The foreground Generic consumer selects only `Available` projections for NN.

The same eligibility/association contract now has a dedicated generic ANTLR
codec in `native_nn_antlr_generic.rs`. It consumes the real
`ParsedGenericAntlrDocument` once, with the associated Source authority and same
Raw allocation/Original, rather than reparsing or rebuilding authority from
serialized metadata. Only the exact `postgresql`/`antlr-postgresql`,
`tsql`/`antlr-tsql` and `cobol85`/`antlr-cobol85` pairs enter this codec;
Oracle/DB2 retain their existing codecs. `NativeSource`/`Generic` names preserve
the original lexeme and complete available owner frames. UID/data_key, Source
SHA, Raw observation, format 4, projection recipe and vector/model guards remain
their existing contracts.

Generic ANTLR extent provenance distinguishes `ExactProjection` from
`SequentialOriginalCoverage`. The latter permits a physical Original extent only
when retained Compound pieces belong to the same Source allocation, remain
ordered/nonoverlapping and every gap is covered by the actual preprocess plan's
`removed_original` spans. For COBOL's Original `programUnit` body, only the
maintained preprocessor's seven actual spaces of generated trivia at the start
of a line may be skipped. That verified padding exception does not admit other
synthetic content, unmappable, ambiguous, reordered or unexplained pieces.
Raw Compound observations remain Compound; `SequentialOriginalCoverage` never
claims an Exact projection. Compound names remain ineligible. The Graph metadata
retains this provenance and NN consumes the genuine available extent with its
normal guards.

A grammar's captured decisions are observations, not calculated cyclomatic or
cognitive complexity. The generic ANTLR model does not invent those metrics.
Missing/ambiguous owner extents, conflicts and labels not selected for embedding
retain explicit unavailability; only genuine `Available` projections enter NN.
SCIP's common index format does not replace the current NN format-4 schema or turn a
compiler symbol into an authorized native projection/vector. Controlled-vector
codec fixtures do not prove real Models inference. The integrated Source consumer
passed Compiler/Absent/Failed scenarios in 51.47 seconds, validating official
SCIP publication for Rust/TSQL/JS, finite NativeNN with Raw/hash/UID/data_key
matching Graph, exact Source and CLI-to-MCP delivery. This proves the recorded
composition and association. Final strict library Clippy passed in 36.58 seconds
for AI, Daemon, AST and App. The installed product lacks this delta, remote images
remain unpublished and other image/platform
qualification remains separate.

Missing extents, native recovery and conflicting observations stay queryable in
Graph with their original diagnostics; they do not abort publication of eligible
entities in the same image. An image without eligible projections contributes
no NN rows, so a replacement also removes formerly eligible rows. Errors from
the retained model, Original, Source association, inference, encoder or stream
still fail publication through the existing error and EOF protocol.
Background AST index embedding RPCs use a 900-second (15-minute) default only
for `DistributionDefault`, covering service admission and local execution.
Every explicit `hub.limits.request.deadline_ms` value from `Configuration`
remains unchanged, smaller or larger, including values equal to the catalog
default. Each fresh RPC RequestContext also takes the earlier deadline of the
parent Original when finite and keeps
the same job cancellation. Foreground requests and the global Hub default,
input limits, model identity/weights and Broker JWT validity (600 seconds) are
unchanged. This is neither a speed claim nor a universal completion guarantee;
it introduces no retry, reservation, polling or scheduler.

The first failed embedding RPC writes an immediate stderr diagnostic with the
logical project ID/version, input byte count and model-service error kind. NN error
retention, Graph EOF and the final publication error keep the same behavior;
the diagnostic records no projection content or physical paths.
Names and owner anchors must agree with the original CodeDocument projection,
Source SHA, Raw observation and same Original instance. Graph display and NN raw
names correlate by UID/data_key and Source SHA; FTS has no schema change.

### Embedding providers and optional query reranking

The shared `NATIVE_NN_FORMAT` is `ast-native-nn-4`. The field layout and vector
identity remain unchanged; `model_generation` is now nullable. Every parser
codec preserves `Some(generation)` for an observed local response and `None`
for a remote response. The validator still requires all nonnullable fields,
source associations, projection hashes, finite vectors and exact dimensions.

The transient format-4 codec rejects incompatible payload schemas. Current
foreground publication uses the model-independent corpus and preserves intact
FILEs; resolving a changed embedding model evolves only its nullable vector
field and schedules current eligible projections. Earlier full-overwrite
startup receipts describe the former separate-NN producer. Imported artifacts
must match the current single-corpus contract.

AST indexing and queries compose the selected Local, Broker or Direct embedding
service through `graphit-app::embedding_service`. Remote execution does not
resolve local model settings, capture model artifacts or start a model child.
The declared protocol, model, model revision, recipe and dimensions identify the
remote vector space; endpoint, credentials, service rollout and hardware do not.
An incompatible space requires the ordinary index update. Local responses retain
their observed model generation; remote rows and queries use a null generation.
Reusing an unchanged projection preserves that optional observation.

Runtime resolves Local selections from a bounded Core capture of the manifest,
using the same declared identities, recipe and execution options as the full
model resolver. It does not hash the GGUF merely to prepare expected metadata.
The native worker still verifies artifact SHA-256, size and layout before its
first inference. Manifest observations grant no authority and do not prove that
weights have been loaded; CLI installation and selection retain their full
artifact validation.

Local model residency uses the GLOBAL `models.idle_timeout_seconds` policy
(positive seconds, default 300). Source resolves it with the model slots from
the same global configuration snapshot. Its existing configuration watcher
refreshes an already started model owner, including a Local reranker used with
remote or disabled embedding. A policy change does not start a model child or
change vector identity; Models owns idle unload, active operations and reload.

Corpus admissions and the published Graph/Raw/NN pair remain independent of the
embedding service admission. Continuous background indexing captures a fresh
finite service request for each batch, retaining the same cancellation and the
earlier parent deadline. A failed batch preserves previously confirmed native
publications. An error never selects another provider or substitutes a vector.

Foreground Source captures exact provider/purpose rules for Embedding and Rerank.
Configured Local execution admits Runtime, Local and Web origins; configured
Direct execution admits Runtime and Local with its exact protocol, endpoint,
model, recipe, dimensions and purpose-specific key. Broker execution retains the
Native token, local Broker key or explicitly allowed anonymous mode selected by
the actual profile. Configured browser Broker inference instead retains the
incoming principal and its exact provider/resource binding. Remote execution
requires its own RemoteRequest ceiling; these rules grant no PROJECT read access.

AST search accepts an optional boolean `rerank`. Omission uses the effective
`search.rerank` configuration, whose default is false; explicit false overrides
it. With reranking off, no rerank service, discovery, download, preparation or
inference is requested. Reranking applies to one retrieved page and preserves
candidate identities, existing retrieval scores and source generations. Added
`rerank_score` and `rerank_rank` describe only that page and query.

Semantic/vector retrieval supplies the validated NN projection. Graph FTS,
hybrid and imported retrieval require exact byte extents from the same published
Raw clone, with matching Source hash and valid UTF-8 boundaries. A missing exact
extent or stored Source fails an explicitly requested rerank operation; it does
not remove candidates or substitute metadata, a live file or another generation.
The caller rechecks corpus authority and retained pins after inference. Local
reranking can run with Embedding disabled: a checked Local rerank lease requests
the existing Foreground process owner through its existing command queue.

The historical format-4 codec/storage qualification passed 53 distinct Linux cases: 52
integration cases across `native_nn`, `antlr_generic_graph`, `native_nn_antlr`,
`native_nn_generic`, `native_nn_json`, the since-retired separate-NN birth suite and
`native_nn_tree_sitter`, plus the normal-reindex/publication/nearest unit case.
The latter rejects an obsolete schema, publishes freshly encoded remote NULL
generations and queries those rows with unchanged vector identity. All five
codecs retain genuine parser/Source validation and local generation observations.
Three stale 20-field test expectations were corrected to the existing 21-field
contract; the failed first attempt remains in the receipt.

The App metadata consumer's five tests and the four Direct/Broker dispatcher
cases also passed, as separate scopes.

The Linux Native AST FTS/Local BGE consumer passed its distinct focused case in
43.72 seconds. It publishes genuine Raw/Graph before MCP requests, with Embedding
disabled. Omission and explicit false succeed without model control; explicit
true uses the pinned BGE F16 through the real control owner and child. Both
candidates, their original scores and identities, Source, published pins and
page metadata are preserved. The fixture reuses the existing single-publisher
boundary and awaits publication; it does not suppress Source-change errors or
retry inference. This does not qualify semantic NN inference, full remote AST
publication, imported/Web reranking or remote USER Memory.

### Single AST corpus and progressive publication

The storage integration replaces the separate lexical, pending-projection and
mandatory-vector datasets with one searchable corpus per project.
The default is `<global>/projects/<PROJECT_ID>/ast/corpus.lance`; `source.lance`
remains the complete FILE source corpus. Every project version selects a native
Lance branch of those same datasets. Core `local_corpus::resolve` validates their
canonical module-relative paths and reads no per-version directory map.
`ast/live/<namespace>` retains the version-scoped Icebug graph publication.
Installation/project captures, Original and admission remain necessary: a URI,
branch name or fingerprint grants no authority. Per-FILE refresh
retains its dataset and physical guards; default corpus birth remains exclusive.

`crates/graphit-ast/src/corpus.rs` defines `ast-corpus-1`. Every lexical entity
retains its actual identity, source association and optional ranges. The row
shares the same row with its eligible parser projection. There is no cohort
namespace or common parse/Source/graph/search generation. Native dataset branches
isolate project versions; their inherited rows are immediately queryable. `name` keeps the graph display name,
while nullable `projection_name` keeps the original parser spelling for the NN
codec; escaped names and compiler display names are not normalized. Entities without an eligible projection
remain searchable, with nullable projection and vector fields; ranges are never
invented to make them embeddable. The 21-field format-4 schema remains the
transient parser/inference/query payload codec, not another persisted corpus.
All active parser/language selection continues through the existing producers. The former
sidecar overwrite/FILE publication and query entry points have been removed.
`native_nn_publication` retains only the causal commit receipt used by the App.
Parser fixtures now derive lexical rows from their genuine semantic graphs and
exercise the same corpus encoding and conditional update APIs.

The base corpus has no vector column before model metadata is resolved. Real
model dimensions determine a nullable `FixedSizeList<Float32>` field, and that
field's metadata binds the model ID, revision, dimensions, vector identity, role
and projection recipe. Data's `BranchSnapshot::add_nullable_columns` uses
Lance `NewColumnTransform::AllNulls`: existing fragments and text are retained,
and the SDK synthesizes NULL for their new field. An incompatible model field
is withdrawn through `drop_vector` before adding its replacement. Evolution
loads `DatasetBuilder` from the captured physical `Dataset::uri()` and retains
its explicit branch/version, session, transport parameters and strict commit
handler. A child generation need not have a corresponding Main manifest;
loading the project root before selecting that branch would be incorrect.
The owner records each confirmed generation before checking late cancellation or
admission failure. A failed later operation does not undo a known commit.

A newly materialized Lance branch inherits Main rows and compatible vectors
through native SDK branch semantics. Those rows remain queryable while the
new version updates its files; no graph-derived membership marker hides them.
Background inference selects eligible projections from the selected branch.

Foreground `ProgressivePublication::publish_file` commits a FILE's searchable
rows before publishing its independently derived graph changes. A removed FILE
deletes its corpus rows without waiting for graph publication. A failure is
reported for the affected operation; it does not make previously committed
corpus rows unreadable or invalidate unrelated files. A retained native snapshot
continues to represent its captured dataset while subsequent requests can see
new commits. Search never recaptures Source or graph heads to demand equality.

A changed FILE can preserve an existing vector only for the same
`(uid, data_key, projection_sha256)` and exact model field descriptor. Its new
source metadata and input fingerprint still replace the previous values. The
fingerprint includes length-delimited identity, projection SHA, full Source SHA,
Raw version and projection recipe. No parsing or embedding is repeated solely
because another FILE advanced the whole-project Raw generation.

The background consumer discovers pending work from current rows with eligible
projections and absent/incompatible vectors. There is no persistent second
queue or previous-NN-pin corpus. It consumes the existing embedding producer and
checkpoints received batches after 64 rows or 60 seconds from the first pending
batch; FILE completion flushes the remainder. These are publication triggers,
not input caps or inference deadlines. Channel EOF still needs the producer's
explicit completion signal. Original cancellation and its independent finite
or service lifetime remain active while waiting.

`corpus::prepare_embeddings` reads only the inferred identity tuples from the
current retained corpus pin. It updates an extant row only when its current
fingerprint equals the inferred input. Missing/deleted or superseded rows are
skipped, and the existing strict native CAS rejects changes after comparison.
It never inserts a missing entity or replays a complete historical FILE.
Confirmed partial vector batches are immediately queryable and reusable after
interruption. `NativeNnPublication` retains the actual commit key/generation and
any late completion error; a known applied effect is recorded even when public
delivery subsequently fails.

The corpus maintains actual native scalar indexes for path/identity, code FTS
indexes for name/content and a cosine IVF_FLAT vector index. Appends extend FTS
coverage without rebuilding old segments. Lance's native scanner covers newly
appended vector fragments; the existing vector index is reused. L2 and dot
queries use the SDK flat scanner with their requested metric, avoiding a cosine
index overriding distance semantics. Parser/source/model validation still
applies to returned candidates.

This removes projection-store, NN-invalidation and final FILE replay commits.
One foreground FILE change requires one corpus row transaction, plus scalar/FTS
index commits for newly uncovered fragments. Embedding checkpoints update that
same corpus. Model schema evolution occurs only when required, and an existing
matching vector index is not recreated per FILE or batch. This is a mechanical
commit-path comparison, not a throughput or space benchmark. Retained native
versions and obsolete files require explicit SDK maintenance against the real
retained pins; there is no periodic GC, quota or retention promise in this
producer.

Portable indexed AST exports capture the available graph FILE observations
and independently selected corpus snapshot under write admission. Source is
streamed into a private SDK dataset from the FILEs' actual historical Raw
versions, validating unique paths and stored-text hashes; only one historical
SDK pin is retained at a time. Empty FILEs and NULL/TextNotStored records retain
their Source contract. Source is not duplicated in each entity.

The corpus is exported independently with its native lexical/vector indices.
Export does not require a Source/graph generation common to every searchable
row. `source_bindings.json`, `PublishedPins` and clone-generation equality gates
have been removed. Actual native dataset manifests, immutable artifact digests,
authorization and file integrity remain the publication/read contract. A graph-only
artifact still exposes only its actual capabilities.

Local Source retains the exact native branch and historical generation selected
by each published FILE. A named pin resolves the real SDK branch reference before
checking out that historical branch/version; the generation need not exist in
Main. Branch/version identity remains checked, and an absent branch cannot fall
back to Main. FILE metadata retains its Raw generation even when the file has no
lexical entities, including an empty source file. A newer Raw HEAD does not
replace that published association.

Physical Source dependencies are determined by the data files, overlays,
deletion files and indices actually used by the selected native manifests.
Unused manifest base-path entries do not establish a consumer. Raw HEAD and
published per-FILE historical pins remain Source consumers after corpus
unification; referenced physical data must remain available to the SDK with its
native branch history. Stable public record identities do not depend on the
local container path.

The affected fixtures are `live_search/tests.rs`, corpus acquisition/branch
fixtures and App's progressive publication/lifetime cases. The storage integration
and its updated fixtures require their own focal validation; historical parser,
checkpoint and installed-host receipts elsewhere on this page qualify only the
recorded implementation and do not establish that this new layout is installed.

### Generic format-3 qualification

C qualifies 15 distinct Linux focal cases: four Generic codec/namespace/async
cases, payload and physical 21-column cases, three specialized controls, three
App schema cases, the Main Graph/FTS regression, decoder negatives and the real
Main NN case. The registered runner passed in Root31j: one nominal test,
24.71 seconds runtime and 151.614 seconds wall time including 126 seconds of
Cargo compilation. It used the normal model server, qualified CPU ORT and the
versioned tiny ONNX fixture, with no skipped or simulated inference.

The recorded Main scenario exercised original raw names, NN format 3, productive Graph/FTS,
semantic distance and hybrid distance/RRF score, increment, projection/vector/
generation preservation, model vector_id change, empty FILE and causal invalid
library rejection with joined cleanup. Its observed Source changes Raw 1→2 and
full SHA while the native UID stays exactly equal; Graph records original Raw
version 2 and a real NN pin. The wait requires the productive Graph SHA as well
as exact source bytes, avoiding acceptance of an older same-name hit.

Reuse qualification is composed with tsk-9d92's completed endpoint-closed
cache proof: Generic uses the same exact tuple/projection key and unchanged
`embed_file` helper. Main observes integrated selection and preserved results;
absence of RPC was observed in the earlier shared-helper proof, not isolated
by the deterministic tiny model in this Main case.

The long Linux gate runs the registered ignored focal after workspace tests;
the manual selector uses the same executor and real-resource preflight. Hook
activation and long-path binding are inspected, and tsk-b559's actual fail-fast
hook proof is reused. Earlier failed fixtures and the zero-test selection remain
history, not acceptance. Evidence and commands are consolidated in
`.build/ast-native-nn-generic/receipt.json`. Root31m Clippy passed the AST
library, App custom test and xtask runner with `--no-deps -- -D warnings`
(30.093 seconds wall, 29.96 seconds Cargo). It ran no functional tests and was
not a workspace/all-targets gate. Windows/macOS native execution remains UNRUN.
