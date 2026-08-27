# "Not measured / not touched" block: fused traversal, measured memory, hardened sidecar, unreproduced corruption

**Date:** 2026-07-27
**Scope:** `internal/ast/treesitter_adapter.go`, `internal/ast/astignore.go` (already committed),
`internal/ast/antlr_sidecar.go`, `internal/daemon/syncmodule_*_test.go`, `docs/upstream/`
**Origin:** Engineer's request to resolve the four items in the "Open — not measured /
not touched" block

---

## 1. Second tree-sitter traversal — fused

`extractDocstringsTS` was a full tree traversal executed **after** the query
pass, which had already found every entity. It visited each node to find the few that
are declarations, and each visit crosses into the C library several times (child, type, null
check), so cost tracked file size instead of entity count.

Now the query pass delivers the sites: for each captured name, `declSiteFor` climbs to the
innermost declaration the language recognizes, and `attachDocstringsTS` examines only those.

**The pairing rule didn't change** — same `(line, name)` key. Declarations whose name is
on a line after the declaration remain without docs, as before.

### Measurement

The component benchmark says 10.6×, and alone it misleads: work was **relocated**, not
eliminated — climbing from name to declaration costs parent jumps, and that cost now lives inside
the query loop, where the component benchmark doesn't see it. Whole parse is the honest
number:

| | time | memory | allocs |
|---|---|---|---|
| before | 34.3 ms | 970 KB | 18,680 |
| after | 31.8 ms | 732 KB | 11,183 |
| | **−7.3%** | **−25%** | **−40%** |

`BenchmarkParseGoFileEndToEnd` exists precisely so the next person doesn't read only the
component.

### Two pre-existing defects the new test exposed

`TestDocstringsSurviveTheRealQueryPipeline` goes through the real pipeline, not the synthetic differential harness. It failed in two cases — **both verified as identical in
the previous code**, running the same test against `HEAD`:

1. **Go types never receive their doc comment.** The query captures the type name,
   whose innermost declaration is `type_spec`, but the comment is a sibling of the `type_declaration`
   wrapping it. Neither the old scan nor the current collection crosses that distance.
2. **Python docstring arrives at the index with `"""` at the end.** `cleanDocstring` strips the opening
   triple quote and not the closing one.

Both are pinned in the test with current behavior and a comment naming the defect, so they
don't pass as correct. **They were not fixed**: this commit is about performance, and sneaking
behavior change in would hide both.

## 2. `Entity.Source` memory — measured

The field stored `parent.Utf8Text(src)`, a heap copy of each entity's body, just so
`isExported` could do a substring check afterward. Removed in `6aad6d2c`, never
measured.

An allocation-rate benchmark **doesn't see** this cost: `Utf8Text` is still called today — verdict and complexity need the text —, so bytes are allocated either way. What changed was how long they stay reachable. The right measure is live heap after forced collection.

Over 40 Go files from this repository (379 KB source, 10,199 entities):

```
Entity.Source would retain:              525 KB  (1.38x source size)
live heap, text discarded (current):     2862 KB
live heap, text retained (old field):    3424 KB
difference:                              562 KB
worst single file: parse_cache.go at 2.66x its own size
```

The first version of the measurement used `Properties` as vehicle and inflated the difference by 3.4 MB —
it was measuring the per-entity `map`, not the field. The test now uses a struct with a plain `string`,
faithful to what existed.

## 3. ANTLR sidecar — hardened

The sidecar exists because ANTLR grammars total **47 MB of generated source** (plsql 16 MB,
tsql 11 MB, db2 8.8 MB, postgresql 6.4 MB, cobol85 5.4 MB). Compiling everything into the binary would
inflate it, so each grammar can come as a separate executable, installed from the Hub at
`<project>/.graphit/grammars/antlr/antlr-sidecar-<lang>` and driven via stdin/stdout — and it has
priority over the native driver when present.

Its three existing tests skip without `ANTLR_SIDECAR_BIN`, so on a normal run the driver
had no coverage at all — including failure handling, where defects live.
A fake sidecar was written that speaks the same protocol and knows how to misbehave in
specific ways. Doesn't depend on ANTLR and runs anywhere.

| defect | state |
|---|---|
| response frame up to 4 GB allocated from header | **reproduced** and fixed: 256 MB limit and growing read via `io.CopyN` |
| dead process returned to pool ("put broken one back to avoid deadlock") | **not reproduced** — lines only run if *restart* also fails. Fixed anyway |
| `Close()` returned on first empty slot and leaked the rest | fixed: drains all, with short wait for slot in use |
| eager spawn in `initOnce` leaked processes when a start failed | fixed: pool starts with empty slots and process starts on demand |

The pool now holds slots that may be empty. An empty slot means "no process, start
one" — that's what prevents a crash from costing a slot: the failure path returns the empty slot
instead of the corpse, and whoever grabs it next tries to start again.

**Not fixed:** `cmd.Stderr = nil` still sends sidecar diagnostics to
`/dev/null`. Capturing without draining risks blocking, and the right fix is a circular buffer —
outside scope of this commit.

## 4. Ladybug string corruption — third attempt, not reproduced

Did not reproduce. What the session added were **eliminations**, which is what the report now
carries.

| hypothesis | result |
|---|---|
| concurrent write | **does not apply** — engine refuses: `Only one write transaction at a time is allowed in the system`. Production writer is serial, which removes the entire class |
| collector moving/freeing Go string behind C pointer | `SetGCPercent(1)` + `runtime.GC()` continuous, 3000 batched inserts, concurrent reader on same table: clean |
| Go pointer illegally passed to C via binding | same probe recompiled with `GOEXPERIMENT=cgocheck2`: clean, no diagnostic |

What's left is **scale together with value size**: field case was 35358 rows of whole
files for 4 bad rows (~1 in 9000), and the largest probe has 3000 synthetic rows.

`docs/upstream/liblbug-string-corruption.md` is the 5th report, written explicitly as
**not reproduced** — field observation plus elimination table. A good report can
say what the defect *is not*, and for silent data loss that's worth more than another
guess.

## Structural finding: tests depended on installed runtime

The full suite broke mid-session, after the Engineer removed `~/.graphit`.
Not a regression: `initTsExtMap()` builds the extension table in the package `init()` **only**
from query files in the installed runtime. Without it, `HasParserForExtension` answers
false for everything, and `classifyBatch` — which routes by extension — returns empty.

Consequences:

- **A project query file can describe a language the parser then refuses
  to open**, because project queries don't register extensions; only runtime does.
- Fresh checkout + `go test` gives confusing failures instead of skipping.

`TestClassifyBatch` and `TestSyncModuleDoesNotTriggerItself` now declare the dependency with
`requireParsers`, which skips with the named cause. `TestDocstringsSurviveTheRealQueryPipeline` and
new benchmarks work around it by registering the extension by hand, which makes them hermetic.

### Fixed at Engineer's request: project queries now register extensions

A query file does two things — declares which extensions it serves and provides extraction
patterns — and the two were read from different places. Patterns went through
`resolveQueriesForLang`, which prefers the project's copy. Extension declaration was only read
by `initTsExtMap`/`initAntlrExtMap`, at `init()`, and **only from the installed runtime**. Result:
project overrode existing language, but didn't add new language — the
`extensions:` line in a new-language file was inert, and the error (`no grammar for .x`) looked like
missing grammar.

ANTLR already had project fallback in `AntlrParser.Parse`, but it was **unreachable**:
`collectFiles` and the watcher filter by extension before any parser is called, and
asked only the global table.

Resolution became lazy and layered, mirroring `resolveQueriesForLang`:

- `initTsExtMap` now registers runtime **and** the user global directory (which was also
  ignored), with user on top.
- `tsLangConfigFor(projectDir, ext)` first consults project queries, memoized per
  directory in `projectTsExtCache` to avoid rebuilding the map for every file indexed.
- New `HasParserForExtensionIn`, `HasTreeSitterForExtensionIn`, `HasAntlrForExtensionIn` and
  `TreeSitterLangForExtensionIn`. Forms without `In` still exist and delegate with
  empty `projectDir`, for callers that have no project (`server.go`, `obsidian.go`).
- Now use the project-aware form: `collectFiles` (discovery), `Watcher.Start`,
  `CompositeParser.Parse`, `TreeSitterParser.Parse` and `classifyBatch` in the daemon.

Covered by `internal/ast/project_language_test.go`, which invents a language existing only in the
project and verifies the three layers where it must appear: registration, discovery and parse with
entities and docstring.

### And reload at runtime, because the daemon lives for days

Registering wasn't enough: query files were read **once per process**. Runtime and
user behind `sync.Once`, project behind a `sync.Map` without invalidation. Installing a
grammar package or editing a YAML by hand had no effect until restart — and silently,
because discovery just discarded files of the new language.

Each directory is now cached against a **signature** of its contents:

- `queryDirSignature` reads the directory and sums name, size and mtime of each `.yaml`. Directory mtime
  alone doesn't work: editing a file in place doesn't move it.
- Check is limited to once per `queryStaleCheckInterval` (2s), because queries it
  feeds run **once per file** — in a 35k-file scan the directory is
  scanned a handful of times, not 35k.
- Signature changed, reload and drop derived state: `mergedQueryCache`,
  `compiledQueryCache`, `projectTsExtCache` and extension tables.
- `InvalidateQueryCaches()` is the shortcut for whoever knows something changed. Wired into
  `installGrammarArchive` and `uninstallGrammarFiles`, so installing grammar takes effect immediately
  instead of waiting for the interval.

**Compiled queries are discarded, not closed.** Nothing in this package ever closed a
`*sitter.Query` — they live for the process lifetime. A parse already holding the slice continues with
valid pointers; closing here would be use-after-free for it. Leaking the handful a reload
orphans is the cheapest mistake, and reload happens when someone installs grammar, not in a loop.

**The four global tables became shared mutable state** and gained `extTablesMu`
(RWMutex, because read is on the hot path per file). ANTLR's `init()` was
removed: `rebuildExtTables` builds both engines from one scan, and a second `init`
would run before or after the other depending on file order, building tables from
sources not yet read.

Covered by `internal/ast/query_reload_test.go`: file added after warm cache,
file **edited in place** (the case directory-mtime signature would miss), and
`InvalidateQueryCaches` taking effect immediately. They run with `-race`.

### Grammar `.so`: restart, not reload

Query file reloads in place; grammar library doesn't, and deliberately.
`resolveTreeSitterLang` memoizes each language for the process lifetime — negative result
included — because a `*sitter.Language` sustains live parse state, and swapping it under an in-flight
parse is not something a mutex fixes.

Engineer's suggestion, and it's the right answer: the daemon **already** exits to be replaced when
the launcher stamp changes (`versionTicker` → `stampChanged()` → `shutdown()` + `ErrReplace`).
New grammar now uses the same door.

- `ast.GrammarSignature(projectDir)` hashes installed libraries — name, size and mtime —
  scanning `<project>/.graphit/grammars/{treesitter,antlr}` and the global pair. Stays in
  `internal/ast` because it's the package that knows where both loaders look; `GrammarDirsFor`
  prevents the caller from reasserting those paths.
- Daemon keeps one signature per accepted directory (`grammarSigs`, under `mu`), and the same tick
  that checks the stamp now also compares those.
- **Directory seen for the first time is registered, not acted on.** A project discovered already with
  installed grammars is not a reason to restart, and treating it as one would make the daemon
  bounce on every new project. Once registered, an installation there triggers normally.
- Removal also counts: the process is still holding the old library.

Covered by `internal/daemon/grammar_restart_test.go`: installation triggers and reports where,
project discovery doesn't trigger but starts watching, removal triggers, and signature is
updated on trigger to avoid requesting replacement in a loop.

## State

Full suite with `-race` clean, without `~/.graphit` present.
