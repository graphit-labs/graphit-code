# Entities know their parent, and ancestor ascent becomes memoized

**Date:** 2026-08-02
**Scope:** `internal/ast/treesitter_context.go` (new),
`internal/ast/treesitter_adapter.go`, `internal/ast/query_loader.go`,
`internal/ast/queries/{xml,json,yaml,svelte}.yaml`,
`internal/ast/treesitter_context_test.go` (new),
`internal/ast/containment_coverage_test.go` (new),
`internal/ast/data_format_kv_test.go`, `internal/ast/css_test.go`
**Origin:** `graphit ast index` on `private-corpus` never finished

---

## What happened

`graphit ast index . --reset` on `private-corpus` printed

```
◦ Indexing private-corpus
  › Grammar overrides: map[.sql:antlr-plsql]
```

and wrote nothing more. The daemon did the same on its own: 86 minutes of CPU,
25 GB RSS, `VmPeak` of 54 GB on a 61 GB machine, and then sat idle holding
memory. It was read as an ANTLR stall, because it's the only project with a grammar
override.

It wasn't ANTLR, and it wasn't a deadlock.

## How it was located

`kill -QUIT` on the index process — possible because the 07-31 changelog made the daemon's
stderr go to the log instead of `/dev/null`. The dump showed **one** live parse goroutine:

```
runtime.cgocall
  tree-sitter._Cfunc_ts_query_cursor_next_match
  ast.(*TreeSitterParser).parseWithConfig   treesitter_adapter.go:329
  runFileWorkerPool.func7.2                 pipeline.go:340
```

and the barrier goroutine stuck in `sync.WaitGroup.Wait` (`pipeline.go:345`) **for 3 minutes**,
waiting for that single worker while the other 19 in the chunk had already left. Hence ~1 core of 20
with 20 workers configured.

The file: `xml/fluxo-grande.xml`, 46 MB, 1,302,246 lines.

The dump pointed at the query cursor, and that was sampling bias — it happened to be
there at that instant. The CPU profile contradicted it:

```
36.46s (73.8%)  ast.resolveParentContextTS
  └ 34.84s      SafeParent → _Cfunc_ts_node_parent   (cgo)
 6.78s (13.7%)  ts_parser_parse            ← the real parse
 1.01s ( 2.0%)  ts_query_cursor_next_match ← the cursor
```

## The cause

`resolveParentContextTS` started like this:

```go
parentTypes := defaultContextTypes
if langConfig != nil {
    if len(langConfig.ContextTypes) > 0 {   // empty falls to default
        parentTypes = langConfig.ContextTypes
    }
}
```

`xml.yaml` declared `context_types: {}`. `len(...) > 0` treats "I declared I have no
containers" as "I had no opinion", and the fallback is `defaultContextTypes` — `class_declaration`,
`function_definition`, `method_declaration` — which can never occur in an XML tree.

Result: for each of the file's 1.6 million entities, the ascent went to the root of a
1.3M-line document, with one cgo call per ancestor, looking for something that would never
exist. Seven of 44 query files declared `{}` — exactly the data formats,
which are the same ones `c72a8338`/`ff924816` turned into "one entity per literal". The
defect was old; those commits multiplied how many times it's paid.

Cross-proof was in the per-query measurement itself: `attributes only` scans the whole file
and costs the same as baseline, because it's the only query in `xml.yaml` with `parent_capture` — and
`parent_capture` is precisely what makes the ascent unnecessary.

And there was a second defect behind it: even with `context_types` declared, the first matched ancestor
is the entity's **own declaration**. `(STag (Name) @name)` captures a `Name` living
inside the `element` it names, and the `name` of a `method_declaration` in Go is the captured node itself.
The entity became its own parent.

## The fix

Three layers, all generic in the engine — no per-grammar code.

1. **`parent_capture`** in the pattern, when the query already expresses containment. Zero cost.
2. **Memoized ascent per node** (`contextResolver`). A node's answer is its parent's answer,
   unless the parent is a container, so one ascent serves every entity below it. Types
   matched by numeric id instead of `Node.Kind()`, which allocates a Go string per call.
3. **`context_name_paths`**, new query-file field: `/`-separated path of fields or
   node types, from declaration to the node carrying the name. Exists because no data-format container has a `name` field — `element` (xml and svelte), `pair` (json),
   `block_mapping_pair` (yaml).

Two more semantic changes:

- Empty `context_types` now **replaces** the default instead of falling through to it.
- If the resolved name is the captured node itself, search continues above. That's what gives XML the correct
  parent and what prevents a Go method from being its own parent.

`css`, `clojure` and `sql` remain without containment — for being truly flat, and now with
justification recorded in `flatLanguages`.

## Measurements

Isolated file `fluxo-grande.xml`, 47 MB, 1,605,441 entities:

| config | before | after |
|---|---:|---:|
| full `xml.yaml` | 136.4 s | **50.7 s** |
| parse only, no queries | 7.7 s | 7.7 s |

The 50.7 s includes computing containment, which before wasn't computed — the memoized version without
any context gives 44.4 s.

Entire `private-corpus`, 36,823 files (35,358 PL/SQL via ANTLR, 1,462 XML), `--reset`:

| | before | after |
|---|---|---|
| completion | never finished | **16m14s** (parse 546 s, write 424 s) |
| RSS | 25 GB steady, peak 54 GB | ~15 GB, fluctuating |

Containment in the resulting graph: **57,547 of 59,009** `Element` have a parent. The 1,462 without are
exactly the number of XML files — the root element of each, the only case where empty is
the correct answer. The Oracle Forms hierarchy appears as `Module → FormModule → Block → Item`.

## Safety net

A grammar entering without containment history would produce entities hanging off File without
anything complaining. Two new tests close that:

- `TestEveryShippedGrammarDeclaresItsContainment` — every query file must declare
  `context_types`, or `parent_capture` in patterns, or be in `flatLanguages` with a reason.
  A reason pointing to a grammar that no longer exists also fails.
- `TestDeclaredContextNamePathsResolveAgainstTheGrammar` — each path segment is
  compiled against the real grammar. Already caught three errors while writing this changelog:
  `dockerfile`, `hcl` and `toml` had been classified as flat and declare context.

## Not done

- `resolveParentContextTS` was removed; ascent is now always the memoized one.
- The ~80 µs per entity dropped, but the match→entity path still dominates cost of a
  large file. `ComputeCyclomaticComplexity` runs over each entity's parent text,
  including on data formats where cyclomatic complexity means nothing.
- Pipeline still has no progress reporting between "Grammar overrides" and final
  result, so a slow file remains indistinguishable from a stall. A file inside
  `ts_query_cursor_next_match` is in cgo and not preemptable nor cancellable: no
  context timeout reaches it.
- `--reset` still wipes the parse cache along with the graph, without warning.

---

## Second round (same day)

### Self-loop in graph: 354 edges `(a)-[:CONTAINS]->(a)`

Node identity rule doesn't see nesting of **same name**: `<frame>` inside
`<frame>` are two distinct nodes with the same text, and Oracle Reports XML is full of it.
The failure was downstream, in `cache_convert.go`:

```go
nameToUID[e.Name] = uid        // overwrites the outer
if e.Context != "" {
    parentUID := nameToUID[e.Context]   // ...and then finds itself
```

Parent lookup now happens **before** registering the own name — entities arrive in
document order, so the name still in the map is the container's. Plus an explicit guard
`parentUID != uid`, because "nothing contains itself" is the invariant wanted, and iteration
order is too fragile a premise to be the sole defense.

### `anon_func_types: []` had the same defect as `context_types: {}`

Declared empty fell through to `defaultAnonFuncTypes`, which cost a `Node.Kind()` — cgo plus
Go string allocation — per ancestor, testing membership in a set of JavaScript nodes
a data format cannot contain. Fixed with the same `!= nil`, and matching switched
to numeric id via `kindMatcher`.

**This gave no measurable gain** (50.4 s → 52.0 s, noise). The hypothesis was wrong; it stays
fixed because it's the same defect, not because it was the bottleneck.

### The bottleneck was repeated resolution, not long ascent

Instrumented: 1,808,753 resolutions for 4,395,910 `parent` calls — **2.43 per
entity**, practically the floor. Ascent was already short; what cost was the volume of
`Node.Parent()`, which allocates a `*Node` on the heap per call.

And 1.8M resolutions happen over far fewer distinct nodes: `elements` and `element_text`
capture **the same** `Name` node. Caching the final result per capture node: **50.4 s → 44.2 s**.

Accumulated on `fluxo-grande.xml`: 136.4 s → 44.2 s, 3.09×, with containment computed.

### Progress in `ast index`

`PipelineOptions.OnProgress` existed and had been emitted since forever in `pipeline.go`. **No
caller consumed it.** Wired in `runASTIndex`, throttled by time not by file count — per-file cost varies by four orders of magnitude here, so "every N files"
stays silent for minutes on slow files and floods on fast ones. Phase switch always prints.

The write phase had no hook at all: 424 s of 970 s in absolute silence after the last
parse line. Now announces the transition and the volume.

### Discarded

`ComputeCyclomaticComplexity` over each entity's parent text, which I had flagged
as the next target. **Doesn't appear in the top 22 of the profile.** The suggestion was a guess; measured, it
doesn't hold.

---

## Third round: cancellation, cache rebuild, and the orphan collector

### `ast index` honored cancellation nowhere

`ctx` appeared in `pipeline.go` only on lines 496 and 499 — the write phase.
Discover, hash and parse had not a single `ctx.Done()`. The SIGINT handler
printed *"Interrupted — saving progress…"*, cancelled the context, and parsing continued to the
last of 36,823 files.

Now the feeder, workers and chunk loop consult `ctx`. The feeder needs the
guard as much as workers: the channel is unbuffered, so without it cancellation becomes
deadlock instead of exit.

The predicate is mirrored in an `atomic.Bool` because parsers consult it from inside a
cgo callback, many times per file, and `context.cancelCtx.Err` takes a mutex on each call.
The watcher is terminated on return, so a daemon doesn't accumulate a goroutine per run.

### Both tree-sitter cancellation hooks are unusable, and one crashed

First version used `Parser.ParseWithOptions` and `QueryCursor.MatchesWithOptions`. Passed
tests and **crashed in the field**, on the first real `ast index --reindex`:

```
SIGSEGV: segmentation violation
PC=0x6f170c8315f0 addr=0x6f170c8315f0
signal arrived during cgo execution
  tree-sitter._Cfunc_ts_query_cursor_next_match
```

`addr == PC` is a jump to a dead address. Cause, in `query.go:786`: `MatchesWithOptions`
builds a `&C.TSQueryCursorOptions` in Go memory and hands it to C; `ts_query_cursor_exec_with_options`
returns immediately and iteration happens later, in `Next()`. Nothing keeps the struct alive,
GC collects it, and the next match jumps through a dangling callback. Violates cgo pointer
rules.

Tests didn't catch it because an always-true callback aborts at the *parser* and never reaches
the cursor. The new test (`TestCancellationBitesInsideTheMatchLoop`) flips cancellation mid-flight,
which is what exercises the loop.

`ParseWithOptions` is safe — `cOptions` is passed by value and parsing ends inside the
call — but **leaks**: it pairs `Save`/`Unref` for the input payload and only does `Save` for the
options one (`parser.go:351`), one handle retained per parsed file. Unbounded in a daemon.

Neither is used. Cancellation is checked in Go, **between** matches: costs nothing, is
safe, and is almost as prompt — cursor time is spread over millions of `Next` calls (~6.6 µs each measured), not concentrated in one long call. What remains uninterruptable is a
single `ts_parser_parse`, bounded by file size: 7.7 s for 47 MB, milliseconds
for anything normal.

### Success reported over a missing graph

`if len(changedFiles) == 0 && ... && !ForceRebuild` returned early without checking whether a database existed.
Proven by moving the database aside: `✓ 798 files up to date (no changes detected)`, and no
database afterward.

Condition now requires `graphPresent`. This also makes reachable the cheap reconstruction that
had no way to be requested: delete the database, keep shards, and writes reproduce them —
**8.3 s** here versus full reparse, ~95 s versus 16 min on `private-corpus`.

Recorded that `--reindex` **does not** do this: it sets `ForceRebuild`, which skips the hash phase and treats every file as changed, costing the same parse as `--reset`. A flag for
"rebuild from cache" still doesn't exist.

### The orphan collector was dead code

369 MB of `ladybugdb.<hex>` beside an 81 MB database. Those are copy+swap working copies
left by dead processes between copy and rename — the `defer` that removes them doesn't run on
`SIGKILL`.

`CleanupInterruptedSwap` exists to collect them and its regex matches all. But it was called from a single place: `connect()`, when `!ReadOnly` (`ladybug.go:213`). And **both** write
paths — `IncrementalRebuild` and `RebuildFromJSON` — build in a copy and rename over,
precisely to never open production read-write. Queries open read-only. So the collector
never ran over the production path. The only real invocation was with the copy's
`DBPath`, scanning `<prod>.<hex>.*` — the wrong place.

Moved to the start of the write phase, which is where this process actually becomes the writer
— the condition the function's comment always described and which `connect()` only approximated.

Verified in the field: 369 MB collected, live database intact, graph queryable.
