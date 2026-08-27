---
Title: Key Word is Not Called, Trigger Cannot Be Accessed, and Fragment Requires Wrapping
status: done
created: 2026-08-20
updated: 2026-08-20
tags: [ast, plsql, grammar, calls, embedded]
---

The keyword is not called, the trigger cannot be invoked, and the fragment requires a wrapper.

## Objective

Depois que o label do chamador deixou de vir de uma lista fixa (commit e928bf5), as
The edges of the display units, flow, and report areas have been written - and
The counting by target showed that **most of it was false**. This work is the actual correction.
Of course, here is the idiomatic English translation:

"Any or no name can be a capture in Go without listing any words."
Fact of Language, then who declares it is grammar.

## O que estava errado, medido no grafo vivo de um consumidor

Symptom | Number
|---|---|
| Alvos de chamada que eram palavra reservada | `PROCEDURE` 16367, `IF` 2567, `DECLARE` 2050, `FUNCTION` 1021, `begin` 976, `procedure` 750, `.` 818 |
| Chamadas de `FormProgramUnit` | 19045, **todas** palavra-chave |
| Arestas para um gatilho de banco chamado `BEGIN` | 9092 |
| Chamadas que resolviam para procedure real | ~40 nomes distintos |

The three causes, each with its declarative corrective statement

Brazilian Portuguese:
The inline 0 accepts identifier NU – and the list of non-reserved words has 1,753 entries.

`PlSqlParser.g4:5881`: `call_statement : CALL? routine_name function_argument? …` — os DOIS
Optional items can be missing, so a bare identifier in position of statement J is already an invocation
completa. E `routine_name → identifier → regular_id → non_reserved_keywords_pre12c`, lista
which contains `BEGIN`, `DECLARE`, `FUNCTION`, `IF`, `PROCEDURE`, and `RETURN` (confirmed: 1753)
palavras).

HYPOTHESIS TESTED AND DISPROVEN: that it came from a grammarly error recovery. I arrived
Implementing detection (marking subtree coming from recovery) and a policy of
linguagem (`on_parse_error: keep | skip_degraded | drop`), medi, e o resultado com e sem
The discard was identical — the parsing is clean. Reverted to its original state: mechanism that the measurement did not alter.
It sustains debt, not a solution. It is registered here so no one repeats the experiment.

Correction: Inline 0 (Inline 1) — Regular expression that names it
Caught, cannot marry; married, the match does not register anything. Declared in all three query windows.
`calls` do `plsql.yaml`, com as palavras que nunca nomeiam rotina em PL/SQL mais
Here is the translation:

The inline identifier not mentioned starts with a letter, so the `.` variable is not a name.
Secured at both ends for stability: `if` without a mooring would refuse to `if_valida_cliente`.

The trigger is not callable, and it was declared as an actionable target.

The ``target_rules.CALLS`` of the ``plsql.yaml`` list was being listed. In PL/SQL, a trigger is fired.
por DML na tabela dele; nenhum programa o invoca pelo nome. Com ele no conjunto, todo
Captured as called, it resolves in the trigger whose name in the database is the identifier.
Cited `"BEGIN"` — formally correct but semantically empty and **not stub**, which
a fazia sobreviver a todo filtro que descarta stub.

Correction: `CALLS: { labels: [Function, Procedure, Package] }`. Measured: 9,092 edges
Falses, except for one true, lost forever (the only trigger ever "called" was this).

The body of the program unit is a fragment, and the fragment cannot be parsed.

``PROCEDURE x(…) IS … END;`` is a declaration that is valid only in a declarative section. Unrestricted:

| Corpo | Resultado |
|---|---|
| `PROCEDURE CGFK$CHK(p IN BOOLEAN) IS BEGIN pck_pedido.pr_grava(1); END;` | zero entidade, zero chamada |
| o mesmo com `CREATE OR REPLACE` na frente | `Procedure:CGFK$CHK`, chamada `pr_grava` |
| o mesmo dentro de `DECLARE … BEGIN NULL; END;` | idem |
Block Anonymous `BEGIN … END;` | Call `pr_grava`

Ou seja: as 19045 chamadas de `FormProgramUnit` eram palavra-chave porque o corpo inteiro
It was invisible—only the first word remained, as in "call."

Correction: `EmbeddedBlock.WrapPrefix` / `WrapSuffix` (_`wrap_prefix`_, _`wrap_suffix`_) — the
A segment that a piece needs to be around in order to become a compilation unit is declared on
BLOCK, not in language: which envelope serves is a fact of POSITION (the same PL/SQL in INLINE_0)
Arrives with `CREATE OR REPLACE` and doesn't need anything. Neither side can contain.
quebra de linha — descartado no load, mesma invariante do `text_normalizers`.

Measured in the same screen file before → after packaging: **34 → 50 calls,
11 → 17 SELECTS, 101 → 129 entidades PL/SQL**. Num arquivo de 1,5 MB: 106 → 144 chamadas,
55 → 68 SELECTS, 333 → 609 entidades.

Why do other SQL grammars not need `name_reject`

Confirmed in grammar, not presumed:

| Linguagem | Regra de chamada | Identificador nu casa? |
|---|---|---|
| PL/SQL | `CALL? routine_name function_argument?` | **sim** |
PostgreSQL: `callstmt : CALL func_application` is not supported.
DB2: Not inline
No queries anchored in INLINE_0 or INLINE_1 are present.
COBOL 85 | Inline 0, Inline 1, Inline 2 | Not

PL/SQL is the outlier case, hence why the declaration is confined to it.

## Files Changed

| File | Change | Reason |
|---|---|---|
Here is the Portuguese text translated into idiomatic English:

| `internal/ast/query_loader.go` | Modified | `NameReject` in the query, `WrapPrefix`/`WrapSuffix` within the block, validation and memo of the regular expression |

This translation maintains the original structure while providing a more natural-sounding English version.
| `internal/ast/treesitter_adapter.go` | Modified | `name_reject` aplicado no caminho tree-sitter |
| `internal/ast/antlr_adapter.go` | Modified | `name_reject` aplicado no caminho ANTLR |
"_`internal/ast/treesitter_embedded.go`_ | Modified | The outer envelope of the fragment before the sub-parse"
Here is the translation:

| `internal/ast/queries/plsql.yaml` | Updated |
| `name_reject` in three queries of `calls`; outside `Trigger` |
| `internal/ast/shard_cache.go` | Modified | `shardCacheVersion` 8 → 9 |
Here is the translation:

"_`internal/ast/name_reject_test.go`_ | Created | The word key is not an aim; grammar declares; trigger is not an aim; invalid regular expression falls into the load."
The modified fragment is parsed, and the line-ending wrapper is discarded.
| `docs/specs/ast_module.md` | Modified | campos novos documentados |
In-line 0 | Modified | Section 3C, Fragment

## Trade-offs & Decisions

The `name_reject` is for queries, not a list of language-specific reserved words. The list
It's complete with Oracle's reserved words, hundreds of entries, and not what it solves: the problem.
The resolution is saying, in that capture, what cannot be named. A list by language.
It would be more inclined towards maintaining and less precise — Inline 0 cannot be routine, but it can.
  nome de coluna.
The outer layer should be in the block, not in the language. See above: it's a fact of position.
The expression inside is complete; it's a name.
Brazilian Portuguese to idiomatic English:

The keyword filter doesn't solve the problem that this query has, which is another one.
Nothing about degraded parse detection implementation, measurement, or discard - see cause 1.

## Technical Debt

- [ ] Inline 0 transforms the expression of an inline target into calls. The name
The outcome is the text of the expression, not a routine. It needs its own design: or resolves
The true dynamic SQL, or stop being calls.
- [ ] Corpo de trigger de tela continua truncando no primeiro `--` quando as quebras de
The line is encoded (36% of bodies in a real corpus). The envelope doesn't fix it —
It is the line break invariant registered in the project's memory.

## Progress Log

### 2026-08-20
- Contagem por alvo no grafo vivo: 25 mil arestas para palavra reservada, 19045 chamadas de
  program unit todas palavra-chave, 9092 para um gatilho chamado `BEGIN`.
Recovery error hypothesis implemented, measured, and discarded; mechanism reversed.
Real cause localized in the very grammar (`call_statement` + `non_reserved_keywords_pre12c`).
- `name_reject`, `Trigger` fora de CALLS e `wrap_prefix`/`wrap_suffix` implementados, com
negative control in each test (remove the statement, the test fails).
Conferred in the other four grammars of ANTLR that only PL/SQL has the permissive rule.
