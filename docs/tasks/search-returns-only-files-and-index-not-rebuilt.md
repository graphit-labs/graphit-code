---
Title: "Search returns only files, and INLINE 0 does not reconstruct an empty index"
status: done
created: 2026-08-24
updated: 2026-08-24
tags: [ast, search, lancedb, ranking, bm25, rrf, index, cli]
---

The search returns only files, and **INLINE 0** does not reconstruct an empty index.

Continue with `docs/tasks/hub-em-s3-icebug-e-lancedb.md` (T15 closed in `a7c0ac3`). The prompt for
Origin is at `docs/tasks/backlog/PROMPT-busca-devolve-so-arquivos.md` (commit `abef386`).

## Objective

Two defects were found running on `make install` against the store.
This project's real deal is 770 files and 61,446 entities. None of the suite tests caught it.

**Defeito 1.** `graphit ast query --hybrid "evictOldestStaged"` devolvia cinco resultados, todos
``"Type": "File"``, there is no entity— despite ``evictOldestStaged`` being an indexed method in
`internal/hub/s3_store.go`.

**Defeito 2.** Um store indexado antes da troca de motor tem `search.lance` criado e vazio.
`graphit ast index` comparava hashes, nada mudou, e respondia "up to date" sem reconstruir — a
busca ficava **silenciosamente vazia**. A guarda que salva esse caso existia (`SearchIndexBuilt` +
Here is the translation:

"**Inline 0**, but not in **Inline 1**, nor in **Inline 2**."

It ended up with four defects because the confirmation of the diagnosis eliminated the original hypothesis.
achou mais dois no caminho.

The Diagnosis of the Prompt Was Wrong - Correcting It Was Worth More Than Fixing It

The prompt stated: the two passages are BM25 based on **different corpora**, with an IDF of one term ranging from 770.
Documents dominate an entity with over 61,000 documents, resulting in a file score being built up through construction.
The instruction was to confirm before making any changes. Confirmed against a **frozen copy of**

This is already English, so it remains unchanged.
Store real (__) is set to 0, and the hypothesis falls:

```
query "evictOldestStaged", canal de palavra-chave
  passada de entidade : 156.39 evictOldestStaged (Method), 104.31 seu doc comment, 53.31, 48.85
  passada de arquivo  :  29.63, 24.37, 23.87, 23.16, 22.96
```

The entity gains five times over in BM25. The corpus size is a false culprit: the entity is
Document short (BM25 normalizes by size and rewards), and the term is rare among 61 thousand (high IDF).
Therefore, both effects push the entity upwards, not downwards.
"already returning the entity first"— exactly why all gates in mode
Keyword was being passed while CLI returned only files.

The truth has two halves, and the second one wasn't in the prompt.

### Causa real 1 — escala de score: fundido contra cru

In a hybrid query that returns an entity's past score, it retrieves the fused score of the engine: sum of RRF, from the previous session.
Order of `1/(60+rank)` or tenths. The file upload **crashes the vector channel** -- the
Table INLINE_0 does not have an embedding column — and returns BM25 in the hundreds. One
The sort about concatenation puts all files above all entities.

E manifesta **sempre** pelo CLI porque `--top` tem default **0 = sem limite**, e a passada de
The file spins when INLINE_0 is set to 1. Therefore, the default invocation is precisely what it should be.
roda as duas passadas.

Reproduzido byte a byte contra o store congelado: os mesmos quatro arquivos do relato
(`search_lance_test.go`, `s3_store.go`, `managed_skills_frontmatter_test.go`,
`embedded_selector_test.go`), na mesma ordem.

Real Cause 2 — `_score` and `_relevance_score` Collapsed by Map Iteration

Found during investigation as to why, in the fixture, the order within the list of entities was
errada. `internal/lancestore/store_lancedb.go` montava o hit assim:

```go
for k, v := range r {
    switch k {
    case "_score", "_relevance_score":
        h.Score = toFloat(v)
```

There are two different columns, and one hybrid line carries both. `_score` is the value of the channel.
Text (BM25); INLINE 0 is what the reranker of the engine produced by merging the two channels. With
The two were in the same INLINE_0, and whoever survived was the last one to visit INLINE_1 — and Go randomly selects.
iteration of map.

Measured, twenty identical queries against an unchanged index returned every line with two results.
scores distintos.

```
ReciprocalRank : 0.0331 (6x) | 0.8344 (14x)
RowGroups      : 0.015625 (3x) | 1.0 (17x)
```

As the caller ordered this field, the symptom was not an unstable number but a **order
Incorrect: The entity that the query named fell into any position. And `_relevance_score` is the
The only two monotonically decreasing values returned by the engine are -0.0331, -0.0167, -0.0161, -0.0159, and -0.0156.
Against ranks 0 through 4— while `_score` is running in reverse for this fixture (0.834, 0, 0.883, 0.939)
1.0).

Justification - Why Separate Lists and Not Normalize

Hard rule of the project, registered in memory and on ADR: The ranking is by the engine. The 331 lines of
The seven pastes with weights in Go were intentionally removed from the T14.
This eliminates the "obvious" correction ("normalize both scores with a weight") – it is exactly the fusion in

Note: The original text was already English, so no translation was needed.
Go que foi removida.

The priority remains: entities first, files second, each list ordered among themselves. Not politics.
Based on rankings, it's order between **two types of responses**— and that is exactly what the original comment says.
The code was already doing the opposite while it was supposed to be happening.

The second input from the prompt (a table with field __INLINE_0) was evaluated and **rejected because it is not
More necessary:** It existed to make scores comparable, and the problem wasn't
Comparability of Corpora. It would cost schema, rebuild, and incremental to resolve something that is not the issue.
causa.

## Plan & Task Breakdown

Done, and it eliminated the hypothesis.
Measured against a frozen copy, see "The DIAGNOSTIC OF THE PROMPT WAS INCORRECT" above. The tracks.
They are what the prompt suggested: entity leads 156.4 to 29.6 in the keyword channel.
The test that failed today is INLINE 0, and the discriminator is __DISCRIMINATOR__.
CANAL VECTORIAL, not large corpus – a direct consequence of T1, and changes the test that the prompt asked for:
Five files will suffice because hundreds earn cents in any size of corpus.
- [x] **T3 — Separar as listas em `SearchIndex.search`** — `internal/ast/search_lance.go`.
The T3b reranker opt-in must understand precedence - request from the middle engineer.
  tarefa. Ver "O reranker" abaixo.
[T4 - Empty Index Guard on Path of `ast index`] -- `internal/ast/pipeline.go`
Symmetric with respect to the existing guard INLINE_0 for the graph.
- [x] **T5 — Teste do T4** — `internal/ast/search_index_missing_test.go`, dois estados de disco.
Gate elevators cannot rise - T6 - INLINE 0 11/11 + 5/5
English:
No one rose, which was the condition.
- [ ] T7 - Verify with CLI using the payload binary - INLINE_0, against the real store and
With an actual embedder. Below is the output.
- [ ] T8 - Documentation - Inline 0 (two new sections), this log and memory.

## Implementation Details

### `internal/ast/search_lance.go` — `SearchIndex.search`

Antes: consulta entidades, consulta arquivos, **concatena e ordena tudo junto** por
`RelevanceScore`, apara para `topK`.

Depois: ordena a lista de entidades entre si, ordena a de arquivos entre si, concatena
entities-first, to compare for `topK`. No comparison of scores between the two.

It continues to apply **within each list** — that's what gives it a deterministic nature between them.
rebuilds — e voltou a ser seguro justamente porque a coluna de score passou a ser a certa.

### `internal/lancestore/store_lancedb.go` — montagem do hit

The exposed fusion is shown by `Hit.Score`.
When there was merger and acquisition, when it didn't happen. Two new fields in INLINE_0, documented in
`lancestore.go`.

### `internal/ast/pipeline.go` — a guarda do atalho

The gate INLINE 0 already existed, with a comment describing **this very defect** for it.
He won the other half of the store for his brother. When nothing changed, he still checked.
`SearchIndexBuilt` — que **conta linhas**, porque `OpenSearchIndex` *cria* o que abre e portanto o
directory exists exactly in the case of broken - and if it is empty, reverts shards via
Without reparse: The parse cache is by definition current in this branch.

Novo campo `PipelineResult.SearchIndexRebuilt`, para o CLI dizer **qual** metade consertou em vez de
reusar a mensagem de rebuild do grafo.

### O reranker

Explicit request from the Engineer mid-task: "has the opt-in re-rank mechanism that also
Understand this prioritization". Verified:

The re-ranking engine operates in INLINE 0, applied **by table** within
Inline 0, and **no production caller calls it**, Inline 1 is only set in tests.
The file query was built by hand in INLINE_0 and **destroyed INLINE_1**. Then, if
Someone would need to configure the reranker, and then the entity list would be judged by the cross-encoder, while the file list would undergo evaluation.
Because of BM25 – the same scale defect, dressed up with a third number.

Corrigido propagando `Rerank` para a passada de arquivo, com guarda
The reranker must be achieved by the **two** past events.

It is worth noting that even with the reranker connected, precedence continues unconditionally: one
The cross-encoder assigns scores to the pair (query, candidate), and these scores are comparable between the two.
Lists aside, merging would become acceptable. But that's a decision to be made based on measurement, with the

This translation maintains the structure and meaning of the original Portuguese text while rendering it into idiomatic English. The key phrases have been translated appropriately without altering the overall tone or context.
The factored reranker is not just an embarkation point without measure on a path that no one follows today.

## Use Cases

Hybrid Search by Entity Name
- **Actor**: agente via MCP, ou pessoa via `graphit ast query --hybrid`.
Preconditions: built index; embeddings present (otherwise degrades to keyword).
- **Main Flow**:
  1. `QueryService.HybridSearch` embute a query e chama `SearchIndex.HybridSearch`.
  2. `search()` consulta `entities` com texto + vetor; o motor funde e devolve score fundido.
The list of entities is ordered by **INLINE_0** (the merged column) with ties broken by
     identidade.
4. If `len(entities) < topK || topK <= 0`, queries `files` with text only, ordered among themselves.
  5. Concatena entidades + arquivos, apara para `topK`.
- **Alternative Flows**:
  - vetor ausente → `Search`, palavra-chave nas duas passadas.
The field is filled by entities → it does not run as an uploaded file.
  - `Rerank` ligado → as duas passadas passam pelo cross-encoder.
Errors Scenario: The file upload fails, and the response is only sent if `ferr == nil` is engaged.
with entities - deliberate degradation. Failure in entity propagation.
Postconditions: entities before files; each list ordered by its own score.
- **Affected Files**: `internal/ast/search_lance.go`, `internal/ast/query.go`,
  `internal/lancestore/store_lancedb.go`.

UC-02: _INLINE_0__ in a store whose search index is empty
Actor: person who swapped versions of binary or the daemon.
- **Preconditions**: parse cache corrente; grafo presente; `search.lance` ausente ou sem linhas.
- **Main Flow**:
  1. O pipeline compara hashes e conclui que nada mudou.
  2. Antes de tomar o atalho, checa `SearchIndexBuilt(ctx, dbPath)` — conta linhas.
  3. Vazio → `BuildSearchIndexFor` replays os shards; `SearchIndexRebuilt = true`.
  4. O CLI reporta `N files up to date; search index was empty and was rebuilt from cache`.
Alternative Flows: populated index → normal flow, unchanged message.
- **Error Scenarios**: falha no rebuild devolve erro (`rebuild search index from cache: …`) em vez
  de reportar sucesso — era o ponto todo.
Postconditions: The index is populated and the search responds without reparse.
- **Affected Files**: `internal/ast/pipeline.go`, `cmd/graphit/commands/runners.go`.

## Test Cases & Acceptance Criteria

Feature: Prioritization Between Two Passages
Ref: UC-01

Scenario: A query that names a method does not return an archive in first
```gherkin
Given an index with one file declaring "evictOldestStaged" and four that only mention it in prose
And all entities with embeddings, the target pointing in the direction of the query vector
When using a hybrid search with "evictOldestStaged" and topK set to 0
The result of rank 1 is not of type File
  And o resultado de rank 1 chama-se "evictOldestStaged"
```

Scenario: Precedence Does Not Equal Exclusion
```gherkin
Given the same index
When using hybrid search with "evictOldestStaged" and topK set to 0
Then existe ao menos um resultado do tipo File
  And nenhum File aparece antes de uma entidade
```

#### Scenario: topK gasta a cota em entidades
```gherkin
Given the same index
When using a hybrid search with "evictOldestStaged" and topK set to 2
Then only two results return.
And none of them is of the type File.
```

Scenario: The keyword channel did not change
```gherkin
Given the same index, without embeddings
When busca por palavra-chave por "evictOldestStaged"
Then o resultado de rank 1 chama-se "evictOldestStaged"
```

### Feature: As duas colunas de score
Ref: UC-01

Scenario: The same hybrid query always returns the same score
```gherkin
With a populated table and an unchanged index
When the same hybrid query runs 20 times
Then cada linha devolve exatamente um valor de score distinto
```

#### Scenario: o score concorda com a ordem do motor
```gherkin
Given uma tabela populada
When a hybrid query returns at least two lines
The score is monotonically non-increasing in the order returned.
And Hit.Score equals Hit.RelevanceScore
```

Scenario: Ranks without Fusion Based on Raw Score
```gherkin
Given uma tabela populada
When just text queries run
Then Hit.Score equals Hit.RawScore
And Hit.RelevanceScore is zero.
```

Feature: Empty Index Guardian
Ref: UC-02

Scenario Outline: An absent or empty index is reconstructed
```gherkin
Given um projeto indexado, com parse cache corrente e grafo presente
When the directory search.lance is left in state "<state>"
And the pipeline runs anew without any changes to the file.
Then no files are reparsed.
And Search Index Rebuilt is true.
  And SearchIndexBuilt passa a ser verdadeiro
  And uma busca pelo termo do fixture devolve resultados

Examples:
  | estado                        |
  | removido por completo         |
  | existente mas vazio (mkdir)   |
```

Feature: The reranker reaches two steps ahead
Ref: UC-01

Scenario: The two have passed through the second stage.
```gherkin
Given an index populated and an Reranker that counts calls
When search roda com Rerank configurado e topK = 0
Then o reranker foi chamado ao menos 2 vezes
```

Feature: Hybrid Path Determinism
Ref: UC-01

Scenario: Top-1 and Stable Set Between Rebuilds
```gherkin
Given the same corpus built eight times, each time with new shard cache
When the same hybrid query runs against each build
The result at rank 1 is the same for all.
And the set of results is the same in all.
```

## Files Changed

| File | Change | Reason |
|---|---|---|
Here is the idiomatic English translation:

"_`internal/ast/search_lance.go`_ | Modified | precedence between pasted; propagates `Rerank` to file pasted comment; measurement with comment"
| `internal/lancestore/store_lancedb.go` | Modified | separa `_score` de `_relevance_score` na montagem do hit |
| `internal/lancestore/lancestore.go` | Modified | campos `Hit.RawScore` e `Hit.RelevanceScore`, documentados |
| `internal/ast/pipeline.go` | Modified | guarda `SearchIndexBuilt` no atalho de "nada mudou"; campo `SearchIndexRebuilt` |
Brazilian Portuguese:
| `cmd/graphit/commands/runners.go` | Updated | The original message for "Reconstructed Cache Index Search" |
Created as guards of precedence, determinism, and reranker
Brazilian Portuguese to idiomatic English:

"Created in two forms of empty index."
| `internal/lancestore/hybrid_score_columns_test.go` | Created | Stability of the score and agreement with the engine order |
Created | The inline 0 created | A measurement harness against a frozen real store (default skip)
Here is the Portuguese text translated into idiomatic English:

"_____`docs/specs/ast_module.md`____ | Modified | Two new sections: precedence between pastes, and the checked shortcut in both halves"

This translation maintains the structure of the original text while rendering it more natural-sounding in English. The underscores (_) are kept as they represent spaces in the original text.

Verification via CLI (T7), with the payload binary

`make install`, contra o store real (61.446 entidades) e com embedder de verdade
(`daemon-embedder (proxy→daemon)`):

```
$ graphit ast query --hybrid "evictOldestStaged"
  0  Method     0.03333  evictOldestStaged            internal/hub/s3_store.go:536
  1  Comment    0.03279  evictOldestStaged drops…     internal/hub/s3_store.go:535
  2  Function   0.02976  TestStagedEventsAreBounded   internal/hub/s3_store_test.go:358
  3  Method     0.02912  …
  …
 19  Function   0.01429  …
 20  File       29.633   internal/ast/search_lance_test.go
 21  File       24.367   internal/hub/s3_store.go
  …
```

Entidades 0–19 com RRF estritamente decrescente, arquivos a partir de 20 com BM25 decrescente. O
The requested method is ranked 1, with `Type` being its label and not `File`.

```
$ graphit ast index          # num store cujo search.lance foi removido
✓ 1 files up to date; search index was empty and was rebuilt from cache (0.1s)
  › Timing: discover 0.00s, hash 0.00s, parse 0.00s, write 0.06s
$ graphit ast query --hybrid "EvictOldestThing"
    "Name": "EvictOldestThing"
```

Done in an outline project (`/tmp/t2-demo`), not in the actual store.

## Trade-offs & Decisions

Prioritization over normalization. Normalizing both scales in Go is equivalent to merging them into one weight.
T14 turned off. The precedence between two types of responses is not a policy of ranking.
A single table with field "`kind`" discarded - it resolved corpus comparison issues, which are not
  a causa. Custaria schema + rebuild + incremental por nada.
- **Ordenar por score continua**, dentro de cada lista, porque a coluna passou a ser a certa. A
The alternative I arrived at for implementing was preserving the order of the engine and not ordering hybrid.
  **revertida** quando a causa 2 apareceu: com a coluna certa, ordenar por score *concorda* com o
Motor and still adds deterministic tiebreaker, which the motor does not give.
Reindexing: spread it, not merge it. See "The Reranker".

## Technical Debt

- [ ] **Ordem entre linhas empatadas nos dois canais permuta entre rebuilds.**
The inline 0 disables ties by identity and on the hybrid channel, the engine
The lines end with distinct values of RRF (only differing in the fourth decimal place because it had to.
Choosing an order, then the tie never occurs. The top-1 and set are stable – verified in
Rebuilding eight times would be necessary, as an unidentifiable internal order of lines does not exist. Repairing would require deciding in Go that
Two rankings of the engine are "close enough for an even split," which is a policy ranking module.
It is registered in `TestHybridTopResultAndSetAreStableAcrossRebuilds` and in spec.
- [ ] **Inline 0 does not test the hybrid path.** It calls
Brazilian Portuguese:
`HybridSearch(..., nil, 10)` — vector **null** — then falls back to keyword. Not corrected
Because the new gate covers the true path, but the name deceives someone.
- `TestHybridSearchQualityFloor` was in SKIP - RESOLVED on the second session, see section
  no fim deste log. Ao rodar pela primeira vez ele mediu **0 de 11** sondas decisivas no commit
Before, what is the post-delivery defect confirmation for `_score` and __INLINE_1? With
Correction, 11 out of 11.
The file upload overwrites its own error (`if ferr == nil`). A failure returns one
Response only with entities, indistinguishable from a non-corpus without files marrying. Pre-existing, not
  tocado.

## System Knowledge

The default is 0 in INLINE_1, and passing an argument runs when
The default invocation always runs both steps — by
This defect was 100% reproducible via CLI and invisible in tests that passed INLINE_0.
The score of entity is higher than the file score in BM25, and two reasons add up: entity is
Short document (BM25 normalized by size), and the term is rare in the large corpus (high IDF).
Intuition of "large corpus dilution" is incorrect here.
The ``_relevance_score`` is the sum of RRF (``~1/(60+rank)``), which is monotonic with respect to the engine order. ``_score`` is the
Value of the text channel does not include order in hybrid query.
A semantic line is pure and has no score — just `_distance`. So `Hit.Score` is 0 in
  `mode: semantic`, e `confidentSemanticResults` filtra por `RelevanceScore < semanticFloorCosine`
Using the mapped field `Distance` before. Not edited.
- **Cuidado ao escrever fixture com vetores:** dar o **mesmo** embedding a todas as entidades faz o
Vector channel is pure noise with maximum confidence; the engine filters this noise using BM25 and a...
  entidade irrelevante toma o rank 1. O primeiro fixture fez isso e o teste **flapava** — passava
Alone, he failed in the full suite. Point at one direction, and the rest at another.
The wiki doesn't have this class of defect: he looks for a table only (`chunks`), so there are no two.
Websites for mixing. Verified, not supposed.
The intermittent segmentation fault occurs under memory pressure - an item in the known backlog.
(buffer pool without coordination between processes) no rollback. Not shown in this session.

## Progress Log

### 2026-08-24
Open before any edit. Read: `hub-em-s3-icebug-e-lancedb.md` (TO CONTINUE +)
T15), the backlog prompt, and the memories of `lancedb` and search— particularly the correction of
Floor of 13/16 (fixing 11/11 + 5/5) and the decision of T14 (prohibiting mergers in Go).
Frozen a copy of the real store and measured the two passes. The prompt diagnosis fell.
Registered above; the find is worth more than the correction, as the original prompt suggested.
- Reproduzido o relato exato com `topK = 0`: os mesmos quatro arquivos, na mesma ordem.
Engineering intervention during the task: "has the opt-in mechanism for re-ranking that also"
It needs to understand this prioritization". It became T3B. They thought the file query was crashing
The issue with scaling in a third number.
T2/T3: written guards failing; precedence implemented; guards passing.
When running the suite with all items checked, the new test failed in another way: the order within the list of items.
  entidades estava errada. Duas causas, nesta ordem: (a) fixture com embedding compartilhado, meu
Error, corrected; (b) **`_score` and `_relevance_score` collapsed by map iteration — defect
Real, in `internal/lancestore`, isolated with 20 identical queries against an unchanged index.
I arrived at implementing "do not order hybrid, preserve the engine's sequence," and then reversed after.
  achar a causa 2: com a coluna certa, ordenar por score concorda com o motor e ainda desempata.
A deterministic gate that I wrote failed; investigated, it was **my super-specific specification** — just
  linhas empatadas nos dois canais permutam. Reescrito para afirmar o que de fato vale (top-1 e
  conjunto), com o residual registrado em Technical Debt e no spec.
T4/T5: stores in INLINE_0, symmetric to the graph's. Verified that both tests fail with
  guarda desligada e passam com ela.
- T6: `TestSearchIndexQualityFloor` 11/11 + 5/5, `TestTruncatedQueryCoverage` 8/8 + recall. Nenhum
  subiu.
T7: `make install` and verification by CLI, both defects. Above output.
T8: with two new sections; this log; memory.
Suites: Green and green, respectively; Green and green, respectively;
OK. Alerts from `go vet` are in an ANTLR-generated parser that is already present.

---

## SEGUNDA SESSÃO (2026-08-24): o `onnxruntime.so` ausente, e os dois defeitos que o skip escondia

Request from Engineer: "resolve this issue of onnxruntime.so not being present and test" — a
Technical debt registered above INLINE 0 was skipped on this machine.

### Por que o ORT nunca era encontrado fora do payload

It had two places: next to the executable and on the disk.
Loader Path. The library stays just outside of the executable, **inside the payload of the launcher**, so:

A binary of `go test` resides in a temporary directory of the toolchain → not found;
The _INLINE_0_ produces a core with no holes → not found;
- e nada punha o loader path.

Without finding, **the inline was not called** and the binding fell on his default name —
Here is the translation:

"**Inline 0**, not the form **Inline 1** that this project distributes. Hence, the message of the report."

Two corrections, and they resolve different things:

This sentence is already in idiomatic English. No changes are needed.

1. **`findORTLibrary` passou a olhar o payload EXTRAÍDO** (`brand.RuntimeDir(version.Version)`),
Where does INLINE_0 lie next to INLINE_1 and INLINE_2? It has the same resolution.
that the AST already does for YAMLS of grammar (_`runtimeQueriesDir`) and Ladybug Store for those.
Extensions (`ExtensionDir`). This fixes the binary local, NOT the tests -- in testing, __INLINE_1.
It is isolated by `internal/brand/testhome.go`, so the runtime points to a discarded home.
Empty. This is correct and deliberate: this isolation exists for tests never to read the actual home.
It was not circumvented.
2. **`make test` places the ORT cache from the Makefile into the loader path** — `ORT_HOST_FETCH`/`ORT_HOST_LIB`
   por GOOS, com `LD_LIBRARY_PATH` e `DYLD_LIBRARY_PATH`. É isto que faz os gates RODAREM.

### O que o skip estava escondendo — dois defeitos, um deles grave

With the ORT achievable, two gates that reported INLINE 0 without running started to run, and **the two.
falharam**. Baseline tirado em worktree no commit anterior (`abef386`) para separar o que era meu do
that was pre-existing:

| gate | no baseline `abef386` | depois |
|---|---|---|
| `TestHybridSearchQualityFloor` | **0 de 11** sondas decisivas | **11 de 11** |
| `TestSearchIndexSemantic` | `SemanticSearch` devolvia NADA | passa |

The first is the retraction confirmation of defect `_score`/`_relevance_score`. On the baseline,
He returned `cf`, she returned `parseConfig`, they returned `connect`, and we returned `CFG_LOAD`. Not.
It's a weak ranking, an absence of a ranking, exactly what the map iteration produces. The correction for
First session switched the hybrid channel from 0/11 to 11/11, which is a better substitute for the lexical (10/11).
And that became apparent only because the gate started operating.

The second is a new defect, pre-existing and serious: `SemanticSearch` never returns anything.
A purely vector query returns `_distance` and no column scores, then.
`RelevanceScore` chegava zero; `confidentSemanticResults` compara esse campo contra
The number was below any floor, so it truncated in the first result and
It returned an empty list. The SQLite index calculated the cosine in Go and wrote it into that field — as if to say, "Here's the result."
The port brought the consultation, leaving the calculation behind.

Corrected by deriving the cosine of the distance. The metric was measured, not supposed, against vectors.
Unitaries of Cosine Known:

```
Cosine 1.000 = Distance 0.000
Cosine of 0.707 equals approximately 0.586, which is about a distance of 0.586 units.
Cosine 0.500 equals distance 1.000.
Cosine 0.000 = Distance 2.000
```

That is `d = 2 - 2cos`, so `cos = 1 - d/2`. Exact and not approximate because the embedder
All vectors that return L2-normalized. No metric is configured in the index, so this is the one.
escolha default do motor — `TestVectorMetricIsSquaredL2OnUnitVectors` existe para falhar se um bump
version to change

The 132 MB download that Skip also hid

The variable `NewLocalEmbeddingClient` was called `EnsureModel` before `initONNXRuntime`. On a machine without the
The runtime downloaded 132 MB and only then discovered that it couldn't use them—cache was no defense against the failure.
Then he had to pay again for the next call. The cache is derived from INLINE_0, and each test binary.
He has his discardable, measured by this machine: **29 abandoned homes, 4. 3 gigabytes, in a tmpfs**.

Isto fecha a metade que faltava da tarefa de 2026-08-07
(`docs/tasks/lentidao-do-make-test-medida-e-o-download-de-132mb-escondido-nos-testes.md`), que
He covered **INLINE_0** but did not **INLINE_1** — where four test files construct a client of
embedding e nenhum semeia cache.

Two corrections:
- **ORT primeiro, modelo depois.** Teste sem ORT agora pula em 0,00 s com cache vazio.
**`<BRAND>_MODEL_CACHE`** points to the root models directory for a shared directory; **`make test`** as well.
Define. The root and not the leaf; this is load-bearing: the reranker resolves his path from there.
The same root, then overwriting one sheet would move a model and leave the other in its home reality.
  primeira tentativa sobrescreveu a folha e o reranker foi para `/tmp/bge-reranker-base` — um teste
It arrived and wrote stubs there. Corrected it by removing the `filepath.Dir(base)` from
  `NewRerankModelManager`, que era a fragilidade que permitiu isso.

The variable was almost completely isolated by the test: INLINE_2.
asserts about models' paths and an override pointing to a shared directory
The defeat occurs in the direction that allows the test to pass by measuring the wrong filesystem.

A fixated lock was corrected, not loosened—why was the first attempt wrong?

`TestHybridSearchQualityFloor` tinha um campo `tie string`, capaz de nomear **uma** alternativa
Defensible. The sputnik `configuration` has seven entities with equal pretensions in this corpus.
(`parseConfig`, `Config`, `loadUserConfig`, `coreConf`, `CONF_MGR`, `configLoader`,
`initConfiguration`) — uma delas com o docstring literal *"Configuration for the parser."*. O campo
virou `ambiguous []string`.

My first attempt was to swap the ambiguous probes for recall@5, but I was wrong. Measured:
Failed because seven justified responses and a five-minute window for the demand only moves the tie-breaker.
Position 1 for Position 5 — and failed on a set of responses that were *good* (five entities)
configuration), just without containing what the line names first. The original intention of the author was
Sure, here is the translation:

Correct; what was wrong was just the size of the list. He returned to being "the winner has to be one of"
Defensible responses, with the complete list.

The gates that couldn't climb up remain where they were: INLINE_0 11/11 + 5/5.
`TestTruncatedQueryCoverage` 8/8 + recall.

Verification

Brazilian Portuguese to idiomatic English:

- Inline 0 exits with **0**, 47 packages, **no** model copies abandoned, Inline 1.
  de 4,3 GB para **740 KB**.
The three new/renovated gates pass **under `-race`**, which is the one that `make test` uses.
The car **rode in 1.35 seconds** against ~30 seconds earlier — the difference was the download.
- Sem ORT no loader path: pula em 0,00 s e o cache de modelo fica **vazio**.

Files Changed in This Session

| File | Change | Reason |
|---|---|---|
____ | Modified | Extracted ORT from payload; ORT before model |
| `internal/ai/model_manager.go` | Modified | `ModelsDir()` com override `<BRAND>_MODEL_CACHE`; `ModelCacheDir` derivada dela |
Here is the translation:

"_`internal/ai/rerank_model.go`_ | Modified | Resolve from root, not from _`filepath.Dir(ModelCacheDir())`_"
| `internal/ai/main_test.go` | Created | limpa o override neste pacote, que afirma sobre caminhos |
The modified status of the issue changed from "ONNX initialization" to "load tokenizer".
The modified value derives the cosine of the distance.
Here's the Portuguese text translated into idiomatic English:

"_______ INLINE 0 ______ | Modified | ____ INLINE 1 _____, with measurement"

This is already in English and doesn't require translation.
| `internal/ast/search_hybrid_floor_test.go` | Modified | `tie string` → `ambiguous []string`; registro do que o gate mediu ao rodar |
Brazilian Portuguese to idiomatic English:

| `internal/ast/vector_metric_test.go` | Created | Strains the engine's metric and returns from `SemanticSearch` |
Here is the translation:

"____ | Modified | `ORT_HOST_*`, `MODEL_CACHE`, `BRAND_ENV`; `test` depends on fetch and exports the three."
Inline 0: Modified | Vector metric, and "Running the tests that require an embedder"

Technical Debt for this session

- The isolation of `HOME` from the payload makes it untestable via ORT, so the correction (1)
It covers the binary locally and not the tests — which rely on `make test` placing the loader path. A `go test`
The crane continues jumping those gates. Alternative not made: a `TestMain` in `internal/ast` that points
  o loader path sozinho, o que significaria um teste mexendo no ambiente de carga do processo.
Nothing raises the skip level in CI. A gate that skips continues reporting INLINE 0, which is the trap.
This entire session is incorrect because it would be appropriate for CI to fail if an essential gate were skipped; not done.
Because it requires deciding what is essential.
