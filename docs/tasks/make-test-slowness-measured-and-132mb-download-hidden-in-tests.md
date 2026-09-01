Task: The slowness of INLINE_0, measured - and the 132 MB that the tests downloaded from Hugging Face

Status: partially completed as of August 7, 2026. Request from the Engineer: "attack on slowness."

The measurement first

I had attributed slowness to "1, 29 million lines of ANTLR compiled twice." The measurement debunked this.
The part that mattered. In a machine with 20 cores and 61 GB of RAM:

| fase | frio | quente |
|---|---|---|
| build+link da passada 1 (`-race -covermode=atomic`) | **181 s** | **18 s** |
Execution of past 1 | 108 seconds | 108 seconds
| passada 2 (parsers, sem race) | 1,7 s | 1,6 s |

The double-build costs 181s once per state of cache; then it's 18s. It wasn't the villain. The cost
Recurring execution — within it, two packages.

Method: INLINE 0 for the cold number (without touching the real cache)
Inline 0 for separating build + link from execution.

## O que estava errado: `internal/ai` baixava o modelo de verdade

`internal/ai` levava **102,9 s**, e sete testes eram ~81 s disso:

```
18.18  TestModelManager_EnsureModel_CreateCacheDir
14.01  TestModelManager_EnsureModel_NeedDownloadModelTooSmall
11.54  TestModelManager_EnsureModel_BundledModels
10.73  TestModelManager_EnsureModel_DownloadModel
 9.18  TestNewEmbeddingClientFromConfig_FallsBackToLocal
 8.96  TestNewEmbeddingClientFromConfig_NoProxy
 8.73  TestNewLocalEmbeddingClient_Fails
```

`modelONNXURL` e `tokenizerJSONURL` eram `const` em `model_manager.go`, e `download()` usa
Without timeout. Then all `make test` went to HuggingFace.co and moved 132 MB, several times.
times. They even created a `httptest.NewServer` — and never pointed to it with `EnsureModel`.

E nenhum afirmava nada: o corpo era `if err == nil { t.Log(...) }`. O resultado era o que a rede
This was not just slow—it was also a test that didn't test and an internal third-party dependency within CI.

### Duas rotas, dois consertos

Route 1 - Who builds `ModelManager` directly (4 tests)? `ModelManager` won two fields.
not exported, `modelURL`/`tokenizerURL`, empty throughout production (`NewModelManager` does not include them)
toca) e resolvidos por `modelSource()`/`tokenizerSource()`. O teste aponta para um `httptest`
The local endpoint was under control, so the outcome became predetermined — then the assertions
They turned their assertions: "A model of 50 bytes is rejected, and the error names the size," "The tokenizer already"
Valid is not revoked." (verified by mtime).

Route 2 - who passes through INLINE_0 (3 tests). They build themselves
``ModelManager`` does not have a place to inject. Purely from the test side: sow the cache that
`NewModelManager` deriva de `$HOME`, e `EnsureModel` retorna no `isValid` antes de cogitar
Download. No production change.

The scattered files are sparse (_`f.Truncate`), because _`isValid` only calls _`Stat` — the code
anterior fazia `make([]byte, modelONNXMinSize+1)`, 100 MB de heap e 100 MB de escrita para
responder uma pergunta sobre `st_size`.

**Resultado: `internal/ai` 102,9 s → 2,67 s.**

The wall gain was less than the CPU gain, and that's perfectly fair to say.

Passed 1: 108.5 seconds → 105.2 seconds. Just 3 seconds.

Because on INLINE_0, INLINE_1 ran in parallel with INLINE_2; subtracting 100 from it wouldn't work.
Move the critical path. The floor is now **18s of build + 83s of `internal/ast`**.

What does the repair actually deliver: 100 GB of CPU and network resources (relevant on a machine with less)
Clusters where INLINE\_0 is not available, the CI no longer relies on Hugging Face being online, and seven.
Tests that did not affirm anything began to assert.

## O que sobrou, quantificado

**INLINE_0** are 83 S and is the critical path. There were 352 tests conducted, of which only 42 called for action.
**— These 310 run in series on a machine with 20 cores. This is where the next order is located.
grandeza.

I am not doing it because I'm conducting a 310-test audit with shared state: global ANTLR caches.
LadybugDB handles, compatible with `t.Parallel`, are implemented in this code snippet.
Pool is not clamped in tests (22 files use `lbug.DefaultSystemConfig()` cru, ~80% of RAM)
Each). Probably, that's why `-p 4` exists.

The two most expensive tests of INLINE_0 are not waste and should not be cut off:
`TestResetAntlrCachesRace` (13,0 s — 8 goroutines parseando enquanto 25 resets acontecem, sob

"`-race` and `TestLadybugStringIntegrityUnderGCPressure` (11, 6 seconds) are stress tests performing work"

real.

## Arquivos

File | Change | Reason
|---|---|---|
| `internal/ai/model_manager.go` | Modificado | campos `modelURL`/`tokenizerURL` + `modelSource()`/`tokenizerSource()`; `EnsureModel` usa os acessores |
| `internal/ai/model_testserver_test.go` | Criado | `modelServer`, `seedModelCache`, `sparseFile`, `writeZeros` |
Modifications | Updated | 4 tests outside the network, with real assertions
| `internal/ai/ai_embedding_test.go` | Modificado | 2 testes fora da rede |
| `internal/ai/ai_coverage_test.go` | Modificado | 1 teste fora da rede |

Verification

`go test -race ./internal/ai/` em 2,67 s, todos passando. `make vet`, `make lint` (0 issues) e
`make ci` verdes.

---

## A METADE QUE FALTAVA, fechada em 2026-08-24

The repair above covered Inline 0 — the two routes, URL injection, and cache seeding. He
It did not cover **INLINE_0**, which also builds an embedding client, in four files.
teste: `abbrev_semantic_test.go`, `hybrid_search_test.go`, `search_hybrid_floor_test.go` e
None of them sowed seeds, so each one continued on to Hugging Face.

And it was invisible for the same reason that made downloading useless: all of them end up being...
When the client does not ascend, then the cost appeared as `ok`, not as a delay of one
Test that someone would investigate. Measured on August 24, 2026: 29 discarded homes with each containing 133 MB.
4,3 GB in tmpfs - that's RAM. Date August 23, 2026, meaning accumulated since very long after this
tarefa.

Three corrections, in `docs/tasks/busca-devolve-so-arquivos-e-index-nao-reconstroi.md`:

1. **`NewLocalEmbeddingClient` checa o ORT ANTES de baixar o modelo.** A ordem estava invertida, e
   por isso o download acontecia mesmo quando o cliente ia falhar de qualquer forma. Agora o teste
   sem ORT pula em **0,00 s** com o cache vazio, contra ~28 s e 133 MB antes.
2. **`<BRAND>_MODEL_CACHE`** points to the root models directory for sharing.
The model is downloaded once, not once per package.
The orthodontist was able to reach her goals — INLINE_0 looked at the extracted payload, and INLINE_1.
It places the Makefile's cache in the loader path. This is what finally made the gates operate, and that's why they...
The hybrid channel responded with **0 out of 11** sensors.
   decisivas, e `SemanticSearch` devolvia nada.

Resultado: `/tmp/graphit-test-homes` saiu de **4,3 GB para 740 KB** num `make test` completo, sem
None of the abandoned model copies, and `make test` outputs 0.

The task has another open item: the 310 inline tests that run on
Series. Nothing here changed it.
