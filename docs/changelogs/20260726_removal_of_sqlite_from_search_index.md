# Removal of SQLite from the AST search index

**Date:** 2026-07-26
**Scope:** `internal/ast/` (production and tests), `cmd/graphit/commands/ast.go`
**Origin:** Engineer's decision, reaffirmed after the regression numbers were presented

---

## WHAT changed

`internal/ast/fts_sqlite.go` (938 lines) was **removed**. The AST search index is now `SearchIndex` on LadybugDB, with FTS and vector in the same database.

| before | after |
|---|---|
| `internal/ast/fts_sqlite.go` | `internal/ast/search_index.go` |
| identifier query helpers inside the SQLite file | `internal/ast/search_common.go` |
| `<dbPath>.search.sqlite` | `<dbPath>.search` (constant `searchIndexSuffix`) |

- Updated consumers: `query.go`, `pipeline.go`, `embedder.go`,
  `cmd/graphit/commands/ast.go`. The public surface didn't change in shape
  (`OpenSearchIndex(path)`, `RebuildFromCache`, `UpdateIncremental`, `Search`,
  `SemanticSearch`, `HybridSearch`, `Close`), so the diff in consumers is the path.
- `CleanupInterruptedSwap` now protects `<dbPath>.search` — it deletes siblings
  `<dbPath>.*` as swap residue, and the search index is not residue.
- `internal/ast/search_common.go` gathers what is engine-independent: `tokenizeQuery`,
  `splitCodeIdentifier`, `identifierTrigrams`, `normalizeForTrigrams`,
  `sortResultsDeterministic`, `deduplicationKey`, `dedupTokens`, `rrfK`. These are measured decisions, not choices, and survive the storage swap.

### Verification that decoupling is real

`go test ./internal/ast/` **without the `fts5` build tag** passes (58s). Before it was impossible:
the package didn't compile without the FTS5 SQLite driver.

## What was NOT removed, and why

`go-sqlite3`, `sqlite-vec` and `BUILD_TAGS := fts5` **remain**, because
`internal/wiki/fts.go` (1494 lines) has its own SQLite FTS5 + vec0 index, consumed by
`chat`, `memory`, `knowledge`, `uiserver` and `daemon`. Migrating it is another migration of comparable scale, outside the scope of this one.

Coverage gap found along the way: wiki has 97 test functions that run in 0.016s
and **none** opens a database with the three `fts5`/`vec0` tables it creates. Removing the build
tag wouldn't make the suite fail — it would break at runtime, silently. Not in scope of this
change, but recorded.

## Accepted cost, measured before the decision

| | SQLite | Ladybug |
|---|---|---|
| expected top-1 (16 probes) | 12/16 | **14/16** |
| total rebuild, 200k entities | 3.2s | 15.8s |
| query latency | 50–146ms | 92–158ms |
| incremental of 1 file | ~330ms | **5.3s** |

The incremental regresses ~16× because liblbug 0.18.2's FTS index is not maintained on insert
(22 of 25 rows invisible, 12/12 iterations), forcing `DROP` + `CREATE` of the seven indexes
after each write. This was presented to the Engineer with numbers; the decision to remove
was reaffirmed.

## Tests: the differential oracle no longer exists

Several tests existed to compare Ladybug against SQLite. Without the second side, each was
converted preserving the expectation, instead of deleted:

| before | after |
|---|---|
| `TestConsolidationQualityGate` (Ladybug × SQLite) | `TestSearchIndexQualityFloor` — absolute floor, with SQLite's 12/16 recorded as reference |
| `TestPrefixIndexGap` (two engines) | `TestTruncatedQueryCoverage` — 11/11 floor, single engine |
| `TestAbbreviatedIdentifierSearchSQLite` | `TestAbbreviatedIdentifierRecall` |
| `TestTrigramBagSearchLatency` | absorbed by `TestSearchIndexScaleCost`, which measures the same at scale and also covers incremental |
| `quoteToken`, `buildPhraseQuery`, `buildANDQuery`, `buildORQuery`, `buildPrefixQuery`, `buildSearchPasses` tests | removed with the functions — LadybugDB has no phrase, explicit boolean or wildcard, so there was nothing to build |

Files renamed to stop lying: `abbrev_sqlite_test.go` →
`abbrev_recall_test.go`, `prefix_gap_test.go` → `truncated_query_test.go`,
`ladybug_search*.go` → `search_index*.go`.

## One result that inverted with the removal

`TestExpansionFieldCeiling` measured 9/9 for a perfect expansion field. On Ladybug the same
test gives **8/9** — same as trigram alone, i.e., expansion buys nothing.

The reason matters: on FTS5 the `CFG_LOAD` hit came from the **prefix index** (`config`
matching `configuration` in the docstring). LadybugDB has no wildcard and the porter stemmer doesn't
reduce `configuration` to `config`. So **that 9/9 ceiling was an FTS5 artifact.**

The test was rewritten to measure both wordings and record the real condition: expansion only
helps if it contains the **exact token** of the query (`"config load"` → 9/9;
`"configuration load"` → 8/9). That's much weaker than 9/9 suggested, and no generator
guarantees repeating the searcher's word — which reinforces, for a new reason, the decision not to
build the field.

## State

Green with `-count=1`: `internal/ai` 98.5s, `internal/ast` 60.0s, `internal/wiki`,
`internal/fswatch`, `internal/daemon`, `internal/sysutil`. `go build ./...` (with and without `fts5` tag) and `go vet` clean.
