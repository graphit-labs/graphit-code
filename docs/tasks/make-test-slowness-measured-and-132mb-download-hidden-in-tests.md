# Task: the slowness of `make test`, measured — and the 132 MB the tests were downloading from HuggingFace

**Status: partially completed** on 2026-08-07. Request from the Engineer: "attack the slowness".

## The measurement first

I had attributed the slowness to "1.29M lines of ANTLR compiled twice". **The measurement disproved
the part that mattered.** On a 20-core / 61 GB machine:

| phase | cold | warm |
|---|---|---|
| pass 1 build+link (`-race -covermode=atomic`) | **181 s** | **18 s** |
| pass 1 execution | 108 s | 108 s |
| pass 2 (parsers, no race) | 1.7 s | 1.6 s |

The double build costs 181s **once** per cache state; after that it is 18s. It was not the villain.
The recurring cost is execution — and inside it, two packages.

Method: `GOCACHE=/tmp/coldcache` for the cold number (without touching the real cache), and
`-run '^NOPE$'` to separate build+link from execution.

## What was wrong: `internal/ai` was downloading the real model

`internal/ai` took **102.9 s**, and seven tests were ~81 s of that:

```
18.18  TestModelManager_EnsureModel_CreateCacheDir
14.01  TestModelManager_EnsureModel_NeedDownloadModelTooSmall
11.54  TestModelManager_EnsureModel_BundledModels
10.73  TestModelManager_EnsureModel_DownloadModel
 9.18  TestNewEmbeddingClientFromConfig_FallsBackToLocal
 8.96  TestNewEmbeddingClientFromConfig_NoProxy
 8.73  TestNewLocalEmbeddingClient_Fails
```

`modelONNXURL` and `tokenizerJSONURL` were `const` in `model_manager.go`, and `download()` uses
`&http.Client{}` **without a timeout**. So every `make test` went to huggingface.co and moved 132 MB,
several times over. Two of them even created an `httptest.NewServer` — and never pointed `EnsureModel`
at it.

And none of them asserted anything: the body was `if err == nil { t.Log(...) }`. The result was
whatever the network gave. It was not just slow — it was a test that did not test, and a third-party
dependency inside CI.

### Two routes, two fixes

**Route 1 — the ones that build `ModelManager` directly (4 tests).** `ModelManager` gained two
unexported fields, `modelURL`/`tokenizerURL`, empty throughout production (`NewModelManager` does not
touch them) and resolved by `modelSource()`/`tokenizerSource()`. The test points at a local
`httptest`. With the endpoint under control, the outcome became determined — so the assertions
became assertions: "a 50-byte model is refused and the error names the size", "an already valid
tokenizer is not downgraded" (verified by mtime).

**Route 2 — the ones that go through `NewEmbeddingClientFromConfig` (3 tests).** Those build their
own `ModelManager` and have nowhere to inject. Purely test-side solution: seed the cache that
`NewModelManager` derives from `$HOME`, and `EnsureModel` returns at `isValid` before even
considering a download. No production change.

The seeded files are sparse (`f.Truncate`), because `isValid` only calls `Stat` — the previous code
did `make([]byte, modelONNXMinSize+1)`, 100 MB of heap and 100 MB of writes to answer a question
about `st_size`.

**Result: `internal/ai` 102.9 s → 2.67 s.**

## The wall-clock gain was smaller than the CPU gain, and it is honest to say so

Pass 1: **108.5 s → 105.2 s**. Only 3 s.

Because under `-p 4` `internal/ai` was running *in parallel* with `internal/ast`; taking 100 s out of
it does not move the critical path. The floor now is **18 s of build + 83 s of `internal/ast`**.

What the fix actually delivers: 100 s of CPU and network given back (relevant on a machine with fewer
cores, where `-p 4` has no slack), CI stops depending on huggingface.co being up, and seven tests
that asserted nothing started asserting.

## What is left, quantified

`internal/ast` is 83 s and it is the critical path. **352 tests, of which only 42 call
`t.Parallel()`** — 310 run serially on a 20-core machine. That is where the next order of magnitude
is.

I did not do it because it is an audit of 310 tests with real shared state: ANTLR global caches
(`ResetAntlrCaches`), `t.Setenv` (incompatible with `t.Parallel`), LadybugDB handles whose buffer
pool is not clamped in the tests (22 files use `lbug.DefaultSystemConfig()` raw, ~80% of RAM each).
That is probably the reason `-p 4` exists.

The two most expensive tests in `ast` are **not** waste and should not be cut:
`TestResetAntlrCachesRace` (13.0 s — 8 goroutines parsing while 25 resets happen, under
`-race`) and `TestLadybugStringIntegrityUnderGCPressure` (11.6 s). They are stress tests doing real
work.

## Files

| File | Change | Reason |
|---|---|---|
| `internal/ai/model_manager.go` | Modified | `modelURL`/`tokenizerURL` fields + `modelSource()`/`tokenizerSource()`; `EnsureModel` uses the accessors |
| `internal/ai/model_testserver_test.go` | Created | `modelServer`, `seedModelCache`, `sparseFile`, `writeZeros` |
| `internal/ai/ai_test.go` | Modified | 4 tests off the network, with real assertions |
| `internal/ai/ai_embedding_test.go` | Modified | 2 tests off the network |
| `internal/ai/ai_coverage_test.go` | Modified | 1 test off the network |

## Verification

`go test -race ./internal/ai/` in 2.67 s, all passing. `make vet`, `make lint` (0 issues) and
`make ci` green.

---

## THE MISSING HALF, closed on 2026-08-24

The fix above covered `internal/ai` — both routes, URL injection and cache seeding. **It did not
cover `internal/ast`, which also builds an embedding client**, in four test files:
`abbrev_semantic_test.go`, `hybrid_search_test.go`, `search_hybrid_floor_test.go` and
`search_index_test.go`. None of them seeds a cache, so each one kept going to HuggingFace.

And it was invisible for the same reason that made the download useless: **all of them end in
`t.Skip`** when the client does not come up. So the cost showed up as `ok`, not as the slowness of a
test that someone would go investigate. Measured on 2026-08-24: **29 throwaway homes with 133 MB
each, 4.3 GB on a tmpfs** — that is, RAM. Dated 2026-08-23, that is, accumulating since well after
this task.

Three fixes, in `docs/tasks/search-returns-only-files-and-index-not-rebuilt.md`:

1. **`NewLocalEmbeddingClient` checks ORT BEFORE downloading the model.** The order was inverted, and
   that is why the download happened even when the client was going to fail anyway. Now the test
   without ORT skips in **0.00 s** with an empty cache, against ~28 s and 133 MB before.
2. **`<BRAND>_MODEL_CACHE`** points the model root at a shared directory, and `make test` sets it.
   That way the model is downloaded once, not once per package.
3. **ORT became reachable** — `findORTLibrary` now looks at the extracted payload, and `make test`
   puts the Makefile cache on the loader path. That is what finally made the gates RUN, and what
   they measured is recorded in that task's log: the hybrid channel answered **0 of 11** decisive
   probes, and `SemanticSearch` returned nothing.

Result: `/tmp/graphit-test-homes` went from **4.3 GB to 740 KB** on a full `make test`, with no
abandoned model copy left behind, and `make test` exits 0.

**The item from this task that remains open is the other one**: the 310 `internal/ast` tests that run
serially. Nothing here touched that.
