---
title: "Rust Storage Contracts"
description: "Current Rust storage APIs, historical native evidence, and remaining consumer integration."
content-type: guide
audience: developers
updated: 2026-10-14
references:
  - target: {type: task, id: tsk-69f2}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: integrates
  - target: {type: task, id: tsk-0a36}
    relation: specified_by
  - target: {type: task, id: tsk-8a17}
    relation: implements
  - target: {type: task, id: tsk-265f}
    relation: implements
  - target: {type: task, id: tsk-a542}
    relation: specified_by
  - target: {type: task, id: tsk-7240}
    relation: relates_to
  - target: {type: task, id: tsk-d581}
    relation: specified_by
  - target: {type: task, id: tsk-1e53}
    relation: relates_to
  - target: {type: task, id: tsk-8218}
    relation: integrates
  - target: {type: task, id: tsk-72ce}
    relation: specified_by
  - target: {type: task, id: tsk-8286}
    relation: implements
  - target: {type: task, id: tsk-bcfd}
    relation: implements
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-4cc4
    relation: derived_from
  - target:
      type: task
      id: tsk-86ec
    relation: supports
  - target: {type: task, id: tsk-185c}
    relation: refined_by
---

# Rust storage contracts

Audience: authors of Graphit stores, indexers, and Rust consumers. The Main Data
library uses official Lance 14.0.2 with Arrow 58 through
[module snapshots](../specs/rust_lance_module_store.md) and
[branch snapshots](../specs/rust_lance_branch_store.md). Records Task/Memory and
Knowledge enforce their own authenticated domain operations over these APIs.
Development requires functional coverage with coherent new producers and
consumers, without compatibility layers or old-data converters. Session `ses-c98a` and refinement `tsk-4cc4` record this choice.
Data delivery `tsk-86ec` owns implementation unless another owner is named below.
Reconcile older Task descriptions/checks with this policy before implementing a
consumer; historical compatibility evidence is not a new acceptance requirement.

## Engines and native boundaries

The product uses official Lance 14.0.2 and the pinned Ladybug sources. See
[official Lance integration](lance-upstream.md) and [maintained native sources](rust-forks.md). Store and branch APIs are documented in the
[module](../specs/rust_lance_module_store.md) and
[branch](../specs/rust_lance_branch_store.md) contracts. The official Ladybug
engine reads captured Icebug data through the Rust/CXX query integration.
Published and imported Icebug queries use an in-memory catalog over captured
Parquet/CSR files, without copying the graph into native tables. The local
progressive AST instead uses a persistent Ladybug database with a transaction
per FILE, so Cypher can observe committed updates before publication finishes.
Connections to that live graph share its database; distinct admitted project,
version and generation bindings remain isolated. Knowledge and Records use
Lance, not the Ladybug graph or its buffer pool.

The Ladybug buffer-pool setting is a per-database maximum, not a process memory
reservation or a bound on total RSS. Query intermediates and other engines can
allocate memory outside that pool. Neither Icebug's format nor an in-memory
catalog makes those allocations free. These primitives do not replace product
authorization or request ownership. See the
[live AST contract](../specs/rust_ast_live_graph.md) for its lifecycle.

## Read views and authorized mutation

`LanceModuleStore` and `LanceBranchStore` are SDK adapters. Their snapshots retain
the actual Dataset and schema through `Arc`; `scan` returns the SDK Scanner for
filters, projections and streamed batches. The adapter accepts staged writes and
strict optimistic publication, but does not authorize them. Records consumers
require the actual PROJECT admission, exact module and original request before
opening the corpus and recheck those controls before SDK effects and output.
The locator, schema, SDK generation and recovery key are data, not permissions.

Logical versions select SDK branches through the admitted VersionContext. A
snapshot stays on its captured SDK generation until the caller explicitly
captures another one. Shallow branches use SDK copy-on-write; queries do not
copy a corpus into a child process. Retained Dataset handles do not protect
against external GC. Retirement and writers must coordinate their actual active
readers and publication operations.

Local consumers use the mature SDK directly, with native project-directory
validation and caller authorization. They do not require a custom local
ObjectStore, captured-byte quota bridge, process per operation or range IPC
framework. An official ObjectStore provider and Session remain appropriate for
a real remote storage consumer. Batch, input and output limits constrain useful
work and retention; they are not allocator or RSS guarantees. Consumers check
the original cancellation/deadline at their actual async and output boundaries.
SDK interruption or lost commit acknowledgement can require observing the
existing transaction outcome; neither Drop nor a counter proves rollback or
quiescence. Functional authorization, pin, concurrency, error and recovery tests
remain with the affected SDK and domain consumers.

## Search, ranking and writes

Run FTS and dense candidate scans against the same Snapshot/version. Identify
and deduplicate rows within their dataset/version identity before fusion. RRF
produces one authoritative rank score, ordered monotonically with the returned
ranking, with a documented deterministic tie rule. Repeated identical queries on
the same version must retain stable scores and ordering.

Retain RRF K, candidate budgets, limits, offsets, and cross-reranking capabilities.
Expose one coherent score contract.
Scanner prefilters apply before top-k selection; projection and redaction apply
before exposing results. SQL/query consumers must respect the same authorization
scope and limits. These are required Graphit behaviors.

Use atomic MergeInsert/CAS/upsert semantics for updates. Do not implement an
update as delete followed by append with an observable missing-row interval.
Validate conflicting writers, fresh fencing, retries, and readers during updates.

Repeated hybrid searches over the same captured version must retain stable scores
and ordering; preserve that behavior through the native consumer tests.

Code FTS maintenance builds postings only for uncovered Lance fragments. It
consolidates comparable existing posting segments with the SDK's uncommitted
`merge_existing_index_segments`, merging pairs in the same coverage-size tier
and carrying their union into its resulting tier. This conventional
size-tiered merge avoids retaining one physical segment per indexed file and
avoids rewriting the large base index for every small update. It reads existing
postings, without reparsing source rows or recomputing embeddings. New coverage
and removal of replaced segment declarations publish in the same strictly fenced
`PreparedIndex` commit. Captured old versions retain their original postings;
uncovered fragments remain immediately searchable through Lance. A fully
covered, consolidated index remains a no-op. Covered older layouts with many
comparable segments are consolidated even without a new file. Maintenance uses
a private SDK session without an index cache so obsolete intermediate posting
readers are released after each pair; useful search snapshot caches are unchanged.
After a successful merge, its consumed private intermediate files are removed
through the SDK ObjectStore. Committed inputs and final outputs are preserved;
SDK errors, cancellation and uncertain publication retain the normal SDK orphan
recovery/GC contract rather than assuming rollback.
This is index maintenance, not a file, corpus,
descriptor or memory quota; physical file reclamation remains SDK GC work and
must respect retained snapshots.

## Branches, clones and recovery

Engine branch creation has two phases and is not an atomic application operation.
Guard it with a lease and detect orphaned state on failure/restart. Force cleanup
is permitted only for a verified orphan, never a concurrent winner. The
two-handle native probe does not establish this recovery policy. Add interrupted
creation, orphan/winner discrimination, competing cleanup and restart tests.

A shallow clone pins referenced fragments. Garbage collection must protect those
references for the clone's lifetime. Writes to a clone must use its own authorized
write path; there is no fallback that writes the shared source. Test source GC,
clone deletion, nested/reference lifetimes and interrupted publication before
allowing product cleanup. The probe proves local fragment sharing and isolated
append only.

## Direct Parquet graph catalog

Mount an in-memory Ladybug catalog over direct Parquet/CSR data without COPY or
ingest. Publish each generation under an immutable prefix. Key caches by project,
branch, generation and authorization identity. Mount and renewal must fail closed
on revocation or lease failure, including access through cached queries.

CSR data must have valid UInt64 targets, monotone row pointers covering
`pointer[source + 1]`, and a final pointer equal to the edge count. The selected
disk contract uses footer `icebug_disk_version=v1`, with coherent NULL properties.
Validate zero-node/zero-edge cases, malformed CSR, target bounds and supported
scalar/property types. These tests are pending: the small local fixture is
partial coverage, not proof for every graph layout or property type.

Generation publication, cache identity/invalidation, permissions and leases are
Graphit obligations. A native in-memory mount or an HTTP Range request does not
establish them.

## Captured local graph sampling library

`graphit-data` implements the first local reader in `tsk-36c4`, using the real
Core capture delivered by `tsk-8ee5`. This API is for Rust integrators. It does
not yet replace the AST HTTP explorer or execute Ladybug/Cypher queries.

Call `CapturedLocalGraph::capture` with a native directory `&Path`, an explicit
`SnapshotPolicy`, positive `SourceReadLimits`, the original `Arc<SnapshotContext>`
and a validated `LocalGraphPolicy`. Every manifest/footer/page read uses Core's
owned captured handles. The context, reader and original physical fingerprint
remain private. `GraphGeneration`, `NodeId` and `EdgeId` are data identities;
serialized values cannot recreate the capture or authorize filesystem access.
Capture and each sample perform full original-source revalidation. A conflict,
cancellation, deadline, I/O admission failure or decoder error discards the
entire response; there is no refresh, retry, repair or partial-success result.

`sample_graph(GraphSampleLimits)` selects seeds by lexical label then physical
row, and forward relationships by lexical physical member then edge position.
The edge cap is global across members. `max_nodes` must accommodate
`seed_nodes + 2 * global_edges`; 300 seeds and 1,000 edges require 2,300 nodes.
Isolated seeds, a forward self-loop once and parallel edges with distinct UIDs
remain observable. The native PROJECT viewer uses
`sample_graph_excluding_marked(limits, "is_file_observation")`: only a true
boolean marker excludes a seed. False, NULL or absent markers remain eligible,
including semantic nodes with the same label as the metadata table. This avoids
metadata consuming every seed when a project has more than 300 files. Actual
relationship endpoints still remain observable; the generic installed-artifact
sample retains its existing selection. Full-table nonnull primary-key uniqueness and full forward
CSR pointer/target validity are checked even beyond the selected output. Every
decoded or skipped row is charged before the initial positive SDK batch.
Reverse descriptors are bounded and their declared files/schema are checked;
reverse content equivalence and reverse traversal remain future consumers.

Nodes expose typed IDs, label and optional name/path/uid/cluster/lang/line.
Missing optional columns and NULL become absent values; line zero is absent.
Edges expose typed source/target IDs, logical relationship and UID. The sample
and summary fields are private and only borrowed getters are public. Held
samples own their vectors and strings directly, without shared slot or memory
permits. Ordinary ownership releases storage when consumers drop their owners.
Sampling bounds seeds, edges, selected nodes and individual strings; it does not
estimate the heap size of the returned DTO.

The reader accepts the current strict Icebug manifest v3 with
`icebug-canonical`, finished publication, relationship UIDs and
`icebug_disk_version=v1` Parquet metadata. Relative filenames must consist only
of normalized UTF-8 ordinary components. `schema_file` and `storage` are inert
metadata. The first scalar subset is STRING/INT64/BOOL nodes and UInt64
`ptr`/`target` CSR columns. Flat schema annotations, row counts, chunk spans,
metadata and encodings are checked before SDK reconstruction. File and column
key/value metadata share aggregate entry/byte limits; `created_by` is charged
to that byte budget. Nonnull/unique
primary keys are required for nonempty tables; empty catalogs and genuine
zero-column/zero-row footers have explicit empty-input semantics.

Parquet 60.0.0 uses Zstd, no dictionary, PLAIN DataPageV1 with bounded RLE or
BIT_PACKED levels. Nested/repeated/foreign logical types, external column files,
encryption, dictionary/V2 pages and other codecs/encodings fail closed. The
published `polars-parquet-format` 0.1.0 protocol checks individual and aggregate
container admission, duplicate struct fields, names and metadata before the
SDK receives the exact immutable footer/trailer/page bytes. It delegates wire
parsing to the generated protocol and keeps its guarded default unknown-field
skip. Page probes must agree with the final immutable frame. HEADER, BODY,
peek/suffix and body-range requests share owner-backed `Bytes` caches; unknown
requests are denied. Page indexes and bloom reads are disabled. Zstd output is
bounded by the supplied declared page size in the SDK, with exact final-length
validation; malformed-input tests use a separate test-only Zstd dependency.

`LocalGraphLimits` covers manifest/file/schema/metadata inputs and
row/group/chunk/page/header/protocol bounds. Primary-key integrity is validated
with a uniqueness set and the operation's individual string limit.
The retired fields max_live_frames, max_buffer_bytes and max_inflight_operations
are removed without compatibility shims. GraphSampleLimits adds
seed/global-edge/node, string, decoded-row/page-work and positive
row-batch bounds. Explicit SDK construction requires positive limits. The production ConfigSnapshot recipe below supplies documented input defaults. Decoded/skipped row work is
checked before TreeBuilder::as_iter; batch size cannot exceed the decoded-row
limit. Parquet receives the real Core RetainedSourceRange directly through
Bytes::from_owner, preserving cache/alias ownership without a Buffer wrapper,
extra payload copy or reservation controller. These limits do not bound the
allocator, native-codec workspace, RSS or global descriptor count. An ordinary
Core source capture hashes files once. Schema, sample and neighborhood operations
recheck the same Original and physical bindings without recapturing or hashing
the entire graph. Individual reads retain native guards and decoded integrity.
This does not make a filesystem operation transactional.

Product integration suites validate their consumers. Linux fixture
success does not prove native Windows/macOS runtime. `tsk-86ec` still requires generic Ladybug/Cypher and HTTP/UI neighborhood integration,
Lance/SQL/FTS/dense search, branches/GC/writers/publication, remote/S3 credentials,
authorization/fencing and real HTTP/CLI/Broker integration. This library does not
remove or classify those capabilities as dead code.

### Configured local graph input limits

The production recipe is `LocalGraphPolicy::resolve(&ConfigSnapshot, &SnapshotContext)`.
It consumes the caller's effective configuration and checks the same original
request before and after resolution. It does not load files, read the process
environment, perform network I/O, create a capture or grant filesystem access.
Pass the resulting policy to `CapturedLocalGraph::capture` alongside its real
snapshot policy, source read limits and original context. Explicit SDK callers
can still use `LocalGraphPolicy::new(LocalGraphLimits)`.

`LocalGraphPolicy::CONFIG_KEY` is `data.local_graph`, an optional JSON object.
For example, `{"max_page_uncompressed_bytes":33554432}` changes just the decoded
page limit and uses the documented defaults for omitted fields. Core precedence
is Command > Environment > Project > Global > Distribution. The highest layer
selects the **whole object**; lower-layer object fields are not merged. Global
installation callers compose Distribution/Global/Environment/Command, without a
PROJECT configuration layer. They include this key in their explicitly captured
environment-key list; the environment variable derived by Core for the Graphit
brand is `GRAPHIT_DATA__LOCAL_GRAPH` and contains JSON. Provenance remains available
through `ConfigSnapshot::source`/inspection; this resolver retains no origin ledger.

These application defaults bound real input structures and decoder allocations;
they are configurable acceptance limits, not claims about limits imposed by the
Icebug/Parquet formats. They allocate or reserve nothing by themselves. Metadata
and page sizes limit potentially hostile parsed input and decompression; counts
limit descriptor/container growth and dimensions of individual files. There is
no admission limit on the aggregate row count of a graph. Decoded rows are
bounded separately for each sample or neighborhood operation, and do not affect
capturing its manifest or schema. These limits are independent of Hub content
limits and test-fixture sizes. There is no live-operation/frame pool, reservation
or aggregate request accounting. All byte units are bytes and all other units
are counts.

The `graphit_data::graph_writer` producer writes canonical Parquet through the
existing Parquet SDK. Each FILE hands over owned node properties and edge keys;
node properties are spooled to private temporary files. Keys and relationships
use a private, disposable SQLite database through [rusqlite](https://docs.rs/rusqlite/0.40.2/).
The writer keeps table schema in memory; SQL resolves endpoints from later FILEs,
rejects duplicate relationship UIDs within a member and orders CSR through an
index. Parquet nodes and indices use chunks of 1024 rows; indptr streams grouped
counts into its required single row group. No corpus-wide key map or edge vector
remains in memory. It accepts
optional STRING, INT64 and BOOL properties. `entity_key` is the primary column;
AST callers preserve the exact `(uid, data_key)` tuple as canonical JSON rather
than merging equal UIDs. Actual Raw versions use decimal strings so a `u64`
is not narrowed to INT64. Original checks continue through spool reads, Parquet
writing and finalization. A failed FILE consumes and drops unpublished staging.
The schema and `finished` manifest are written after the node and CSR files.
The resulting directory is temporary output, not a publication or an access
grant: the daemon still has to publish it with Core and capture readers under
PROJECT authorization. The producer and native Ladybug query are qualified;
the daemon callback and its native graph consumers remain under functional qualification.

The foreground assembles graph nodes, edges and one file observation from the same
retained Rust/Go/JSON document used by NN. Graph assembly does not require an
embedding model or an enabled Embedding module. Ordinary NN failures close that
NN stream with an error and allow the graph to reach Source EOF and publication;
they do not publish an incomplete NN image. Authorization, cancellation, source
binding and integrity failures stop publication. Confirmed graph changes remain
in the reported effects when an NN operation fails. The temporary graph is
published through Core under the same writer lock and Original. App library
compilation is verified; public daemon/HTTP/CLI/MCP journeys are still being
qualified and must not be inferred from the library tests.

Core `ArtifactWriter::read_published_copy(key, kind, policy, limits, original)`
selects a confirmed Copy entry under the native PROJECT catalog lock. It rejects
pending state, another Original, a wrong kind, a Link or absent key. Copy
publication records destination observations before and after content verification;
the reader compares the complete current native inventory, identities, modes,
sizes and modification/change times before reusing those verified digests. It
does not reopen the original staging source or hash graph bodies during capture.
There is no serialized-record or hash-only reader restoration. A partial receipt
or a publication with unreadable preserved Unix modes cannot supply this reader.
The caller may drop the writer before reading: the reader retains its Original
and native bindings, so replacement, mutation and cancellation remain conflicts.
Module authorization stays with the PROJECT consumer. This factory is Linux-native;
its Core tests do not qualify the daemon callback or HTTP/CLI/MCP graph consumers.

`ArtifactWriter::ensure_publication_parent(destination)` creates missing private
directories relative to the admitted native PROJECT root, under the same writer
lock used for publication. Existing links, regular-file parents and destinations
outside the owner are rejected. Cancellation or failure can leave empty parent
directories; it does not create a catalog entry. Native directory and source-file
mutexes serialize legitimate concurrent readers using standard Rust locks;
poison, cancellation and physical-binding failures still reject the operation.
The old requirement that ordinary mutex contention return `WouldBlock` is
superseded; its busy-only assertions were removed, while poison and concurrent
positional-read coverage remain.

The Source service exposes native PROJECT POST routes at
`/api/graphs/project/{query,schema,sample,neighborhood}`. Every request carries
`selection` with optional `project_dir` (the native path wire object) and
`project_version`; an empty selection chooses the current live PROJECT.
Unknown or ambiguous explicit selections fail without fallback. Query adds
`query: {query, parameters}` and neighborhood adds its typed anchor, direction,
relationship, endpoint label, limit and optional cursor. The exact live PROJECT
record, ReadAst authorization and current module settings are checked before
capture and again before delivery. A valid credential alone grants no PROJECT
access. The reader is built from the confirmed publication on that request and
uses the same Original for schema, Parquet reads and Ladybug query execution.

Source MCP advertises `graphit_ast_query`, `graphit_ast_schema`,
`graphit_ast_sample` and `graphit_ast_neighborhood`. PROJECT locators use the same
JSON path string. An exclusive `context: id@version` selects an installed
graph through its existing admission, not a native PROJECT fallback. The CLI
`ast query`, `ast schema`, `ast sample` and `ast neighborhood` consume these tools
through the existing Source discovery, runtime credential and MCP connection.
CLI query parameters and neighborhood cursors are JSON objects, and results are
structured JSON. The isolated real HTTP tests qualify all four native operations,
selection failure, permission denial and replacement detection. The complete
Foreground/MCP/CLI journey has its own evidence and is not inferred from those
isolated tests.

Corpus file and row totals do not impose admission quotas. Metadata inputs and
individual query decoding remain bounded; declared rows must still match the
physical Parquet data.

| Field | Default | Concrete purpose |
| --- | ---: | --- |
| `manifest_bytes` | 4194304 | Manifest JSON input |
| `max_tables` | 1024 | Node-table descriptors and u32 table identities |
| `max_members` | 4096 | Forward/reverse member descriptors and u32 member identities |
| `max_columns` | 1024 | Flat Parquet/schema column count |
| `max_name_bytes` | 4096 | UTF-8 descriptor names and request identifiers |
| `query_decoded_rows` | 1000000000 | Rows decoded by one sample/neighborhood query; not corpus admission |
| `max_row_groups` | 65536 | Parquet footer row-group count |
| `max_footer_bytes` | 16777216 | Parquet footer input |
| `max_protocol_bytes` | 16777216 | Compact Thrift individual input value |
| `max_list_entries` | 1048576 | One Thrift container/field list |
| `max_protocol_entries` | 4194304 | Aggregate Thrift container entries |
| `max_header_bytes` | 65536 | Page-header probe input |
| `max_metadata_entries` | 65536 | File/column/language metadata entries |
| `max_metadata_bytes` | 16777216 | UTF-8 key/value metadata input |
| `max_chunk_bytes` | 1073741824 | Declared compressed chunk extent; not a whole-chunk allocation |
| `max_pages` | 1048576 | Page-frame chain in a chunk |
| `max_page_compressed_bytes` | 33554432 | Compressed page/frame input |
| `max_page_uncompressed_bytes` | 67108864 | SDK decompression output |

Malformed types, unknown fields, zero values and invalid u32 identity counts
return `io::ErrorKind::InvalidData`; policy validation keeps its
`DataReadError::InvalidPolicy` cause. Invalid configuration never falls back to
defaults. Cancellation and deadline preserve their original Core I/O error.
Defaults/partial overrides and Config layers are qualified by the focused
`configured_local_graph` tests using canonical Parquet written by the mature SDK;
this recipe does not certify the installer/HTTP/CLI consumers before their own
integration tests run.

## Captured local graph neighborhood library

`tsk-bcfd` adds `CapturedLocalGraph::neighborhood_graph` in
`crates/graphit-data/src/neighborhood.rs`. Rust integrators pass a borrowed
`GraphNeighborhoodRequest`, explicit positive `GraphNeighborhoodLimits`, and
reuse the same captured graph. This does not replace the active AST HTTP route
or its React explorer.

The request names the anchor label and its declared STRING or INT64 primary
key, logical relationship, other endpoint label, incoming/outgoing direction,
a page limit from 1 through 100, and an optional borrowed cursor. Input names
are bounded by `LocalGraphLimits.max_name_bytes`; a STRING anchor is bounded
before hashing, cloning or source I/O. Empty STRING keys are real values.
Unknown labels, unknown relationship types, unavailable endpoint partitions,
wrong key types, invalid requests and invalid cursors have separate static
`DataReadError` variants. A missing primary key returns `AnchorNotFound` after
the final source guard; a present anchor with no matching edges returns its
complete node and an empty edge list. Only a zero-row, zero-column table returns
`AnchorNotFound` without a declared PK scalar, after the physical guard; it never
projects a nonexistent column. A present schema with a missing or BOOL PK returns
`InvalidAnchorKey` before source I/O. A known STRING/INT64 PK requires the matching
key type at that same boundary. FLOAT remains rejected during capture.

Both directions scan typed forward members. Identity is the same
`GraphGeneration`/`NodeId`/`EdgeId` used by `sample_graph`, including physical
forward member/row identity. Incoming source ordinals come from validated CSR
pointers, never from reverse-member ordinals. Parallel relation UIDs and a
physical self-loop once are preserved. Results are globally ordered by UTF-8
UID bytes across all selected members. A bounded heap retains at most
`limit + 1` candidates; the extra candidate determines `has_more`. A separate
bounded set validates every matching UID, including edges before the cursor
and candidates evicted from the heap. Duplicate UIDs fail instead of silently
deduplicating or returning a truncated page.

`GraphNeighborhoodPage` owns private nodes, edges and property values and
exposes borrowed getters. Endpoint nodes are unique and sorted by typed ID;
`node(NodeId)` looks them up without copying. Properties are sorted by name and
retain STRING, INT64, BOOL and NULL, including empty strings, zero and false.
Node properties include the primary key. Relation properties include UID and
all ordinary scalar fields, excluding the physical UInt64 `target` column.
The edge UID getter borrows its UID property. These complete properties differ
from the sample's optional summaries, where line zero remains absent.

`next_cursor()` borrows the last returned UID from a nonfinal page. Its data
contains the generation and a SHA-256 canonical query fingerprint with tagged,
checked length-prefixed UTF-8 fields and typed INT64 bytes. Direction and every
query field participate; page size does not, so it may change between pages.
`GraphNeighborhoodCursor::from_parts` accepts typed input data and does not
restore a capture or grant. Another generation/query or an empty/oversized UID
returns `InvalidCursor` before source I/O. A valid cursor beyond the last UID
returns an empty page containing the present anchor's complete properties.

Samples and neighborhood pages own their vectors and strings directly. A
neighborhood request bounds each string, properties per record and the number
of matching edges. The candidate heap retains at most `limit + 1` entries;
the uniqueness set still checks matching UIDs before the cursor and outside
that heap. Property counts are checked before projection allocation; edge
output excludes `target`, while the full projection is bounded by `max_columns`.
Collections use fallible `try_reserve`. Nonnull and duplicate primary-key checks
remain independent of output selection.

`tsk-e304` removes approximate catalog/primary-key/DTO heap accounting and the
redundant aggregate UID-byte budget. `LocalGraphLimits` has no
`max_catalog_bytes` or `max_primary_key_bytes`. `GraphSampleLimits::new` accepts
`(seed_nodes, global_edges, max_nodes, max_string_bytes, max_decoded_rows,
max_page_work, row_batch_size)`. `GraphNeighborhoodLimits::new` accepts
`(max_string_bytes, max_properties_per_record, max_matching_edges,
max_decoded_rows, max_page_work, row_batch_size)`. Removed fields are rejected
as unknown configuration fields; there is no compatibility alias.

Thrift container counts and aggregate declared entries are checked before the
generated SDK reserves lists. Individual wire values, footer bytes, metadata
strings, page sizes and decompression limits remain input guards. No `size_of`
forecast or parsed-owner total is charged. An inner decoder/semantic error
still reaches the original final source/context guard; no replacement request
is created. These checks do not measure allocator use or RSS.

All anchor, pointer, target/UID, full-property and full-primary-key passes use
one private `DecodeLimits`/`Work` contract derived from the real operation
limits. All rowgroups and decoded/skipped rows are charged before initial SDK
batches. Incoming source mapping may require another complete pointer pass;
property materialization scans once per needed table/member rather than once
per returned entity. Pure request validation errors perform no source I/O.
After the initial physical guard, the same Original and physical bindings are
checked again before exposing success or an inner semantic/decoder error; a final
source conflict takes precedence. This recheck does not rehash the entire corpus.

The strict Parquet subset remains unchanged. Zero-row relations can use an
official SDK file with declared columns and no rowgroups. An empty
rowgroup with zero page offsets remains rejected by the existing chunk-span
guard. The canonical functional tests live in
`crates/graphit-data/src/neighborhood/tests.rs`; the product integration suites validate their consumers.
This is a local library boundary. Arbitrary Ladybug/Cypher, real HTTP/CLI/UI
integration, Lance/search, remote/S3 authorization, writers and native
Windows/macOS runtime validation remains platform-specific.

## S3 and cloud authorization

The new S3 contract includes endpoint, region, path style, explicitly allowed HTTP
when needed, and a credential provider. Refreshing credentials must not change
bucket, prefix or topology. Do not persist credentials in manifests, caches,
project records or logs. Validate read-only publication, denied writes, lease
freshness, fencing, revocation and STS at the product boundary.

## Record storage APIs

Records and Knowledge use their actual Arrow schemas with
[Lance module snapshots](../specs/rust_lance_module_store.md) and
[Lance branch snapshots](../specs/rust_lance_branch_store.md). The SDK validates the
schema, retains immutable versions and performs branch publication and recovery;
authorization remains with each caller. The earlier standalone catalogue codec,
schema fingerprint and initial catalogue preparation had no product consumer
and were removed in `tsk-dff0`, together with their work accounting, dedicated
fixtures and guard registrations. They are not prerequisites for native storage.

## Native Knowledge first publication

The Source72ce Data implementation in `crates/graphit-data/src/knowledge_index.rs`
uses the real Core PROJECT capture/Writer, classified Auth admissions, and the
qualified Foundation/Branch SDK13 APIs. Its public Main namespace is
`graphit_data::knowledge_index`; Auth and Tokio are production dependencies, and
Hub supports the development fixtures. The initial sandbox is
`.build/rust-records-data/knowledge-index-source`. Main activation `tsk-8218` is
qualified on Linux: six Knowledge unit tests, eight current Wiki integration
tests, three projection doctests, strict all-target Data Clippy and formatting
passed. Two Wiki cases cover concurrent heads work from `tsk-a542`; this receipt
does not close that independent delivery. Logs are in
`.build/rust-main-knowledge-data-activation-20261011`. Knowledge/M1b remains
separate consumer integration. The older Lance12
probes above are historical evidence, not this producer's SDK qualification.

`publish_native_authored` requires original Knowledge WRITE and prepared H rows.
`read_native_knowledge_index` requires original Knowledge READ and returns either
authoritative `NoCorpus` or the complete owned FULL16 `Published` generation.
Both APIs derive the SDK branch exclusively from
`admission.project()?.version.lance_branch()`; they accept no separate selector.
An empty result selects default; a nonempty result selects the encoded Core
branch. Real Auth grants bind each logical version. An absent named target
inherits default; a materialized branch retains its own pin as default advances.
The private SDK finalization fixture proves `PartialDefault` and the original
SDK cause using a real confirmed default; invalid SDK selectors are not public
API inputs.
See [Data boundary and physical schema](../architecture/rust-knowledge-data-boundaries.md#produtor-data-de-primeira-publicação-e-leitura-nativa)
for exact nullability, time conversion, caps, probes, pin/EOF and partial effects.
The contract does not provide full Records P-D/P-V, an initial recovery resolver,
or continuous SDK pathname confinement. Foundation errors preserve the original
recovery key/cause; bootstrap errors preserve their actual observations.

Run the affected Main suite with these repository-relative commands:

```text
cargo test --locked --offline -p graphit-data -j1 --lib knowledge_index
cargo test --locked --offline -p graphit-data -j1 --test wiki_corpus
cargo test --locked --offline -p graphit-data -j1 --doc knowledge_document_projection
cargo clippy --locked --offline -p graphit-data -j1 --all-targets -- -D warnings
cargo fmt -p graphit-data --check
```

The three affected doctests belong to the prepared document projection used by
this facade; `knowledge_index` has no separate doctests. Main qualification
reuses the SDK sandbox target with development/test debug disabled and incremental
compilation off. The unchanged Foundation six and Branch eight tests retain
their Main `tsk-8a17` receipt and are not repeated for these dependency/export
edges. Complete offline Cargo metadata resolution followed by locked metadata
verifies the new dependency edges without changing SDK pins.

The FULL16 nonempty test must create, reopen and scan a real SDK dataset, comparing
all data and schema/children/metadata. Static schema support or an encoder test is
insufficient. Class-fence tests use real issuer grants; no synthetic admission
factory stands in for Auth. Linux results are recorded only after execution;
Windows/macOS and Main caller integration require their own qualification.

Current Knowledge Read/Write Content retains only the individual identifier
bound. Arrow schema, digests, timestamps, checked conversions, namespace,
state, witnesses, EOF, CAS and actual publication effects remain validated.
There is no Knowledge serialized-row quota. Native file capture and
Artifact/Payload pass-through do not establish a Lance-image allocation or RSS bound.

## Explicit named Knowledge retirement refinement

Knowledge WRITE authorizes explicit mutation/removal of its own authenticated
named branch. Default is outside the first cut; Task/Memory/AST authority is not
implied. No public Data retirement API is delivered yet. Independent `tsk-1e53`
qualifies Lance13 `Dataset::delete_branch`, references, old pins, concurrency and
errors between branch metadata removal and directory cleanup before accepting
the Data API. Its SDK tests have not run.

A Runtime consumer may update the maintained inventory after the explicit
confirmed effect, preserving that effect if Core metadata subsequently fails.
Core rejects registered checkouts selecting the version and deletes no corpus;
there is no cross-store atomicity/rollback. This operation introduces no usage
ledger, quotas or universal contribution proof as an extra N5 gate.
See [named retirement boundary](../architecture/rust-knowledge-data-boundaries.md#retirada-explícita-do-branch-knowledge-nomeado).

## Knowledge metadata head reads

Source `tsk-a542` adds `knowledge_index::read_native_knowledge_heads` to the
existing Data module. Main Linux qualification passed 8 integration tests,
8 focused library tests, 20 doctests, strict Data all-target Clippy, owned
formatting and diff-check. Those counts are historical qualification.
The current API requires the original SnapshotContext Arc, a borrowed
Knowledge ProjectReadAdmission and `KnowledgeHeadReadLimits` containing
only positive `max_slug_bytes`, native capture limits and `batch_rows`.
There is no aggregate head/text/reference budget, and no former text cap
is converted into an identifier cap.

The result is NoCorpus or Published with the observed KnowledgeGeneration
and owned heads. The scanner projects the eight metadata/witness fields,
excluding body and recipe. It validates namespace, state, projection and
witness alongside slug/title/update metadata. Removed rows are validated
and included in duplicate detection before exclusion. Empty titles and
missing dates remain literal. Invalid slug, required nulls, schema mismatch,
invalid witness, duplicates or late errors reject the complete result.

Original Auth/request/native capture checks and the admitted version drive
all reads. There is no independent branch selector, birth, repair, mutation,
retry or partial output. Valid B1 probe absence differs from an available
empty Published result; present corruption/schema/SDK/Auth/IO/request errors
stay causal. Generation is internal storage observation, not authority.
Never export its corpus URI/physical branch to Now or remote consumers.

The future Runtime compositor sorts dates descending (None last), then slug,
before latest20 selection; total and has_more come from the complete admitted
head set. It supplies logical project/version/slug links and invents no
Knowledge author. FULL16 readers remain available for M1b. This API does not
qualify Now CLI, Broker ACL/transports, Task leases, Memory heads or USER.

Focused invocation for the current Main composition:
`cargo test --locked --offline -p graphit-data --test wiki_corpus` and
`cargo test --locked --offline -p graphit-data --lib knowledge_index::`.
Doctests and strict Clippy must use the same Main composition. Qualification
artifacts belong to `.build/rust-records-data/knowledge-heads-source/`;
Linux execution alone does not qualify other platforms. The existing
Foundation6/Branch8 producer receipt is reused without rerunning those suites.

The storage APIs preserve the FULL18 physical schema, complete EOF, witnesses, namespace/state, CAS, actual causes and individual identifier limits without aggregate corpus accounting.

### Browser AST reads and sample anchors

The foreground BrowserAuth service exposes POST `/api/ast/contexts` with an empty JSON object, and POST `/api/ast/query`, `/schema`, `/sample`, `/neighborhood` with the exact PROJECT selection and the corresponding typed input. Discovery returns only live, currently AST-enabled projects admitted for ReadAst; `current` is null when the operational current selection is not admitted. PROJECT selectors contain NativePath `project_dir` and the actual `project_version`; `project_id` is display identity rather than a selector.

The Web cookie and canonical Origin identify a registered Web caller. A valid locally verified JWT still requires the separate Broker ReadAst PROJECT decision. The captured physical scope, admission and Original survive until final delivery checks. The native Data job uses the confirmed Copy reader, while the authorization remains on its async owner. Task permission and runtime credential coercion are not involved.

PROJECT samples expose the primary key already observed by the bounded Parquet reader as `primary_key:{name,value:{type:"string"|"int64",value:string}}`. The name is the schema's actual key column; Int64 is exact decimal text, including values beyond JavaScript's safe integer range. The interface can pass this value directly as a typed neighborhood anchor. Native installed samples retain their existing JSON fields; new Browser imported samples include the same observed primary-key field. A primary-key DTO carries no authorization or physical binding.

PROJECT Source consumers now use one SourceParameters DTO and the captured SDK
composer. Files can be inspected without a graph, using indexed Source first and
native disk capture on an explicit miss with independent ReadAst permission even when AST indexing is
disabled. Graph and entity operations retain the AST module gate. Original,
live physical selection, authorization and native Source guards remain required.
PROJECT entity lookup consumes the real
Copy reader and exact available graph extents, with Raw generation/branch and
Source SHA association before applying Core's selectors. A captured scope or
observed digest grants no authority; native and Web owners retain their genuine
ReadAst PROJECT admission until the final checks. The Web response serializes
SourceOrigin plus Core's existing selection result, never a restorable capture.

## Knowledge FILE-local reconciliation and semantic publication

The current Knowledge Existing consumer captures selection metadata and prepares
one FILE at a time. Data validates the old FULL18 pin to EOF, keeps compact
identities/classification, compares exact local RecordBatch slices and prepares
only changed rows plus explicit Removed patches. The SDK merge uses slug and
unmatched Keep. No-op precedes prepare/key/staging; commit requires the complete
desired EOF, Source and admission guards. Late failure can leave unpublished
staging, and known commit generations/recovery keys remain causal outcomes.
See [document lifecycle](../specs/rust_knowledge_document_lifecycle.md) and
[selection stream](../specs/rust_knowledge_source_stream.md).

The semantic sidecar uses local document/vector batches, bounded channel transfer
and the real Models Document RPC. Existing vectors are validated from the same
pin without a global copied vector image. Equality remains float slice equality,
including +0 equal to -0; ReplaceComplete checks source membership and deletion,
while Attach preserves exact no-op/skip behavior. See [semantic Data](../architecture/rust-knowledge-semantic-data.md)
and [Models pipeline](../architecture/rust-knowledge-semantic-pipeline.md).

Installed AST Browser readers retain an ImportedAstReadAdmission independently
of PROJECT and imported Knowledge authority. The exact registry owner, captured
installation, GlobalNative/Native entry, id/version and native dataset/graph guards
remain active. Web admission stays on the async owner while blocking graph jobs
consume the real captured reader; the owner rechecks publication bindings and
admission before delivery. Installed entity Source derives extents from the same
reader and applies the existing Source composer without PROJECT fallback.
The native catalogue uses the same installed_ast_contexts SDK reader and validates
ReadAst before and after selection. Neither catalogue DTOs nor primary-key DTOs
restore a physical capture or authorize access.

PROJECT Web semantic retrieval receives the already captured physical tuple,
verified Web caller, fresh ReadAst PROJECT admission and unchanged Original.
It calls SourceServer's shared native/Web kernel with the existing model resources;
no independent model client or request context is constructed by the Browser route.
