---
title: Keyword Is Not a Call, Trigger Is Not Callable, and Fragment Needs a Wrapper
status: done
created: 2026-08-20
updated: 2026-08-20
tags: [ast, plsql, grammar, calls, embedded]
---

# Keyword Is Not a Call, Trigger Is Not Callable, and Fragment Needs a Wrapper

## Objective

After the caller's label stopped coming from a fixed list (commit e928bf5), the `CALLS`
edges of screen, flow, and report units started being written — and the per-target count
showed that **most of them were false**. This work is the real fix for that, in full, and
without a word list in Go: what can or cannot be a name in a capture is a fact of the
LANGUAGE, so it's the grammar that declares it.

## What was wrong, measured in the live graph of a consumer

| Symptom | Number |
|---|---|
| Call targets that were reserved words | `PROCEDURE` 16367, `IF` 2567, `DECLARE` 2050, `FUNCTION` 1021, `begin` 976, `procedure` 750, `.` 818 |
| Calls from `FormProgramUnit` | 19045, **all** keywords |
| Edges to a database trigger named `BEGIN` | 9092 |
| Calls that resolved to a real procedure | ~40 distinct names |

## The three causes, each with its own declarative fix

### 1. `call_statement` accepts a bare identifier — and the non-reserved word list has 1753 entries

`PlSqlParser.g4:5881`: `call_statement : CALL? routine_name function_argument? …` — BOTH
optionals can be absent, so a bare identifier in statement position is ALREADY a complete
call. And `routine_name → identifier → regular_id → non_reserved_keywords_pre12c`, a list
that contains `BEGIN`, `DECLARE`, `FUNCTION`, `IF`, `PROCEDURE`, and `RETURN` (confirmed:
1753 words).

**HYPOTHESIS TESTED AND DISCARDED:** that this came from ANTLR error recovery. I went as
far as implementing detection (flagging a subtree originating from recovery) and a language
policy (`on_parse_error: keep | skip_degraded | drop`), measured it, and the result with and
without discarding was **identical** — the parse is clean. Reverted entirely: a mechanism
that the measurement doesn't support is debt, not a solution. Recorded here so no one
repeats the experiment.

**Fix:** `ExternalQueryDef.NameReject` (`name_reject`) — a regular expression the captured
name must NOT match; if it matches, the match records nothing. Declared on the three
`calls` queries in `plsql.yaml`, with the words that never name a routine in PL/SQL plus
`^[^A-Za-z_]` (an unquoted identifier starts with a letter, so a bare `.` is not a name).
Anchored on both ends on purpose: an unanchored `if` would reject `if_valida_cliente`.

### 2. A trigger is not callable, and it was declared as a possible target

`target_rules.CALLS` in `plsql.yaml` listed `Trigger`. In PL/SQL a trigger is FIRED by DML
on its table; no program invokes it by name. With it in the set, every `begin` captured as
a call resolved to the trigger whose name in the database is the literal identifier
`"BEGIN"` — a formally correct, semantically empty edge, and **not a stub**, which let it
survive every filter that discards stubs.

**Fix:** `CALLS: { labels: [Function, Procedure, Package] }`. Measured: 9092 fewer false
edges, zero true ones lost (the only trigger ever "called" was this one).

### 3. A program unit body is a FRAGMENT, and a fragment doesn't parse

`PROCEDURE x(…) IS … END;` is a declaration, valid only inside a declarative section. On
its own:

| Body | Result |
|---|---|
| `PROCEDURE CGFK$CHK(p IN BOOLEAN) IS BEGIN pck_pedido.pr_grava(1); END;` | zero entities, zero calls |
| the same with `CREATE OR REPLACE` in front | `Procedure:CGFK$CHK`, call `pr_grava` |
| the same inside `DECLARE … BEGIN NULL; END;` | same |
| anonymous block `BEGIN … END;` | call `pr_grava` |

In other words: the 19045 calls from `FormProgramUnit` were keywords because the entire
body was invisible — only the first word survived, as a "call".

**Fix:** `EmbeddedBlock.WrapPrefix` / `WrapSuffix` (`wrap_prefix`, `wrap_suffix`) — the text
a fragment needs around it to become a compilation unit. Declared on the BLOCK, not on the
language: which wrapper applies is a fact of POSITION (the same PL/SQL in a `.sql` file
arrives with `CREATE OR REPLACE` and needs nothing). Neither side may contain a line break —
discarded at load time, the same invariant as `text_normalizers`.

Measured on the same screen file, before → after the wrapper: **34 → 50 calls, 11 → 17
SELECTs, 101 → 129 PL/SQL entities**. On a 1.5 MB file: 106 → 144 calls, 55 → 68 SELECTs,
333 → 609 entities.

## Why the other SQL grammars don't need `name_reject`

Checked in the grammars, not assumed:

| Language | Call rule | Does a bare identifier match? |
|---|---|---|
| PL/SQL | `CALL? routine_name function_argument?` | **yes** |
| PostgreSQL | `callstmt : CALL func_application` | no |
| DB2 | `call_statement : CALL procedure_name arg_list_paren?` | no |
| T-SQL | queries anchored on `execute_body` / `function_call` | no |
| COBOL 85 | `performProcedureStatement`, `callStatement`, `goToStatement…` | no |

PL/SQL is the outlier case, which is why the declaration lives only there.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/query_loader.go` | Modified | `NameReject` on the query, `WrapPrefix`/`WrapSuffix` on the block, regexp validation and memoization |
| `internal/ast/treesitter_adapter.go` | Modified | `name_reject` applied on the tree-sitter path |
| `internal/ast/antlr_adapter.go` | Modified | `name_reject` applied on the ANTLR path |
| `internal/ast/treesitter_embedded.go` | Modified | fragment wrapping before the sub-parse |
| `internal/ast/queries/plsql.yaml` | Modified | `name_reject` on the three `calls` queries; `Trigger` removed from `target_rules.CALLS` |
| `internal/ast/shard_cache.go` | Modified | `shardCacheVersion` 8 → 9 |
| `internal/ast/name_reject_test.go` | Created | keyword is not a target; the grammar declares it; trigger is not a target; invalid regexp fails at load |
| `internal/ast/embedded_host_span_test.go` | Modified | a wrapped fragment parses, and a wrapper containing a line break is discarded |
| `docs/specs/ast_module.md` | Modified | new fields documented |
| `docs/specs/embedded_language_parsing.md` | Modified | section 3c, fragment |

## Trade-offs & Decisions

- **`name_reject` is per query, not a per-language list of reserved words.** Oracle's full
  reserved-word list has hundreds of entries and isn't what solves this: what solves it is
  stating, for that specific capture, what can never be a name. A per-language list would be
  more data to maintain and less precise — `end` can't be a routine, but it can be a column
  name.
- **Wrapper on the block, not on the language.** See above: it's a fact of position.
- **`dyn_sql` (`execute_immediate`) was left out.** The "name" there is an entire expression;
  filtering keywords doesn't solve the actual problem with that query, which is a different
  one.
- **No degraded-parse detection.** Implemented, measured, discarded — see cause 1.

## Technical Debt

- [ ] `dyn_sql` turns the expression of an `EXECUTE IMMEDIATE` into a CALLS target. The
  resulting name is the text of the expression, not a routine. It needs its own design:
  either it resolves dynamic SQL for real, or it stops being CALLS.
- [ ] A screen trigger body still gets truncated at the first `--` when line breaks come
  encoded (36% of bodies in a real corpus). The wrapper doesn't fix this — it's the
  line-break invariant, recorded in the project's memory.

## Progress Log

### 2026-08-20
- Per-target count on the live graph: 25 thousand edges to reserved words, 19045
  program-unit calls all keywords, 9092 to a trigger named `BEGIN`.
- Error-recovery hypothesis implemented, measured, and discarded; mechanism reverted.
- Real cause located in the grammar itself (`call_statement` + `non_reserved_keywords_pre12c`).
- `name_reject`, `Trigger` removed from CALLS, and `wrap_prefix`/`wrap_suffix` implemented,
  with a negative control in each test (remove the declaration, the test fails).
- Checked the other four ANTLR grammars: only PL/SQL has the permissive rule.
