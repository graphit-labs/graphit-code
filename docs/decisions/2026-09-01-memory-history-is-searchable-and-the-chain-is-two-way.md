---
title: Memory history is searchable, and the revision chain is two-way
status: accepted
date: 2026-09-01
updated: 2026-09-02
tags: [memory, wiki, search, history, lancedb]
---

# Memory history is searchable, and the revision chain is two-way

## Status

Accepted. Implemented on 2026-09-01 and revised on 2026-09-02 after LanceDB became the only
authoritative memory store. The functional decision remains; its original Markdown-directory
implementation does not.

## Context

A memory can change without making the belief it replaced useless. We need to answer both “what is
current?” and “what did this say before the correction?” while preventing old and current revisions
from competing as independent search results.

The original implementation archived Markdown files under `history/<id>/`. That representation was
removed when memory writes moved directly to scoped Lance tables. Existing data is development test
data, so no file migration, legacy archive reader, or compatibility naming scheme is retained.

## Decision

History is stored in the same authoritative Lance memory table as current revisions. A revision is
another row, not a file or a second store.

Each row carries:

- `entity_id`: stable identity of the memory chain;
- `revision_id`: unique identity of this revision;
- `previous` and `next`: immediate neighbors in the chain;
- `superseded`: whether this row is no longer current;
- `current_id`: the current revision for a superseded row;
- `revision`: its sequence within the chain.

Updating a memory writes the archived form of the previous row and the new current row directly to
the table. There is no raw Markdown write, Git publication step, shard synchronization, or later
migration into LanceDB.

Search collapses hits by `entity_id`. A current match is returned once. When only an older revision
matches, the result is marked `superseded` and names `current_id`, allowing the caller to inspect
history without mistaking it for the active belief. `top_k` is applied to distinct memory chains.

The local memory wiki is a derived Lance projection used for retrieval. It preserves the chain
columns and exposes page bodies and metadata from the index; it does not materialize Markdown pages.

## Consequences

### Gained

- Current state and belief history are both queryable.
- Search does not rank two revisions of one belief as unrelated facts.
- The authoritative table expresses the complete chain without a parallel filesystem hierarchy.
- Direct table writes work for local and S3-backed scopes through the same model.

### Paid

- Every revision remains a stored row and contributes to table size.
- Search must over-fetch before collapsing results by chain.
- Callers must respect `superseded` and `current_id` when an old revision is the matching row.

### Risks accepted

- A caller can still treat a superseded result as current if it ignores the explicit metadata and
  instruction text. The search surface and memory protocol both identify that state.
- The chain metadata is part of the generic wiki chunk schema because decisions and other knowledge
  can also be superseded.

## Alternatives considered

| Alternative | Why not |
|---|---|
| Keep history invisible | Prevents answering how a belief changed |
| Return every matching revision | Lets obsolete and current beliefs compete on rank |
| Keep Markdown archives beside the table | Restores a second source of truth and synchronization problem |
| Migrate the removed archive layout | Development data is disposable and compatibility is not required |

## References

- Task log: `docs/tasks/memory-revision-chain-searchable-history.md`
- LanceDB-only task: `docs/tasks/lancedb-is-the-only-store-for-knowledge-and-memory.md`
- Implementation: `internal/memory/{memory.go,table.go,wiki.go,search.go}`
