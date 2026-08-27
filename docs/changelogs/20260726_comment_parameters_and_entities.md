# Parameters stop disappearing; comments become searchable entities

**Date:** 2026-07-26
**Scope:** `internal/ast/antlr/common/matcher.go`, `internal/ast/antlr_adapter.go`,
`internal/ast/queries/plsql.yaml`
**Origin:** Engineer's request after the explanation of parameter loss

---

## 1. PL/SQL parameters never reached the cache — 35.4% of entities

`ConvertToCache` discards parameters and fields without context, to avoid creating orphans in the graph. The
context came from `resolveParentContextAntlr`, which looked at **only the immediate parent** of the match — and
for the pattern `//parameter/parameter_name` the parent is `parameter`, while the function body
is several levels above. Every parameter left without an owner and was filtered out.

**Fix:** the matcher now **carries context on the way down**
(`Pattern.MatchWithContext`), with O(1) state and no allocation — `TreeNode` has no pointer
to its parent, and an ancestor map would cost one entry per node in 700 KB files. `Match`
still exists and delegates with a nil predicate, so no other caller changes.

| | before | after |
|---|---|---|
| `ACAO_PERMITIDA.sql` | 10 → 5 | **10 → 10** |
| sample of 367 files | 967 missing (35.4%) | **0** |
| parameters in cache | 0 | **967** |

Guard: `TestOracleParametersReachTheCache`.

## 2. The ghost node `CREATE`

The fix above exposed a defect that already existed and was hidden: the context came out as
`CREATE`, not `ACAO_PERMITIDA`. `FirstTerminalText()` returns the first token of the
context node, and a declaration starts with keywords — `create_function_body` has
`'CREATE' 'EDITIONABLE' 'FUNCTION'` before `function_name`.

Effect: entities assigned to a context called `CREATE`, which no node has, leaving
`HAS_PARAMETER` pointing to a ghost. The function itself already suffered from this
(`uid=…::CREATE.ACAO_PERMITIDA`).

**Fix:** `declarationName` looks for the direct child whose rule ends in `_name`
(`function_name`, `procedure_name`, `tableview_name`, `package_name`) — the convention of this
family of grammars — and falls back to the first terminal for grammars that don't follow it.
It also unwraps delimited identifiers (`"GC"` → `GC`).

Now all entities in the file leave with `ctx=ACAO_PERMITIDA`.

## 3. Comments become entities, with the text as the name

`COMMENT ON ... IS '<text>'` was extracted only as a `REFERENCES` relation to the
commented column, and **the text was never captured**. In the reference Oracle export this is the
entire data dictionary thrown away: **2209 files** that contain only `COMMENT ON`, and that
yielded zero entities.

Three queries in `plsql.yaml` were changed to `type: entity`, `graph_label: Comment`, with pattern
`//comment_on_column/quoted_string` (and the `table` and `materialized` variants), so that the
**entity name is the text itself** — which is what someone searches for. The relation to the commented
object was kept, so "what is documented" and "what the documentation says" remain
both answerable.

```
Comment: "Indicador se Caixa e para Almoxarifado"
search almoxarifado -> hit
search indicador    -> hit
```

Effect on the census by object type:

| | before | after |
|---|---|---|
| `comments` type | 0 entities, 12/12 empty | **47 entities, 0 empty** |
| total sample (151 files) | 1282 entities, 8% empty | **1329 entities, 0% empty** |

Guard: `TestOracleCommentsAreEntitiesAndSearchable`, which verifies the entity, the absence of
delimiters in the name and search by a word that exists only inside the comment.

## Pitfall encountered along the way

**The query files are not embedded in the binary.** They are read from
`~/.graphit/runtime/<version>/ast/queries/`, so editing `internal/ast/queries/plsql.yaml` in the
repository **has no effect at all** until copied there. `Makefile:268` copies to
`cmd/launcher/runtime/ast/queries/`, from where the launcher extracts on installation; for
development you need to sync `~/.graphit/runtime/dev/ast/queries/` by hand.

This cost a debugging round: the change was correct and the result stayed zero.
It is recorded in the test comment so it doesn't cost again.

## State

Full suite green with `-count=1`: `internal/ai` 119.6s, `internal/ast` 17.5s,
`internal/ast/antlr/...`, `internal/fswatch`, `internal/daemon`, `internal/wiki`,
`internal/sysutil`. `go build -tags fts5 ./...` clean.

Reindex note: UIDs changed (context stopped being `CREATE`), so the existing index
needs to be rebuilt.
