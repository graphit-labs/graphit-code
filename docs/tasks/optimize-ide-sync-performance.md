---
title: Otimizar Performance do Sync IDE Adapter e IDE Rule
status: done
created: 2026-06-18
updated: 2026-06-18
tags: [performance, sync, ide-adapter, idempotency]
---

# Otimizar Performance do Sync: IDE Adapter e IDE Rule

## Objective

O `graphit sync` (e `graphit_sync` MCP) estava lento mesmo quando nada mudava no projeto.
Os passos "Updating IDE rules" e "Syncing IDE adapter" executavam I/O desnecessário (leitura+reescrita
de arquivos) em cada invocação, independentemente de qualquer mudança real.

## Implementation Details

### Problema 1 — `UpsertMandateTrigger` fazia cleanup legacy antes da idempotência

**Arquivo**: `internal/hub/adapters/ide/mandate.go`

**Antes**: A função sempre executava `cleanupLegacy(targetPath)` antes de verificar se o conteúdo
do trigger já estava correto. O `cleanupLegacy` lê e potencialmente reescreve o AGENTS.md 3 vezes
(uma por bloco legacy), e era chamado 5 vezes por sync (uma por módulo: knowledge, ast, hub, memory,
improvements).

**Depois**:
1. Lê o arquivo uma única vez no início
2. Verifica se há blocos legacy presentes (string search, sem I/O destrutivo)
3. Verifica se o conteúdo do trigger já está correto
4. Se não há legacy E conteúdo está correto → **return nil** (zero writes)
5. Só executa `cleanupLegacy` se necessário

**Funções adicionadas**:
- `readMandateContentFromString(content string) string` — extrai o inner do bloco mandato a partir
  de uma string já lida, evitando re-leitura do disco

**Fix no marcador legacy**: O formato de detecção estava errado (`START`/`END` no nome), corrigido
para o formato real do `HTMLBlockStyle`: `<!-- MARKER -->` / `<!-- END MARKER -->`.

### Problema 2 — `copyArtifact` folder-mode sempre fazia RemoveAll+copyDirAll

**Arquivo**: `internal/hub/adapters/ide/base.go`

**Antes**: Para artefatos do tipo `skill` (folder-mode), o código sempre fazia:
```go
_ = os.RemoveAll(dest)   // destrói o destino
return copyDirAll(...)   // recopia tudo
```
O `copyDirAll` usa `copyFile` com idempotência (compara size+content), mas como o `RemoveAll`
destruía o destino antes, a idempotência nunca funcionava.

**Depois**: Antes de `RemoveAll`, chama `dirContentsEqual(src, dst)`. Se os diretórios forem
idênticos (mesma árvore, mesmos tamanhos, mesmo conteúdo), retorna nil imediatamente.

**Função adicionada**:
- `dirContentsEqual(src, dst string) bool` — compara dois diretórios usando `filepath.Walk`:
  verifica que todos os arquivos do src existem no dst com mesmo tamanho e conteúdo, e que o dst
  não tem arquivos extras. Usa `filepath.SkipAll` para short-circuit em caso de divergência.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/hub/adapters/ide/mandate.go` | Modified | Idempotência antes de cleanup legacy; helper readMandateContentFromString |
| `internal/hub/adapters/ide/base.go` | Modified | dirContentsEqual para folder-mode; skip RemoveAll+copy quando idêntico |

## Key Decisions

- **Leitura única do arquivo**: Em vez de ler o arquivo múltiplas vezes (uma para cleanup legacy,
  uma para ler o mandato), agora lemos uma vez e passamos o conteúdo como string para os helpers.
- **String search para detecção de legacy**: Usar `strings.Contains` é O(n) mas apenas uma vez,
  vs. compilar regex e executar `RemoveBlockStyled` 3 vezes (que inclui leitura+escrita).
- **Contagem de arquivos para dst extra**: A verificação de igualdade de diretórios faz dois walks
  adicionais só para contar arquivos. Isso poderia ser combinado, mas a legibilidade é melhor assim
  e o custo é mínimo comparado ao RemoveAll+copy.

## Notes

- O warning `null character(s) preserved in literal` no build é do pacote de terceiros
  `go-tree-sitter/lua` — irrelevante e pré-existente.
- `filepath.SkipAll` (adicionado no Go 1.20) está disponível na versão usada pelo projeto.
- Os testes do pacote `internal/hub/adapters/ide` passam todos sem modificação.
