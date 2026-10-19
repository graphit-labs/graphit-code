# Local progressive AST search

AST uses eventual consistency between Source, the Icebug graph and the searchable
Lance corpus. Each component publishes its own processed data. A query does not
wait for a common project generation, a complete scan or finished embeddings.
There is no `LiveSearch` coordinator, cohort membership column or clone-pin file.
`EntitySearch` reads the selected corpus through one native Lance snapshot.

## Full and incremental publication

Source is stored once per file in `source.lance`. Lexical entities, eligible parser
projections and nullable embeddings share `corpus.lance`. Each project version
selects a native branch of those datasets. A new branch's inherited records and
compatible vectors are immediately queryable, including before its graph exists.
No per-row namespace marker hides inherited records while that version updates.

For each processed file, the foreground producer commits its searchable rows and
maintains their FTS indices before publishing graph changes. Parsing and inference
do not hold the corpus writer lock. A removed file deletes its corpus rows; a
changed file replaces only its rows. Graph publication is a separate operation:
a failure is explicit but does not roll back searchable content already committed
or make unrelated files unreadable. Schema and Cypher require the graph, Source
requires Source, and FTS/NN require the searchable corpus. A missing independent
component does not block an operation that does not use it.

The full scan admits files progressively. A newer watcher delta takes priority
between complete file publications; unfinished paths remain pending in the
existing scheduler. Deletions are reconciled only after a successful full
selection enumeration. Cancellation, a failed tail or a yielded scan must not
turn unvisited files into deletions. The same paths are used for incremental
updates, without rebuilding unrelated records or repeating their inference.

## Text and semantic search

Native code-tokenized FTS indexes cover `name` and `content`; native scalar indexes
support file and identity lookups. Append maintenance covers new fragments rather
than rebuilding old segments. Lance can also search uncovered fragments. Native
index commits retain CAS, cancellation and recovery behavior; an already complete
index does not require an empty replacement commit.

Background inference reads eligible projections already persisted in that same
corpus. It checkpoints vector batches progressively. An update applies only to
an extant `(uid, data_key)` whose current input fingerprint still matches the
computed input. Changed or deleted candidates are skipped individually. The
native write CAS prevents an intervening change from being overwritten. A saved
prefix survives an interrupted later batch, and the existing scheduler can resume
remaining projections; there is no second authoritative queue or vector corpus.

Compatible vectors survive file updates when identity, projection and model
metadata still agree. Model ID, revision, dimensions and projection recipe define
the vector space. A genuinely incompatible model uses normal vector-column
replacement and re-inference. Unrelated Source, graph or corpus commits never
make intact files' vectors ineligible solely because a counter advanced.

Semantic search ranks only compatible, non-NULL vectors already available in the
selected native snapshot. Hybrid search keeps lexical candidates without vectors;
FTS works without a model. Service errors remain explicit. Optional rerank uses
published projection text, lexical text or the candidate's identity metadata from
that same corpus; it does not require an exact Source extent. An absent candidate
is handled at its own position, preserving the remaining candidate/score mapping.
Malformed projection hashes remain integrity errors.

## Source and identities

Search results retain stable public identities, per-file Source associations and
coordinates. They do not expose `search_generation`, `graph_generation`,
`raw_generation`, `source_original_generation`, `nn_generation` or
`query_model_generation`. Request-specific native pagination and physical graph
IDs remain scoped to the actual dataset or graph operation that produced them.
Browser combines independent graph results through public identities.

Source operations validate the requested file and extent. An entity's historical
native Raw version and source hash prevent serving another revision's text.
Actual missing entities/files are reported for that selection; an unrelated file
update does not invalidate all search results. The documented disk fallback is a
Source operation, not a requirement for FTS, NN or rerank.

Byte ranges are half-open. Line ranges include the first and last occupied line;
a trailing LF/CRLF does not create an occupied empty line in an entity extent.
Unicode, source bytes and public identities retain their existing contracts.

## Snapshots, concurrency and authorization

A retained native Lance snapshot can finish a read while a new operation observes
later commits. Readers do not recapture latest heads to demand equality. Required
schema fields are selected by name; unused extra fields in native historical
snapshots do not participate in queries. New corpus writes contain no cohort
column. Historical snapshots and immutable artifacts are preserved.

A graph operation retains its actual immutable shard lifetime. Sample and
neighborhood subqueries share one captured graph so physical IDs are meaningful
within that operation. This does not coordinate the graph with Source, FTS or NN.
Writers retain serialization and native CAS where needed to prevent lost writes.

PROJECT/version and installed-artifact authorization, provider credentials,
branch identity, immutable file/artifact integrity, cancellation and deadlines
remain required. A URI, cursor, source hash or vector identity grants no access.
An unavailable component is not an excuse for silent local/provider fallback.

## Explicit export and publication

Export captures the graph, its file observations and original SCIP documents in
one short graph/SCIP capture. It releases the writer guard before Source/corpus
cloning, packaging or upload. Historical per-file Source is assembled through
native SDK reads and verified hashes; the available search corpus is captured
independently. Export does not reparse the project or run inference to manufacture
a common generation.

Local and authorized remote consumers read the exported components through their
native snapshots and immutable artifact manifest/digests. `source_bindings.json`,
`PublishedPins` and whole-project clone-generation equality checks are removed.
Publishing a new artifact is explicit; local incremental updates do not mutate an
already installed immutable Hub release.

## Focused verification

`live_search/tests.rs` covers progressive text/vector publication, incremental
replacement/deletion, compatible vector reuse, stale-inference rejection, native
write conflicts, retained snapshots, branch inheritance, model mismatches,
cancellation and pointwise missing rerank documents. App tests exercise the same
paths through authenticated CLI/MCP/HTTP and imported artifact consumers. These
checks distinguish a component being absent from authorization, integrity or
service failures; producer-only tests are not installed-product evidence.
