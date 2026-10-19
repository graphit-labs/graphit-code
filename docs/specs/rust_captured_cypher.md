---
references:
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

# Captured local Cypher

An original may have no deadline for a continuous operation. The native query
adapter maps `None` to Ladybug's documented zero timeout, which disables that
timer; it keeps finite positive durations above zero and rejects an expired
original. Its observer always waits for cancellation or completion, adding a
deadline timer only when one exists. Completion still finishes and joins the
native observer before connection/IO destruction. Scope and caller admission
checks remain required; this context grants no authority.

The captured-file/cancellation boundary, native query consumer and standard Cargo build are activated. All fifteen affected Linux tests pass, including genuine Parquet and an actual installed runtime-authenticated graph. SDK865d is qualified independently; its query-consumer symbol isolation is validated with Graphit’s separate ANTLR runtime linked in the same executable. This document does not certify HTTP, React or browser PROJECT query delivery.

ImportedGraph must retain its existing ReadAst admission and original snapshot, revalidating its installation and graph before and after a query. The operation owns a lazy in-memory Ladybug database, connection, captured IO and result. A native-owned observer uses the caller's existing Tokio Handle to wait for the original cancellation/deadline or operation completion. It interrupts the actual SDK and joins before connection/IO destruction. The caller keeps its runtime alive and runs the operation in its existing blocking job.

The query adapter receives explicit native buffer size and thread count. The service caller defaults to the SDK's automatic buffer sizing (`0`, approximately 80% of available system memory) and two threads, preserving explicit positive overrides; the official maxDBSize remains unchanged because its virtual address capacity is not RAM usage. Before mounting the external Icebug tables, the adapter disables the SDK's default internal hash indexes. Those empty indexes do not contain the Parquet keys; the SDK's columnar scan and filter preserve primary-key lookup. This avoids their allocation without increasing the buffer pool or retaining another database. Actual query text/parameters, response rows/bytes and nesting are bounded inputs/outputs; no memory ledger or reservation framework is introduced.

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

Native values must cross a fallible public SDK boundary with NULL/type/shape checks, exact numeric/temporal values and explicit unsupported-result errors. The Rust SDK iterator/Arrow panic conversion is not used. Query cells retain type information; native identities map to canonical captured generation/table/member/row data, which does not grant access.

Mount metadata and parameter input use the official SDK JsonWrapper/yyjson parser. Native results use small checked callbacks into Rust serde_json; the adapter does not assume an additional C++ JSON library or serialize an untyped fallback.

Canonical identities carry the same 32 generation bytes as the installed graph sample/neighborhood, with table/member indexes and decimal row text. No additional hex identity codec is needed by the caller. QueryOptions is one shared CXX structure with the existing five settings. The original context is checked again after native prepare and its remaining timeout refreshed before execute.

JSON parameters map ordinary integers to native INT64 and larger positive integers to UINT64 without precision loss, with native DOUBLE/bool/string/null, homogeneous lists and structs. Mixed list element types are rejected causally. The response byte guard counts actual serde-encoded bytes; it does not estimate heap usage.

## API and results

ImportedGraph::query receives the caller runtime Handle, Cypher text, a BTreeMap of JSON parameters and QueryOptions. It runs in the caller’s existing blocking job and returns owned QueryResult { columns, rows } or the existing ImportedGraphError chain. Runtime/local installed acquisition remains the existing API; a valid captured path or artifact identity cannot replace ReadAst admission.

Each cell contains its native type and value. Integer/decimal/temporal values use exact decimal text; bool and finite floats use native JSON values. Nonfinite floats use explicit NaN/Infinity text with the original native type. Lists/structs/maps/UNION and node/relationship/path contents retain typed children. Native entity IDs carry generation bytes and table/member plus row text. The node/relationship projection includes checked identities, labels and typed property values. Unsupported native types and malformed shapes fail explicitly. Native result row count is checked before projection, and serde-encoded response bytes/depth are checked while building cells. This bounds the response rather than estimating memory used by the engine.

## Qualification

The fifteen focused tests in crates/graphit-ast/src/icebug_query/tests.rs exercise the actual CXX SDK query path, canonical multipart/endpoints/parallel/self-loop IDs, typed parameters, NULL/heterogeneous UNION/precise UINT64 and INT128/temporal/nested/nonfinite values, captured absolute-path/effect/write denial, physical mutation, non-UTF8 parent, and independent work during cancellation/deadline with owned join. The installed fixture uses an actual RuntimeKeyOwner, ReadAst issuer, installation identity and NativeRegistry. Its schema.cypher is not executed. The test writer is the existing mature Parquet60 SDK with a required v1 marker; no production Arrow version is converted. Only Linux runtime behavior is qualified here. HTTP/React/browser PROJECT and remote query delivery remain separate required consumers.

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
observer boundary. The focused Linux run passed ten tests; its attempts and
prior compile failure remain under `.build/ast-option-deadline/`. Windows and
macOS are UNRUN for this extension.
