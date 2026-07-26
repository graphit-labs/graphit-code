# Expansão de abreviações: medição do teto e do caminho semântico

**Data:** 2026-07-26
**Escopo:** `internal/ast/abbrev_semantic_test.go` (só medição — nenhuma mudança de produção)
**Origem:** proposta do Engenheiro — usar a IA local para normalizar o nome em outro campo
(`coreCfg` → "core configuration") no lugar de trigramas

---

## O QUE foi medido

Duas rotas para alcançar um identificador abreviado a partir da palavra escrita por
extenso, porque o passe de trigrama deixa **exatamente um caso** aberto: `CFG_LOAD` não
compartilha trigrama com `config` e é inalcançável por qualquer método léxico.

### 1. Campo de expansão — teto medido: 9/9

`TestExpansionFieldCeiling` usa expansões escritas à mão (o que um gerador perfeito
emitiria) para medir o teto da ideia **sem depender de gerador nenhum**:

| probe | trigrama | campo expandido |
|---|---|---|
| `config` → coreConf, CONF_MGR, configLoader, initConfiguration | 4/4 | 4/4 |
| `conf` → idem | 4/4 | 4/4 |
| `config` → **CFG_LOAD** | **0/1** | **1/1** |
| total | 8/9 | **9/9** |

A expansão entra pela coluna de docstring, que serve de proxy para um campo dedicado:
é indexada pelo mesmo índice de prefixo, então o mecanismo exercitado (`config`
alcançando a palavra "configuration") é o que um `name_expanded` real usaria. As
docstrings estão vazias no resto do corpus, então todo acerto é atribuível à expansão.

**Conclusão:** a ideia funciona e compra exatamente o caso que falta. O que ela não
resolve é quem escreve o texto.

### 2. Caminho semântico — não medido, bloqueado

`TestSemanticReachOfAbbreviations` embeda os nomes e a consulta com o cliente real e
ranqueia por cosseno. **Pulou:** o embedder local não inicializa nesta máquina.

## Correção de premissa: o modelo local não gera texto

O modelo é **CodeRankEmbed** (ONNX int8, 768d) — um *embedder*. Ele mapeia texto para 768
floats; não existe inverso, logo ele **não pode** produzir "core configuration" a partir
de `coreCfg`. A rota literal da proposta exige um modelo **generativo**, que é dependência
nova: segundo modelo local, inferência sobre ~1M entidades na indexação, e risco de
expansão errada gravada no índice (`ENTRG` → ?) sem determinismo.

O que o embedder **pode** fazer é a rota 2, e ela **já está implementada**:
`buildEmbeddingText` já inclui o nome da entidade, e `HybridSearch` já funde BM25 com
busca vetorial por RRF. Não precisa de campo novo nem de modelo novo.

## Achado independente: embeddings quebrados desde 2026-07-22

O embedder local não inicializa:

```
The requested API version [26] is not available, only API versions [1, 25]
are supported in this build. Current ORT Version is: 1.25.0
```

- `go.mod` → `github.com/yalue/onnxruntime_go v1.31.0`, cujo header declara
  `#define ORT_API_VERSION 26` (exige ONNX Runtime ≥ 1.26).
- `Makefile:41` → `ORT_VERSION := 1.25.0`, inalterado desde `8b765f4b` (2026-05-26).
- O binding foi elevado em `10ce4503` (2026-07-22, "chore: upgrade dependencies and
  simplify lbug setup configuration") sem acompanhar o runtime.
- As três cópias de `libonnxruntime.so` na máquina são 1.25.0.

**Efeito:** `NewEmbeddingClientFromConfig` falha, o cliente fica nil, e `SemanticSearch`
devolve `nil, nil` — a metade semântica da busca degrada **silenciosamente** para
FTS-apenas. Isso também é o motivo de a rota 2 não ter podido ser medida.

Duas saídas, ambas decisão do Engenheiro por mexerem em runtime nativo empacotado para
três plataformas:
1. `ORT_VERSION := 1.26.x` no Makefile (implica novo download e revalidação do bundle
   linux/darwin/windows);
2. fixar `onnxruntime_go` numa versão que peça API ≤ 25.

## Recomendação

Antes de construir campo de expansão gerado por IA, medir a rota 2 — ela já existe e o
ganho pretendido pode já estar disponível. Para isso o mismatch de ORT precisa ser
resolvido primeiro. Se depois disso a rota semântica não alcançar `CFG_LOAD`, a rota 1 é
justificável, e aí a pergunta passa a ser a fonte da expansão: um **dicionário estático de
abreviações** (`cfg`→config, `pagto`→pagamento, `nft`→nota fiscal) é determinístico, sem
latência e provavelmente mais preciso em domínio PL/SQL em português do que um modelo
generalista — que não vai expandir `ABCD01` corretamente de forma alguma.

`TestSemanticReachOfAbbreviations` fica no repositório e passa a medir sozinho quando o
runtime for corrigido.
