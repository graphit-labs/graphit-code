---
title: Remote Icebug graph queries
type: specification
references:
  - target: {type: task, id: tsk-9199}
    relation: implemented_by
---

# Remote Icebug graph queries

The Rust SDK mounts an immutable Icebug release directly through an already admitted `Arc<dyn object_store::ObjectStore>`. It reads manifest/footer ranges and selected projected column chunks with Parquet60's official `AsyncFileReader` and `ParquetRecordBatchStreamBuilder`. It does not download a graph package or materialize the corpus.

The implementation is [remote_graph.rs](../../crates/graphit-data/src/remote_graph.rs), with the SDK adapter in [parquet_reader.rs](../../crates/graphit-data/src/remote_graph/parquet_reader.rs). Its fixtures are [tests.rs](../../crates/graphit-data/src/remote_graph/tests.rs).

## API and admission

`RemoteIcebugGraph::open(store, prefix, expected_manifest_digest, policy, original)` returns a graph handle. `sample_graph(limits, original)` returns the existing `GraphSample` DTO asynchronously.

The caller supplies the configured SDK store, exact release prefix, expected SHA-256 of the exact icebug.json bytes, existing `LocalGraphPolicy`, and its original `Arc<SnapshotContext>`. The digest, paths and metadata identify data. They do not authorize access. Provider selection, captured subject and PROJECT admission remain caller responsibilities. This module neither discovers credentials from environment nor creates grants or alternative defaults.

Open reuses the canonical manifest parser, bounded compact-protocol footer parser and scalar/chunk validators. Every declared node, forward member and reverse member has its footer/schema checked. Reverse CSR tables are retained for incoming adjacency, while reverse edges are not emitted as extra sample edges.

Object metadata is captured once. Range requests carry its ETag condition and object version when supplied by the SDK backend. A backend that provides neither still requires an immutable published release prefix. ETag is not a content hash or authority. Only the manifest bytes are hashed; this module does not claim a digest of all corpus files.

## Result and I/O semantics

Samples preserve seed nodes, a global edge cap over forward members sorted by physical table, seed/endpoint union, logical row IDs and sorted nodes. Node IDs use the shard's logical `row_base`; `row_offset` only locates its physical Parquet slice. Consolidating physical storage does not renumber those IDs. Multiple edges with distinct UIDs remain distinct; self-loops appear once. Loaded primary keys are typed, nonnull and unique among returned nodes. Loaded edge UIDs are nonempty and unique. Nullable summaries remain absent, Unicode remains intact and line_number zero becomes None.

Each returned node includes `GraphNodeSummary::primary_key()` with the observed
field name and its STRING or INT64 value. The sample reuses the key already
decoded for that selected node, so a consumer has its typed exploration anchor
without another graph query. The key remains distinct from the capture-local
logical node ID; neither value is replaced or derived from display text.

CSR pointers are checked for bounds and monotonicity over the consumed range. When a full member is sampled, its last pointer and pointer row count must match the declared member. Unselected rows are not scanned to prove unrelated global primary-key membership.

The SDK uses ProjectionMask and RowSelection with a finite batch size. The edge prefix is one consecutive SDK range, without an intermediate row-ID vector; requested node rows are validated and converted through the SDK range constructor. Without page indexes it may fetch an entire selected compressed column chunk, even for a small requested row subset. It does not fetch every corpus column. Each loaded chunk is checked using the existing canonical page/header/span/value-count guards before SDK decompression. Existing footer/header/chunk/page/string/count guards and actual decoded-row/page-work checks remain. There is no estimated heap ledger, reservation, output-byte quota, or new runtime/pool.

Arrow60 arrays remain private to this reader. Lance and the public Arrow58 contracts are unchanged. No Arrow array reinterpretation is performed. The deprecated ParquetObjectReader is not used.

## Neighborhood, properties and cursor

Task tsk-71f2 supplies `RemoteIcebugGraph::neighborhood_graph(request, limits, original)`, returning the existing `GraphNeighborhoodPage`. It uses the same graph/store/metadata and receives a fresh original context for each operation. It introduces no second Parquet backend or cursor representation.

`input_limits()` returns the exact `LocalGraphLimits` retained by this graph pin, independently of query output row/byte limits. It adds no defaults and performs no object I/O.

`resolve_query_node(label, key, row_batch_size, original)` resolves only the PK using this pin's captured `query_decoded_rows`, `max_pages` and `max_chunk_bytes` for STRING values. The explicit positive batch comes from the caller's official SDK vector capacity and is clamped to `query_decoded_rows`. Label names retain `max_name_bytes`; neighborhood matching-edge/property limits do not participate. The returned anchor borrows this reader, and the fresh Original is checked before lookup and before delivering its result.

Its result is `Option<ResolvedGraphNode>`: only the exact typed PK-absence result becomes `None`, after the final Original guard. Duplicate keys, PK type mismatch, integrity, limit, I/O and Original failures remain errors. No Source resolution occurs.

`read_query_node(&anchor, columns, row_batch_size, original)` validates this exact reader borrow and reads only the resolved row and requested declared columns, adding the real PK. It returns `GraphPropertyNode` with sorted typed properties. Empty columns return just the PK; unknown columns fail explicitly as projection errors. The method reuses captured object ranges, row selection and decoder limits, without another PK lookup, neighborhood traversal or Source read. Unrequested column bodies remain unread; selected corruption fails. The fresh Original is rechecked before delivery.

The request selects a typed STRING or INT64 primary-key anchor and optional relationship, other endpoint label and direction filters (all absent means all kinds and Both) and page size (1..=1000). The reader uses captured SDK statistics to exclude key groups which cannot match, then projects the primary key to find the unique exact anchor. Modern truncated min/max remain conservative bounds, including for STRING keys longer than the SDK's default 64-byte statistics limit. Missing or legacy statistics do not exclude a group; modern inverted bounds fail explicitly. It does not build a corpus primary-key map or prove uniqueness of unrelated keys. Nonnull/type/string guards still apply to loaded keys. Reserved summary columns retain their canonical types; an INT64 primary key uses its declared INT64 field, not a STRING summary such as uid.

Outgoing traversal reads the anchor's two CSR pointers, validates that observed interval and applicable first/final boundary, and selects only that adjacency range. Incoming traversal uses declared reverse CSR to select adjacent source rows and UIDs. After UID page selection it resolves selected UIDs within their source's forward adjacency, batching requests for the same adjacency into one read and preserving each parallel occurrence, forward member/row identity and properties. Coverage or selected forward/reverse differences are errors. If reverse storage is absent, exact-target statistics prune forward groups and selected pointer probes resolve source ordinals. No reverse ordinal is interpreted as a forward offset; reverse declarations add no duplicate/self-loop edges.

The page uses the existing UID ordering and framed query fingerprint shared with the local DTO. `next_cursor()` binds the typed anchor and query filters and continues by complete logical relationship group (UID, relationship kind, source/target labels and typed primary keys), plus the ordinal of indistinguishable occurrences. It carries no Graph generation, manifest digest or durable physical member/row offset. Page size can change between pages. A cursor for another query, an empty UID or an oversized UID fails. App additionally binds the opaque cursor to the logical PROJECT/version or installation/artifact/version; those identifiers remain data, not authority. Each operation captures its own reader, so a later FILE update does not reject a logical keyset solely because the graph changed. Cursor data cannot restore a graph, original request or authority. Every page scans the requested matching stream using its keyset; all matching results remain reachable through continuation, subject to the current input/work limits.

Page selection retains complete continuation information for the selected logical relationship group. Repeated UIDs do not collapse distinct types, endpoints or indistinguishable parallel occurrences. No corpus-wide UID proof set is constructed. Matching-edge count, decoded batch rows and loaded-page work use the existing concrete input guards. Exceeding an input/work limit is an error, not a silently truncated page.

`GraphPropertyNode::primary_key()` borrows the already projected property name and STRING/INT64 value. Only page anchor/endpoints and edge rows have complete properties projected. Each property edge exposes its complete logical occurrence position; App serializes the position's ordinal as a decimal string. Field names are sorted; STRING, INT64, BOOL and null values remain typed. Edge target is structural and excluded from ordinary properties; uid remains a property. Nullable name and raw line_number zero remain null/zero properties (the separate summary API still presents zero line as absent). Selected PK duplicates, missing endpoint rows and target/UID reread differences fail. Node/edge offsets belong only to their capture; public typed primary keys and logical relationship groups identify data when composing separate pages. Offsets from different operation captures are not compared as persistent identity.

The narrow shared code in [neighborhood.rs](../../crates/graphit-data/src/neighborhood.rs) only exposes crate-private DTO storage, Candidate/selected_rows, existing text bounds and the existing fingerprint to this consumer. `resolve_node(label, key, limits, original)` returns `ResolvedGraphNode` borrowing the actual graph pin. `neighborhood_graph_at(&anchor, request, limits, original)` verifies reader identity and the selected PK without repeating key resolution. An anchor from another graph is rejected even if its table/row are equal. No generation token restores or substitutes that borrow. Remote implementation and fixtures are [remote neighborhood](../../crates/graphit-data/src/remote_graph/neighborhood.rs) and [SDK query tests](../../crates/graphit-data/src/remote_graph/neighborhood/tests.rs).

## Cancellation and lifecycle

Open, sample and neighborhood queries use the caller's original context before work, after I/O and batches, and before returning. Tokio awaits use its optional absolute deadline and permanent cancellation. Ordinary `Some(Instant)` keeps its deadline; `SnapshotContext::service` supplies `None` without a synthetic timer. A pending SDK read still cancels and final Original checks preserve Interrupted/TimedOut causes. Dropping a pending operation drops the SDK future and local buffers. SDK decoding is cooperative between polls; this API does not claim hard preemption of synchronous decoding.

The graph retains the SDK store, immutable manifest, table metadata and input policy. It retains no opener/query Original, decoded corpus or detached job. Each sample or neighborhood query receives its own context, so cancelling one does not cancel a sibling operation. The last graph/store owner releases the SDK handles through Drop.

RemoteGraphError preserves DataReadError, original io errors, object_store errors and Parquet errors through Error::source. Missing objects, malformed data and denial are errors, not optional component absence. A manifest digest mismatch is distinct.

## Qualified scope and remaining integration

Task tsk-c098 delivers the SDK sample reader. Fixtures use genuine SerializedFileWriter Parquets with ZSTD in the official InMemory store; the official ThrottledStore exercises I/O cancellation and deadlines. They do not certify an authenticated S3/STS deployment.

Task tsk-71f2 delivers the SDK neighborhood/property/cursor method. Caller tsk-5651 now connects installed RemoteNative sample and typed-neighborhood routes with actual Web admission, exact artifact selection and real Broker HubRead/STSv3/S3 on Linux. SDK-only fixtures retain their narrower scope, and GraphExplorer/application UI retain their own deliveries. The maintained [S3 guide](../guides/s3-and-ui-network.md) describes the ecosystem storage behavior; this SDK slice supplies direct Icebug sample and neighborhood queries within that behavior.

Earlier sample-reader qualification, preceding the current cursor and position changes: 11 SDK fixtures passed with zero failures/ignored tests; strict cargo clippy --no-deps --lib --tests passed with -D warnings. Scoped format/diff checks and API/documentation links passed. Receipts are in .build/rust-remote-icebug-c098/qualification.json. This is SDK fixture qualification, without a claim of native S3/STS deployment.

Earlier neighborhood qualification, preceding the current cursor and position changes: ten distinct native SDK fixtures passed, covering typed anchors/properties, incoming/outgoing traversal, logical CSR source rows, parallel edges/self-loops, complete keyset pages, invalid cursors/data, input guards and original cancellation/deadline/drop. Six compile-fail doctests preserve public DTO privacy. Strict analysis, scoped format/diff and source/documentation results are recorded in .build/rust-remote-neighborhood-71f2/qualification.json. Two initial fixture failures and their causal corrections remain in the receipts. SDK fixtures do not certify an authenticated S3/STS deployment.
