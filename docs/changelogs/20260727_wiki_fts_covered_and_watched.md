# Wiki FTS gains coverage, and the daemon actually watches the wiki

**Date:** 2026-07-27
**Scope:** `internal/wiki/fts_db_test.go`, `internal/daemon/syncmodule.go`,
`internal/fswatch/fswatch.go`, `internal/ignorer/ignorer.go`, daemon tests
**Origin:** two Engineer requests — fix wiki FTS without coverage, and then
"the daemon also needs to watch and sync the wiki just like ast"

---

## 1. Wiki FTS — from zero to 33 of 40 functions

`internal/wiki/fts.go` is 1494 lines with the entire wiki storage and retrieval layer, and
**nothing opened a `WikiDB`**. Tests with "search" in the name cover the AI search loop and pure helpers — trigrams, snippets, query-string building — none touches
SQLite.

The sharpest consequence was the build tag. The chunk index is an FTS5 virtual table,
so `go build` without `-tags fts5` produces a binary whose wiki fails to open the database. The suite
would pass green over that.

That's why new tests **fail instead of skipping** when FTS5 is unavailable, with a
message saying the binary was built without the tag. Skipping would restore exactly the blind spot they exist to close.

Covered: opening and creating the index, reopening preserving content, search round-trip
(body, title, summary, multi-word), accent handling — the corpus is Portuguese, and a
tokenizer that breaks diacritics would be silent damage —, rebuild that replaces instead of
accumulating, `CheckAllHashesMatch` in the three cases that decide whether a rebuild happens,
cross-references at depth 1 and 2, the four `Browse` filters, sync log round-trip, and hostile queries (`"`, `AND`, `NEAR(`, `"unbalanced`) that must return nothing
instead of error.

7 functions remain uncovered, all from the embeddings path — `SemanticSearch`,
`HybridSearch`, `PendingEmbeddings`, `InsertChunkVector`, `EmbeddingStats`,
`semanticSearchLocked` — plus `optimizeTables`. They need real vectors.

## 2. The daemon watched the wiki through `.astignore`

Engineer's question that exposed this: in code, are `.wikiignore` and `.astignore` each for its own?

**For what each indexes, yes.** `.astignore` is applied by `ast.NewAstIgnoreChecker`;
`.wikiignore` by `knowledge.NewKnowledgeIgnoreChecker`, inside the wiki pipeline
(`internal/knowledge/wiki.go:40`).

**For what triggers, no.** There is no wiki watcher. `fswatch.New` appears in three places
— `syncmodule` (project), `memorysyncmodule` (memory) and `ast/watcher` (CLI `ast watch`) —
and the project one was built with the AST checker, being the only event source for both
consumers.

Concrete case: `docs/` in `.astignore` is plausible configuration, since AST parses markdown
and you may not want that. With it, the watcher didn't even register watch on the directory. Editing
a document generated no event, wiki was never notified, and `.wikiignore` remained
perfectly correct about a pipeline that wasn't running.

### Why not a second watcher

It would be the obvious design, and it's wrong here: `knowledge.docs_dir` is `"."` by default, so a
wiki-owned watcher would duplicate the entire project tree — doubling inotify watch
consumption, which `fswatch` already treats as scarce enough to have a dedicated error
message.

### What was done

One watcher, permissive enough for both, with routing by checker:

- `fswatch.Config.Ignore` went from `*ignorer.IgnoreChecker` to interface `fswatch.Ignorer`
  (`IsIgnored` + `ShouldDescend`). The concrete type still satisfies it.
- `ignoreUnion` skips a path **only when all members skip**, so watching covers
  the union of what both want. Both exclude the brand directory by default, so the union
  still excludes and the daemon doesn't wake on its own writes.
- `classifyBatch` receives both checkers and each consumer applies its own to what arrives.

### The regression the interface caused, and the fix

Swapping a concrete pointer for an interface reopens the **typed nil** trap: a
nil `*ignorer.IgnoreChecker` stored in an interface makes `Ignore != nil` pass, and the method is
called with a nil receiver. `TestDetectsCreateAndModify` in `fswatch` blew up with SIGSEGV.

Fix landed in `ignorer`, not `fswatch`: `IsIgnored` and `ShouldDescend` now
treat a nil receiver as "ignore nothing". The caller cannot catch this — its nil
check doesn't see a typed nil — so the guard must live on the receiver.

## Tests

- `internal/wiki/fts_db_test.go` — ten tests, all opening a real database.
- `internal/daemon/syncmodule_wikiwatch_test.go` — docs excluded from AST still reaches wiki
  (the broken case), the mirror, path excluded by both reaches neither, and the
  decision table for `ignoreUnion`.

Routing tests are **hermetic**, using the project-query extension-registration feature from the previous commit: `stageProjectParsers` writes query files into
the temporary project's `.graphit/ast/queries`, so `.sql`, `.go` and `.md` are recognized without
installed runtime. Before they'd skip.

Full suite with `-race` clean, without `~/.graphit` present.
