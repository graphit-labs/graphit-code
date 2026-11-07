---
references:
  - target: {type: task, id: tsk-5651}
    relation: implements
  - target: {type: task, id: tsk-868f}
    relation: implements
  - target: {type: task, id: tsk-c9f0}
    relation: derived_from
  - target: {type: task, id: tsk-3b54}
    relation: implements
---

# Installed native AST graph reads

Task `tsk-868f` connects the existing authenticated installation registry to local
Icebug schema, sample and neighborhood reads. Its native routes are composed into
the existing Source service, with the same `DaemonAuth` and service stop token.
Captured Cypher has its native SDK producer in `tsk-c9f0` and runtime/local HTTP
consumer in `tsk-3b54`. The installed RemoteNative dispatcher in `tsk-5651` now
adds qualified Linux Broker Web schema/sample/Cypher/typed-neighborhood reads
and remote AST search. React Explorer/query-editor and browser PROJECT
admission remain separate from this installed global read qualification.

The actual local package stores its payload root in `SourceRef.directory()`.
`graph.icebug/` and its regular `schema.cypher` are required; optional
`source.lance` and `search.lance` are independent indexes. Package export/import
uses the current native ZIP envelope, removed during installation. Local graph
reads do not require the remote `native.json` manifest and do not execute schema
file text as trusted DDL.

`open_imported_graph_runtime` and `open_imported_graph_local` accept
`ImportedGraphInput`: the existing registry, exact `ArtifactId` and
`ConcreteVersion`, captured installation scope, snapshot/read/graph/directory
policies and the original `RequestContext`. Authentication and `Read(Ast)` precede
registry or payload IO. The registry must belong to that actual installation;
the trusted Rust composer supplies the issuer's own installation, matching the
existing imported Source precondition. `GraphServer` obtains this capture from
its same `DaemonAuth`; incoming clients cannot provide a scope or registry.
selection requires `GlobalNative`, backend `Native`, payload `Original` and the
same concrete version. There is no latest-version, Git, inventory, path or
metadata fallback. IDs and query fingerprints are data, never grants.

The returned `ImportedGraph` is private and request scoped. Its checked
`revalidate`, `schema`, `sample_graph` and `neighborhood_graph` methods reuse the
existing Data SDK. Installation/root/directory/schema and captured-content
validation surround work and final delivery. Missing installation, denied Auth,
invalid origin, changed files, corrupt Parquet/CSR and cancellation/deadline
retain their causal errors. The schema borrows validated manifest and footer
metadata, including all declared labels/PKs/columns/endpoints/counts, independently
of the sample. Readers and their original contexts are released with the operation.

`GraphServer` exposes four POST routes:

- `/api/graphs/query`: `{ "id": "artifact-id", "version": "concrete-version", "query": "MATCH (n) RETURN n", "parameters": {} }`. Parameters are optional, typed JSON values; unknown request fields are rejected.
- `/api/graphs/schema` and `/api/graphs/sample`: `{ "id": "artifact-id", "version": "concrete-version" }`.
- `/api/graphs/neighborhood`: the same selection plus `anchor_label`, typed
  `anchor_key`, `limit` (1–1000) and optional opaque `cursor`. The optional
  `relationship`, `other_label` and `direction` (`incoming` or `outgoing`) fields
  filter the page. Omitting them requests both directions and all relationship
  families in one page, without fetching schema or making a request per family.
  The CLI and MCP expose the same optional filters.

Native callers provide exactly one `x-graphit-auth-origin: runtime|local` and one
`Authorization: Bearer ...` header. A copied bearer is retained only in
`Zeroizing`; failures expose fixed public categories, never credentials or local
paths. Request bodies and emitted JSON obey the existing actual content byte
boundaries. No browser receives the runtime secret.

Schema responses have `nodes` and forward `relationships`. Samples have node
summaries and distinct relationship UID links. Node IDs contain SDK table and row;
edge IDs contain SDK member and row. These offsets belong only to their operation's
capture; consumers combine pages using public primary keys and relationship UIDs.
There is no Graph generation field or comparison across responses.
Row counts, row IDs, INT64 properties,
line numbers and integer anchor values use decimal strings for lossless JavaScript
transport. Integer anchors are explicit `{ "type": "int64", "value": "9007199254740993" }`;
string anchors use `{ "type": "string", "value": "main.rs" }`. Property values
carry `string`, `int64`, `bool` or `null` tags. The sample uses 300 seed nodes and
1000 global edges, with the existing decoder/string guards; it is never presented
as the whole repository. Numeric PKs absent from summaries must be acquired from
real typed node/property data in the future UI integration, not inferred from
names or renderer row IDs.

Neighborhood responses contain `anchor`, typed property `nodes`/`edges` and
`next_cursor`. Each node declares its observed `primary_key` as
`{name,value:{type:"string"|"int64",value:string}}`, without another schema query.
Each relationship exposes `position` with UID, kind, source/target labels and
those endpoints' typed keys, plus `occurrence` as lossless decimal text. A complete page is delivered immediately; a nonnull cursor means
more results can be requested. The local reader resolves the anchor primary key
once and passes its borrowed capture guard to `neighborhood_graph_at`.

The opaque cursor contains a logical query fingerprint and continuation within a
complete relationship group: UID, relationship kind, source and target labels with
their typed primary keys, plus the ordinal of indistinguishable occurrences.
Physical member and row offsets are not durable continuation keys.
App binds the fingerprint to PROJECT/version or installation/artifact/version
and the typed anchor/filters; Data validates the query parameters and UID keyset.
No revision, file set, manifest digest or physical offset enters that fingerprint.
A FILE update does not invalidate continuation. Each request acquires its own
current capture; continuation does not promise one snapshot across requests, and
an explicit refresh starts traversal again when changed data must be revisited.
Parallel relationships remain distinct, including repeated UIDs. A cursor cannot
restore a capture or grant access. Reverse storage does not duplicate schema groups or links. Existing input
and output bounds fail explicitly instead of returning a silent partial page.

Each operation creates one finite original context and an owned blocking job.
Dropping its handler cancels that operation; sibling requests remain independent.
Hyper does not promise immediate handler cancellation on TCP EOF, so this component
does not impose that expectation. Service stop/deadline close admission or cancel
delivery. Shutdown keeps the `JoinSet` in its owner while joining native work;
cancelling a shutdown waiter cannot detach it. The composed Source service joins
GraphServer before dropping the daemon's runtime/Auth owners, preserving existing
Ask, administrative, native-install, managed-role and additional routes.

Source: [imported_graph.rs](../../crates/graphit-ast/src/imported_graph.rs),
[graph_server.rs](../../crates/graphit-app/src/graph_server.rs) and narrow
[source_server.rs](../../crates/graphit-app/src/source_server.rs) composition.
Native fixtures: [graph_server.rs](../../crates/graphit-app/tests/support/graph_server.rs).
Schema SDK: [rust_icebug_schema.md](rust_icebug_schema.md).
Earlier installed-graph qualification, preceding the current cursor and catalog
changes: eight distinct native fixtures PASS (initial seven
plus one origin/integrity negative), one reader-privacy compile-fail doctest PASS,
strict AST/App lib/tests with --no-deps and -D warnings PASS0, scoped format/diff
and twelve Source/document agreements PASS. Receipts are saved under
`.build/rust-local-imported-graph-868f/`. Initial ENOSPC, duplicate-fixture and
foreign-lint/environment failures remain recorded; they are not counted as PASS.

## Captured Cypher HTTP query

The query route uses the existing owned blocking operation and the GraphServer's
single shared QueryCatalog. The slot retains only the native Database, structural
DDL descriptors and real settings. It serializes table rebind/execution under the
current Original's cancellation/deadline, creates a connection per operation and
binds the current capture's official readers. It retains no authorization,
Original, graph capture, ObjectStore or STS credentials between operations. Equal
structure can reuse the catalog across captures; changed files do not require a
global graph token. PROJECT queries use this same service-owned slot, so reuse
survives release of an operation's LiveGraph handle. Ladybug parses and prepares a single read-only query; the captured
filesystem denies external and write effects. Every member of a relationship
group remains independently mounted. The caller's installation, ReadAst,
concrete artifact version and original request are validated before IO and again
before delivery. Native syntax/value/effect errors remain failures with their
cause; credential failures retain the fixed private Auth category.

The JSON result has `columns` and `rows`. Each cell contains its native
`type` and `value`: exact integers and temporal units are decimal strings,
NULL is JSON null, and nested structures preserve their typed children. NODE and
REL cells include Data offsets belonging to the current capture; REL endpoints
and recursive path nodes/relationships retain those offsets. See
[rust_captured_cypher.md](rust_captured_cypher.md) for the SDK result contract.

Foreground captures `ast.query.buffer_pool_bytes` (default `0`, delegating to
Ladybug's native sizing of approximately 80% of available system memory) and
`ast.query.threads` (default 2, positive) from its existing ConfigSnapshot once at
startup. An explicit positive pool size in bytes must be at least 4096. Query rows, encoded bytes and nesting reuse ContentLimits
collection/output/depth bounds. The actual Ladybug Database opens lazily and stays
in the service-owned slot across compatible queries; caching serialized schema
while constructing another Database does not satisfy this contract. Each operation
owns its connection, and its cancellation observer and native query join before
its current readers drop. The SDK
maxDBSize default remains unchanged. These controls configure actual SDK cache
and parallelism; they do not reserve global memory or establish an RSS guarantee.
Retention qualification measures private memory/PSS after complete queries and
project switches using the effective buffer configuration. It distinguishes live
readers from SDK buffers and allocator retention; released reader/Original Weak
witnesses alone do not establish a reduction in resident memory. The service keeps
one catalog slot rather than adding instances per request or project.

Query validation or syntax errors return 400; cancellation/deadline returns 504.
Missing or repeated credential/origin headers return 401; an invalid credential
or denied ReadAst returns 403; a missing installed version returns 404. An invalid
or changed captured origin returns 409. A native error remains an error rather
than an empty successful table. Use the exact installed artifact/version and
matching runtime/local credential to retry; the server remains usable after a
rejected query.
