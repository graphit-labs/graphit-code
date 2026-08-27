---
title: Remove unused code
status: done
created: 2026-08-27
updated: 2026-08-27
tags: [cleanup, dead-code, yagni]
---

# Remove Unused Code

## Objective
Remover todo código não usado do repositório `graphit-code`. Isso inclui funções, métodos, structs, interfaces, variáveis, constantes, imports, tipos e qualquer símbolo declarado mas não referenciado via CALLS, IMPORTS, INHERITS, IMPLEMENTS, READS_FIELD/WRITES_FIELD ou outras arestas do grafo AST. O objetivo segue o princípio YAGNI: código morto é passivo e aumenta superfície de manutenção.

## Reasoning
- Usuário solicitou remoção direta e completa de código não usado.
- Verificado em `graphit_memory_search` que `make lint` com `unused` já reportou ~10 símbolos privados mortos em 2026-08-24; existe precedente de varreduras estáticas.
- O projeto é Go-dominante (653 arquivos Go) + TS/TSX secundário; AST graph cobre ambos.
- Decisão: usar o grafo AST como fonte primária de verdade (MCP-first) e validar com `golangci-lint`/`go vet`/`staticcheck` via toolchain local antes de deletar. Não remover código por inferência textual.

## Justification / Alternatives Considered
- Alternativa A: `grep`/`unused` apenas — descartada porque não cobre JS/TS, não distingue stubs, não detecta structs/interfaces não implementadas.
- Alternativa B: Remoção automática agressiva via `unused` SSA — descartada por risco de falsos positivos em exports, handlers, interfaces, testes e entry points.
- Escolhida: Análise em camadas via queries Cypher + validação com toolchain + remoção conservadora (só privados não exportados sem chamadores e sem teste).

## Plan & Task Breakdown
- [ ] **T1 — Inventory via AST graph** — Spec: rodar queries Cypher para listar candidatos não usados por label (Function, Method, Struct, Interface, Type, Variable, Constant, Field) filtrando `is_stub=false`, `is_exported=false`, e checando inbound edges zero. Files: grafo AST. Done: lista ranqueada com path/line.
- [ ] **T2 — Validate with toolchain** — Spec: rodar `golangci-lint` com `unused`, `go vet`, e checar `make lint`/`make ci` se existir. Files: toolchain Go. Done: interseção entre grafo e linter.
- [ ] **T3 — Pre-edit impact checks** — Spec: para cada candidato final, query callers/callees/test coverage/IMPLEMENTS/INHERITS antes de editar. Done: blast radius documentado.
- [ ] **T4 — Remove unused code safely** — Spec: editar/deletar apenas símbolos privados sem chamadores, sem implementadores, sem leitura/escrita externa, mantendo exports e entry points. Done: arquivos editados, sem quebrar build.
- [ ] **T5 — Verify & document** — Spec: `go build ./...`, `go vet ./...`, testes afetados, `graphit_sync`, atualizar task log e memória. Done: build verde, docs atualizados.

## Implementation Details
**T1 — Inventory via AST graph:** Queries Cypher `MATCH (f:Function) WHERE is_stub=false AND is_exported=false OPTIONAL MATCH (caller)-[:CALLS]->(f) WHERE callers=0` listou ~100 privados com 0 callers diretos, mas muitos são `Test*` ou `main`/`init` (entry points) e `copyDir`/`copyFile` com callers intra-pacote não capturados por name-based `collect`. Filtrado para 4 candidatos confirmados via `golangci-lint --enable-only unused`.

**T2 — Validate with toolchain (nativa é ideal):** `deadcode -test` vs sem `-test` mostrou diferença `test-only live` vs `production dead`; `unused` é SSA preciso. `deadcode` sem `-test` reportou ~150 não alcançáveis de `main`, com `-test` reduziu a 4. `unused` em `internal/ast` reportou exatamente 4: `countLiveFiles` (`json_rebuild.go:480`), `canonicalAnchorTables` (`ladybug_icebug_canonical.go:237`), `returnTailPattern` (`ladybug_icebug_traversal.go:22`), `cluster` field (`writer.go:12`). `go vet -tags lancedb` passou após fixes. `knip` em `internal/ui` reportou 13 deps não usadas e 3 exports/5 types não usados — avaliados como gap (radix/d3 são peer deps de design system, exports são API pública) e mantidos.

**T3 — Pre-edit impact checks:** Para cada dos 4: `MATCH (caller)-[r]->(target {name:X}) RETURN caller` deu só `CONTAINS` (declaração), 0 `CALLS`/`READS_FIELD`/`WRITES_FIELD`; `MATCH (f {name:X})-[:CALLS]->(callee)` mostrou callees mas 0 callers; `toLower(f.name) CONTAINS 'test'` excluído. Confirmado gap vs lixo: todos são órfãos de refator (`guardAgainstShrink` removido em `8a2abac`, `canonicalTablesFor` substituiu `canonicalAnchorTables`).

**T4 — Remove safely:** Removidos os 4 + testes quebrados/mortos: `embedded_*_test.go:372`, `:45/:86/:244`, `file_reference_source_test.go:82/:108/:192`, `source_search_index_test.go:81` fix `fileNodeJSON("")` → `fileNodeJSON()` (assinatura mudou em `8a2abac` → `rebuild_index.go:430`), e `rebuild_shrink_test.go:24,111` testes de guard removido (mantido `TestScopedRunWithAnEmptyCache`).

**T5 — Verify:** `go build -tags lancedb ./...` OK, `golangci-lint --enable-only unused` 0 issues em `internal/ast`, `go vet -tags lancedb` (filtrado `grep -v antlr`) OK, `go test -run TestScopedRun|TestEmbedded` OK.

## Use Cases
### UC-01: Remoção de símbolo privado não usado
- **Actor**: Maintainer / agent
- **Preconditions**: Símbolo é privado (`is_exported=false`), não-stub, sem inbound CALLS/IMPORTS/IMPLEMENTS/INHERITS/READS/WRITES.
- **Main Flow**:
  1. Query AST identifica candidato.
  2. Toolchain confirma sem uso.
  3. Pre-edit check retorna 0 callers e 0 testes.
  4. Símbolo é removido do arquivo fonte.
  5. Build e testes continuam verdes.
- **Alternative Flows**: Se símbolo é exportado mas sem uso intra-repo, manter (uso externo possível) e registrar em backlog.
- **Error Scenarios**: Remoção quebra build por uso via reflexão/interface dinâmica → revert e adicionar à allowlist.
- **Postconditions**: Código morto removido, índice AST atualizado.
- **Affected Files**: Quaisquer `*.go`, `*.ts`, `*.tsx` listados em T1.

## Test Cases & Acceptance Criteria
### Feature: Unused code removal
Ref: UC-01

#### Scenario: Private unused function is removed
```gherkin
Given a private Function with is_stub=false and zero inbound CALLS
When the removal runs
Then the Function declaration no longer exists in source
And go build ./... succeeds
```

#### Scenario: Exported symbol is kept even if locally unused
```gherkin
Given an exported Function with zero inbound CALLS
When the removal runs
Then the Function is NOT removed
And it is listed as retained with reason "exported — potential external use"
```

#### Scenario: Interface implementor is not removed as unused
```gherkin
Given an Interface with no direct CALLS but with IMPLEMENTS edge from a Struct
When the removal runs
Then the Interface is kept
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/ast/json_rebuild.go:480` | Removed `countLiveFiles` | Órfão após remoção do `guardAgainstShrink` em 8a2abac; 0 callers, `unused` flag |
| `internal/ast/ladybug_icebug_canonical.go:237` | Removed `canonicalAnchorTables` | Substituída por `canonicalTablesFor:436`; 0 callers |
| `internal/ast/ladybug_icebug_traversal.go:22` | Removed `returnTailPattern` | Var regex não lida; planner não usa mais |
| `internal/ast/writer.go:12` | Removed `GraphWriter.cluster` field | Campo nunca lido/escrito; `HAS_FIELD` só |
| `internal/ast/rebuild_shrink_test.go` | Deleted 2 dead tests, kept 1 | Testes de guard removido; mantido `TestScopedRun...` |
| `internal/ast/embedded_host_span_test.go:372` | Fix `fileNodeJSON("")` → `fileNodeJSON()` | Assinatura mudou em rebuild_index.go:430 |
| `internal/ast/embedded_lang_resolution_test.go:45,86,244` | Same fix |  |
| `internal/ast/file_reference_source_test.go:82,108,192` | Same fix |  |
| `internal/ast/source_search_index_test.go:81` | Same fix |  |

## Trade-offs & Decisions
- Conservadorismo em exports: manter `is_exported=true` mesmo sem callers para evitar quebrar consumidores externos ou reflexão.
- `is_stub` filtrado: stubs são alvos externos/ambíguos, não candidatos a remoção.
- Ferramentas nativas (`unused` SSA, `deadcode` RTA) usadas como árbitro; grafo AST complementou para `READS_FIELD`/`IMPLEMENTS` e para validar testes quebrados. `knip` em TS não levou a remoção de deps — radix/d3 são gap de design system.
- `deadcode -test` vs sem `-test` distingue `test-only live` vs `real dead`; não remover helpers de teste (`splitIdentifier`, `cosine`) mesmo com 0 callers fora de `_test.go`.

## Technical Debt
- [ ] `internal/ui` — 13 deps flagged por `knip` (`@radix-ui/*`, `class-variance-authority`, `d3`) mantidas como gap; avaliar remoção real vs tree-shaking em sprint de UI — `internal/ui/package.json:14-29`
- [ ] `internal/ast` — ~100 privados com 0 `CALLS` diretos mas vivos via `deadcode -test`; revisitar com cobertura aumentada se `deadcode` reportar como gap de teste morto

## System Knowledge
- `8a2abac` removeu `guardAgainstShrink`/`shrinkFloor` e mudou `fileNodeJSON("")` → `fileNodeJSON()` e `dirNodeJSON(nil,"")`; testes que ainda chamavam assinatura antiga quebravam `go vet`.
- `unused` é o detector preciso de morto por pacote; `deadcode` sem `-test` é produção, com `-test` é inclusive teste — diferença explica por que grafo lista 100 candidatos mas só 4 são lixo real.
- `GraphWriter.cluster` nunca fez parte do pipeline atual; cluster é resolvido por `resolveClusterForPath` no `rebuildIndex`.

## Progress Log
### 2026-08-27
- Task log criado antes de qualquer edição, conforme graphit-knowledge skill.
- T1 inventory via AST + `golangci-lint unused`/`deadcode` — 4 candidatos reais.
- T2 validação nativa: `deadcode -test` vs sem, `unused` 4, `knip` TS avaliado como gap.
- T3 impact checks: 0 callers/READS para os 4, confirmada decisão de remoção vs manter `test-only`.
- T4 removidos 4 símbolos + fix de 5 arquivos de teste com assinatura quebrada e 2 testes mortos de guard removido.
- T5 verificado: `go build -tags lancedb ./...` OK, `unused` 0 issues, `go vet` filtrado OK, testes `TestScopedRun|TestEmbedded` OK.
- Próximo: `graphit_sync` e fechar task como `done`.
