# Daemon e2e validation: two watcher regressions, one masking the other

**Date:** 2026-07-27
**Scope:** `internal/daemon/syncmodule.go`, `internal/daemon/syncmodule_e2e_test.go`,
`internal/daemon/syncmodule_classify_test.go`, `internal/daemon/syncmodule_feedback_test.go`
**Origin:** item 5 of the handover — "daemon e2e validation with the new watcher", the only gap
where production code had never been executed the way it really runs

---

## Summary

The e2e was written, executed for the first time, and **failed**. Two causes, both in
`handleBatch`, introduced by `bd19121b` ("replace all git polling with filesystem
watcher"), on 07-26. The second was hidden behind the first: while the classifier
was broken, no path reached the AST pipeline, and the feedback loop had
no way to happen. Fixing the first made the second reachable.

**The severity of the two is quite different, and worth recording without exaggeration:**

- **Defect 1 was live in production** since 07-26, in any project, regardless of
  configuration.
- **Defect 2 was not**, on the normal path. `graphit init` injects `.graphit/` into
  `.gitignore` (`cmd/graphit/commands/lifecycle.go:187`), and the watcher **obeys**
  `.gitignore` — measured in the three scenarios below. It was reachable only where that entry doesn't
  exist.

## Defect 1 — daemon stopped reindexing code

`handleBatch` classified each path as "documentation" or "code" by location:

```go
if isUnder(p, docsPath) { knowledgeTouched = true; continue }
```

`config.ResolveDocsDir` returns `"."` when `knowledge.docs_dir` is not configured
(`internal/config/config.go:396`). So `docsPath == projectDir`, and `isUnder` returns
true for **every file in the project**. `astChanged` was never populated; the `continue`
ensured nothing proceeded to the pipeline.

Effect: since `bd19121b`, for any project that doesn't configure `knowledge.docs_dir` to a
subdirectory, **the daemon never reindexed AST from filesystem events**. Only the `batch.Rescan` path (events lost by the kernel) still triggered a full scan.

The previous poller didn't have this defect because it classified nothing: `reindex` called
`reindexAST` and `reindexKnowledge` unconditionally.

### Second problem in the same snippet

The two destinations are not exclusive. `.md`, `.yaml`, `.json`, `.xml`, `.proto`, `.graphql` and
`.wsdl` are knowledge extensions **and** have an AST parser — measured, not assumed. And a
full scan indexes `docs/guide.md` (verified: generates `heading` and `file` entities).
The `continue` meant that, even with a correctly configured `docs/`, an `.md` under it
never went to AST — incremental and full disagreed about index contents.

### Fix

Classification extracted to a pure function `classifyBatch`, with the two decisions
independent:

- **AST**: extension and nothing else, exactly as the full scan decides.
- **Knowledge**: under the docs directory **and** with an extension the wiki indexes. Location
  alone cannot decide, because the default is the project root.

## Defect 2 — unbounded feedback loop

With AST receiving paths again, the e2e went from 5.1s per change to 1.2s —
the `syncMaxDebounce` ceiling, not the `syncDebounce` silence. Sign the tree never
went quiet.

A probe with a single external write and the tree untouched afterward:

```
batch 1:  1 changed,  0 removed — b.sql
batch 2:  5 changed, 14 removed — b.sql.edges.json
batch 3: 14 changed, 28 removed — manifest.json.nodes.json
batch 4: 25 changed, 32 removed — manifest.json.nodes.json.nodes.json
batch 5: 51 changed, 37 removed — manifest.json.edges.json.edges.json
batch 6: 99 changed, 76 removed — index.md.edges.json.nodes.json.edges.json
```

Not just waste, it's **amplification**. The daemon writes its shards into `.graphit`, inside the
tree it watches. A shard is `.json`, and `.json` has a parser. Indexing a shard emits a
shard of the shard. Each round produces more files than the last, without limit — the probe blew
the 2-minute timeout still growing.

The full scan never saw these files because discovery skips dot-directories
(`internal/ast/writer.go:61`). The scoped path (`RunPipelineForPaths`) skips discovery
entirely — that's the gain of the optimization — and lost the rule along with it.

### What the real exposure was

The probe above ran in a `t.TempDir()` without any ignore file. Measuring the ignorer as it
was built before this change (`defaultPatterns` nil):

```
git repo, .graphit/ in .gitignore    → IsIgnored(".graphit", dir) = true
git repo, without .gitignore         → false     ← the probe scenario
not a git repo, but with .gitignore  → true
```

The watcher obeys `.gitignore`, and `graphit init` injects `.graphit/` into it. In a
project initialized via the normal path the loop **did not happen**. It required the entry to be
missing — which is possible, because injection is best-effort: in `lifecycle.go:188` failure becomes
`StepWarn`, and in `internal/mcpstdio/tools_lifecycle.go:125` the error is discarded with `_ =`.

In other words: it was an armed bomb, not a detonated one.

### Fix — two layers

1. **The brand directory is now excluded by default, on the AST side.** Not as an inline literal in the daemon, but in `ast.DefaultAstIgnorePatterns`, consumed by
   `ast.NewAstIgnoreChecker` — mirroring what the `knowledge` package has long done, where
   `brand.DotDir() + "/"` is in `DefaultKnowledgeIgnorePatterns` from the start
   (`internal/knowledge/knowledgeignore.go:32`). That was the asymmetry: the
   knowledge side defended itself, the AST side depended on `.gitignore`.

   This covered **three** consumers at once, not one:

   | site | before | exposed to loop? |
   |---|---|---|
   | `internal/daemon/syncmodule.go` | `ignorer.New(…, nil)` | yes |
   | `internal/ast/watcher.go:49` (`graphit ast watch`) | `NewAstIgnoreChecker` → `nil` | **yes** — uses `RunPipelineForPaths` just like the daemon |
   | `cmd/graphit/commands/runners.go:1709` | `ignorer.New(…, nil)` inline | yes |
   | `internal/ast/writer.go:46` (discovery) | `NewAstIgnoreChecker` → `nil` | no — already skipped dot-directories |
   | `internal/dream/dream.go:475` | `ignorer.New(…, nil)` | no — skips `brandDir` by hand |

   The two sites that built `ignorer.New` inline now use the shared checker,
   and the `ignorer` import left both files.
2. **`classifyBatch` applies the same rule as discovery**: no path with a directory component starting with a dot is a source. This holds beyond `.graphit` and is the independently valuable part: full discovery skips **every** dot-directory
   (`internal/ast/writer.go:61`), but the AST ignorer is built without `defaultPatterns`, unlike
   the knowledge one. So `.venv`, `.idea` and `.cache` were only skipped on incremental
   if they were in `.gitignore` — full and incremental diverged there with or without `.graphit`.
   Only *directory* components: discovery skips dot-directories, not dot-files,
   so a `.hidden.sql` at the root remains a source.

After the fix the e2e dropped from 5.1s to **1.2s** per change — the natural 1s debounce.

## Adjustment to the e2e itself

The test asserted that a file created **before** the daemon started would appear in the index. The
daemon never promised that: nothing scans a project when it is adopted (`reconcileProjects`
only starts modules), and the index is seeded by `ast index`. The test now seeds the index
before bringing the module up, as production does — which makes the "rest of the index
still there" assertions meaningful instead of vacuously true.

## Behavioral drift left for the Engineer to decide

The old poller ran full `RunPipeline` **on every detected change**, so it piggybacked any file changed while the daemon was stopped. The watcher is strictly
incremental from the moment it comes up. **Changes made with the daemon stopped are no
longer recovered.** It's a direct consequence of scope optimization, not a bug — but it's an
undocumented behavior change, and the decision to accept it or to do a scan
on project adoption is yours.

## Tests

- `TestAstIgnoreCheckerExcludesBrandDirByDefault` and
  `TestAstIgnoreCheckerStillReadsProjectIgnoreFiles` — the default holds without any `.gitignore`,
  doesn't swallow real source, and doesn't replace the project's `.astignore`.
- `TestClassifyBatch` — 10 cases, unit and fast, covers both rules and both regressions.
- `TestSyncModuleDoesNotTriggerItself` — one external write, exactly one batch; also verifies
  that the change *reached* the index (a mute watcher would pass a "no loop" check) and that no shard-of-shard exists.
- `TestSyncModuleEndToEnd` — create, edit, delete and write file without parser, through the
  public `Start`, with real watcher and debounce.

Full suite with `-race` clean.
