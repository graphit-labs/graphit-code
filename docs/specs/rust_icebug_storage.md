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

The canonical v3 manifest describes topology and storage without a graph-wide
`generation` field. Both the direct Icebug writer and the Live publication writer
emit that shape. The strict reader rejects unknown fields, including the removed
field; no legacy reader, alias or generated replacement digest restores it.
Publication namespaces, transactional revisions, FILE observations, physical
object ETags/versions and SDK dataset pins retain their separate operational
purposes. They do not replace the original request or grant access. Existing
stored payload changes belong to an explicit publication/distribution operation;
the reader does not rewrite active data to make an old payload pass.

On first relationship use, the native adapter checks the physical edge row count
and streams exactly `source_rows + 1` non-null UINT64 pointers through the official
SDK reader before CSR traversal or degree summaries. Pointers start at zero, never
decrease, never exceed the member's edge count, and end at that count. This check
is local to the used relation; node-only queries do not decode unrelated CSR.

The validated local graph exposes graph-root relative storage names without copying buffers: `GraphNodeSchema::shards()` (file, logical row base, physical row offset and row count), and `GraphRelationshipSchema::indices_file` / `indptr_file` with their physical row offsets. Relationships expose forward storage; reverse storage does not duplicate graph relationships. These names borrow the existing validated node manifest and Parquet table metadata. `rows()` is the live row total; `row_extent()` is the largest active shard end, including holes. Each relationship member names one source/target shard using `source_row_base`, `source_rows`, `target_row_base` and `target_rows`. Its indptr has `source_rows + 1` entries and its target values are local ordinals in the target shard.

`CapturedLocalGraph::open_file(relative)` delegates to the private captured Core reader. It checks the retained original operation before and after opening. The returned opaque `CapturedSourceFile` owns its captured handle; range and stream reads retain the same original context even after the graph handle drops. Cancellation, deadline, changed-file and invalid/uncaptured-path errors remain causal. It opens neither uncaptured additions nor an arbitrary absolute path. Native parent paths need no UTF-8 conversion.

Storage names are relative POSIX paths with variable depth. Nodes, CSR members
and the schema may use different fan-out directories; readers follow their
manifest declarations rather than deriving a path from a label or FILE name.
A physical path cannot declare conflicting storage roles, such as both manifest
and schema. Disjoint slices may reuse a physical file within the same role;
their ranges and declared metadata still undergo the normal validation.
Empty components, `.` and `..`, absolute paths, backslashes, NUL and colons are
rejected before opening declared storage. The local Core capture also rejects
symbolic links in the file or its ancestors and rechecks captured bindings during
I/O. A generated metadata capture does not admit an undeclared file just because
it exists beside a declared shard.

The same names select objects beneath the admitted remote release prefix.
Query handles expose only captured graph files. Each range is checked against
the captured object length and sent through the official object-store SDK with
the captured ETag/version preconditions supplied by the store and the current operation's cancellation and
deadline. A replaced object fails that operation; the reader does not download
another copy or reopen a newer release to conceal the change. The native package
keeps its existing payload root, canonical entry rules and configured depth/path
limits. Variable graph depth does not bypass those package limits.

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
files. A node declaration contains ordered
`shards: [{file, row_base, row_offset, rows}]`. `row_base` is the logical node
address; `row_offset` selects rows in the physical Parquet. Physical offsets
default to zero. Logical active ranges must not overlap and their row counts
must add up to the declared node-table total. Consolidation changes physical
storage without renumbering public nodes or changing their keys.

CSR members still bind exact logical source and target shard ranges. Their
`indices_row_offset` selects `rows` physical edge records;
`indptr_row_offset` selects `source_rows + 1` physical pointers. Both offsets
default to zero. Each pointer slice starts at zero and ends at that member's
edge count. Targets remain ordinals within its declared target shard. An edge
offset in a shared file is never added to pointer values or exposed as a new
public relationship identity.

Shared files must have a compatible role and schema: node slices share a label,
indices slices share a relationship kind, and pointer slices use the fixed
`ptr UINT64` schema. Nonempty physical ranges in one file must not overlap;
empty slices occupy no rows. Schema and manifest paths cannot alias payload
files. The parser checks roles, range overflow and overlap; the actual Parquet
footer must additionally prove bounds and complete row-group alignment. A
pointer slice contains exactly one complete row group, even when the physical
file contains groups for several members. Empty slices are valid at admitted
physical boundaries, including the end of the file.

Readers retain the official Parquet decoders. The native adapter selects complete
groups through a standard Parquet footer view over the captured ranges, without
copying the body into another file or downloading a complete remote graph.
Manifest metadata and footer views do not broaden the caller's authorization or
open uncaptured files. A captured file handle still names the physical file;
logical slice selection does not create a new byte-level authority boundary.
This is the maintained producer/reader contract; the reader
does not import a persistent Ladybug database.

The Graphit adapter uses official ExternalTableHooks to compose Ladybug
IceDisk node and CSR readers under each logical label. They read Parquet
row groups on demand and translate node offsets
from the captured row base. Removed ranges are invisible. An older FILE shard
may omit a subsequently added nullable property; both the captured native adapter and Rust property
readers return NULL for that property. CSR source/target bases preserve joins
without renumbering unrelated FILEs or rewriting their Parquets. Public keys and
relationship UIDs retain their existing meaning. Namespace validation preserves
PROJECT/version ownership without deriving a graph generation from its revision.
Node and edge ordinals describe the captured result; they are not portable keys.

`CapturedSourceReader::capture_generated_metadata` captures exactly the manifest
declarations and their ancestor bindings. It neither walks unrelated files nor
hashes or copies Parquet bodies. Metadata-only nodes expose no payload digest;
full-content validation cannot claim a digest for such a capture. Range reads
check the same Original, opened descriptor and native observations before and
after I/O. The publication owner retains its read fence through query completion.
`CapturedLocalGraph::capture_generated_metadata` validates the manifest and
Parquet footers through that Core seam. Locally produced catalog bytes/counts have no imported-artifact corpus quota; structural shard/schema validation is shared, and concrete Parquet I/O and response bounds remain. It rejects a changed manifest between
declaration capture and file capture.

The generated path interprets the manifest once. After capturing its declared
files, it compares the captured manifest bytes with that same input before using
the interpreted declarations. It does not reopen and parse the manifest again.
Within this operation, physical Parquet files already share validated footer and
SDK metadata across their logical node/CSR slices. Those readers and metadata
remain bound to the current Original; this does not retain a graph capture for
another request.
Data retains only the `live.namespace` field from the AST publication metadata;
Serde validates and skips the remaining JSON values without allocating their
object tree. The AST codec remains responsible for its FILE catalog. Namespace
comparison is still mandatory for the admitted local publication, and malformed
namespace arrays fail the Data parser.
When present, `live` must be a JSON object; a sequence containing a valid namespace
cannot stand in for that object.

## Local sample reads

Graph capture validates the catalogue and Parquet footers before sampling.
`CapturedLocalGraph::sample_graph` then reads bodies only from the shards and
row groups involved in selecting seeds, edges and their endpoints. Selection
stops considering later relationship members when the edge allowance reaches
zero. A selected member's pointer slice still supplies the validated CSR mapping
from its edge ordinals to logical source rows.

The private [selected-row reader](../../crates/graphit-data/src/parquet.rs)
decodes each intersecting row group completely through the official SDK and the
existing adapter, including EOF and page/count validation for the requested
columns. It delivers only the requested logical rows to the sample consumer;
shards and groups outside that selection do not request body ranges. Cancellation,
deadlines, captured-file checks and actual decode/page limits remain in force.

Marker filtering excludes a `true` row only from seed selection. Missing, NULL
and false markers remain eligible seeds, and a marked row can still appear as
a real relationship endpoint. Primary keys must be non-NULL, correctly typed
and unique among the selected nodes of their label. A duplicate in an unselected
row does not invalidate another preview. Errors encountered in the metadata or
body actually read remain explicit; a sample does not certify unread bodies.

The [sample producer](../../crates/graphit-data/src/lib.rs) preserves logical
node addresses, member and edge ordinals, UIDs, response order, wire format and
sample defaults. Selecting a physical slice does not renumber public identities.

The [PROJECT live caller](rust_ast_live_graph.md#queries-exports-and-progressive-consumers)
composes this reader under its operation guard and existing limits. The borrowed
`ResolvedGraphNode` retains its actual reader; `neighborhood_graph_at` rejects an
anchor from another capture, even when table and row happen to coincide.

## Localized key and adjacency reads

`input_limits()` returns the exact `LocalGraphLimits` retained by the capture,
so consumers can use its real decoder input policy independently of query output
row/byte limits. It adds no policy defaults and performs no source I/O.

`resolve_query_node(label, key, row_batch_size)` resolves only the PK for native
query consumers. The caller supplies its official SDK vector capacity as the
positive batch size, clamped to captured `query_decoded_rows`. Actual work uses
captured `query_decoded_rows` and `max_pages`; STRING values use `max_chunk_bytes`,
while `max_name_bytes` bounds the label. Neighborhood matching-edge and property
limits do not participate. It returns the same reader-borrowed anchor and retains
the original physical/source guards.
The result is `Option<ResolvedGraphNode>`: an absent PK becomes `None` only after
the final guard. Duplicate keys, mismatched PK types, invalid data, work limits
and Original/physical failures retain their typed errors. It does not read Source.

`read_query_node(&anchor, columns, row_batch_size)` reads only the resolved row
and requested declared columns, always adding the real PK. It returns a
`GraphPropertyNode` with sorted typed properties and validates that the guard
belongs to this exact reader. An empty column list returns just the PK; an unknown
column is an explicit projection error. It reuses projected SDK reads and the
captured decoder policy without a repeated PK lookup, neighborhood traversal or
Source read. Unrequested column bodies stay unread; selected corruption fails.

`resolve_node(label, GraphAnchorKey, GraphNeighborhoodLimits)` projects the declared
STRING or INT64 primary key. Captured Parquet statistics prune shards and row
groups which cannot contain that exact value. Modern truncated min/max remain
conservative bounds, including for STRING keys longer than the SDK's default
64-byte statistics limit. Absent or legacy statistics cannot exclude a group;
modern inverted bounds fail explicitly. The
selected key must be present exactly once. No UID
prefix is trusted as a path, row or authority, and no persistent lookup dataset is
created. Missing statistics can require reading more key groups.

`neighborhood_graph_at(&anchor, request, limits)` reuses that reader-bound anchor
and checks its selected PK against the request. Relationship, other endpoint
label and direction are optional; three absent filters mean all kinds and Both
directions. The page size is 1..=1000. `GraphPropertyNode::primary_key()` borrows
the already projected `GraphProperty` with its real name and STRING/INT64 value.
`GraphPropertyEdge::position()` exposes the actual logical occurrence position
used by the cursor, so clients do not merge independent edges by a short tuple. Outgoing reads select two CSR
pointers and their adjacency interval. Incoming reads use declared reverse CSR,
then locate selected UIDs in their source's forward adjacency for the original
forward member/row and properties. Requests for the same source adjacency are
batched into one read, preserving each parallel occurrence. Reverse row ordinals never become forward IDs.
Declarations without reverse CSR use a projected exact-target lookup with the
same Parquet statistics, followed by selected pointer probes. Declared reverse
coverage and selected forward/reverse differences fail explicitly.

Complete properties project only page edge and endpoint rows. PK projections also
cover the necessary UID boundary groups for deterministic logical ordering;
matching raw coordinates remain bounded by the existing matching-edge input guard.
The retained response prefix is at most page size plus one. Each selected endpoint
key is checked for duplicates in its matching key groups. Every loaded projected
group keeps its decoder, EOF and page/count checks; unread bodies are not certified.
The cursor contains the logical request fingerprint and `GraphNeighborhoodPosition`:
UID, relationship, source label and typed PK, target label and typed PK, and a
zero-based occurrence ordinal within that complete logical group. All matching
occurrences are retained, including identical UIDs across or within members;
self-loops appear once per physical forward occurrence when traversing Both.
The ordinal is recalculated inside the current capture and never stores a
member/row from another reader. Changes to that same logical group may change
its occurrence assignment; continuation is not a retained snapshot of old pages.
There is no global generation. A later page resolves its anchor in its own current capture. The caller
binds that cursor to the admitted PROJECT/version or exact installed artifact,
without requiring unrelated FILEs or derived corpora to share a revision.

## Native query storage

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
