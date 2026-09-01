---
title: Crash-loop of the embedding module — a library panic brought down the whole process
status: done
created: 2026-08-10
updated: 2026-08-10
tags: [daemon, embedding, tokenizer, resiliencia, observabilidade]
---

# Crash-loop of the embedding module — a library panic brought down the whole process

## Objective

The daemon's `embedding` module had been restarting in a loop since 2026-07-29 — 66 occurrences in
`~/.graphit/daemon/daemon.log`, with exponential backoff up to 30 s and indefinite restarting:

```
embedding: crashed (attempt=9, error=panic: runtime error: slice bounds out of range [:596] with capacity 595)
embedding: crashed (attempt=2, error=panic: runtime error: slice bounds out of range [:551] with capacity 550)
```

None of the 66 lines said file, function or line, because `runProtected` recorded only
the VALUE of the panic. Practical consequence: no new entity from this project had entered semantic
search for twelve days, and the daemon burned CPU every two minutes only to die again.

## Implementation Details

**1. Diagnosis.** Reproduced with a binary compiled from source running `ast embed`
(the same `Embedder.RunCycle` path the daemon module uses), with the ONNX library
located via `LD_LIBRARY_PATH`. The stack pointed outside our code:

```
github.com/sugarme/tokenizer/normalizer.(*NormalizedString).RangeOriginal  normalized.go:313
github.com/sugarme/tokenizer/normalizer.(*NormalizedString).Slice          normalized.go:407
github.com/sugarme/tokenizer.(*AddedVocabulary).splitWithIndices           added-vocabulary.go:483
...
internal/ai.(*localEmbeddingClient).EmbedBatch                             embedding_local.go:179
internal/ast.(*Embedder).processBatch                                      embedder.go:318
```

The bug is in `sugarme/tokenizer` v0.3.0. `NormalizedString.Slice`, in the
`NormalizedTarget` branch, derives the range over the ORIGINAL string with `ConvertOffset`, which —
unlike `IntoFullRange`, used in the other branch — **does not clamp** to the length.
`RangeOriginal` then slices `[]byte(n.original)` past the end. `go list -m -versions` confirms
that v0.3.0 is the newest published version: there is no upgrade that fixes it.

**2. Containment, in `internal/ai/embedding_local.go`.** `encodeSingle` wraps
`tk.EncodeSingle` and converts the panic into an error. `EmbedBatch` marks the text that failed, keeps the
tensor row as padding and returns `nil` at that position of the result — `processBatch` already
treated an empty vector as "do not embed this one", so the alignment between inputs and outputs is
kept and the rest of the batch is embedded normally. When NO text in the batch is encodable, the
function returns before assembling tensors, because a shape with a zero dimension is not something to
hand to the model.

Two smaller guards on the same path, found while reading: `mask` is truncated to the size of
`ids` before the cut by `maxSeqLen` (the cut assumed both had the same length), and the
extraction of the output vector checks the end of `outputData` before slicing.

**3. Testability.** The `tk` field went from `*tokenizer.Tokenizer` to the
`textEncoder` interface (one method), which allows injecting an encoder that panics. It is not
decorative indirection: the trigger of the bug is not characterizable from outside the library — twelve
synthetic candidates (accents, CJK, emoji, literal special tokens, combining marks,
control characters, Turkish uppercase `İ`) all tokenized without error — so what is
tested is the containment, which is ours, and not the third-party bug.

**4. Observability, in `internal/daemon/project.go`.** `runProtected` now includes
`debug.Stack()` in the error. Without that, the next module that goes into a crash-loop repeats this
whole investigation blind.

**5. Diagnosis of what was left out, in `internal/ast/embedder.go`.** `processBatch` counts
the entities that came back without a vector and emits a `Warn` at the end, with the label, the count and a
sample of up to 5 `path::uid`. Before, a silent `continue`.

## Use Cases

### UC-01: Embed a batch in which one text is not tokenizable
- **Actor**: `Embedder.processBatch`, called by `RunCycle` — the daemon's `embedding` module, `graphit ast embed`, the `ast_embed` MCP tool.
- **Preconditions**: local embedding client (ONNX + tokenizer); one of the texts in the batch triggers the `sugarme/tokenizer` panic.
- **Main Flow**:
  1. `processBatch` assembles up to `BatchSize` texts and calls `EmbedBatch`.
  2. `EmbedBatch` calls `encodeSingle` per text; the one that panics becomes an error and is marked.
  3. The tensor row of that text stays entirely as padding.
  4. Inference runs for the whole batch.
  5. Vectors are extracted only for the encodable texts; the one that failed comes back as `nil`.
  6. `processBatch` skips the empty vector, counts the entity as skipped and caches the rest.
  7. At the end of the label, a `Warn` names the count and a sample.
- **Alternative Flows**:
  - No text in the batch is encodable: `EmbedBatch` returns `batchSize` empty vectors without touching the ONNX session.
  - Tokenization returns an error instead of a panic: same handling.
- **Error Scenarios**:
  - ONNX inference failure: error for the whole batch, as before — not the case this task changes.
  - `outputData` smaller than `batchSize*hiddenDim`: extraction stops, instead of slicing past the end.
- **Postconditions**: the process stays alive; encodable entities get an embedding; the others stay pending and are retried on the next cycle, with no incorrect vector cached.
- **Affected Files**: `internal/ai/embedding_local.go`, `internal/ast/embedder.go`.

### UC-02: Record a panic of a daemon module
- **Actor**: `runProtected`, in the module supervisor.
- **Preconditions**: a module panics inside `Start`.
- **Main Flow**:
  1. The `defer`/`recover` captures the value.
  2. The error now carries the value AND `debug.Stack()`.
  3. The supervisor logs the line and applies the usual backoff.
- **Error Scenarios**:
  - Panic with an `error` instead of a string: same handling, `%v` formats both.
- **Postconditions**: the log identifies the file, function and line of the panic.
- **Affected Files**: `internal/daemon/project.go`.

## Test Cases & Acceptance Criteria

### Feature: Containment of the tokenizer panic
Ref: UC-01

#### Scenario: a text that makes the tokenizer panic becomes an error
```gherkin
Given an encoder whose tokenization panics with a slice bounds error
When encodeSingle is called with a text that triggers it
Then the call returns an error instead of propagating the panic
  And the error identifies the cause as "tokenizer panic"
  And no id or mask is returned
```

#### Scenario: a text that does not trigger the panic keeps being tokenized
```gherkin
Given the same encoder
When encodeSingle is called with a text that does not trigger the panic
Then tokenization succeeds
```

#### Scenario: a wholly non-tokenizable batch never reaches the model
```gherkin
Given an embedding client whose ONNX session is nil
  And an encoder that panics for every text in the batch
When EmbedBatch is called with 3 texts
Then the call returns with no error
  And returns exactly 3 vectors, all empty
  And the 3 texts were attempted
  And the ONNX session is never used — using it would bring the test down
```

### Feature: Stack trace in a module panic
Ref: UC-02

#### Scenario: the panic error carries the stack
```gherkin
Given a module whose Start panics with "boom!"
When runProtected runs it
Then the error starts with "panic: boom!"
  And contains the stack trace, with runProtected named
```

#### Scenario: panic with an error value
```gherkin
Given a module whose Start panics with a formatted error
When runProtected runs it
Then the error starts with "panic: formatted panic"
  And contains the stack trace
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ai/embedding_local.go` | Modified | `encodeSingle` converts the panic into an error; `EmbedBatch` degrades per text; `tk` becomes the `textEncoder` interface; guards in `mask` and in the vector extraction |
| `internal/ai/embedding_local_panic_test.go` | Created | Covers the containment of the panic and the wholly non-encodable batch |
| `internal/daemon/project.go` | Modified | `runProtected` includes `debug.Stack()` in the error |
| `internal/daemon/module_test.go` | Modified | The two `runProtected` tests compared the message by exact equality; they now require a prefix + the presence of the stack |
| `internal/ast/embedder.go` | Modified | `processBatch` counts and names the entities that came back without a vector |

## Trade-offs & Decisions

- **Contain instead of sanitize.** The alternative would be to transform the text (strip non-ASCII,
  normalize) before tokenizing. Rejected: without knowing what triggers the bug — and twelve
  synthetic candidates did not trigger it — any sanitization is guesswork that degrades the
  embedding of the whole corpus to protect against an uncharacterized condition.
- **`nil` instead of a zero vector.** Caching a zeroed vector would remove the rework, but it would plant
  in the index a vector that describes nothing and would survive until the file changed. The
  "empty vector = not embedded" contract already existed in `processBatch`; reusing it cost nothing.
- **Entities that fail are retried forever.** Since nothing is cached, every cycle
  tries again and fails again. The cost is the tokenization of those few entities, and the cycle
  already ran every two minutes anyway; `RunCycle` returns 0 and the expensive rebuild is not even
  triggered. Accepted in exchange for not having an incorrect vector cached.
- **A one-method interface just for the test.** `textEncoder` exists to allow injecting the
  panic. It is minimal indirection and `*tokenizer.Tokenizer` satisfies it with no adapter.

## Technical Debt

- [ ] The `sugarme/tokenizer` v0.3.0 bug is still open upstream (`ConvertOffset` without a clamp
  in `NormalizedString.Slice`). If a new release appears, upgrade and reassess whether the
  containment is still necessary — it stays cheap to maintain either way.
- [ ] It is not known WHICH entities of this corpus trigger the panic. The new `Warn` will
  name them on the daemon's next cycle; when they show up, it is worth reducing one of them to a
  minimal case and reporting it upstream.
- [ ] `TestHandleBatchAbandonsTheQueueOnCancel` (`internal/daemon/syncmodule_gate_test.go`) is
  **flaky and that predates this task** — verified in a worktree of a clean HEAD, where it
  fails when running the whole package and passes in isolation. It depends on state left by another test
  in the package. Not investigated here.

## System Knowledge

- **The daemon's `embedding` module is `ast.RunEmbeddingLoop`**, via `EmbeddingModule.Start`
  (`internal/daemon/adapters.go:37`), and not the wiki one, which is another module with another name.
- **The cycle has two phases and only the second is expensive**: `RunCycle` embeds, and only when it
  returns n > 0 does `triggerEmbeddingRebuild` rebuild the database to inject the vectors.
  A cycle that embeds nothing does not pay for the rebuild.
- **Reproducing the daemon's path locally**: build `./cmd/graphit` and run `ast embed`
  in the project — same `Embedder.RunCycle`. The ONNX library is looked for next to the
  executable and then in `LD_LIBRARY_PATH`; in a binary outside the runtime, point
  `LD_LIBRARY_PATH` at `~/.graphit/runtime/dev`, where `libonnxruntime.so` lives.
- **The CodeRankEmbed tokenizer** uses `BertNormalizer` with `clean_text`,
  `handle_chinese_chars` and `lowercase` turned on — three transformations that change the length
  between the original string and the normalized one, which is exactly the condition the
  `ConvertOffset` bug depends on. The special tokens are `[PAD] [UNK] [CLS] [SEP] [MASK]`, and it is the
  presence of one of them that takes `AddedVocabulary.splitWithIndices` to the `Slice` that fails.

## Progress Log

### 2026-08-10
- Confirmed in the daemon log: 66 crashes since 2026-07-29, always with the bound exactly
  equal to the capacity + 1.
- Discovered that `runProtected` was discarding the stack — fixed first, since it was what
  prevented any diagnosis.
- Panic reproduced with a full stack by a local binary running `ast embed`. Cause outside
  our code: `sugarme/tokenizer` v0.3.0, with no fixed version available.
- Twelve synthetic inputs tested against the real tokenizer; none reproduces it. Decided to
  test the containment instead of the upstream bug.
- After the fix, `ast embed` completed **4022 entities** where it used to die in ~2 s.
- Green: `internal/ai`, `internal/daemon` and `internal/ast` with and without `-race`; `go vet`,
  `golangci-lint` (0 issues) and `gofmt` clean on the files touched.
