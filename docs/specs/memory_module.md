# Memory Module Specification

The Memory module stores durable preferences, corrections, decisions, constraints, facts, and
learned procedures for AI agents. Each scope has one authoritative LanceDB table. That table holds
the authored records, revision history, full-text indexes, and embeddings; there is no raw Markdown
store, compiled memory wiki, sidecar query projection, shard cache, or pull/publish mirror.

## Scopes and storage

Memory has two primary scopes:

- `project`: shared architectural facts, workflows, and team conventions, keyed by the project's
  ULID;
- `user`: personal preferences and workstation conventions, keyed by the authenticated Hub user or
  by the reserved `anonymous` identity.

Named imported memory contexts use the same table schema. Every scope resolves to exactly one
authoritative table:

```text
local-only mode:
  ~/.graphit/memory/memory-<scope>-<id>/

S3 mode:
  project:            s3://<bucket>/<provider-prefix>/v2/projects/<project-ulid>/memory/
  authenticated user: s3://<bucket>/<provider-prefix>/v2/users/<user-id>/memory/
  anonymous user:     ~/.graphit/memory/memory-user-anonymous/
```

Nothing is copied into a project's `.graphit` directory or into `~/.graphit/wiki/`. See
[Storage Layout](../architecture/storage_layout.md).

## Optional S3 backend

Memory uses the active provider's storage; there is no memory-specific repository or bucket key.
For a Broker provider, project memory and authenticated user memory obtain independent in-memory
STS grants and topology for their respective fixed roots. When the active provider has an S3 bucket, LanceDB opens project and authenticated-user scopes directly at
their `s3://` URIs. Otherwise the same schema and operations run against the local table directory.

Project memory is authorized as part of the enclosing project ULID. User memory is authorized by
the trusted user subject. Without authentication, the scope ID is `anonymous` and its authoritative
table remains local regardless of S3 configuration. `unit.id` neither selects the user-memory scope
nor authenticates a remote user. Every remote operation follows
[Hub Access Control](hub_access_control.md), and cached Hub metadata cannot authorize a memory read
or mutation. Broker user-memory scope requires an S3 grant that applies to `*` or `global`; an
exact project grant cannot open the user root.

There is no synchronization phase between a local projection and S3:

- inserts and updates commit with `Upsert` on the record key;
- deletes call `DeleteByKey` on that key;
- independent records can be written concurrently;
- same-key concurrent updates are last-writer-wins after commit retry;
- a mutation is durable when the operation returns.

The scope-reference lock records which scopes this machine uses. Pruning a scope removes only its
local table; it does not delete a remote table another machine may still use.

## Record schema and revision chain

`MemoryRecord` carries identity, title, body, type, tags, importance, mandatory status, timestamps,
scope identity, revision links, content hash, and embedding. `important` and `mandatory` are
independent: important marks curated reference material; mandatory means the full memory must be
loaded unconditionally at session start. The embedding column travels with a remote table and needs
no separate vector cache.

A memory is one chain across all revisions, identified by a stable ULID:

- the live row key is `<id>`;
- an archived revision key is `<id>/<revision-id>`;
- `revision` is the write count, starting at 1;
- `previous` and `next` make the chain walkable;
- `revision_id` is empty on the live head and set on archived rows;
- `superseded` distinguishes history from the current belief;
- deletion keeps the final archived revision with no successor.

Updates create the archived row and replace the live row through table operations. There is no
history directory to scan or repair.

## Direct indexing, search, and reading

The authoritative table owns inverted-text indexes for `body`, `title`, `tags_json`, and `type`,
scalar indexes for identity and relevance fields, and a vector index once enough embedded rows
exist. Writes refresh the table's indexes directly. The daemon embeds records whose in-row vector is
missing and periodically folds, compacts, and prunes the same table.

`graphit_memory_search` queries this table directly. It searches current and archived rows and then
collapses multiple hits from one revision chain:

- if the current revision matched, older hits are omitted;
- if only an archived revision matched, it is returned as `superseded` with the current memory id;
- every result is classified by the current memory as `mandatory`, `important`, or `normal`;
- results are ordered by that category, in that exact sequence, and then by `updated_at` descending
  inside each category (`created_at` is the fallback for legacy rows);
- lexical or semantic score remains match metadata and never outranks category or date;
- `top_k` is applied after collapse and canonical ordering, so it counts distinct memories and
  cannot exclude a higher-priority memory in favor of a higher-scoring lower-priority one.

`graphit_memory_source` reads `<id>` or `<id>/<revision-id>` directly and renders the row as
Markdown. Memory is not a wiki scope: `graphit_wiki_*` tools operate only on Knowledge, while every
Memory search, browse, read, index, and mutation goes through `graphit_memory_*`.

Catalogue surfaces such as `memory_list`, `memory_important`, and `memory_mandatory` return live
records only. They use the same category-first, newest-first ordering. `memory_mandatory` is a
direct filter and returns complete content rather than scored results.

## Initial recall protocol

An agent starts memory recall with two ordered operations:

1. call `graphit_memory_mandatory` with no query and read every returned memory;
2. call `graphit_memory_search` for the current context with `exclude_mandatory: true`, then read
   selected ids with `graphit_memory_source`.

The exclusion prevents the contextual result window from repeating memories already loaded in
full. Importance alone does not imply mandatory recall, and mandatory status does not imply
importance.

## Presentation format

The public read surface renders a record as Markdown with YAML frontmatter. This is a presentation
format produced from table columns, not persisted source data:

```yaml
---
id: "01ARZ3NDEKTSV4RRFFQ69G5FAV"
title: "Prefer table-driven Go tests"
type: "convention"
tags: ["go", "testing"]
created_at: 2026-08-24T12:00:00Z
updated_at: 2026-08-24T12:00:00Z
important: true
mandatory: true
revision: 3
updated_by: "unit-id"
---

# Prefer table-driven Go tests

## What
Use table-driven cases for related Go scenarios.

## Why
The shared setup makes edge cases visible and keeps assertions consistent.
```

## Write and consolidation cycle

A normal mutation has one persistence step:

```text
write authoritative LanceDB row -> refresh that table's indexes
```

Consolidation reads records and embeddings from the table, deduplicates or resolves them, and
applies mutations back to the same table. A survivor inherits importance and mandatory status
independently; bare delete suggestions cannot remove either an important or mandatory memory.

## Memory Explorer

The Observatory exposes Memory at `/memory/explorer/<scope>/<memory-id>` through a dedicated
Memory component and the `/api/memories` HTTP API. It never adapts a memory row into a wiki page or
calls `/api/wiki`.

The catalogue searches the selected project or user table directly and filters current rows by
type, tag, importance, and mandatory status. A selected memory shows the current row, authoritative
metadata, and a navigable revision chain with current/superseded state, timestamps, author,
previous/next addresses, scope identity, and content hash. Create, edit, flag, and remove actions
call the Memory service; removal requires explicit confirmation.

The body is authored and edited as Markdown source. The detail view renders that source with GFM
headings, emphasis, links, lists, tables, quotes, code, and images using the Memory renderer. This
presentation support does not make Memory a wiki or create a second persistence format.

## Daemon behavior

The daemon owns one maintenance and embedding loop per active project scope and one machine-wide
pair for the user scope. Embedding checks run immediately and then every two minutes. Maintenance
checks run immediately and then every 15 minutes; the table's due-time gate decides whether to fold
indexes, compact fragments, build the vector index, and prune versions. Empty tables are skipped and
failures reach the supervisor. A remote table is already the shared source; a local-only table
remains entirely local.
