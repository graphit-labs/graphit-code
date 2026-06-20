# fix: mandate canonical module ordering

**Data:** 2026-06-20
**Escopo:** `internal/hub/adapters/ide/mandate.go`, `internal/hub/adapters/ide/ide_test.go`

## Problema

`UpsertMandateTrigger` removia o trigger existente e o reappendava ao final do bloco do mandate. Isso fazia com que módulos adicionados pela primeira vez (ex: `imp_rule` via `graphit sync`) mudassem de posição, gerando diffs grandes mesmo sem mudança de conteúdo.

## Causa Raiz

```go
// antes — sempre appenda ao final
inner = inner + "\n" + wrapped
```

## Solução

Substituída a estratégia de append por reescrita completa ordenada:

1. `parseTriggers(inner)` — extrai todos os triggers existentes em um `map[string]string`
2. Atualiza o trigger alvo no mapa
3. `assembleTriggers(triggers)` — reescreve o inner seguindo `canonicalTriggerOrder`:
   `mem_rule → ast_rule → hub_rule → doc_rule → imp_rule`
   Triggers desconhecidos (hub-installed) ficam após os canônicos, ordenados alfabeticamente

## Resultado

- `graphit sync` num projeto já sincronizado → diff zero
- `graphit sync` com novo módulo → módulo aparece na posição canônica, não ao final
- Ordem de chamada dos `InstallRule` não importa mais

## Arquivos Modificados

- `internal/hub/adapters/ide/mandate.go`: adicionado `canonicalTriggerOrder`, `parseTriggers`, `assembleTriggers`; `UpsertMandateTrigger` reescrito
- `internal/hub/adapters/ide/ide_test.go`: 4 novos testes (`TestParseTriggers`, `TestAssembleTriggers_CanonicalOrder`, `TestAssembleTriggers_UnknownTagsSorted`, `TestUpsertMandateTrigger_CanonicalOrdering`, `TestUpsertMandateTrigger_Idempotent`)
