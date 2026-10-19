---
title: Rust paired GGUF reranking
description: BGE v2-m3 F16 paired classification with bounded native execution.
type: specification
updated: 2026-10-24
---

# Paired GGUF reranking

`graphit_ai::gguf_rerank::GgufRerankClient` executes the actual
`BAAI/bge-reranker-v2-m3` classifier. It reuses the existing CXX bridge,
llama.cpp kernel lifecycle, artifact verification and retained native worker.
It does not derive rerank scores from embedding cosine similarity.

## Model and recipe

The canonical model revision is
`953dc6f6f85a1b2dbfca4c34a2796e7dde08d41e`.
The F16 GGUF is 1159774912 bytes with SHA256
`3c2de408d2c0a85a9472dc09f9d5a22c9b73743c6343952c15053299c777c298`.
Its runtime engine is the unmodified upstream revision selected by the
[Cargo integration](../development/llama-cargo.md).
The upstream architecture is a 24-layer, width-1024 BERT-family classifier
with one output and training context 8192. The SDK preserves the upstream
XLM-R tokenizer and dense/tanh/output classifier. F32 bias tensors inside the
F16 artifact are legitimate; alternative model or precision aliases are rejected.
See the complete [manifest recipe](rust_model_manifest.md).

The native tokenizer prepares `<s> query </s></s> document </s>` without
parsing special tokens in user text. Longest-first, right-side truncation
includes the four special tokens in the configured 4..8192 bound. Each actual
pair uses an explicit noncausal RANK context with embeddings enabled and F32 K/V.
CPU requests the pinned SDK's automatic flash attention selection; accelerator
contexts explicitly enable its flash attention implementation to avoid
materializing the full attention matrix. The SDK chooses supported operator
placement; accelerator selection is not a guarantee that every operator
executes there. Automatic attention selection on CPU does not select an
accelerator or change `provider.rerank.execution.device`. Its requested context,
batch and microbatch size follow the actual truncated pair length. The SDK may
round allocation upward;
the 64-token fixture observed 256 context tokens rather than an unconditional
8192. Context ownership ends after each operation, including cancellation and
failure; the shared model remains loaded.

The only output is the finite single classifier logit. `RawLogit` returns it
as f64; explicit `Sigmoid` applies a numerically stable logistic transform.
There is no softmax, guessed positive class or automatic score interpretation.

## Client, worker and execution

`GgufRerankClient::load(GgufModelSource, options, limits, context)` verifies the
actual streamed artifact before loading it. `score(query, documents, context)`
preflights the complete raw input and original context, tokenizes each pair once
and runs those exact IDs. An error returns no partial score vector. Errors
retain their actual cause and native status while safe diagnostics expose only
finite typed categories. They never include prompts, documents or credentials.

Options supply threads, token bound, F16 precision, score transform and an
explicit `LocalProvider` plus index. CPU is the product default; CPU0, CUDA(index),
Metal0 and Auto use the [shared device contract](rust_local_runtime.md).
Explicit unavailable devices fail. No model/hash/Auth/cancel/deadline failure
causes a CPU retry.

`native_embedding::worker::GgufRerankWorker::start` creates the non-Send native
client inside its existing bounded worker thread. `score` exchanges owned
strings/results through the bounded queue. `shutdown` seals admission and
retains the actual join. It shares the private worker lifecycle implementation
with `NativeEmbeddingWorker`, without creating a second executor or a thread
per request. [Worker ownership](rust_native_embedding_worker.md) covers queued
cancellation, deadline, dropped callers and retained joins.

Models captures this worker lazily only for an actual local rerank purpose.
`search.rerank=false` causes no rerank discovery, download, capture, native
session, computation or RPC. The query override selects effective demand;
the existing rerank RPC is sufficient, without an added admission wire flag.
Remote clients keep their existing provider contracts and do not manufacture
local ExpectedModel metadata.

## Recorded qualification and limits

Real F16 CPU tests passed paired EN/PT ranking, explicit sigmoid, a truncated
64-token pair, actual native-callback cancellation, subsequent client reuse,
verified staging cleanup and retained worker shutdown. The CPU comparison to
the original F32 reference passed all six EN/PT tokenizer-ID sequences and
both ranking orders. The declared absolute-logit tolerance was 0.05; the
observed maximum was 0.018140792846679688. This is measured finite-fixture
agreement, not zero numerical loss or an all-language quality benchmark.

The reference fixture took 21.62 seconds inside its test; Cargo wall time also
included build-lock wait and compilation and is not inference latency.
The same six-pair comparison passed on actual CUDA0 on Linux/RTX3060, with
exact token IDs and ranking order, maximum absolute logit error
0.014725208282470703 and the same 0.05 tolerance. The test took 51.45 seconds;
wall time 54.73 seconds includes artifact verification and loading. Process-only
one-second sampling observed CUDA libraries and at most 692 MiB of VRAM. That
sampled maximum is not a true peak or proof that every graph operation ran on GPU.
Metal and other native platforms remain unqualified. Backend discovery or a
standalone GPU matmul alone does not qualify this classifier.


After the shared startup-join correction, the actual paired F16 worker repeat
also passed (37.74 seconds in the test, 37.75 seconds wall time). It verifies
real thread-local loading, paired output, original pre-cancellation and joined
shutdown under the revised shared lifecycle. These totals include staging and
loading and do not establish encoder-only latency or comparative efficiency.

The 2026-10-22 attention correction extends the existing paired CPU/CUDA focal
with a full 8192-token pair prepared by the unchanged tokenizer recipe. CUDA
with explicitly enabled flash attention passed in 8.13 seconds, including the
existing EN/PT ranking, score association, sigmoid, 64-token context,
cancellation and reuse checks. SDK diagnostics showed CUDA flash graph placement.
The long-pair logit was 0.42159808. Source, Models and other desktop processes
remained running. One-second global GPU sampling observed 4141..5283 MiB used;
this includes other processes and is not a true peak or a capacity guarantee.

CPU with automatic attention passed the same focal in 145.88 seconds with the
unchanged two threads and 180-second fixture request deadline. Its long-pair
logit was 0.4090107; the observed cross-device difference was 0.01258738.
Linux child peak RSS was 9717476 KiB (about 9.27 GiB). The disabled-attention
baseline had shown about 13.3 GiB RSS and failed to complete the long pair in
187.74 seconds; it produced no long-pair logit. That direct native fixture
failure is not classified as an OOM. Product errors still pass through the
original request context so cancellation and expiry remain terminal.

These timings include staging, model loading and the whole focal, rather than
isolated inference. The memory observation addresses the concrete oversized
attention operation; CPU memory for long pairs remains significant. Historical
F32-reference receipts above remain historical evidence and were not repeated
or represented as qualification of the new CPU attention selection. Metal and
unexecuted platforms remain unqualified. No model, precision, token recipe,
device fallback, request deadline or rerank opt-in policy changed.

## Upstream output allocation

The runtime uses upstream commit `620d2225cc436ab0b001e3200e590e3b286ef9ab`,
which contains the official absent-logits correction `ed019cd8b94a8194881e3d2448075156939d32a1`.
Graphit's former embedding/BERT output patch and patch-keyed CMake SDK producer
are removed. RANK pooling still returns the classifier score, not embedding
similarity. The model artifact and paired-tokenization recipe remain unchanged.

The affected real CPU fixture `actual_v2_m3_f16_bounded_output_cpu` passes with
the Cargo-built shared engine. A diagnostic run sampled peak PSS 1,497,374 KiB,
anonymous memory 293,424 KiB and shared-memory PSS 1,132,596 KiB, including the
verified model snapshot. These are per-field sampled maxima, not measurements
of a memory leak or a before/after gain. Native output remained 0.96 MiB and
compute storage 20.51 MiB at 512 tokens: the unnecessary per-token vocabulary
logits allocation did not return. The test completed in 25.43 seconds, including
staging/loading and under concurrent compilation; this is not inference latency.
Earlier static-link measurements remain historical Task evidence rather than
qualification of the final shared-runtime package. GPU paired scoring and
EmbeddingGemma2 compatibility pass on the available Linux RTX3060; that does not
qualify other GPUs, native platforms or remote CI.
