---
references:
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
  - target: {type: task, id: tsk-24b6}
    relation: implements
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: { type: task, id: tsk-c9f0 }
    relation: implements
  - target: { type: task, id: tsk-865d }
    relation: requires
  - target: { type: task, id: tsk-3b54 }
    relation: supports
---

# Captured Cypher

An original may have no deadline for a continuous operation. The native query
adapter maps `None` to Ladybug's documented zero timeout, which disables that
timer; it keeps finite positive durations above zero and rejects an expired
original. Its observer always waits for cancellation or completion, adding a
deadline timer only when one exists. Completion still finishes and joins the
native observer before connection/IO destruction. Scope and caller admission
checks remain required; this context grants no authority.

The adapter uses the official Ladybug SDK over captured Parquet ranges and the standard Cargo build. Library qualification and productive HTTP/React/Broker delivery have separate checks; the installed SDK and ANTLR runtime remain independently owned producers.

ImportedGraph must retain its existing ReadAst admission and original snapshot, revalidating its installation and graph before and after a query. The graph owner may retain the official in-memory Ladybug catalogue; each operation owns a fresh connection, captured IO and result. A native-owned observer uses the caller's existing Tokio Handle to wait for the original cancellation/deadline or operation completion. It interrupts the actual SDK and joins before connection/IO destruction. The caller keeps its runtime alive and runs the operation in its existing blocking job.

The query adapter receives explicit native buffer size and thread count. The service caller defaults to the SDK's automatic buffer sizing (`0`, approximately 80% of available system memory) and two threads, preserving explicit positive overrides; the official maxDBSize remains unchanged because its virtual address capacity is not RAM usage. Before mounting the external Icebug tables, the adapter disables the SDK's default internal hash indexes. Those empty indexes do not contain the Parquet keys; the SDK's columnar scan and filter preserve primary-key lookup. This avoids their allocation without increasing the buffer pool or adding a separate key index. Actual query text/parameters, response rows/bytes and nesting are bounded inputs/outputs; no memory ledger or reservation framework is introduced.

Icebug node scans materialize only the properties demanded by the native plan,
including columns used by predicates. Parquet positions remain physical: a
column name maps to its catalog identifier and then to the demanded output;
skipped positions do not shift row or column identities. The reader skips
unrequested column buffers, prefetch and decoding, while the existing row-group
offset, semi-mask, NULL and cast paths remain active. A requested wide value
still consumes the configured native pool and can report its genuine allocation
error; a narrow projection does not decode unrelated JSON observation fields.

Native scans also materialize a batch of demanded values before evaluating a
predicate. The [producer's node row groups](rust_icebug_storage.md) therefore use
the real serialized spool size as well as row count when forming a batch. This
avoids combining hundreds of MiB of compressible observations just because they
fit within 1,024 rows. It neither raises the query pool nor removes its allocation
errors for an individually large requested value. Outcomes, row order and IDs are
unchanged; older publications require ordinary recomposition to acquire the layout.

The local captured filesystem fills the SDK's requested buffer from the existing
positional source stream. Parquet can request a column chunk larger than the
retained-range limit even when a filter selects one small result. Such reads use
the stream's existing block and total-byte limits, retaining Original, physical
binding and EOF validation without allocating another returned-range buffer.
Cancellation's `Interrupted` is terminal at this boundary; the adapter does not
use `Read::read_exact`, which would retry that error. Retained-range limits and
native pool/result limits remain unchanged. Remote ETag/version-pinned reads keep
their existing path.

Only metadata-derived known table definitions may be mounted. The adapter uses the SDK identifier quoting and admitted file handles, and never executes schema.cypher. The scoped FileSystem handles all paths so an unknown path cannot fall back to the host filesystem. Write/effect methods deny causally. The official parser must accept one query statement, and prepare must classify it read-only before execution.

Native values must cross a fallible public SDK boundary with NULL/type/shape checks, exact numeric/temporal values and explicit unsupported-result errors. The Rust SDK iterator/Arrow panic conversion is not used. Query cells retain type information; native identities map to captured table/member/row coordinates, which do not grant access.

Mount metadata and parameter input use the official SDK JsonWrapper/yyjson parser. Native results use small checked callbacks into Rust serde_json; the adapter does not assume an additional C++ JSON library or serialize an untyped fallback.

The canonical producer/decoder manifest also omits graph generation; its strict
shape and physical-reference contract are maintained in [captured Icebug
storage](rust_icebug_storage.md). Catalogue compatibility uses logical types and
native settings, never that removed publication field.

Native INTERNAL_ID values carry table/member indexes and decimal row text within the captured graph. They carry no Graph generation, replacement hash or token. Public entity keys and UIDs remain properties supplied by their producer; native coordinates grant no authority. QueryOptions is one shared CXX structure with the existing five settings. The original context is checked again after native prepare and its remaining timeout refreshed before execute.

JSON parameters map ordinary integers to native INT64 and larger positive integers to UINT64 without precision loss, with native DOUBLE/bool/string/null, homogeneous lists and structs. Mixed list element types are rejected causally. The response byte guard counts actual serde-encoded bytes; it does not estimate heap usage.

## Catalogue lifetime and operation binding

`icebug_query::QueryCatalog::new()` creates an empty, lazy execution slot with no
IO. The service's existing `GraphServer::Shared` owns one `Arc<QueryCatalog>`
across requests. Its Live branch uses the existing `LiveGraph::with_graph`
callback, retaining the publication guard only for that operation. This keeps
reuse independent of the registry's `Weak<LiveGraph>` lifetime and does not
retain LiveGraph, a selection, an admission or a capture after a request.

`execute` and `execute_remote` receive the newly admitted capture, Original,
live Tokio Handle, text, parameters and QueryOptions on every call. The caller
runs native work in its existing blocking job and rechecks admission before
delivery. The slot's Tokio mutex serializes table rebinding and execution; an
Original cancelled or expired while waiting leaves without waiting for the
current native operation. It never cancels that earlier operation. Each call
has its own Connection, read transaction, query timeout and interrupt observer.

The adapter compares typed `GraphLayout` descriptors: table order, labels,
primary keys, column types and effective relationship pairs/properties.
`GraphMount` sends current row extents, shard slices and CSR members directly
through the existing CXX bridge. Full schema JSON and metadata-derived CREATE
DDL are generated only when the logical layout or native settings change.
Compatible requests keep the **same official Ladybug Database instance** and
catalogue, including calls after the previous request's handles have dropped.
Native pool size and thread count are catalogue settings: the SDK creates its
QueryProcessor pool at Database construction. A real change to either creates
a replacement instance. Response row/byte/depth bounds remain per operation.

Before a compatible call runs, official `StorageManager::createTable` factories
replace external table adapters with the current capture's metadata, and
`clearCachedPlannerTableStats` removes previous physical statistics. Rebinding
is exclusive with earlier calls: old readers leave before new readers are
bound, even when two captures have identical relative file names. A partial
rebind error destroys the affected instance. No hash, generation token, global
snapshot registry or payload cache coordinates this state with other corpora.

The captured filesystem binds each call's IO by ClientContext and denies paths
outside that live binding. Node cursors/readers belong to scan state; explicit
SDK rowgroup initialization uses the morsel already assigned by the official
hook. CSR SDK tables and their initialization state belong to the operation, so
an error cannot retain a partially loaded indptr prefix for another request.
Observers join and source-mode states are released before IO is unbound.
Captured footer metadata and projected buffers are cleared at unbind; they do
not accumulate between requests. Native graph metadata contains no Original,
ClientContext, ObjectStore, STS credentials or reader owner.

Metadata-only counts still revalidate the physical files actually used. Local
validation uses the retained positional source boundary. Remote validation
uses a nonempty conditional range through the existing pinned object reader,
with the current Original, before and after metadata consumption. Body ranges
keep their existing conditional reads. This does not download the graph or
validate every corpus body for a point query.

The official 0.22.1 planner keeps scan plus filter for columnar primary keys.
The adapter reuses its existing public parser AST preflight to recognize a
mandatory single labeled node with a plain primary-key equality against a
literal or parameter. Projections and ordering must refer directly to that
same node or its properties. WITH, UNION, OR, OPTIONAL, paths, functions,
aggregations, SKIP and LIMIT remain ordinary official SDK execution. No query
text is rewritten and no predicate string is parsed.

For the recognized form, the declared primary-key type must match STRING or
INT64 exactly. A different operand type remains with the official binder and
scan. The existing Data `resolve_query_node` locates the key under the capture's
input policy, using the SDK's explicit vector capacity as decode batch size.
It does not use response row/byte limits as a corpus budget. Only Data's typed
missing-anchor outcome becomes absence after its final guards; duplicates,
physical changes, authorization, IO, cancellation and decode-limit errors stay
errors. The resolved ordinal remains scoped to that capture.

The native operation attaches the resolved row, or absence, to its ClientContext.
Each scan owns an official SemiMask for the point selection and intersects any
existing mask. Rowgroups outside the selection do not read their column bodies.
The mask uses shard-local coordinates, so a logical row base above 2^32 is not
truncated. A physical shard beyond the SDK's uint32 mask-range API stays with
the ordinary official scan. This does not reject the capture or add a row
admission limit. Logical row bases and projected shard offsets remain distinct. Original SDK
predicates and projections still determine the output. Cleanup removes the hint
with the connection's source/CSR state; no key index, row cache or coordinate
survives for a later operation.

With `test-support` enabled, `test_observer().counts()` reports successful native
Database constructions and completed CREATE DDL statements from the actual C++
path. `last_witness()` contains only weak references to the last operation's
reader and Original. It is not an authority, a product metric or a growing
history. Tests check release after dropping their legitimate owners; external
session/STS caches can legitimately retain a valid store. Without the feature,
these witnesses, counters and native callbacks are absent.

The retained instance also keeps official buffer-manager bookkeeping. In SDK
0.22.1, `EvictionQueue` eagerly allocates one atomic candidate per configured
4KiB pool page, with capacity rounded to its 64-entry batch. The candidate has
two 32-bit coordinates. This is separate from `VMRegion` virtual reservations
and from Parquet pages fetched by a query; a large automatic pool can therefore
have a resident cost even for a small graph. The adapter preserves the official
default and explicit caller overrides. It does not add a purge policy or change
the SDK allocator. An isolated observation must compare private/PSS memory
before the lazy slot, after releasing captures, idle, after a schema switch and
after slot destruction. Neither virtual capacity nor process RSS alone
establishes retained graph payload, and residual process memory after drop is
not by itself proof of a reader leak or of allocator retention.

Pinned upstream APIs:
[Connection](https://github.com/LadybugDB/ladybug/blob/v0.22.1/src/include/main/connection.h),
[Database configuration](https://github.com/LadybugDB/ladybug/blob/v0.22.1/src/main/database.cpp),
[StorageManager](https://github.com/LadybugDB/ladybug/blob/v0.22.1/src/include/storage/storage_manager.h)
[storage factories](https://github.com/LadybugDB/ladybug/blob/v0.22.1/src/storage/storage_manager.cpp),
[buffer manager](https://github.com/LadybugDB/ladybug/blob/v0.22.1/src/storage/buffer_manager/buffer_manager.cpp)
and [eviction queue](https://github.com/LadybugDB/ladybug/blob/v0.22.1/src/include/storage/buffer_manager/buffer_manager.h).

## API and results

ImportedGraph::query receives the caller runtime Handle, Cypher text, a BTreeMap of JSON parameters and QueryOptions. It runs in the caller’s existing blocking job and returns owned QueryResult { columns, rows } or the existing ImportedGraphError chain. Runtime/local installed acquisition remains the existing API; a valid captured path or artifact identity cannot replace ReadAst admission.

Each cell contains its native type and value. Integer/decimal/temporal values use exact decimal text; bool and finite floats use native JSON values. Nonfinite floats use explicit NaN/Infinity text with the original native type. Lists/structs/maps/UNION and node/relationship/path contents retain typed children. Native entity IDs carry table/member plus row text. The node/relationship projection includes checked identities, labels and typed property values. Unsupported native types and malformed shapes fail explicitly. Native result row count is checked before projection, and serde-encoded response bytes/depth are checked while building cells. This bounds the response rather than estimating memory used by the engine.

## Qualification

The focused tests in crates/graphit-ast/src/icebug_query/tests.rs exercise the actual CXX SDK query path, canonical multipart/endpoints/parallel/self-loop IDs, typed parameters, NULL/heterogeneous UNION/precise UINT64 and INT128/temporal/nested/nonfinite values, captured absolute-path/effect/write denial, physical mutation, non-UTF8 parent, and independent work during cancellation/deadline with owned join. The installed fixture uses an actual RuntimeKeyOwner, ReadAst issuer, installation identity and NativeRegistry. Its schema.cypher is not executed. The test writer is the existing mature Parquet60 SDK with a required v1 marker; no production Arrow version is converted. Only Linux runtime behavior is qualified here. HTTP/React/browser PROJECT and remote query delivery remain separate required consumers.

The retained-catalogue fixtures use fresh captures and Originals over compatible
layouts, including different row counts and payloads at identical relative
paths. They check the actual native Database/DDL counts, released weak reader
witnesses, response limits, expired/cancelled lock waiters, native cancellation,
multiple rowgroups and physical mutation. Primary-key cases cover STRING and
exact INT64 values, absence followed by another lookup, captured input limits,
ordinary SDK handling of excluded forms, empty shards and logical row addresses
above 2^32. The Live producer cases check the published values and coordinate
shape without relying on a graph-wide generation digest.

The isolated performance observation uses the existing sixteen-type, forty-field
fixture shape with real producer-written relationships. It alternates a new
catalogue per call and one retained catalogue for equivalent identity,
neighborhood and all-type count queries. Capture, native work and serialization
are recorded separately; responses and released Originals are asserted.
Observations with the SDK automatic pool record its effective configuration and
private/PSS memory through warm calls, idle, schema replacement and slot drop.
These are scoped observations, not universal latency or memory promises. Remote
GraphServer delivery and its current object-store pins remain consumer checks.

The projected-scan regression writes 32 real file observations with short path
and filter values plus a large unrequested metadata column. With the same
8MiB native pool, the old scan fails while the projected scan returns the
filtered path, preserves identities across scans and handles an identity-only
projection. Requesting the wide column still reports the native allocation
error. These library results do not qualify a running Broker query.

Two optional-deadline cases extend the original eight fixtures. A real service
query completes with no timeout, then a cancelled service query joins while a
sibling remains usable. Its cancellation after a short wait does not identify
the exact native phase interrupted. A private control unit verifies the SDK
zero-timeout value, finite/expired controls and completion/cancellation at the
observer boundary. The optional-deadline fixtures exercise that library boundary. Windows and
macOS runtime qualification remains with their native distribution checks.
