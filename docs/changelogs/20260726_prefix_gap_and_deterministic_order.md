# Prefix index gap measured + deterministic search order

**Date:** 2026-07-26
**Scope:** `internal/ast/prefix_gap_test.go` (new), `internal/ast/ladybug_llm_test.go` (new),
`internal/ast/search_determinism_test.go` (new), `internal/ast/fts_sqlite.go` (fix)
**Origin:** Engineer's instruction — measure the prefix gap before implementing;
and question about local LLM embedded in LadybugDB

---

## 1. LadybugDB does not ship an embedded local LLM

`TestLadybugLLMExtension`. The `llm` extension loads and exposes `CREATE_EMBEDDING`, but as a
**scalar** function (not table function) and with **mandatory** provider:

```
Expected: (STRING,STRING,STRING) -> LIST      # (text, provider, model)
```

- `'open-ai'` → `Could not read environmental variable: OPENAI_API_KEY` (hosted provider)
- `'local'` → `Provider not found: local` (**no embedded provider exists**)
- `'ollama'` → **accepted**, returned a real vector

The `ollama` case worked because **this machine has Ollama running** (`ollama serve`, PID
4989, with `nomic-embed-text` loaded) — external daemon called via HTTP at
`localhost:11434`, not an embedded model. In an environment without Ollama the same probe fails.

Consequence: swapping the in-process ONNX for Ollama-via-Ladybug would trade an
embedded dependency for an external daemon installed by the user, in a binary that today works
standalone and offline. Moreover CodeRankEmbed is code-specific and measured well
(4.4× separation), while `nomic-embed-text` is general purpose. **The gain from
consolidation remains the vector index, not the LLM extension.**

## 2. Prefix gap: narrow and mitigable

`TestPrefixIndexGap` compares the production SQLite index against a **prototype of the proposed
design** for Ladybug (per-field FTS index + split identifier + trigram bag, fused via RRF), over the same corpus and the same probes.

Final result, **11 truncated-query probes**:

| | expected top-1 | empty |
|---|---|---|
| SQLite (production) | 9/11 | 0 |
| Ladybug (prototype) | **11/11** | 0 |

The real gap was precisely located: **only queries with fewer than 3
characters**. `cf` → `CFG_LOAD` returned empty on Ladybug, because a query shorter than
a trigram produces no gram and there is no wildcard operator. From 3 characters
upward the trigram bag covers everything the prefix index covered.

Mitigation measured (not hypothetical): `CONTAINS` fallback for sub-trigram queries, which
closes the case (11/11). It's a scan, but it only fires for 1–2 characters and with limited
row count. `CONTAINS` availability was already proven in
`TestLadybugFTSFeatureParity`.

### Two confounds fixed along the way

- **File vs entity.** The first version measured 9/11 Ladybug × **5/11** SQLite. It was
  bias: the SQLite index also indexes files (`file_fts`) and the prototype didn't, so
  `retry.go`, `conf_mgr.sql`, `cfg_load.sql` and `db.go` appeared at the top and were counted as
  errors — measuring file × entity ranking, not prefix index. Fixed with
  `sqliteEntitySearch`, which filters file results. With that SQLite rises to 9/11.
- **Indefensible probe.** Probe `data` → `connectDatabase` has no correct answer:
  "data" is a substring of `Database` in both `connectDatabase` and `closeDatabase`.
  Replaced by `connect` → `connectDatabase`.

## 3. Production defect found: non-deterministic search order

Measuring item 2, `valid` on SQLite oscillated between runs: 3/5 `validateSchema`, 2/5
`PKG_VALIDACAO_PAGAMENTO`. Same corpus, same query, different top-1.

**Diagnosis (the first was wrong).** The initial hypothesis was `docScores` map iteration
with unstable `sort.Slice` at the end. Test with 25 calls on the same index
**passed** — within one process the order is stable. The cause is the **build**:
`RebuildFromCache` inserts by iterating `cache.AllEntries()`, which is a map, so the FTS5 rowids
change on every build; FTS5 breaks equal BM25 ties by rowid, which changes **rank
position** per pass — and RRF scores by position, so the scores themselves change. Hence
tie-breaking only on the fused list would not fix it.

**Fix.** `sortResultsDeterministic` — total order by decreasing relevance with
tie-break by `deduplicationKey` (path+name+line, unique per document) — applied at
four points: `queryFTS` output, `queryTrigram` output, and final sorts of `Search` and
`HybridSearch`. Applying per pass is the essential part, because it is the position in the pass that feeds
RRF.

**Declared limit, not fixed:** *which* tied rows fall inside a pass's `LIMIT`
remains decided by SQLite. A tie exactly at the search window edge can still
vary. Forcing that would require secondary `ORDER BY` in SQL, giving up FTS5's
top-N path on every query to stabilize the least significant row of an
over-sized window.

**Guard.** `TestSearchOrderIsDeterministic` and `TestHybridSearchOrderIsDeterministic`
build the same corpus 8 times and require identical order. The "25 calls on the same
index" version was discarded because it passes by vacuum — recorded in the comment so it
doesn't come back.

## State

Full suite green with `-count=1` (no cache), ORT 1.26.0 and active embedder:

```
ok  internal/ai        105.769s
ok  internal/ast        15.180s
ok  internal/fswatch     0.518s
ok  internal/daemon      6.165s
ok  internal/sysutil     0.003s
```

`go build -tags fts5 ./...` and `go vet` clean.

With this, the last unknown listed in `20260726_native_ladybug_vector_measured.md` (the prefix
index gap) is measured and mitigated. Nothing else blocks writing migration step 1
covering FTS and vector together.
