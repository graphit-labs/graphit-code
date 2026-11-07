---
title: "Local AST graph by FILE"
doc_type: technical
description: "Icebug Parquet FILE updates and lazy native queries without waiting for embeddings."
references:
  - target: {type: task, id: 01M5K5SCW5QHTDS20D9JAWXSK2}
    relation: implements
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: relates_to
---

# Local AST graph by FILE

`graphit_ast::live_graph::LiveGraph` persists the active local structural graph
as Icebug Parquet shards and a canonical `graph/icebug.json` manifest. Ladybug
runs only in memory to query captured Parquet ranges. Local publication does not
create `.lbug`, WAL or shadow files, load the whole graph into a mutable native
database, or re-export the whole corpus for each FILE edit. Installed and
exported graphs use the same node-shard and bounded CSR format described in the
[Icebug storage contract](rust_icebug_storage.md).

## Authority and opening

The composing daemon supplies the admitted PROJECT/version directory, the
original request and a stable namespace. `open_path_owned(path, original,
namespace, create)` performs no project discovery and grants no authorization.
Only an explicitly creating writer initializes a missing graph directory and
manifest. Readers require the retained directory and committed FILE catalogue;
opening rejects a missing or malformed catalogue or a different namespace.
Reopening decodes the FILE catalogue directly, without building and cloning a
separate JSON tree for the complete manifest. It does not scan FILE payloads or
build private lookup indexes. The first mutation builds those temporary indexes;
node-range and FILE-ownership validation remains explicit when their consuming reader or
mutation runs. The committed Parquet generation is not rewritten by opening.
Previous `.lbug` cohorts are not read or imported.

The registry retains project/version ownership and a writer mutex. Queries retain
only the actual graph shard lifetime for their own operation; Source, search and
embedding publication have no shared read fence.
The graph close notification follows destruction of the owner and temporary
indexes. There is no persisted Ladybug checkpoint on close. A later admitted
capture rechecks directory identity, namespace and the current manifest. Source,
lexical publication and model work retain their existing cancellation,
PROJECT/version and physical-directory checks.

## FILE publication

`LiveFileInput` supplies public nodes/edges, source SHA-256, historical Raw
version, typed SCIP/Oracle facts, optional official SCIP document bytes and an
optional normalized pending compiler payload. The producer validates those
associations before publication. Public nodes use FILE-sized Parquet shards;
nonpublic facts, intentions and payloads stay in separate Parquet records and
are never mounted as public graph tables. Public node properties, including
FILE resolution outcomes, remain on the node shards; resolved relationships and
their UIDs remain in the CSR members. Private fact records retain the parser and
compiler evidence needed for later resolution. Raw Source remains in its
existing source dataset.

Each label allocates monotonically increasing row ranges. Replacing a FILE
allocates fresh ranges and withdraws its old ranges from the manifest; holes
are invisible to readers. A FILE edit retains unrelated node shard paths,
bytes and addresses. Batched storage maintenance may rebind their physical
files and offsets while preserving those logical addresses and public keys.
The node `row_offset` is a physical coordinate distinct from `row_base`.
CSR members bind exact active source and target shard
ranges and carry local offsets. A changed endpoint rebuilds only incident
owners' CSR members; unrelated CSR files remain unchanged. Empty typed labels
and relationship pairs remain queryable through empty shards/members.

A FILE mutation writes its changed node, fact and adjacency shards, then
atomically replaces the manifest. That replacement is the point of effect.
Pre-effect errors keep the previous committed catalogue and roll back the
lookup transaction. Post-effect cancellation or cleanup failure reports the
applied structural revision and leaves the current owner and reopen aligned
with that committed manifest. A failed temporary lookup checkpoint marks the
index for reconstruction before its next use. The per-FILE path does not scan
the global shard directory or prune all unreferenced entries after publication.
Fan-out directories are created when a batch actually writes them.

Maintenance consolidates immutable node, fact and CSR records in batches and
publishes their new physical references with the same atomic manifest boundary.
Private FILE facts keep their path and `facts_row` coordinate in the committed
catalogue. Public node ranges, Source associations, FILE tags and CSR endpoints
retain their meaning. Shared payloads use complete Parquet row-group slices;
each member's pointer group remains local to its own edge slice. The
[storage contract](rust_icebug_storage.md#local-file-publications-and-the-shared-reader)
defines the offsets, sharing and validation rules.

Collection is separate from consolidation. `with_graph` holds the publication
guard through its callback; maintenance waits for that active operation to
release it. Collection retains the current catalogue, manifest, external facts
and all ancestor directories, including shared files still used by another
FILE. This guard is operation-scoped: cloning a captured graph out of the
callback does not retain the LiveGraph publication guard. Completed exported
bundles own their independent immutable files. A failed or interrupted maintenance operation must leave
the last committed manifest readable; reopening uses that manifest rather than
adopting uncommitted staging files. Source, FTS and embedding publication remain
independent of this structural storage work.

`maintain_shards(original)` publishes at most one batch of 128 FILE records.
The planner groups catalogue members by their physical facts file and keeps
existing groups together when selecting a batch. It does not divide the global
lexically ordered path list into fixed chunks: inserting an earlier path must
not repartition unrelated consolidated bundles. A physical input larger than
the batch size is processed in bounded portions of its own membership.

The returned `LiveGraphMaintenance` contains only:

| Field | Meaning |
| --- | --- |
| `revision` | Committed structural revision after this call. |
| `files` | Current active FILE count. |
| `consolidated_files` | FILE records consolidated by this batch, zero when no batch is needed. |
| `more` | Another consolidation batch is still needed. |
| `reclaimed_files` | Obsolete physical files removed by collection. |
| `reclaimed_directories` | Empty obsolete directories removed by collection. |
| `reclaimed_bytes` | Sum of logical lengths of the removed files. |

Intermediate calls with `more=true` leave residue in place and return zero
reclamation counters. When `more=false`, collection validates the shard tree
without following links, then removes obsolete entries under the same guard.
This is one collection walk after the final batch, not a walk after each FILE
or an extra before/after storage inventory for reporting. An already consolidated
graph can therefore report zero consolidated FILEs while collecting old staging
residue. Logical `reclaimed_bytes` does not measure allocated blocks freed or
deduplicate hardlinked inodes; operational disk measurements are separate.

Schema evolution retains observed scalar types and endpoint pairs. Conflicting
property types fail explicitly; older shards missing a newly observed nullable
column read as typed NULL. A replacement advances the FILE supersession tag
even when Source and Raw versions are unchanged. `enrich_file` requires the
exact current FILE tag, Source SHA and Raw version, advances structural revision
and preserves that FILE tag. Superseded enrichment returns `WouldBlock`.

## Affected compiler and edge resolution

The owner keeps the small FILE/schema catalogue in memory. Rebuildable SQLite
lookup tables live only in a temporary directory and index node addresses,
SCIP symbols, Oracle lookups and edge endpoint intentions. The first mutation
lazily rebuilds those indexes by streaming each retained FILE once, then reuses
them while the writer is alive. Readonly open, queries,
schema, FILE observations and selected-FILE validation never build the locator
index. This does not retain the graph corpus in RAM or persist a second graph
database. Mutation keeps only the current affected
FILE's graph properties and candidate facts while resolving it.

Changed symbols and Oracle lookup keys select affected reference-owner FILEs.
SCIP resolution retains document-local symbol scope, permitted target labels,
unique-target decisions, the existing `scip:` UID recipe and parser-edge proof.
References remain bound to their actual profile observation, language/grammar,
Source association and outcome coordinates. Oracle uses the existing typed
resolver and original observation evidence. Resolution changes outcomes and
derived intentions while preserving unrelated public FILE properties.
Unresolved intentions survive target deletion and resolve when the target
returns. Node and compiler facts from unrelated FILEs are not regenerated.

Pending compiler membership is small catalogue metadata. Its normalized payload
is stored in that FILE's fact Parquet. `pending_compiler_files` enumerates only
pending FILE observations; `pending_compiler_file` loads the selected payload
with its Source/tag fence. Successful current enrichment clears only its own
pending payload. A stale fence or failed publication preserves prior work.

Official SCIP document bytes are validated against their FILE path and optional
source text hash, then retained in its fact shard. External symbols are replaced
atomically per compiler family from an iterator, using bounded Parquet writes
and temporary disk uniqueness checks. Other families remain intact. Document
and external visitors stream one payload/batch at a time under the committed
read guard; cancellation and visitor failures remain explicit.

## Queries, exports and progressive consumers

Each synchronous Cypher query captures the committed Icebug manifest and mounts its
Parquet ranges through the same lazy reader used for local and S3 artifacts.
The native query database is in memory. `ast.query.buffer_pool_bytes` and
`ast.query.threads` configure that query execution; they are not constructor or
FILE-writer controls. Typed scalars, exact integers, nested values and request
limits remain unchanged. Data coordinates belong
to the captured result, without a derived graph-generation hash. A resolved anchor
borrows the real reader; logical keyset cursors do not require unrelated FILEs or
derived corpora to share a revision.

Local sampling validates captured metadata and footers, then reads only the row
groups and shards needed for seeds, selected edge members and endpoints. Each
group read is decoded completely in the requested projection, including EOF;
primary-key uniqueness is checked among selected nodes. Marker filtering affects
seeds only, so real endpoints remain eligible. The wire format, defaults, logical
IDs and UIDs retain their existing contracts. See
[local sample reads](rust_icebug_storage.md#local-sample-reads).

The PROJECT live sample calls
`CapturedLocalGraph::sample_graph_excluding_marked` with `is_file_observation`
inside the same `with_graph` callback, using the existing sample limits and
response-byte bound. `validate_namespace` checks the admitted owner without
producing a generation digest. Sampling and typed neighborhood reads consume the
captured Data tables directly; namespace and authorization remain caller
obligations. Cypher execution remains in the official Ladybug adapter.

Exact primary-key point queries within the native adapter's supported shapes
and physical mask bounds first resolve uniqueness through the captured Data
tables. The native scan opens only the containing logical range and
rowgroup; a confirmed absence schedules no node scan. Other query forms retain
the ordinary Ladybug scan. Physical binding validation is deduplicated per
operation and checked again before completion; no captured reader or lookup
hint survives that operation.

The existing readonly query checks reject writes, LOAD, procedure calls,
multiple statements and internal metadata names. Original cancellation and
deadlines interrupt native execution. Query limits bound the response, rather
than the corpus. The composing caller executes blocking native work inside its
live Tokio runtime and retains PROJECT admission.

The graph server binds its configured installation root to the captured
installation's native canonical root. Native aliases for the same directory
are accepted; a distinct root is rejected before serving graph operations.
This path comparison does not replace the retained authorization and physical
binding checks on each operation.

The owner holds its read guard until query completion and destruction of the
native captured IO. Export, candidate validation and compiler payload visits
hold the same guard. A FILE writer and shard collection therefore wait until
those readers release their files. `write_snapshot` streams one current FILE's
public nodes and resolved edges at a time into the ordinary `IcebugWriter`;
explicit export alone builds an immutable complete bundle. Candidate validation
compares requested entity keys, UID/data key and Source/FILE association with
the retained FILE shards. A changed FILE tag requests a retry; divergence under
the same tag reports a broken projection.

Cypher and FTS become available after the FILE's structural and lexical
publication, independently of embeddings. The native parser owner and the
asynchronous embedding task are scheduled separately. The job joins both on
completion, and a pending model RPC leaves the structural producer runnable.
Source, text/projections and nullable
vectors keep their existing datasets, pins and incremental publication/reuse
contracts. See [native publication](rust_native_publication.md#local-progressive-readers).
Neither producer source tests nor this format change establish which binary is
installed or claim a measured reduction in daemon RSS.

## SCIP identity promotion and existing vectors

A compiler may associate global SCIP symbols with a declaration after its syntax
is already searchable. `apply_scip_document` returns the identity changes for
those projections together with their captured Source association. Nodes,
relations, definitions and references use the promoted identity.

Before publishing the FILE, the caller applies that same mapping to prepared NN
rows and to its previous corpus rows. During enrichment this happens before the
lexical and projection union, so the syntax and semantic identity do not become
two searchable copies of one declaration. Only UID, primary key and the affected
input fingerprint change. Arrow buffers holding projection text, Source hashes,
vectors and model metadata remain shared.

The ordinary corpus encoder still reuses a vector only when its projection and
model are compatible. The current content fingerprint protects subsequent
embedding updates; a stale or conflicting mapping is an error. Promotion itself
does not invoke inference or change the embedding recipe, and it does not impose
a common generation on Source, graph, lexical search and embeddings.

When an existing corpus row has a different UID and no explicit identity map is
available, the encoder can reuse one previous occurrence of the same FILE with
identical Source hash, projection hash, extent, language, grammar, kind and data
key. It builds this temporary lookup only after an exact UID lookup misses, from
the FILE rows already loaded for publication. Ambiguous candidates supply no
vector. The previous fingerprint, document role, recipe and model must remain
valid; the new row receives its current identity and fingerprint. A different
Raw dataset revision alone does not require inferring unchanged content again.
No previous UID format is decoded, and no alias or additional dataset is kept.

If identity and occurrence lookup both miss, publication can still reuse a
previous vector for exactly the same model input within that FILE. It compares
the projection bytes and document role, requires a unique candidate and validates
its existing fingerprint, recipe and complete model/vector-space metadata. The
CodeDocument recipe supplies the same model prefix for both projections. A Source
edit or a moved extent therefore does not force inference for unchanged input.
Only the vector and its compatible model metadata are reused; identity, Source
association and fingerprint come from the newly processed row. Changed input,
incompatible metadata or ambiguous candidates remain pending for inference. This
publication fallback does not relax the stricter in-flight checkpoint below.

If inference finishes after promotion, the checkpoint first looks up its exact
UID/data-key pairs. For missing identities only, it scans the affected FILEs and
can attach a vector to a unique occurrence with the same Source hash, raw
observation, projection hash, language, grammar, kind, data key and full extent.
An operation-local hash locates candidates; exact Arrow values still establish
equality. Ambiguous, removed or changed occurrences are skipped. Only matching
rows from that inference batch are retained; there is no alias store or corpus
cache. The ordinary current fingerprint and native commit CAS still protect the
write, so this race does not resurrect deleted content or require another model
inference solely because its UID was promoted.
