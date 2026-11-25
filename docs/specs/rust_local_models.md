---
title: Rust local GGUF model clients
description: Local GGUF execution, model recipes and caller-owned context.
type: specification
updated: 2026-10-21
references:
  - target: {type: task, id: tsk-170b}
    relation: implements
  - target:
      type: task
      id: tsk-e87c
    relation: corrects
  - target:
      type: task
      id: tsk-26fe
    relation: implements
  - target:
      type: task
      id: tsk-c4ba
    relation: uses
  - target:
      type: task
      id: tsk-658a
    relation: uses
---

# Rust local GGUF model clients

This reference is for Rust inference consumers. `NativeEmbeddingClient` produces
EmbeddingGemma2 embeddings; `GgufRerankClient` produces one learned classification
score per query/document pair. They use the pinned SDK and embedded GGUF
vocabularies. Their declarations and recipes are different
[model backends](rust_model_manifest.md), with separate devices, options and
limits. No embedding distance or cosine substitutes for reranking.

Both synchronous clients remain on their creating thread. They expose the actual
captured model/revision and registered device. `NativeSdkRoot` belongs to bootstrap
and outlives real worker joins. Product routing, Auth, provisioning and profile
selection remain consumer responsibilities.

## Embedding

`NativeEmbeddingClient::load(GgufModelSource, NativeEmbeddingOptions,
EmbeddingLimits, InferenceContext)` verifies/stages the exact artifact before load.
Options explicitly include threads, device/index, dimensions, precision and token
limit. Query, Document, CodeQuery and CodeDocument preserve their exact prefixes,
projection, F32 normalization and MRL recipe. Complete long documents use the
qualified token windows. See [native embedding](rust_native_embedding.md).

## Paired rerank

`GgufRerankClient::load(GgufModelSource, GgufRerankOptions,
NativeRerankLimits, InferenceContext)` loads the actual v2-m3 F16 classification
model. `score(query, documents, context)` returns complete owned finite f64 scores
in document order. Raw logit is the default model recipe; sigmoid is explicit.
No batch is published after any item fails.

Raw request preflight is public and reused before optional artifact capture and
worker admission. Each pair is tokenized once by the SDK, bounded with the
explicit paired recipe, then passed as those same token IDs to the classification
head. The context is sized from that pair and freed on success/error/cancel;
only model weights remain resident. See [GGUF rerank](rust_gguf_rerank.md).

## Thread-safe producers

The existing worker adapter exposes concrete `NativeEmbeddingWorker` and
`GgufRerankWorker` proxies. Their private shared lifecycle uses bounded mpsc,
oneshot, original cancellation/deadline, sealing and retained real joins.
Neither native client crosses a Tokio executor or process boundary.
There is no automatic runtime creation or thread per request.

The old `LocalModelRuntime`, `LocalEmbeddingClient`, `LocalRerankClient`, ONNX
recipes, tiny tensor fixtures and their acceptance checks are superseded by the
GGUF replacement. Historical records remain historical evidence; actual paired
GGUF, worker and consumer qualification is recorded separately.
