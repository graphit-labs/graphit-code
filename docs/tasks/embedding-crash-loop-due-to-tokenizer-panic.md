---
Title: Module Embedding Loop Crash - Library Panic Broke Entire Process
status: done
created: 2026-08-10
updated: 2026-08-10
tags: [daemon, embedding, tokenizer, resiliencia, observabilidade]
---

Crash loop of the embedding module - a library panic brought down the entire process

## Objective

The module `embedding` of the daemon was restarting in a loop since July 29, 2026 - 66 occurrences on
Here is the translation from Brazilian Portuguese to idiomatic English:

"`~/.graphit/daemon/daemon.log`, with exponential backoff up to 30 seconds and indefinite retry:"

```
embedding: crashed (attempt=9, error=panic: runtime error: slice bounds out of range [:596] with capacity 595)
embedding: crashed (attempt=2, error=panic: runtime error: slice bounds out of range [:551] with capacity 550)
```

None of the 66 lines mentioned an "archive," a "function," or a "line" because `runProtected` merely registered.
The VALUE of panic. Practical consequence: no new entity from this project entered the search.
Semantics had been dormant for twelve days, and the daemon was burning up its CPU every two minutes to die again.

## Implementation Details

Diagnosis. Reproduced with inline compilation of the source code running INLINE_0
The same path as the module used by the daemon, with the ONNX library
Located by `LD_LIBRARY_PATH`. The stack is pointing outside our code:

```
github.com/sugarme/tokenizer/normalizer.(*NormalizedString).RangeOriginal  normalized.go:313
github.com/sugarme/tokenizer/normalizer.(*NormalizedString).Slice          normalized.go:407
github.com/sugarme/tokenizer.(*AddedVocabulary).splitWithIndices           added-vocabulary.go:483
...
internal/ai.(*localEmbeddingClient).EmbedBatch                             embedding_local.go:179
internal/ast.(*Embedder).processBatch                                      embedder.go:318
```

The bug is in `sugarme/tokenizer` v0.3.0. `NormalizedString.Slice`, under the branch
`NormalizedTarget`, deriva o intervalo sobre a string ORIGINAL com `ConvertOffset`, que —
Contrary to `IntoFullRange`, used in another branch, it does not perform a clamp on length.
Then slices `[]byte(n.original)` beyond the end. `go list -m -versions` confirms.
The version 0.3.0 is the latest one published: no upgrade can fix it.

Contingency, in **INLINE_0**.

INLINE_1 involves
Here's the translation:

"INLINE_0 and converts the panic into an error. INLINE_1 marks the failed text while maintaining it."
tensor line as padding and returns `nil` at that position in the result — `processBatch` already
It treated an empty vector as "not embedding this," so the alignment between inputs and outputs became
It maintains all of the rest of the lot embedded normally. When none of the text in the lot is encodable, it.
function returns before constructing tensors, because a shape with dimension zero is not something for
entregar ao modelo.

Two smaller guards on the same path, found after reading: It is truncated to its size of
`ids` antes do corte por `maxSeqLen` (o corte assumia os dois com o mesmo comprimento), e a
The extraction of the output vector checks for the end of `outputData` before splitting.

**3. Testabilidade.** O campo `tk` passou de `*tokenizer.Tokenizer` para a interface
Here's the translation:

The `textEncoder` method, which allows injecting an encoder that panics upon entry. Not

This is a placeholder for a C++ inline function name "`textEncoder`" and its purpose of allowing the injection of an encoder that will panic on entering. The phrase "Not" at the end indicates that this feature or capability is not being used in the current context.
Decorative direction: The trigger of the bug is not identifiable outside the library—twelve.
Synthetic Candidates (Accents, CJK, Emoji, Special Characters Literally, Combinable Marks)
Characters of control were inline 0 uppercase Turkish-tokenized correctly – then what?
The test is our containment, not a third-party bug.

**4. Observabilidade, em `internal/daemon/project.go`.** `runProtected` passou a incluir
Without this, the next module entering a crash loop repeats it.
intensive investigation without any regard.

5. Diagnosis of what is missing, in `internal/ast/embedder.go`. `processBatch` counts.
The entities that returned without a vector and emitted an `Warn` at the end, with the label, count, and one
sample of up to 5 `path::uid`. Before, a `continue` quiet one.

## Use Cases

UC-01: Embed a batch where text is not tokenizable
Actor: `Embedder.processBatch`, called by `RunCycle` — module `embedding` of the daemon, `graphit ast embed`, tool MCP `ast_embed`.
- **Preconditions**: cliente de embedding local (ONNX + tokenizador); um dos textos do lote dispara o panic de `sugarme/tokenizer`.
- **Main Flow**:
The code compiles up to `BatchSize` texts and calls `EmbedBatch`.
The `INLINE_0` function calls `INLINE_1` by text; any that enter panic mode become errors and are marked.
  3. A linha do tensor daquele texto fica toda em padding.
The inference runs throughout the lot.
Vectors are extracted only for encodable texts; what failed goes as INLINE_0__.
  6. `processBatch` pula o vetor vazio, conta a entidade como pulada e cacheia os demais.
After the label, an INLINE_0__ name designates count and sample.
- **Alternative Flows**:
No batch, none of the elements in the lot are encodable: INLINE_0 returns an empty vector without touching the ONNX session.
Tokenization returns an error instead of panicking: same treatment.
- **Error Scenarios**:
Error in ONNX Inference: The entire batch error persists as before — this task does not change.
- The extraction is performed within, rather than outside of, `batchSize*hiddenDim`.
Postconditions: The process continues alive; encoded entities retain their embeddings; the rest remain pending and are retained in the next cycle without an incorrect vector in cache.
- **Affected Files**: `internal/ai/embedding_local.go`, `internal/ast/embedder.go`.

UC-02: Register a module panic of the daemon
Actor: `runProtected`, under module supervisor.
Preconditions: A module enters panic within `Start`.
- **Main Flow**:
  1. O `defer`/`recover` captura o valor.
  2. O erro passa a conter o valor E `debug.Stack()`.
  3. O supervisor registra a linha e aplica o backoff de sempre.
- **Error Scenarios**:
  - Panic com `error` em vez de string: mesmo tratamento, `%v` formata os dois.
Postconditions: the log identifies the file, function, and line of the panic.
- **Affected Files**: `internal/daemon/project.go`.

## Test Cases & Acceptance Criteria

Feature: Tokenizer Panic Containment
Ref: UC-01

#### Scenario: um texto que faz o tokenizador entrar em panic vira erro
```gherkin
Given an encoder whose tokenization panics due to slice size errors
When `encodeSingle` is called with text that triggers it
Then a chamada retorna erro em vez de propagar o panic
  And o erro identifica a causa como "tokenizer panic"
And nothing is returned, whether it's an ID or a mask.
```

Scenario: A text that does not trigger the panic continues being tokenized.
```gherkin
Given o mesmo encoder
When `encodeSingle` is called with text that does not trigger the panic
Then tokenization was successful.
```

The entire lot is not tokenizable and does not reach the model.
```gherkin
Given an embedded client with an empty ONNX session
  And um encoder que entra em panic para todos os textos do lote
When EmbedBatch is called with three texts
Then a chamada retorna sem erro
  And devolve exatamente 3 vetores, todos vazios
  And os 3 textos foram tentados
And an ONNX session has never been used—using it would invalidate the test.
```

Feature: Module Panic Trace Stack Dump
Ref: UC-02

#### Scenario: o erro do panic carrega o stack
```gherkin
Given an module whose start triggers a panic with "boom!"
When runProtected o executa
The error begins with "panic: boom!"
It contains the stack trace with the named "runProtected"
```

#### Scenario: panic com valor de erro
```gherkin
Given an module that starts in panic upon receiving a formatted error message
When runProtected o executa
Then it starts with "panic: formatted panic"
It contains the stack trace.
```

## Files Changed

| File | Change | Reason |
|---|---|---|
Here is the translation:

| `internal/ai/embedding_local.go` | Modified | `encodeSingle` converts panic into an error; `EmbedBatch` degrades to text; `tk` turns into interface `textEncoder`; stores in `mask` and extracts from the vector |

This translation maintains the structure and technical terms of the original Portuguese text, preserving its meaning while rendering it in idiomatic English.
Created | Covers containment of panic and entirely unencodable lot |
| `internal/daemon/project.go` | Modificado | `runProtected` inclui `debug.Stack()` no erro |
The two tests of `runProtected` compared the message by exact equality; they now require a prefix plus presence on the stack.
| `internal/ast/embedder.go` | Modificado | `processBatch` conta e nomeia as entidades que voltaram sem vetor |

## Trade-offs & Decisions

Contain instead of sanitize. The alternative would be to transform the text (remove non-ASCII characters).
  normalizar) antes de tokenizar. Rejeitada: sem saber o que dispara o bug — e doze
Candidates synthesized did not shoot him – any sanitization is guesswork that degrades the situation.
embedding of the entire corpus for protection against an uncharacterized condition.
- **`nil` em vez de vetor zero.** Cachear um vetor zerado tiraria o retrabalho, mas plantaria
In the index, there is a vector that describes nothing and would survive until the file changes. The contract
Vector empty = not embedded; it already existed in INLINE_0_; reusing it cost nothing.
Entities that fail are retained forever. As nothing is cached, each cycle
Try again and fail again. The cost is the tokenization of these few entities, and the cycle.
It runs every two minutes, regardless of how it's done; INLINE_0 returns 0 and the rebuild is cheap.
It is fired. I accept in exchange for not having an incorrect vector in cache.
There is an inline method only for testing. `textEncoder` exists to allow injecting into it.
panic. It's minimal indirectness and INLINE_0__ satisfies without adapter.

## Technical Debt

- [ ] O bug de `sugarme/tokenizer` v0.3.0 continua aberto upstream (`ConvertOffset` sem clamp
  em `NormalizedString.Slice`). Se aparecer uma release nova, atualizar e reavaliar se a
Contingency measures are still necessary—she remains cheap to maintain in any event.
The following entities in this corpus trigger the panic. The new approach is `Warn`.
Name them next cycle of the daemon; when they appear, reduce one of them to a case.
minimum and report upstream.
is
Flaky and this is prior to this task - verified in a clean headwork tree, where
  falha ao rodar o pacote inteiro e passa isolado. Depende de estado deixado por outro teste
The package. Not investigated here.

## System Knowledge

The module `embedding` of the daemon is `ast.RunEmbeddingLoop`, through `EmbeddingModule.Start`.
(`internal/daemon/adapters.go:37`), and not the one from the wiki, which is another module with a different name.
The cycle has two phases, and only the second one is expensive: `RunCycle`, and it embeds, and only when it does.
returns when n > 0, which INLINE_0 reconstructs the database to inject vectors.
A cycle that does not embed anything pays for the rebuild.
- **Reproduzir o caminho do daemon localmente**: compilar `./cmd/graphit` e rodar `ast embed`
The project - even INLINE_0_. The ONNX library is being sought alongside
Executable and then in `LD_LIBRARY_PATH`; outside of the run-time, point to
Inline 0 for Inline 1, where is Inline 2?
- **O tokenizador do CodeRankEmbed** usa `BertNormalizer` com `clean_text`,
When `handle_chinese_chars` and `lowercase` are connected — three transformations that change the length
Between the original string and the normalized version, which is exactly the condition of the bug in question.
The ``ConvertOffset`` depends. The special tokens are ``[PAD] [UNK] [CLS] [SEP] [MASK]``, and it is.
Presence of one that takes `AddedVocabulary.splitWithIndices` to `Slice` which fails.

## Progress Log

### 2026-08-10
- Confirmado no log do daemon: 66 crashes desde 2026-07-29, sempre com o limite exatamente
equal to capacity plus one.
- Descoberto que `runProtected` descartava o stack — corrigido primeiro, por ser o que
It hindered any diagnosis.
Panic retriggered with full stack due to an inline binary running `ast embed`. Causes crash.
Our code: `sugarme/tokenizer` v0.3.0, without a corrected version available.
Twelve synthetic inputs tested against the real tokenizer; none reproduced. Decided.
Test for containment instead of upstreaming the issue.
After correction, **4022 entities** were completed where previously it would have died in ~2 seconds.
- Verde: `internal/ai`, `internal/daemon` e `internal/ast` com e sem `-race`; `go vet`,
  `golangci-lint` (0 issues) e `gofmt` limpos nos arquivos tocados.
