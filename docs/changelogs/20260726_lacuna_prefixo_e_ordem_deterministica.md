# Lacuna do índice de prefixo medida + ordem de busca determinística

**Data:** 2026-07-26
**Escopo:** `internal/ast/prefix_gap_test.go` (novo), `internal/ast/ladybug_llm_test.go` (novo),
`internal/ast/search_determinism_test.go` (novo), `internal/ast/fts_sqlite.go` (correção)
**Origem:** instrução do Engenheiro — medir a lacuna de prefixo antes de implementar;
e pergunta sobre LLM local embutido no LadybugDB

---

## 1. LadybugDB não traz LLM local embutido

`TestLadybugLLMExtension`. A extensão `llm` carrega e expõe `CREATE_EMBEDDING`, mas como
função **escalar** (não table function) e com provedor **obrigatório**:

```
Expected: (STRING,STRING,STRING) -> LIST      # (texto, provedor, modelo)
```

- `'open-ai'` → `Could not read environmental variable: OPENAI_API_KEY` (provedor hospedado)
- `'local'` → `Provider not found: local` (**não existe provedor embutido**)
- `'ollama'` → **aceito**, devolveu vetor real

O caso `ollama` funcionou porque **esta máquina tem Ollama rodando** (`ollama serve`, PID
4989, com `nomic-embed-text` carregado) — daemon externo chamado por HTTP em
`localhost:11434`, não modelo embutido. Em ambiente sem Ollama o mesmo probe falha.

Consequência: trocar o ONNX in-process por Ollama-via-Ladybug trocaria uma dependência
embarcada por um daemon externo instalado pelo usuário, num binário que hoje funciona
standalone e offline. Além disso o CodeRankEmbed é específico para código e mediu bem
(separação de 4,4×), enquanto `nomic-embed-text` é de propósito geral. **O ganho da
consolidação continua sendo o índice vetorial, não a extensão LLM.**

## 2. Lacuna de prefixo: estreita e mitigável

`TestPrefixIndexGap` compara o índice SQLite em produção contra um **protótipo do desenho
proposto** para o Ladybug (índice FTS por campo + identificador dividido + saco de
trigramas, fundidos por RRF), sobre o mesmo corpus e as mesmas sondas.

Resultado final, **11 sondas de consulta truncada**:

| | top-1 esperado | vazios |
|---|---|---|
| SQLite (produção) | 9/11 | 0 |
| Ladybug (protótipo) | **11/11** | 0 |

A lacuna real ficou localizada com precisão: **somente consultas com menos de 3
caracteres**. `cf` → `CFG_LOAD` retornava vazio no Ladybug, porque uma consulta menor que
um trigrama não produz gram nenhum e não existe operador de wildcard. De 3 caracteres para
cima o saco de trigramas cobre tudo o que o índice de prefixo cobria.

Mitigação medida (não hipotética): fallback `CONTAINS` para consultas sub-trigrama, que
fecha o caso (11/11). É varredura, mas só dispara para 1–2 caracteres e com contagem de
linhas limitada. Disponibilidade de `CONTAINS` já estava provada em
`TestLadybugFTSFeatureParity`.

### Dois confundimentos corrigidos no caminho

- **Arquivo contra entidade.** A primeira versão media 9/11 Ladybug × **5/11** SQLite. Era
  viés: o índice SQLite também indexa arquivos (`file_fts`) e o protótipo não, então
  `retry.go`, `conf_mgr.sql`, `cfg_load.sql` e `db.go` apareciam em 1º e eram contados como
  erro — medindo ranking de arquivo × entidade, não índice de prefixo. Corrigido com
  `sqliteEntitySearch`, que filtra resultados de arquivo. Com isso o SQLite sobe para 9/11.
- **Sonda indefensável.** A sonda `data` → `connectDatabase` não tem resposta certa:
  "data" é substring de `Database` tanto em `connectDatabase` quanto em `closeDatabase`.
  Substituída por `connect` → `connectDatabase`.

## 3. Defeito de produção encontrado: ordem de busca não determinística

Medindo o item 2, `valid` no SQLite oscilou entre execuções: 3/5 `validateSchema`, 2/5
`PKG_VALIDACAO_PAGAMENTO`. Mesmo corpus, mesma consulta, top-1 diferente.

**Diagnóstico (o primeiro estava errado).** A hipótese inicial foi iteração do mapa
`docScores` com `sort.Slice` instável no fim. Teste com 25 chamadas no mesmo índice
**passou** — dentro de um processo a ordem é estável. A causa é o **build**:
`RebuildFromCache` insere iterando `cache.AllEntries()`, que é mapa, então os rowids do
FTS5 mudam a cada build; o FTS5 desempata BM25 igual por rowid, o que altera a **posição de
rank** de cada passe — e o RRF pontua por posição, então os próprios scores mudam. Por isso
desempate apenas na lista fundida não resolveria.

**Correção.** `sortResultsDeterministic` — ordem total por relevância decrescente com
desempate por `deduplicationKey` (path+nome+linha, único por documento) — aplicada em
quatro pontos: saída de `queryFTS`, saída de `queryTrigram`, e o sort final de `Search` e
`HybridSearch`. Aplicar por passe é o essencial, porque é a posição no passe que alimenta o
RRF.

**Limite declarado, não corrigido:** *quais* linhas empatadas caem dentro do `LIMIT` de um
passe continua decidido pelo SQLite. Um empate exatamente na borda da janela de busca ainda
pode variar. Forçar isso exigiria `ORDER BY` secundário em SQL, abrindo mão do caminho
top-N do FTS5 em toda consulta para estabilizar a linha menos significativa de uma janela
super-dimensionada.

**Guarda.** `TestSearchOrderIsDeterministic` e `TestHybridSearchOrderIsDeterministic`
constroem o mesmo corpus 8 vezes e exigem ordem idêntica. A versão "25 chamadas no mesmo
índice" foi descartada por passar por vácuo — está registrado no comentário para não
voltar.

## Estado

Suíte completa verde com `-count=1` (sem cache), ORT 1.26.0 e embedder ativo:

```
ok  internal/ai        105.769s
ok  internal/ast        15.180s
ok  internal/fswatch     0.518s
ok  internal/daemon      6.165s
ok  internal/sysutil     0.003s
```

`go build -tags fts5 ./...` e `go vet` limpos.

Com isso, a última incógnita listada em `20260726_vetor_nativo_ladybug_medido.md` (a lacuna
do índice de prefixo) está medida e mitigada. Nada mais bloqueia escrever o passo 1 da
migração cobrindo FTS e vetor juntos.
