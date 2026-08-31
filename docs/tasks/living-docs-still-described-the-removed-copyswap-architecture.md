---
title: Living docs still described the removed copy+swap architecture
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [docs, ast, architecture]
---

# Living docs still described the removed copy+swap architecture

## Objective

After two performance changes to `exportDirectWithReverse`
([relationship export parallelization](relationship-export-passes-now-run-concurrently.md)),
the engineer asked for every documentation surface to be brought current, explicitly
including user-facing architecture docs — not just this session's own task logs.

## Reasoning

Task logs and changelogs under `docs/tasks/` and `docs/changelogs/` are point-in-time
records by this project's own convention and were deliberately left untouched (rewriting a
historical record to reflect a later state defeats its purpose as a record). The audit was
scoped to LIVING documentation: `docs/specs/`, `docs/architecture/`, `docs/guides/` — pages
that describe the system as it is now, not as it was on some date.

Grepped those three directories for the vocabulary of the removed architecture
(`copy+swap`, `AtomicSwapDB`, "written as nodes and relationships into LadybugDB",
"database is locked", "one read-write handle per database") — the local, on-the-fly graph
stopped being a file-based `ladybugdb` catalog published via copy+swap when
[local-icebug-filesystem-in-memory.md](local-icebug-filesystem-in-memory.md) (commit
`4796672`, 2026-08-27/28) landed, so any live doc still using that vocabulary is describing
something that predates a real rewrite by days, not describing current behavior.

Four hits, all confirmed against the current code before editing (not assumed from the
grep alone):

1. `docs/specs/ast_module.md`'s "Indexing Pipeline" section said entities are "written as
   nodes and relationships into LadybugDB" and that an incremental "removes all existing
   nodes and edges... from LadybugDB" — internally inconsistent with the SAME file's own
   "Database Architecture" section 1000 lines above it, which already correctly said "no
   `ladybugdb` file... no `AtomicSwapDB`". Confirmed current behavior against
   `internal/ast/pipeline.go` and `internal/ast/direct_icebug.go`: the shard cache is
   exported directly to Parquet, there is no intermediate database write.
2. `docs/specs/daemon_module.md` cited "a LadybugDB buffer pool per open database (two per
   database during a copy+swap rebuild)" as the reason for the cross-pipeline resource
   gate — confirmed `internal/ast/direct_icebug.go`'s export never opens a LadybugDB handle
   at all now.
3. `docs/guides/troubleshooting.md`'s "Database locked" entry cited a `database is locked`
   message and a `.graphit/ast/` storage path. Grepped the whole codebase: that error string
   does not exist anywhere in current Go source. The REAL current error in this class —
   `failed to open database with status 1`, a transient open-during-publish race with a
   built-in retry — does exist (`internal/ast/ladybug.go:228`, `internal/ast/rule.go:1207`),
   confirmed still current and not itself stale. The storage path is wrong too:
   `internal/store/store.go`'s `ASTProjectDir` resolves to the global graphit directory, not
   a project-local `.graphit/ast/`.
4. `docs/architecture/storage_layout.md`'s directory diagram showed a Hub-mounted context as
   `ladybugdb the CATALOG only... (legacy file catalog; will migrate to :memory:)` — the
   "will migrate" was already done: confirmed `internal/hub/ast_store.go:146` — "no
   ladybugdb file exists, which is the point of the mount" — Hub contexts mount
   `schema.cypher` + `icebug.json`, same as a local project.

## Implementation

- `docs/specs/ast_module.md`: rewrote "Full Indexing Pipeline" and "Incremental Indexing
  Pipeline" to describe shard cache → direct Parquet export → in-memory mount, folded in
  the shard interning behavior and the two write-parallelizations from the prior two
  commits at the level of detail appropriate for a spec (not the implementation-level detail
  that belongs in their own task logs). Fixed the "Performance Characteristics" table and
  the "nothing changed" section's `os.Stat` target.
- `docs/specs/daemon_module.md`: corrected the Cross-Pipeline Resource Gate's justification
  to describe LadybugDB buffer pool cost per query connection, not per copy+swap rebuild,
  and pointed out the export's own concurrency now also draws from `CPUBudget()`.
- `docs/guides/troubleshooting.md`: replaced the "Database locked" entry with a
  `failed to open database with status 1` entry describing the real current cause (transient
  open during a bundle publish, already retried internally) and pointed at `graphit daemon
  status` / `graphit ast index --reset` instead of a wrong local path.
- `docs/architecture/storage_layout.md`: fixed the Hub context row in the directory diagram
  to show `schema.cypher` + `icebug.json` instead of a `ladybugdb` file.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/specs/ast_module.md` | Modified | pipeline section rewritten for the current shard→Parquet export architecture |
| `docs/specs/daemon_module.md` | Modified | resource-gate justification no longer cites copy+swap |
| `docs/guides/troubleshooting.md` | Modified | replaced a symptom/cause pair that no longer occurs with the real current one |
| `docs/architecture/storage_layout.md` | Modified | Hub context mount diagram matches the actual (already-shipped) in-memory layout |

## What was deliberately NOT touched

`docs/tasks/*.md` and `docs/changelogs/*.md` — point-in-time records by convention, not
living documentation. Several of them (correctly) describe copy+swap because that is what
was true when they were written; rewriting them would erase the history they exist to keep.

A `graphit_knowledge_lint --deep` pass was run first to look for a broader, systematic list
of contradictions; it returned mostly broken-link and stale-page findings unrelated to this
architecture (109 errors, not inspected individually — out of scope for this pass). The four
fixes here came from targeted greps against vocabulary specific to the removed architecture,
each verified against current source before editing, not from the lint pass.

## Progress Log

### 2026-08-31
Scoped the audit to living docs only, per this project's own task-log-is-a-record
convention. Grepped `docs/specs/`, `docs/architecture/`, `docs/guides/` for the vocabulary
of the removed copy+swap architecture, found four hits, verified each against current
source (`internal/ast/pipeline.go`, `internal/ast/direct_icebug.go`,
`internal/store/store.go`, `internal/hub/ast_store.go`, `internal/ast/ladybug.go`) before
rewriting, and fixed all four.
