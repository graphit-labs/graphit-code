# Abbreviation expansion: ceiling measurement and the semantic path

**Date:** 2026-07-26
**Scope:** `internal/ast/abbrev_semantic_test.go` (measurement only — no production changes)
**Origin:** Engineer's proposal — use local AI to normalize the name into another field
(`coreCfg` → "core configuration") instead of trigrams

---

## WHAT was measured

Two routes to reach an abbreviated identifier from the full word,
because the trigram pass leaves **exactly one case** open: `CFG_LOAD` shares no
trigram with `config` and is unreachable by any lexical method.

### 1. Expansion field — measured ceiling: 9/9

`TestExpansionFieldCeiling` uses hand-written expansions (what a perfect
generator would emit) to measure the ceiling of the idea **without depending on any generator**:

| probe | trigram | expanded field |
|---|---|---|
| `config` → coreConf, CONF_MGR, configLoader, initConfiguration | 4/4 | 4/4 |
| `conf` → idem | 4/4 | 4/4 |
| `config` → **CFG_LOAD** | **0/1** | **1/1** |
| total | 8/9 | **9/9** |

The expansion enters through the docstring column, which serves as a proxy for a dedicated field:
it is indexed by the same prefix index, so the mechanism exercised (`config`
reaching the word "configuration") is what a real `name_expanded` would use. The
docstrings are empty in the rest of the corpus, so every hit is attributable to the expansion.

**Conclusion:** the idea works and buys exactly the missing case. What it does
not solve is who writes the text.

### 2. Semantic path — not measured, blocked

`TestSemanticReachOfAbbreviations` embeds the names and the query with the real client and
ranks by cosine. **Skipped:** the local embedder does not initialize on this machine.

## Premise correction: the local model does not generate text

The model is **CodeRankEmbed** (ONNX int8, 768d) — an *embedder*. It maps text to 768
floats; there is no inverse, so it **cannot** produce "core configuration" from
`coreCfg`. The literal route of the proposal requires a **generative** model, which is a new
dependency: second local model, inference over ~1M entities at indexing time, and risk of
wrong expansion written to the index (`ENTRG` → ?) without determinism.

What the embedder **can** do is route 2, and it **is already implemented**:
`buildEmbeddingText` already includes the entity name, and `HybridSearch` already merges BM25 with
vector search via RRF. No new field or new model needed.

## Independent finding: embeddings broken since 2026-07-22

The local embedder does not initialize:

```
The requested API version [26] is not available, only API versions [1, 25]
are supported in this build. Current ORT Version is: 1.25.0
```

- `go.mod` → `github.com/yalue/onnxruntime_go v1.31.0`, whose header declares
  `#define ORT_API_VERSION 26` (requires ONNX Runtime ≥ 1.26).
- `Makefile:41` → `ORT_VERSION := 1.25.0`, unchanged since `8b765f4b` (2026-05-26).
- The binding was bumped in `10ce4503` (2026-07-22, "chore: upgrade dependencies and
  simplify lbug setup configuration") without updating the runtime.
- The three copies of `libonnxruntime.so` on the machine are 1.25.0.

**Effect:** `NewEmbeddingClientFromConfig` fails, the client stays nil, and `SemanticSearch`
returns `nil, nil` — the semantic half of search degrades **silently** to
FTS-only. This is also why route 2 could not be measured.

Two ways out, both an Engineer decision because they touch native runtime packaged for
three platforms:
1. `ORT_VERSION := 1.26.x` in the Makefile (implies new download and revalidation of the bundle
   for linux/darwin/windows);
2. pin `onnxruntime_go` to a version that requests API ≤ 25.

## Recommendation

Before building an AI-generated expansion field, measure route 2 — it already exists and the
intended gain may already be available. For that the ORT mismatch needs to be
resolved first. If after that the semantic route does not reach `CFG_LOAD`, route 1 is
justifiable, and then the question becomes the source of the expansion: a **static abbreviation
dictionary** (`cfg`→config, `pagto`→payment, `nft`→invoice) is deterministic, has no
latency and is probably more accurate in a Portuguese PL/SQL domain than a
generalist model — which will not expand `ABCD01` correctly anyway.

`TestSemanticReachOfAbbreviations` stays in the repository and will measure on its own when the
runtime is fixed.
