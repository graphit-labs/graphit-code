# Memory Module Specification

The Memory module stores durable preferences, corrections, decisions, and learned
procedures for AI agents. Raw Markdown is the source of truth; a compiled wiki is
the query surface. Optional S3 publication shares scopes across machines without a
Git repository.

## Scopes and local storage

Memory has two primary scopes:

- `project`: shared architectural facts, workflows, and team conventions, keyed
  by the project's ULID.
- `user`: personal preferences and workstation conventions, keyed by the user
  identity hash.

Imported memory contexts use the same mechanism with a context name. All stores
live once in the global brand directory:

```text
~/.graphit/
├── memory-raw/memory-project-<project-id>/   raw Markdown, source of truth
├── memory-raw/memory-user-<user-id>/         raw Markdown, source of truth
└── wiki/memory/<scope>/<id>/                 compiled searchable wiki
```

Nothing is copied into a project's `.graphit` directory. See
[Storage Layout](../architecture/storage_layout.md).

## Optional S3 backend

Memory shares the Hub's S3 configuration; there is no `memory.repo` or separate
memory bucket key. With `hub.bucket` configured, a scope maps directly to:

```text
s3://<bucket>/<hub.prefix>/memory/<scope>/<id>/
```

A complete explicit credential pair from global Graphit config is passed to the
S3 client. When either explicit value is absent, the AWS SDK provider chain is
used. With no bucket, every memory operation remains available locally and no
objects are uploaded. See
[S3 Credentials and UI Network Configuration](../guides/s3-and-ui-network.md).

### Pull, publish, conflict, and deletion semantics

- `Pull` merges remote objects into the raw directory. It does not mirror-delete
  local files, because an unpublished local memory must survive synchronization.
- `Publish` uploads the raw scope in the background. The CLI and daemon wait for
  pending uploads on shutdown so the last write is not abandoned.
- `RemoveFile` records a local deletion; the next publish removes the matching
  remote object.
- Independent memory IDs do not conflict. Concurrent edits to the same object are
  last-writer-wins.
- Pruning a local scope reclaims local disk and deregisters it; it deliberately
  does not delete the remote prefix another machine may still use.

Object storage has no commit message. Audit history lives in the memory itself:
`revision`, `previous`, `next`, `revision_id` and `updated_by` frontmatter identify
the chain, and superseded bodies are archived under the memory history path.

## Revision chain

A memory is one chain across all of its revisions, addressed by a single ULID that
never changes. An update rewrites `<id>.md` in place and files the replaced version
under `history/<id>/<revision-ulid>.md`.

| Field | Present on | Meaning |
|---|---|---|
| `previous` | any revision after the first | path of the version this one replaced |
| `next` | superseded revisions only | path of the version that replaced this one; `<id>.md` when that is the live memory |
| `revision` | all | write count, starting at 1 |
| `revision_id` | superseded revisions only | the revision's own address inside the chain |
| `updated_by` | all | the unit that made the last write |

A live memory carries no `next` and no `revision_id`; that absence identifies it as
the head of its chain. An archived revision keeps the chain's `id`, so the current
version is nameable from any old one without traversal.

Archive file names are ULIDs: lexicographically time-ordered, so a listing sorts
chronologically, and collision-free, so concurrent archiving in a
last-writer-wins store cannot overwrite. Archives written before this used a
zero-padded counter and are not renamed — a rename would invalidate a live memory's
`previous` — and `0001` sorts before every ULID, so a mixed directory still orders
correctly.

Deletion archives the final revision with an empty `next`, because nothing replaced
it. A deleted memory's chain is therefore found by listing `history/<id>/` rather
than by following a pointer, since the memory that would have carried the pointer is
the one removed.

### Searchability and chain collapse

Every archived revision is compiled into the memory wiki and is searchable and
readable through `wiki_source` exactly like a live memory. Two rules keep that from
putting two versions of one belief in front of a reader:

- when several hits belong to one chain, only the current revision is returned, and
  the superseded ones are not referenced. `top_k` is applied after the collapse, so
  it counts distinct memories rather than index rows
- when a hit is an archived revision whose current version did not match, it is
  returned annotated with `superseded` and the `current` memory id

The catalogue surfaces — the wiki index page, `memory_list` and
`memory_important` — list live memories only. A superseded revision is not part of
what the project currently knows; it is reachable by search and by slug.

Chain metadata is carried in the compiled page's frontmatter (`superseded`,
`current`, `revision_id`) rather than in the wiki database, so resolving a hit's
chain costs one small read per hit and the database schema stays shared with the
knowledge wiki unchanged.

## Identity integrity

A memory's id is the `id` its frontmatter declares. A file name is a fallback only
for content that declares none, and `MemoryIDFor` is the single resolver every read
site uses.

This is an invariant with history: a write path that recovered the id from the file
name turned `<ulid>_important_.md` — a fossil of a layout where the name carried the
importance flag — into a memory whose id was `<ulid>_important_`, forking the memory
into a twin that then accumulated its own revisions and its own history directory.
Two guards prevent recurrence: the wiki rejects a source file whose name is a memory
id with anything appended, and the compile step keeps one document per declared id.

`RepairForkedMemories` folds existing twins back into their chain and runs on every
index, because the twins live in the shared bucket and every clone that pulls them
must heal itself. Per twin: promote it when the chain has no live memory, remove it
when its body already exists in the live memory or in any archived revision, and
otherwise archive it into the chain as a superseded revision before removing it.
Removal goes through `RemoveFile` so the remote object is deleted — a local unlink
would be undone by the next merging `Pull`.

## Memory card structure

Each card is a Markdown file with YAML frontmatter and a structured body:

```yaml
---
title: "Prefer table-driven Go tests"
type: "preference"
tags: ["go", "testing"]
created_at: 2026-08-24T12:00:00Z
updated_at: 2026-08-24T12:00:00Z
important: true
revision: 3
updated_by: "unit-id"
previous: "history/01ARZ3NDEKTSV4RRFFQ69G5FAV/01M1F9XQ2K8ZR0P4TDAWER5W9P.md"
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

The What/Why/How/Impact sections make the instruction actionable for an agent.

## Shards and compiled wiki

After a successful compile, content-addressed chunk and embedding shards are
mirrored beside the raw scope under `.wiki/shards/` and published with it. Another
machine can reuse those vectors instead of embedding unchanged text again.

The compiled pages and LanceDB index remain local and are rebuilt from raw
Markdown plus valid shards. Imports are additive; a shard is used only when its
source hash matches. The generated search database is never the source of truth.

## Write and consolidation cycle

A normal memory mutation follows this chain:

```text
pull/merge remote → write raw Markdown → compile wiki → mirror shards → publish in background
```

The consolidation cycle can deduplicate related cards, resolve superseded
instructions, promote durable facts, and maintain history. It changes raw cards
first and recompiles the same scope; it never edits the compiled index as primary
data.

## Daemon synchronization

The global memory sync module discovers active scopes from the global memory lock
and watches the `memory-raw/` root. It debounces filesystem changes, recompiles only
the touched scopes, and handles lost-event rescans by recompiling all registered
scopes. The module runs once per daemon because memory storage is global, not once
per project.

Remote failures do not erase the local raw source. They are logged and retried by
later synchronization, while an unconfigured bucket stays intentionally local-only.
