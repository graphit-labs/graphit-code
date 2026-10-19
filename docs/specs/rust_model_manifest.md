---
references:
  - target: {type: task, id: tsk-3832}
    relation: implements
  - target: {type: task, id: tsk-926b}
    relation: uses
---

# Rust GGUF model backends

`ModelManifest` contains model ID/revision, task, artifact declarations and one
explicit backend. The closed `graphit-model/v1` schema accepts
`gguf-embeddinggemma2` and `gguf-bge-reranker-v2-m3`. Artifact format is `gguf`.
Unknown, duplicate, missing, removed or cross-family fields fail. The removed
ONNX/external tokenizer/tensor schema has no compatibility decoder.

`ManifestLimits` bounds manifest bytes, artifact count, role/path/ID/revision
lengths, dimensions and token count. The captured-source binder preserves the
exact Original, canonical bundle, relative paths, node kind, SHA-256 digests,
required artifacts and selection. Metadata does not grant native reopen access;
SDK load verifies the actual consumed artifact again.

## EmbeddingGemma2

Only task `embedding` is accepted. The required GGUF model role has an embedded
tokenizer, `architecture: gemma-embedding2`, noncausal attention and mean pooling.
Dimensions are 128/256/512/768, precision is explicit bf16/f32 and token limit is
at most 8192. The exact Query/Document/CodeQuery/CodeDocument prefixes, F32 L2 and
truncate-then-F32-L2 MRL recipe remain unchanged. See
[native embedding](rust_native_embedding.md) for the complete recipe.

## BGE reranker v2-m3

Only task `rerank` is accepted, with its own required model role. The complete
recipe is explicit:

```json
{
  "architecture": "bert",
  "tokenizer": "embedded",
  "attention": "noncausal",
  "pooling": "rank",
  "pair_template": "xlm_roberta",
  "truncation": "longest_first_right",
  "max_tokens": 8192,
  "precision": "f16",
  "score": "raw_logit",
  "outputs": 1
}
```

The token limit includes all four special tokens and must be 4..8192 within the
caller limit. F32/BF16/Q4/Q8 precision, embedding dimensions/MRL/mean pooling,
softmax/auto transforms and unrelated architectures are rejected. `sigmoid` is
the other explicit score transform. Native load checks the actual model family,
24 layers, width 1024, one classifier output, F16 file type, training context
8192 and XLM-R special token IDs. GGUF pooling metadata is not required: the
operation explicitly selects RANK. See [paired rerank](rust_gguf_rerank.md).

The runtime uses the unpatched official engine selected by the normal
[Cargo dependency](../development/llama-cargo.md). Changing the engine build
alone does not change the model artifact, recipe or portable vector identity.
No model-family alias or implicit precision/provider fallback is retained.

## Identity and validation

`ConfigId` covers the verified recipe/content, operational paths, threads/queue
and canonical device/index. Portable vector identity excludes hardware,
scheduling and paths while retaining embedding content and recipe. Rerank has no
embedding width and cannot manufacture an embedding schema.

`model_bindings_tests` exercises both actual captured metadata families, MRL,
explicit precision, field rejection, paths/digests, exact limits, Original
identity, peer mutation and cancellation: ten tests passed on Linux. These
integrity-only fixture bytes are never submitted to an SDK. Native inference
and product journeys need their separately recorded real-model evidence.
