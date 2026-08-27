# Search: bag of trigrams instead of trigram phrase

**Date:** 2026-07-26
**Scope:** `internal/ast/fts_sqlite.go` (+ tests)
**Origin:** Engineer requirement — "it must be possible to search for `config` and find e.g. `coreConf`"

---

## What changed

1. **`entity_trigram` no longer uses `tokenize='trigram'`.** The table now has a `name_tri` column with trigrams **precomputed at write time**, indexed with word tokenizer (`unicode61 remove_diacritics 2`). The original name became an `UNINDEXED` column (returned only in results).
2. **`queryTrigram` reduces the query to the same bag of trigrams** and does `OR` between terms, instead of matching the query as a phrase.
3. **`ORDER BY rank` was added** to the trigram pass (did not exist before).
4. **`ftsSchemaVersion` 3 → 4** — migration recreates FTS tables, therefore **requires reindexing** for the new field to be populated.
5. New helpers `normalizeForTrigrams` and `identifierTrigrams` in production code.

## Why

`tokenize='trigram'` in FTS5 matches query trigrams as an **ordered phrase**, which is substring containment: the query must occur inside the document. So `conf` reaches `coreConf`, but `config` **does not** — `coreConf` does not contain trigrams `nfi`/`fig`.

Measured in `TestAbbreviatedIdentifierSearchSQLite`, searching **by name alone** (without prose help):

| probe | before | after |
|---|---|---|
| `config` → coreConf, CONF_MGR, configLoader, initConfiguration | 2/4 | **4/4** |
| `conf` → same | 4/4 | 4/4 |
| `config` → CFG_LOAD | 0/1 | 0/1 |
| **total** | **6/9** | **8/9** |

Scoring a **bag** of trigrams with BM25 keeps partial overlap rankable, which is exactly what the abbreviated case needs. `CFG_LOAD` remains unreachable because it shares no trigram with `config` — that is a semantic search or alias case, not FTS.

`ORDER BY rank` is an independent fix: RRF scores by **position** in ranking, so without ordering the trigram pass injected arbitrary positions into the fusion.

## How it was verified

- `TestAbbreviationRecallByNameAlone` — the requirement, written before implementation (red: `config` did not reach `coreConf`/`CONF_MGR`; green after).
- `TestAbbreviatedIdentifierSearchSQLite` — measures both directions of partial matching in two corpora. The "names only" variant exists to remove a **confounding factor found during work**: corpus docstrings contained "configuration", and the FTS5 prefix index matched via prose, not via identifier. With prose the number was 8/9 both before and after, hiding the rule under test.
- `TestTrigramNoiseDoesNotDisplaceExactMatches` — precision invariant: noise below a true hit is acceptable cost; noise that displaces it is regression. 6/6 probes keep exact match at top-1.
- `TestTrigramBagSearchLatency` — 200k synthetic entities with small vocabulary (adversarial case for `OR`). A/B on same machine:

   | query | before (phrase) | after (bag) |
   |---|---|---|
   | config | 32ms | 90ms |
   | conf | 50ms | 100ms |
   | checksum | 34ms | 75ms |
   | ENTRG | 32ms | 50ms |
   | EXTRAIR_DOC | 80ms | 116ms |
   | validaSchema | 70ms | 146ms |

   Worst case confirmed in 3 runs: 153ms / 159ms / 162ms (variance 1.06×). The "before" side is single run — treat ~2× factor with margin, though direction is consistent across 6 queries.
- `TestAbbreviatedIdentifierSearch` (new, Ladybug) — measures same corpus in raw FTS, split and trigram on Ladybug: 1/9, 3/9 and 8/9. Evidence that gain comes from **representation** (bag vs phrase), not engine.
- Suites `internal/ast`, `internal/fswatch`, `internal/daemon`, `internal/sysutil` green; `go build ./...` and `go vet` clean.

## Accepted cost, explicitly

- **Precision:** unrelated names sharing a common trigram enter the set (`checksum` and `validateSchema` share `che`). Stays **below** real hits because pass weight is 0.7 in RRF vs 1.5–3.0 for exact passes — invariant guarded by test.
- **Latency:** ~2× worst case, 155ms on 200k entities. Acceptable for MCP tool call; test ceiling is 1s to catch blowup, not to serve as target.
- **Mandatory reindexing:** schema bump empties existing FTS tables. Index at `~/.graphit/ast/project/ladybugdb.search.sqlite` must be rebuilt.

## Consequence for SQLite → LadybugDB migration

Measured gain comes from trigram representation, reproducible on either engine — **the search quality argument for migrating no longer exists**. The dependency-reduction argument (`go-sqlite3`, `sqlite-vec`, build tag `fts5`) remains real but has no measured urgency: no measurement in this session pointed to search as slow or incorrect. Decision to continue or stop migration is with the Engineer.

Related documentation fix: comment on `TestOracleIdentifierSearch` claimed "consolidate into Ladybug FTS alone" based on 6/6 = 6/6. That measurement only covered **whole-token** queries; comment was corrected to say so and point to new measurement, instead of leaving wrong conclusion in repo.
