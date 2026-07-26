# ORT 1.26 + rota semântica medida: campo de expansão descartado

**Data:** 2026-07-26
**Escopo:** `Makefile`, `.github/workflows/release.yml`, `internal/ai/ai_test.go`,
`internal/ast/abbrev_semantic_test.go`
**Origem:** execução das três recomendações do changelog anterior

---

## 1. Mismatch de ONNX Runtime corrigido

`ORT_VERSION` 1.25.0 → **1.26.0** no `Makefile`, mais as três chaves de cache do
`release.yml` (linux, darwin-arm64, windows). 1.26.0 é exatamente a `ORT_API_VERSION 26`
que `onnxruntime_go v1.31.0` declara — nenhum salto para 1.27/1.28 sem motivo.

Comentário adicionado no `Makefile` amarrando as duas versões, porque foi justamente a
falta desse vínculo que produziu o bug: o binding subiu em `10ce4503` (2026-07-22) e o
runtime ficou para trás, deixando o embedder nil e degradando a busca semântica para
FTS-apenas **em silêncio**.

Verificado localmente: `make fetch-ort-linux` baixa 1.26.0 e o embedder passa a
inicializar (`CodeRankEmbed-137M-INT8`).

**Não verificado:** darwin-arm64 e windows-x64. Só as chaves de cache e a variável
mudaram — o download é o mesmo mecanismo — mas nenhuma das duas plataformas foi executada.

## 2. Rota semântica medida — e ela cobre a lacuna

`TestSemanticReachOfAbbreviations`, embeddings **só do nome** (sem prosa, para não
repetir o confundimento de docstring):

| identificador | `config` | `configuration` |
|---|---|---|
| **CFG_LOAD** | **0.3928 (1º/7)** | 0.3670 (4º/7) |
| CONF_MGR | 0.3813 | 0.4379 |
| configLoader | 0.3694 | 0.3341 |
| initConfiguration | 0.3487 | 0.5261 |
| coreConf | 0.3445 | 0.4022 |
| computeChecksum | 0.0789 | 0.0694 |
| PKG_ACCOUNT_UPDATE | 0.0701 | 0.0671 |

`CFG_LOAD` — o único identificador que **nenhum** método léxico alcança, por não
compartilhar trigrama com `config` — ranqueia **primeiro**. A separação entre o pior
relacionado (0.3445) e o melhor irrelevante (0.0789) é de 4,4×.

## 3. Campo de expansão: descartado, com fundamento

O ganho marginal medido do campo de expansão era **1/9** (só o `CFG_LOAD`;
`TestExpansionFieldCeiling`). Esse mesmo caso é coberto pela busca semântica **que já
existe** — sem campo novo, sem modelo generativo, sem inferência sobre ~1M entidades na
indexação, sem expansão errada gravada no índice.

Nada foi construído. O dicionário estático de abreviações também não é necessário.

Cobertura final da lacuna de abreviação: **trigrama** (8/9, léxico e determinístico) +
**semântico** (o 9º), fundidos por RRF no `HybridSearch` que já está no lugar.

## 4. Dois testes que passavam pelo motivo errado

Corrigir o ORT deixou `internal/ai` vermelho, expondo dois testes que eram verdes por
acidente de ambiente:

- `TestLazyEmbeddingClient_InitError` — o comentário admitia depender de "não temos o
  modelo ONNX no ambiente de teste". Com o runtime funcionando, a init passa a ter
  sucesso e o teste falha. Ele nunca testou propagação de erro; testou ausência de
  modelo.
- `TestLazyEmbeddingClient_MultipleCalls` — fazia `lazy.err = errors.New(...)` para
  "simular" falha. Não funciona: `init()` roda `NewLocalEmbeddingClient` dentro de
  `once.Do` e **atribui por cima** de `l.err`. A simulação nunca teve efeito; o teste
  passava porque a init real falhava de verdade.

Ambos agora injetam a falha via helper `failedLazyClient`, que consome o `sync.Once` —
único jeito de a injeção pegar — e **falha explicitamente** se a injeção não pegar,
para que não voltem a ficar verdes por vácuo.

`TestSemanticReachOfAbbreviations` também deixou de pular em silêncio: erro contendo
"API version" agora é `Fatal` com a causa (mismatch Makefile × go.mod), enquanto ausência
de modelo continua `Skip`. Esse guarda teria pegado a regressão de 2026-07-22 no dia.
As medições semânticas passaram de log para asserção: separação relacionado/irrelevante e
`CFG_LOAD` na metade de cima.

## Estado de verificação

- `internal/ast` completo: verde (antes das mudanças em `internal/ai`, que não o afetam).
- `internal/ai` subconjunto `TestLazyEmbeddingClient`: verde.
- `TestSemanticReachOfAbbreviations`: verde com ORT 1.26.
- **Pendente:** suíte `internal/ai` completa (~5 min) e `go build ./...` após as mudanças
  desta rodada. A execução foi recusada; precisa rodar antes do commit.

Para rodar com o embedder ativo, o `LD_LIBRARY_PATH` precisa do ORT novo:

```bash
export LD_LIBRARY_PATH="$(go env GOPATH)/pkg/mod/github.com/\!ladybug\!d\!b/go-ladybug@v0.17.0/lib:/tmp/onnxruntime-cache/onnxruntime-linux-x64-1.26.0/lib"
```
