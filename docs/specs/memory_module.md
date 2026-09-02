# Memory Module Specification

The Memory module stores durable preferences, corrections, decisions, and learned procedures for
AI agents. A LanceDB table is the source of truth. A separate LanceDB wiki is the query projection;
there is no raw Markdown store, shard cache, pull/publish mirror, or file migration path.

## Scopes and storage

Memory has two primary scopes:

- `project`: shared architectural facts, workflows, and team conventions, keyed by the project's
  ULID;
- `user`: personal preferences and workstation conventions, keyed by the user identity hash.

Named imported memory contexts use the same table schema. Every scope resolves to exactly one
authoritative table:

```text
local-only mode:
  ~/.graphit/memory-table/memory-<scope>-<id>/

S3 mode:
  s3://<bucket>/<hub.prefix>/memory/<scope>/<id>/

local query projection:
  ~/.graphit/wiki/memory/<scope>/<id>/index.lance/
```

Nothing is copied into a project's `.graphit` directory. See
[Storage Layout](../architecture/storage_layout.md).

## Optional S3 backend

Memory shares the Hub's S3 configuration; there is no memory-specific repository or bucket key.
When `hub.bucket` is configured, LanceDB opens the scope directly at its `s3://` URI. Otherwise the
same schema and operations run against the local table directory.

There is no synchronization phase between local files and S3:

- inserts and updates commit with `Upsert` on the record key;
- deletes call `DeleteByKey` on that key;
- independent records can be written concurrently;
- same-key concurrent updates are last-writer-wins after commit retry;
- a mutation has reached its store when the operation returns.

The scope-reference lock records which scopes this machine uses. Pruning a scope removes only its
local table and wiki; it does not delete the remote table another machine may still use.

## Record schema and revision chain

`MemoryRecord` carries all authored fields: identity, title, body, type, tags, importance, mandatory
status, timestamps, scope identity, revision links, content hash, and embedding. `important` and
`mandatory` are independent: important marks curated reference material; mandatory means the full
memory must be loaded unconditionally at session start. The embedding column travels with a remote
table and removes the need for a separate vector cache.

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

## Searchability and chain collapse

Every archived revision is compiled into the memory wiki and is searchable and readable through
`wiki_source`. Retrieval collapses multiple hits from one chain:

- if the current revision matched, older hits are omitted;
- if only an archived revision matched, it is returned as `superseded` with the current memory id;
- `top_k` is applied after collapse, so it counts distinct memories rather than rows.

Catalogue surfaces such as `memory_list`, `memory_important`, and `memory_mandatory` return live
records only. `memory_mandatory` reads the authoritative table directly and returns complete content;
it is not a ranked search. Chain metadata is stored as columns, so retrieval never opens a source
file to discover identity.

## Initial recall protocol

An agent starts memory recall with two ordered operations:

1. call `graphit_memory_mandatory` with no query and read every returned memory;
2. call `graphit_memory_search` for the current context with `exclude_mandatory: true`.

The exclusion prevents the contextual ranking window from repeating memories already loaded in full.
Agents mark system facts, current state, or standing instructions mandatory only when they must be
present in every session, and unmark them when that unconditional requirement ends. Importance alone
does not imply mandatory recall, and mandatory status does not imply importance.

## Memory content format

The public read surface renders a record as Markdown with YAML frontmatter. This is a presentation
format produced from the row, not persisted source data:

```yaml
---
id: "01ARZ3NDEKTSV4RRFFQ69G5FAV"
title: "Prefer table-driven Go tests"
type: "preference"
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

## How
Name every case and run it with `t.Run`.

## Impact
New tests follow one reviewable pattern.
```

The What/Why/How/Impact sections make an instruction actionable for an agent.

## Compiled memory wiki

`GenerateMemoryWikiFromTable` reads the authoritative table and synchronizes the local
`index.lance/` query projection. `FastPathCheck` compares the desired slug/content-hash projection
with the stored chunks. When a delta exists, `WikiDB.Sync` deletes missing slugs and upserts only new
or changed rows, preserving embeddings on untouched rows.

The compiled wiki contains the same four tables as a knowledge wiki: `chunks`, `xrefs`, `sync_log`,
and `meta`. It contains no generated page files, manifest, process cache, or shards.

## Write and consolidation cycle

A normal memory mutation is:

```text
write authoritative LanceDB row → synchronize the local wiki projection
```

Consolidation reads records, deduplicates or resolves them, applies table mutations, and recompiles
the same scope. A survivor inherits both importance and mandatory status independently; bare delete
suggestions cannot remove either an important or a mandatory memory. It never treats the query
projection as authored data.

## Daemon behavior

Memory writes synchronize their affected scope directly. The daemon no longer watches a raw-memory
filesystem tree and no longer runs a separate pull/publish loop. It does own periodic table
maintenance: one loop per active project scope and exactly one machine-wide loop for the user scope.
Each loop checks immediately and then every 15 minutes; the table's due-time gate decides whether to
fold indexes, compact fragments, and prune versions. Empty tables are skipped and failures reach the
supervisor. A remote table is already the shared source; a local-only table remains entirely local.
