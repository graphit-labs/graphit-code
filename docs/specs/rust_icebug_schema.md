# Borrowed Icebug schema

`CapturedLocalGraph::schema() -> Result<GraphSchema<'_>, DataReadError>` supplies
the schema used by the local GraphExplorer and native query catalogue.

The method checks the operation context. Its view borrows manifest declarations
and retained Parquet metadata without copying strings or buffers. It exposes
metadata as data, not authority; no derived graph-generation digest is returned.
The borrowed view cannot outlive its graph.

`GraphSchema::node_tables()` yields borrowed `GraphNodeSchema` views with
`label()`, `primary_key()`, `rows()`, `row_extent()`, `shards()`, `columns()`
(name/type pairs) and `language_counts()` (language/count pairs). These are the
complete manifest declarations, not inference from a sampled page. Generated
local nodes validate their actual physical schema and primary key when consumed;
external and remote captures keep their eager validation. See the
[storage contract](rust_icebug_storage.md#local-file-publications-and-the-shared-reader).

GraphSchema::relationships() yields GraphRelationshipSchema for each actual forward member: kind(), from_label(), to_label(), rows(), properties(), source_row_base(), source_rows(), target_row_base() and target_rows(). Properties use validated footer names/types STRING/INT64/BOOL/UINT64; structural target is excluded. Reverse storage does not create extra relationship groups or duplicate self-loops.

`CapturedLocalGraph` retains its operation context. Cancellation, deadline and
filesystem errors preserve their I/O cause through `DataReadError::Source`;
invalid physical Parquet data has its typed decoder error. The metadata view
contains data, while acquisition and delivery authorization remain the
consumer's `ReadAst` obligation.

Source: [lib.rs](../../crates/graphit-data/src/lib.rs). Native fixtures: [schema_tests.rs](../../crates/graphit-data/src/schema_tests.rs). Existing capture/sample/neighborhood and remote reader APIs retain their typed identities and cancellation behavior; their physical readers now resolve immutable node and CSR shards.

The current local native package contract is SourceRef.directory() as payload root with graph.icebug and a regular graph.icebug/schema.cypher; genuine graph-only authored assets may omit Source, while the indexed AST producer publishes separate source.lance and corpus.lance roots. Hub native_package validates the envelope, removed at install, and App native_install admits graph.icebug with CapturedLocalGraph. Local payloads do not require remote native.json.

Local installed graph consumers select the exact AST artifact ID/version and
retain current `ReadAst` authorization. Remote mounts additionally use their
authorized storage and temporary credentials. A schema view does not itself
qualify any transport, installation or platform.

## Live PROJECT schema

The live PROJECT consumer also uses this metadata producer. `LiveGraph::with_schema`
captures the current publication under its publication mutex, retains the
declared open files and releases the mutex before lending the schema to the
response serializer. It validates the owned namespace, retains the caller's
PROJECT authorization and checks the operation context again after
serialization. Schema inspection does not start the Cypher engine or recount
every table and relationship through separate queries.

The FILE publisher already records exact node totals and language counts in
`icebug.json`. The consumer preserves columns and primary keys using the shared
`NodeSchema` serializer. Forward CSR members are summed by relationship kind,
source label and target label, so several FILE shards produce one relationship
entry with the total row count. Empty retained labels and relationship groups
remain visible with zero rows. String row counts and the output-byte boundary
remain; schema metadata does not grant access or align independently published
corpora.

Samples and typed neighborhoods read captured Parquet directly; arbitrary Cypher
uses the official in-memory Ladybug engine over those captured files. Physical
ordinals remain local to the result, resolved anchors borrow their actual reader,
and keyset cursors bind the logical query and admitted scope rather than a global
revision. Original cancellation and native snapshot guards remain. Source locations are
[live_graph.rs](../../crates/graphit-ast/src/live_graph.rs) and
[graph_server/live.rs](../../crates/graphit-app/src/graph_server/live.rs).
