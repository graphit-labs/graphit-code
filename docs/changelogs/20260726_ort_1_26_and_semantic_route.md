# ORT 1.26 + semantic route measured: expansion field discarded

**Date:** 2026-07-26
**Scope:** `Makefile`, `.github/workflows/release.yml`, `internal/ai/ai_test.go`,
`internal/ast/abbrev_semantic_test.go`
**Origin:** executing the three recommendations from the previous changelog

---

## 1. ONNX Runtime mismatch fixed

`ORT_VERSION` 1.25.0 → **1.26.0** in `Makefile`, plus the three cache keys in
`release.yml` (linux, darwin-arm64, windows). 1.26.0 is exactly the `ORT_API_VERSION 26`
that `onnxruntime_go v1.31.0` declares — no jump to 1.27/1.28 without reason.

Comment added in `Makefile` tying the two versions together, because it was precisely the
lack of that link that produced the bug: the binding went up in `10ce4503` (2026-07-22) and the
runtime stayed behind, leaving the embedder nil and degrading semantic search to
FTS-only **silently**.

Verified locally: `make fetch-ort-linux` downloads 1.26.0 and the embedder now
initializes (`CodeRankEmbed-137M-INT8`).

**Not verified:** darwin-arm64 and windows-x64. Only cache keys and variable
changed — the download is the same mechanism — but neither platform was executed.

## 2. Semantic route measured — and it covers the gap

`TestSemanticReachOfAbbreviations`, embeddings **of name only** (no prose, to avoid
repeating the docstring confound):

| identifier | `config` | `configuration` |
|---|---|---|
| **CFG_LOAD** | **0.3928 (1st/7)** | 0.3670 (4th/7) |
| CONF_MGR | 0.3813 | 0.4379 |
| configLoader | 0.3694 | 0.3341 |
| initConfiguration | 0.3487 | 0.5261 |
| coreConf | 0.3445 | 0.4022 |
| computeChecksum | 0.0789 | 0.0694 |
| PKG_ACCOUNT_UPDATE | 0.0701 | 0.0671 |

`CFG_LOAD` — the only identifier that **no** lexical method reaches, because it
shares no trigram with `config` — ranks **first**. The separation between the worst
related (0.3445) and the best irrelevant (0.0789) is 4.4×.

## 3. Expansion field: discarded, with evidence

The measured marginal gain of the expansion field was **1/9** (only `CFG_LOAD`;
`TestExpansionFieldCeiling`). That same case is covered by semantic search **that already
exists** — no new field, no generative model, no inference over ~1M entities at
indexing time, no wrong expansion written to the index.

Nothing was built. The static abbreviation dictionary is also not needed.

Final coverage of the abbreviation gap: **trigram** (8/9, lexical and deterministic) +
**semantic** (the 9th), fused via RRF in `HybridSearch` which is already in place.

## 4. Two tests that passed for the wrong reason

Fixing ORT turned `internal/ai` red, exposing two tests that were green by
environment accident:

- `TestLazyEmbeddingClient_InitError` — the comment admitted it depended on "we don't have the
  ONNX model in the test environment". With the runtime working, init now succeeds
  and the test fails. It never tested error propagation; it tested absence of
  model.
- `TestLazyEmbeddingClient_MultipleCalls` — did `lazy.err = errors.New(...)` to
  "simulate" failure. Doesn't work: `init()` runs `NewLocalEmbeddingClient` inside
  `once.Do` and **overwrites** `l.err`. The simulation never had effect; the test
  passed because the real init truly failed.

Both now inject failure via helper `failedLazyClient`, which consumes the `sync.Once` —
the only way the injection sticks — and **fails explicitly** if the injection doesn't stick,
so they don't go green by vacuum again.

`TestSemanticReachOfAbbreviations` also stopped skipping silently: error containing
"API version" is now `Fatal` with the cause (Makefile × go.mod mismatch), while missing
model remains `Skip`. That guard would have caught the 2026-07-22 regression on the day.
Semantic measurements moved from log to assertion: related/irrelevant separation and
`CFG_LOAD` in the upper half.

## Verification state

- `internal/ast` complete: green (before changes in `internal/ai`, which don't affect it).
- `internal/ai` subset `TestLazyEmbeddingClient`: green.
- `TestSemanticReachOfAbbreviations`: green with ORT 1.26.
- **Pending:** full `internal/ai` suite (~5 min) and `go build ./...` after the changes
  in this round. Execution was denied; needs to run before commit.

To run with the embedder active, `LD_LIBRARY_PATH` needs the new ORT:

```bash
export LD_LIBRARY_PATH="$(go env GOPATH)/pkg/mod/github.com/\!ladybug\!d\!b/go-ladybug@v0.17.0/lib:/tmp/onnxruntime-cache/onnxruntime-linux-x64-1.26.0/lib"
```
