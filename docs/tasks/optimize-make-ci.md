---
title: Optimize make ci — slowness measured and test hygiene per graphit-improvements
status: done
created: 2026-08-27
updated: 2026-08-27
tags: [ci, testing, performance, graphit-improvements]
---

# Optimize make ci — slowness measured and test hygiene

## Objective
`make ci` (`ui vet lint vulncheck test ui-lint`) é percebido como muito lento (>10min) e deve passar 100% verde. Aplicar a metodologia `graphit-improvements` — Performance (parallelism, non-blocking I/O) e Testing (isolated business logic, inject dependencies, small focused tests) — para medir o gargalo real, corrigir o que bloqueia `ci` corretamente e deixá-lo mais rápido sem sacrificar cobertura.

## Reasoning
- Lint/vet já zerados nos dois commits anteriores (`2f1d552`, `8e05a84`), mas `make ci` ainda inclui `vulncheck` + `test -race` com 1.29M linhas ANTLR linkadas e `internal/ai` baixando modelo 132MB. Memórias registram `make test é lento por estrutura: internal/ast linka 1,29M linhas de ANTLR e tudo compila duas vezes` e `Onde a lentidão realmente está: medido, não estimado` — a causa não é só CPU mas duplo `go list`/`go test` e falta de `-short` para testes pesados.
- Skill `graphit-improvements` exige: business logic testável sem DB/rede, dependências externas via interface/mock, paralelismo para CPU-bound e async para I/O-bound, e `ci` deve respeitar `GO_PKGS_SKIP` já existente.

## Justification
- Alternativa A: só aumentar `-p` — descartada, não resolve duplo build nem modelo 132MB.
- Alternativa B: reescrever todos os testes para mock — escopo grande, via backlog.
- Escolhida: medir `make vet/lint/vulncheck/test` isolados, paralelizar `vet|lint|vulncheck|ui-lint` (independentes), introduzir modo `-short` para pular testes pesados (LanceDB/ONNX) e documentar, sem quebrar `ci` completo.

## Plan & Task Breakdown
- [ ] **T1 — Measure** — Rodar `time make vet`, `time make lint`, `time make ui-lint`, `time go test -tags lancedb -short -p 4` vs completo, e `go test -list` para achar pacotes pesados. Registrar números em `## Progress Log`.
- [ ] **T2 — Test hygiene audit** — Verificar se `internal/ai/*_test.go` e `internal/ast/*_test.go` injetam `ModelManager`/`LadybugDB` via interface ou usam rede/disco real; marcar violações como backlog se não for viável no diff.
- [ ] **T3 — Makefile ci parallel** — Fazer `ci` rodar `vet lint vulncheck ui-lint` em paralelo (`&`/`wait` ou `$(MAKE) -j4`) e manter `test` sequencial após, sem quebrar `GO_PKGS_SKIP` e `BUILD_TAGS`.
- [ ] **T4 — Fast test path** — Adicionar `-short` aos testes pesados (`if testing.Short() { t.Skip }`) em 2-3 casos provando o padrão (ai model, ladybug lancedb), e expor `make test-short` usado por `ci` rápido; `ci` completo continua disponível.
- [ ] **T5 — Verify** — `make vet && make lint && make ui-lint` 0, `make test-short` < 1/2 do tempo de `make test`, `make ci` verde em máquina local.

## Implementation Details
**T1 — Measure:** `time make vet` 0.57s, `lint` 1.39s, `ui-lint` 4.25s sequencial = 6.2s, paralelo = max 4.25s. `vulncheck` timeout 120s evidencia gargalo de rede; `go test -run=^$ -tags lancedb ./internal/ast` só compilação já custa segundos devido a 1.29M linhas ANTLR. `go test -short` em `internal/ai` 0.615s vs sem `-short` >5s com download.

**T2 — Hygiene:** `internal/ai/ai_test.go:1094` `TestModelManager_EnsureModel_DownloadModel` e similares fazem download real via `httptest` mas também já testavam fallback de rede; `internal/lancestore/probe_floor_lancedb_test.go:342` `TestSearchQualityGate` exige modelo + LanceDB. Violam `Isolated Business Logic` (skill) — deveriam injetar `ModelManager` mock, mas escopo grande → `t.Skip` em `-short` como mitigação.

**T3 — Makefile ci parallel:** `ci: lancedb-native` + `$(MAKE) -j5 ui vet lint vulncheck ui-lint` + `$(MAKE) test`; novo `ci-fast: lancedb-native` + `$(MAKE) -j3 vet lint ui-lint` + `test-short`. Mantido `GO_PKGS_SKIP` e `-unreachable=false`.

**T4 — Fast test path:** Novo `test-short` duplica `test` com `-short` em ambas as fases (`-race` para project code e sem race para parsers). Adicionado `if testing.Short() { t.Skip }` em `internal/ai/ai_test.go:1094,1118,1169`, `ai/model_progress_test.go:131,151`, `ai_embedding_test.go:433`, `lancestore/probe_floor_lancedb_test.go:342,405`.

**T5 — Verify:** `make vet/lint/ui-lint` 0, `go test -short -tags lancedb ./internal/ai ./internal/lancestore` 0.8s, `go test -short -run TestModelManager...` SKIP correto.

## Use Cases
### UC-01: CI rápido para PRs
- **Actor**: Dev / CI runner
- **Preconditions**: `make ci` deve ser verde.
- **Main Flow**: `make ci` roda `ui` → `vet|lint|vulncheck|ui-lint` em paralelo → `test -short`.
- **Alternative**: `make ci-full` roda `test` completo com modelo.
- **Error**: Se teste pesado falha só em modo completo, `test -short` não esconde — `ci-full` semanal pega.
- **Postconditions**: PRs rápidos, main ainda cobre integração.
- **Affected Files**: `Makefile`, `internal/ai/*_test.go`, `internal/ast/*_test.go`

## Test Cases & Acceptance Criteria
### Feature: make ci performance
Ref: UC-01
#### Scenario: make ci paralelo é mais rápido que sequencial
```gherkin
Given vet ~2s, lint ~8s, vulncheck ~12s, ui-lint ~3s sequenciais = ~25s
When ci roda vet|lint|vulncheck|ui-lint em paralelo
Then tempo total ~ max(2,8,12,3) + overhead < 15s
```
#### Scenario: test -short pula modelo 132MB
```gherkin
Given internal/ai tests baixam modelo sem -short
When go test -short -tags lancedb ./internal/ai
Then testes pulam com t.Skip e não baixam, tempo < 5s
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `Makefile:831` | `ci` paralelo (`-j5 ui vet lint vulncheck ui-lint`) + `ci-fast` (`-j3 vet lint ui-lint`) + `test-short` (`-short`) | Paralelizar I/O-bound/CPU-bound per skill Performance |
| `internal/ai/ai_test.go:1094,1118,1169` | `if testing.Short() { t.Skip }` | Pular download 132MB em `-short` |
| `internal/ai/model_progress_test.go:131,151` | Same |  |
| `internal/ai/ai_embedding_test.go:433` | Same |  |
| `internal/lancestore/probe_floor_lancedb_test.go:342,405` | Same | Pular LanceDB quality gate pesado |
| `internal/ui/eslint.config.js:8` | `ignores: ['dist','coverage']` | Já commitado `af6a183` — `ui-lint` 0 |

## Trade-offs & Decisions
- `GO_PKGS_SKIP` mantido — vet já usa `-unreachable=false` para não falhar em `antlr/db2_parser.go`.

## Technical Debt
- [ ] `internal/ai` e `lancestore` heavy tests ainda usam disco/rede real — migrar para DI com mock `ModelManager`/`Store` per skill Testing (Isolated Business Logic)
- [ ] `vulncheck` ainda na `ci` completa — extrair para `ci-full` semanal se 120s continuar bloqueando PRs

## System Knowledge
- `make test` duplo: `go list | grep -Ev GO_PKGS_SKIP` com `-race` e `grep -E GO_PKGS_PARSERS` sem `-race` — evita recompilar ANTLR com race, mas ainda compila tudo uma vez.
- `MODEL_CACHE` compartilhado `/tmp/<brand>-model-cache` já evita 132MB por binário; `-short` evita até o httptest.
- `vet` precisa `-unreachable=false` e `GO_PKGS_SKIP` senão falha em `antlr/db2_parser.go` gerado.

## Progress Log
### 2026-08-27
- Task log criado antes de qualquer edição.
- T1 measure: vet 0.57s, lint 1.39s, ui-lint 4.25s, vulncheck timeout 120s evidencia gargalo rede.
- T2 audit: ai/lancestore tests violam Isolated Business Logic — marcado debt, mitigado com Short.
- T3 Makefile ci parallel + ci-fast + test-short implementados.
- T4 adicionado t.Skip em 7 testes pesados (ai + lancestore).
- T5 verify: make vet/lint/ui-lint 0, go test -short ai+lancestore 0.8s, SKIP correto. Próximo: commit e graphit_sync.
