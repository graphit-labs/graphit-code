---
title: The wiki index came out empty because an UNWIND can't mix a row with a vector and a row without one
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [wiki, ladybug, embedding, bugfix, strace]
---

# What Was Left Over from `wiki-db-pre-migration-sqlite-file.md`

That task log closed three defects and left one open, stated like this:

> With the three fixed, `memory index` **reaches** `RebuildDB` with 152 chunks, it returns
> no error, and the store still comes out empty.

Both halves of that sentence were wrong, each for a different reason.

---

## State at the start

`graphit memory index --reset` over 152 memories, with the freshly installed binary:

```
› Cleared …/wiki/memory/project/01KSH1CRFFG8Z74B5ZS78WW808
✓ Memory index complete (0.2s)
```

152 pages written, 455 shards alongside, a `wiki.db` of **16,384 bytes**, no errors. Two
things give away the problem without naming it: 0.2s is far too little for 152 chunks plus
an FTS index plus a vector index, and 16 KB is less than the `user`-scope wiki, which has
**zero** memories and takes up 1.9 MB. A store with nothing in it was LARGER than the store
with 152 pages next to it — a sign that the 16 KB one never even got as far as receiving the
schema.

## How the defect stayed invisible

**`Rebuild` publishes via swap**: it writes a new store to `wiki.db.new` and renames it over
the live one. Whenever any step fails, `cleanup()` **deletes** the temporary file and the
old index stays exactly where it was. The only trace of a failure is the ABSENCE of a
change — which is indistinguishable from "there was nothing to do."

It was `strace` that named the defect before I knew what it was:

```
openat("…/wiki.db.new", O_RDWR|O_CREAT)      = 8     ← created
pwrite64(9<…/wiki.db.new.wal>, …)                    ← populated
pwrite64(8<…/wiki.db.new>, "…WikiSyn"…)              ← checkpoint entered the file
unlinkat("…/wiki.db.new", 0)                 = 0     ← DELETED
```

No `rename`. The path taken was the error path, and `Rebuild` was returning an error.

**Method, because it's worth more than this one bug:** when a pipeline with an atomic swap
"does nothing," trace the syscalls. Publishing and aborting look different on disk even when
they produce the same visible result.

## Why "returns no error" looked true

`internal/memory/wiki.go` was reporting the failure like this:

```go
slogutil.Resolve(nil).Error("memory wiki index build failed; …")
```

`slogutil.Resolve(nil)` returns `NOP()` — a `discardHandler` whose `Enabled` is `false`. An
earlier commit had replaced `_ = wiki.RebuildDB(...)` with a log line and **kept the
silence**: the error started being written and discarded. The previous session read "no
errors" from a log that could never have had one, and went looking for the defect somewhere
else.

`IndexMemories` — the funnel every `memory index` goes through — passes no logger. In other
words, the only caller that mattered was exactly the one reporting to nowhere.

## The error, once visible

```
rebuild wiki db: insert wiki chunks: failed to convert Go value to Lbug value:
failed to create LIST value with status: 1. please make sure all the values are of the same type
```

This is **the same defect as commit 1a8839c**, fixed in the AST search index on 08/17 and
never in the wiki. The driver builds a single LIST for the entire `$batch` parameter and
rejects mixed element types, so a batch with a row carrying `FLOAT[768]` next to a row with
`emb` nil dies whole.

**Mixed is the normal case, not the edge case.** A chunk only gets a vector if three things
hold at once — its content hash is in the embedding cache, the vector has the right
dimension, and the chunk has at least `wikiEmbedMinWords` (10) words. Every wiki whose
embedding is partial produces both kinds of rows, and that includes every wiki the embedder
hasn't finished yet, and every wiki with a short page.

Why the `user` wiki survived: zero memories, zero rows, nothing to mix. Why the other
project survived: 8 memories that all landed on the same side.

## The fix

**`internal/wiki/store.go` — `writeChunks` splits each batch into two homogeneous halves**
and uses two queries: `insertWikiChunkQuery` with `emb`, and `insertWikiChunkQueryNoVec`
without the property, leaving the column NULL — which is what the vector query already
ignores by construction.

Each half is skipped when empty. This isn't defensive coding: an UNWIND over an empty list
fails with "failed to create LIST value because the slice is empty," so a wiki with a vector
on every chunk — or on none — can't be handed the other query.

The two obvious workarounds had already been measured in 1a8839c and don't work: a typed nil
`[]float32` and `[]float32{}` both fail with the same "slice is empty."

**`internal/memory/wiki.go` — `errLogger`**: prefers the caller's logger and falls back to
`slog.Default()`, never to NOP. `slogutil.Resolve` is still correct for routine logging;
what it can't be is the reporting path that says the index doesn't exist.

## Measured result

| | before | after |
|---|---|---|
| project `wiki.db` | 16,384 B | 30,777,344 B |
| pages | 152 | 153 |
| `memory index --reset` time | 0.2 s | 2.7 s |

The 0.2s was the cost of writing the pages and failing; the 2.7s is the index actually being
built.

`graphit_memory_search` kept responding the whole time, via the BM25 fallback over the `.md`
files — which is exactly why nothing looked broken while every query relied on the whole
directory.

## Tests

`internal/wiki/chunk_partial_embedding_test.go`:

- `TestRebuildIndexesAWikiWhoseEmbeddingIsPartial` — five vector distributions over the same
  6 chunks (alternating, only the first, only the last, all, none). Checks both the chunk
  count **and** the vector count: partitioning the batch can't cost a chunk its embedding.
- `TestPartialEmbeddingSurvivesMoreThanOneBatch` — `wikiBatchRows + 7` chunks, because the
  partitioning happens per batch and the result can't depend on where the batch gets cut.

The fixture writes 40-word bodies on purpose: below `wikiEmbedMinWords`, no row gets a
vector, and the test would stop exercising the mix without failing.

`internal/git/hook_env_test.go` belongs to the other defect from this session — see
`docs/tasks/daemon-inherited-git-hook-environment.md`.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/wiki/store.go` | Modified | `writeChunks` partitions the batch; two insert queries |
| `internal/memory/wiki.go` | Modified | `errLogger`, so index failure stops being discarded |
| `internal/wiki/chunk_partial_embedding_test.go` | Created | Mixed-batch regression, including batch traversal |
| `docs/tasks/wiki-db-pre-migration-sqlite-file.md` | Modified | Closes the item left open there |

## Technical Debt

- [ ] **`BuildDBFromCache` returns 0 chunks over a memory-wiki directory with 455 shards.**
  Observed while probing this defect, not investigated: `LoadAllChunks()` finds nothing
  where `ExportShards` just wrote. Doesn't affect memory — nothing installs a memory wiki
  from shards — but it's the path through which a wiki published by the Hub gets mounted, so
  it's worth confirming whether the memory shard format and what `LoadAllChunks` expects are
  the same.
- [ ] **The partitioning goes away once the load migrates from UNWIND to COPY.** `COPY`
  doesn't have this limitation — measured in 1a8839c, it loads `FLOAT[768]` with NULL mixed
  in. Applies to both the AST and the wiki.
