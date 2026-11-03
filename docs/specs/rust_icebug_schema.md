# Borrowed Icebug schema

Task tsk-9706 supplies CapturedLocalGraph::schema() -> Result<GraphSchema<'_>, DataReadError> for the actual local installed GraphExplorer consumer.

The method checks the existing Original and captured-content validation. The view borrows the already validated manifest and Parquet metadata; it copies no strings/buffers and parses no new format. It exposes generation as data, not authority. Public fields/constructors remain private and the view cannot outlive its captured graph.

GraphSchema::node_tables() yields GraphNodeSchema views with label(), primary_key(), rows(), row_extent(), shards(), columns() borrowed (name,type) pairs and language_counts() borrowed (language,count) pairs. Values come from canonical node declarations whose schema/row counts were checked at capture. This is the complete declared schema, not inference from a sampled page.

GraphSchema::relationships() yields GraphRelationshipSchema for each actual forward member: kind(), from_label(), to_label(), rows(), properties(), source_row_base(), source_rows(), target_row_base() and target_rows(). Properties use validated footer names/types STRING/INT64/BOOL/UINT64; structural target is excluded. Reverse storage does not create extra relationship groups or duplicate self-loops.

CapturedLocalGraph remains bound to its Original. Cancellation/deadline/content-change errors stay DataReadError::Source with their IO cause. The metadata view itself contains data; acquisition/delivery authorization remains the consumer's ReadAst obligation.

Source: [lib.rs](../../crates/graphit-data/src/lib.rs). Native fixtures: [schema_tests.rs](../../crates/graphit-data/src/schema_tests.rs). Existing capture/sample/neighborhood and remote reader APIs retain their typed identities and cancellation behavior; their physical readers now resolve immutable node and CSR shards.

The current local native package contract is SourceRef.directory() as payload root with graph.icebug and a regular graph.icebug/schema.cypher; genuine graph-only authored assets may omit Source, while the indexed AST producer publishes separate source.lance and corpus.lance roots. Hub native_package validates the envelope, removed at install, and App native_install admits graph.icebug with CapturedLocalGraph. Local payloads do not require remote native.json.

This SDK metadata producer does not qualify HTTP/UI/query/search or remote mounts. The local imported_graph/graph_server consumer tsk-868f must reuse exact installed Ast/id/version selection, current ReadAst and the same Original. Full Cypher/query-editor functionality remains a separate required mature-engine integration tsk-30db; remote installed mounts remain tsk-5651 with actual STS/admission.

Current Main qualification: four affected native fixtures PASS, two compile-fail borrow/privacy doctests PASS, strict Data --no-deps lib/tests PASS0. Receipts are in .build/rust-local-icebug-schema-9706/qualification.json.

## Live PROJECT schema

The live PROJECT consumer also uses this metadata producer. `LiveGraph::with_schema`
captures the current publication once under its writer guard and the caller's
PROJECT read fence, validates its owned namespace, and lends the schema to the
response serializer. It retains the same Original and checks it again after
serialization. Schema inspection does not start the Cypher engine or recount
every table and relationship through separate queries.

The FILE publisher already records exact node totals and language counts in
`icebug.json`. The consumer preserves columns and primary keys using the shared
`NodeSchema` serializer. Forward CSR members are summed by relationship kind,
source label and target label, so several FILE shards produce one relationship
entry with the total row count. Empty retained labels and relationship groups
remain visible with zero rows. The PROJECT generation, string row counts and
output-byte boundary stay unchanged; schema metadata does not grant access.

This changes schema delivery only. Sample, neighborhood and arbitrary Cypher
queries still use the existing in-memory Ladybug engine over captured Parquet
files. Their query execution, cancellation signals and generation-bound IDs and
cursors are unchanged. Source locations are
[live_graph.rs](../../crates/graphit-ast/src/live_graph.rs) and
[graph_server/live.rs](../../crates/graphit-app/src/graph_server/live.rs).
