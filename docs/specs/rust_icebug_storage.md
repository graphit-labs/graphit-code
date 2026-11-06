---
references:
  - target: {type: task, id: tsk-c9f0}
    relation: supports
  - target: {type: task, id: tsk-3b54}
    relation: supports
  - target: {type: task, id: tsk-5651}
    relation: relates_to
---

# Captured Icebug storage API

The validated local graph exposes graph-root relative storage names without copying buffers: `GraphNodeSchema::shards()` (file, row base and row count), and `GraphRelationshipSchema::indices_file` / `indptr_file`. Relationships expose forward storage; reverse storage does not duplicate graph relationships. These names borrow the existing validated node manifest and Parquet table metadata. `rows()` is the live row total; `row_extent()` is the largest active shard end, including holes. Each relationship member names one source/target shard using `source_row_base`, `source_rows`, `target_row_base` and `target_rows`. Its indptr has `source_rows + 1` entries and its target values are local ordinals in the target shard.

`CapturedLocalGraph::open_file(relative)` delegates to the private captured Core reader. It checks the retained original operation before and after opening. The returned opaque `CapturedSourceFile` owns its captured handle; range and stream reads retain the same original context even after the graph handle drops. Cancellation, deadline, changed-file and invalid/uncaptured-path errors remain causal. It opens neither uncaptured additions nor an arbitrary absolute path. Native parent paths need no UTF-8 conversion.

Data metadata and paths are observations, not grants. The actual caller must admit ReadAst and the installed project/version before protected IO, then preserve revalidation and final delivery checks. This API does not expose the reader, authenticate a caller or restore authority from filenames.

The official Ladybug adapter uses these files in a private in-memory database through the official FileSystem seam: read-only captured IO, without external/default-filesystem fallback or writes. The [captured Cypher adapter](../../crates/graphit-ast/src/icebug_query.rs) and [authenticated GraphServer HTTP caller](../../crates/graphit-app/src/graph_server.rs) provide query admission, fallible native results, cancellation interruption and owned worker shutdown. This storage API alone does not implement the complete Explorer.

The per-FILE producer synchronizes its completed node, fact and CSR files through
an existing-file handle opened for writing before manifest publication. This
synchronization opens neither a new file nor a truncated replacement. Windows
[`FlushFileBuffers`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)
requires write access; a read-only handle cannot perform this producer flush.
Captured Source readers retain their read-only access.

The [Icebug producer](../../crates/graphit-data/src/graph_writer.rs) forms node
row groups from at most 1,024 spool records and an 8MiB serialized-input target.
A record larger than that target gets its own group; its properties are preserved
without truncation or rejection. This controls the batch materialized by native
scans before filtering, including highly compressible JSON observations; compressed
file size alone does not describe that memory use. The target applies to one writer
batch, not the corpus or a query quota. Changing row-group boundaries preserves
record order, global table offsets and relationship endpoints. Existing publications
are left intact; the ordinary producer writes this layout on its next publication.

Focused genuine ZSTD Parquet fixtures cover independently renamed storage files, exact bytes and stream position, invalid/uncaptured paths, changed files, and cancellation/deadline after the graph owner drops. The production API has no second manifest parser or corpus materialization.

## Local FILE publications and the shared reader

Local and installed Hub graphs use the same canonical Icebug Parquet node and CSR
files. A node declaration contains ordered `shards: [{file, row_base, rows}]`;
active ranges must not overlap and their row counts must add up to the declared
node-table total. Members bind actual active shard ranges. This is the maintained
producer/reader contract; the reader does not import a persistent Ladybug database.

The Graphit adapter uses official ExternalTableHooks to compose Ladybug
IceDisk node and CSR readers under each logical label. They read Parquet
row groups on demand and translate node offsets
from the captured row base. Removed ranges are invisible. An older FILE shard
may omit a subsequently added nullable property; both the captured native adapter and Rust property
readers return NULL for that property. CSR source/target bases preserve joins
without renumbering unrelated FILEs or rewriting their Parquets. Public keys and
relationship UIDs retain their existing meaning. A local query retains the
producer identity `SHA256(namespace || revisionLE)`.

`CapturedSourceReader::capture_generated_metadata` captures exactly the manifest
declarations and their ancestor bindings. It neither walks unrelated files nor
hashes or copies Parquet bodies. Metadata-only nodes expose no payload digest;
full-content validation cannot claim a digest for such a capture. Range reads
check the same Original, opened descriptor and native observations before and
after I/O. The publication owner retains its read fence through query completion.
`CapturedLocalGraph::capture_generated_metadata` validates the manifest and
Parquet footers through that Core seam. Locally produced catalog bytes/counts have no imported-artifact corpus quota; structural shard/schema validation is shared, and concrete Parquet I/O and response bounds remain. It rejects a changed manifest between
declaration capture and file capture.

The existing CXX adapter creates only `Database(":memory:")`. Local files and
authorized S3 ranges reach its same read-only captured FileSystem; storage
credentials and PROJECT admission remain in their composing callers. No native
database is persisted or populated with a copy of the graph. Snapshot export
continues through the canonical producer described in
[native publication](rust_native_publication.md).

The native Linux regression covers counts, sparse IDs, missing nullable columns,
forward/reverse and two-hop CSR joins, replacement generations and overlapping
range rejection. Core range fixtures exercise an 8 GiB sparse declared file,
ignored unrelated paths, changed bindings, links, escape and cancellation. Data
fixtures exercise the same shard addressing through local files and object
storage. Rust consumer test execution is recorded separately from native SDK
qualification; this contract does not imply Windows/macOS execution.
