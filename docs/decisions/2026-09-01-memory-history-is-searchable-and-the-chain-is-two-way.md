---
title: Memory history is searchable, and the revision chain is two-way
status: accepted
date: 2026-09-01
tags: [memory, wiki, search, history]
---

# Memory history is searchable, and the revision chain is two-way

## Status

Accepted. Implemented on 2026-09-01. Supersedes the reasoning recorded on `HistoryDirName`
in `internal/memory/identity.go`, which stated that an archived revision is "never compiled
into the wiki".

## Context

A memory update rewrites `<id>.md` in place and files the version being replaced under
`history/<id>/`. Until now that archive was deliberately unreachable: every listing in
`internal/memory` reads one directory level and skips subdirectories, so an archived
revision was never indexed, never compiled, and never returned by search. The only way to
reach one was to read the `previous` path out of a live memory's frontmatter — and since the
raw store is global and outside any workspace, an agent could not do even that.

That design bought one thing, and it is worth naming because it is the thing that had to be
preserved: **a search never returns two versions of the same belief.** A store that indexes
its own history invites the failure mode temporal-knowledge-graph systems exist to solve —
the superseded fact and the current one score similarly, nothing knows one replaced the
other, and the agent answers from whichever ranked higher.

What it cost was larger. Three questions had no answer at all:

- what did this memory say before the correction?
- when did the project change its mind, and what was the reasoning it replaced?
- a measurement, a constraint or a decision that no longer holds — where is it, when the
  current memory only says that it does not hold?

The last one is not hypothetical in this repository. Several memories carry a
`[SUPERSEDED <date>]` prefix in their own title with the old content pasted underneath,
which is an agent hand-rolling exactly this feature inside a single page because the
mechanism for it was unreachable.

A second problem forced the timing. 184 of 496 files in this project's memory scope were
twins forked under a corrupted id, and the divergent one among them held a rewrite that
existed nowhere else. Deleting it loses knowledge; keeping it keeps a duplicate. Neither is
acceptable, and the third option — file it as a superseded revision — only exists if
history is a real, reachable place.

## Decision

**History is compiled into the memory wiki and is searchable and readable like any other
memory. The property that made it safe to hide is preserved by deduplication instead of by
invisibility.**

Four parts:

1. **The chain is doubly linked.** `previous` already pointed backwards. A superseded
   revision now also carries `next` — the path of what replaced it, which is another archive
   or `<id>.md` when the successor is the live memory — plus `revision_id`, its own address
   inside the chain. A live memory carries neither, and that absence is its definition: it is
   the head. The `id` on an archive stays the CHAIN id, which is what makes the current
   revision nameable from any old one without walking anything.

2. **Archives are addressed by ULID.** `history/<id>/<ulid>.md` replaces
   `history/<id>/%04d.md`. A ULID is lexicographically time-ordered, so the counter's only
   real property survives, and it is collision-free, so two units archiving concurrently
   cannot overwrite each other — which a shared counter can, in a store whose conflict model
   is per-object last-writer-wins. Existing `%04d.md` archives are **not renamed**: a rename
   would invalidate the `previous` pointer of a live memory, and `"0001"` sorts before every
   ULID anyway.

3. **Search collapses a chain to one result.** When several hits belong to one chain, only
   the current revision is returned, with no reference to the older ones. `top_k` is applied
   after the collapse, so it means "distinct memories" rather than "index rows". When a hit
   is an archived revision whose current version did **not** match, it is returned annotated
   with `superseded` and `current`, and the agent decides what to read. This is the trade
   that replaces invisibility: the ranking never offers two versions of one belief, but an
   old version is reachable when it is the thing that answers the question.

4. **A memory's id comes from its frontmatter, never from its file name.** `MemoryIDFor` is
   the single resolver, and the file name is a fallback only for content that declares no id.

## Consequences

### Gained

- The history of a belief is a first-class, queryable artifact. "What did we think before,
  and why did it change" is answerable without a git log that no longer exists.
- The divergent forked twin was preserved rather than deleted, because there was somewhere
  correct to put it.
- Concurrent archiving is safe, where the counter could silently overwrite.
- `[SUPERSEDED …]` prefixes hand-written into memory titles are now redundant; the mechanism
  does it.

### Paid

- The wiki holds a page per revision, so a heavily edited store compiles more pages. The
  index page and `memory_list` still show only live memories, so the catalogue does not grow.
- Search does one small frontmatter read per hit to resolve the chain. Bounded by
  `top_k × 4`.
- Over-fetching is required, because collapsing after ranking would otherwise return fewer
  results than asked for.

### Risks accepted

- **An agent reads a superseded revision and treats it as current.** Mitigated in three
  places rather than one: the compiled page opens with a banner naming the current id, the
  search result carries `superseded` and `current` columns with an instruction line, and the
  memory rule has a trigger for it. Not eliminated.
- **Chain metadata is carried as generic columns on the wiki index** — `entity_id`,
  `revision_id`, `superseded`, `current_id`, with a bitmap index on `superseded`. It began as
  page frontmatter read per hit, on the reasoning that supersession might be memory-only; that
  was revisited the same day and decided the other way, because an ADR replaced by a later ADR
  is the same shape and the knowledge wiki will want it. The frontmatter copy remains as the
  fallback for the markdown-scan path, which has no columns to project.

## Alternatives considered

| Alternative | Why not |
|---|---|
| Keep history invisible | Refuses the requirement, and leaves a divergent forked twin with nowhere to go but deletion |
| Index history, no dedup | Reintroduces the failure this design exists to avoid: two versions of one belief competing on rank, with nothing saying one replaced the other |
| `next` points at the chain head instead of the immediate successor | Loses forward stepping. The head is already recoverable from an archive's `id`, so one extra small write per update buys strictly more information |
| Rename legacy `%04d.md` archives to ULIDs | Every rename invalidates a live memory's `previous`. Mixed naming already sorts correctly |
| Put supersession in the wiki DB schema | Correct eventually, but it changes a schema and two search paths shared with the knowledge wiki for a gain measured in a handful of small file reads |

## References

- Task log: `docs/tasks/memory-revision-chain-searchable-history.md`
- Implementation: `internal/memory/{identity.go,memory.go,wiki.go,search.go,repair.go,rule.go}`
