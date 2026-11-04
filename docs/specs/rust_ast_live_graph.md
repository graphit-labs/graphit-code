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
Reopening reads the catalogue without scanning FILE payloads or building private
lookup indexes. The first mutation builds those temporary indexes; node-range
and FILE-ownership validation remains explicit when their consuming reader or
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
are invisible to readers. Unchanged node shards retain their exact paths,
bytes and addresses. CSR members bind exact active source and target shard
ranges and carry local offsets. A changed endpoint rebuilds only incident
owners' CSR members; unrelated CSR files remain unchanged. Empty typed labels
and relationship pairs remain queryable through empty shards/members.

A FILE mutation writes its changed node, fact and adjacency shards, then
atomically replaces the manifest. That replacement is the point of effect.
Pre-effect errors keep the previous committed catalogue and roll back the
lookup transaction. Post-effect cancellation or cleanup failure reports the
applied structural revision and leaves the current owner and reopen aligned
with that committed manifest. A failed temporary lookup checkpoint marks the
index for reconstruction before its next use. Superseded immutable shard
directories are collected after the effect under the same writer guard.

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

Each synchronous query captures the committed Icebug manifest and mounts its
Parquet ranges through the same lazy reader used for local and S3 artifacts.
The native query database is in memory. `ast.query.buffer_pool_bytes` and
`ast.query.threads` configure that query execution; they are not constructor or
FILE-writer controls. Typed scalars, exact integers, nested values and the
existing result envelope remain unchanged. Graph identities use SHA-256 of the
logical namespace and little-endian structural revision. Samples and
neighborhoods retain the same generation-bound identities and request limits.

The existing readonly query checks reject writes, LOAD, procedure calls,
multiple statements and internal metadata names. Original cancellation and
deadlines interrupt native execution. Query limits bound the response, rather
than the corpus. The composing caller executes blocking native work inside its
live Tokio runtime and retains PROJECT admission.

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
