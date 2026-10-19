---
references:
  - target: {type: task, id: tsk-170b}
    relation: implements
  - target: {type: task, id: tsk-3832}
    relation: implements
  - target: {type: task, id: tsk-c4ba}
    relation: implements
  - target: {type: task, id: tsk-36ec}
    relation: implements
  - target: {type: task, id: tsk-9840}
    relation: uses
  - target: {type: task, id: tsk-9bc3}
    relation: supports
---

# Captured GGUF model bindings

`bind_captured_model(reader, original, manifest_relative, expected, limits)`
validates portable metadata and artifact integrity against the caller's existing
Core captured source. The same original context is checked before and after
reads, decoding and final revalidation. The result retains validated metadata
and snapshot fingerprint, never the reader or an authorization grant.

`read_captured_model_manifest(reader, original, manifest_relative, expected,
limits)` is the shared bounded metadata reader used by that full binding and
runtime declarations. An empty relative path selects a captured root FILE;
the full bundle binder still requires its portable nonempty manifest path.
This reader establishes no weight verification. Its caller owns the final
reader fence: full binding checks all declared artifact nodes and the captured
tree, while a runtime declaration fences only its manifest and verifies actual
weights at native load.

The closed `graphit-model/v1` schema supports the two recipes in
[GGUF model backends](rust_model_manifest.md): EmbeddingGemma2 embedding and
BGE reranker v2-m3 paired classification. ID, revision and task must match the
explicit selection. Artifacts have unique roles and portable relative paths,
GGUF format, a SHA256 digest and byte bounds. Schema validation cannot prove
that artifact bytes are a usable native model; the pinned SDK validates that
separately at lazy load.

The eight `ManifestLimits` fields bound manifest bytes, artifact count,
role/path/ID/revision bytes, dimensions and tokens. All limits are positive.
Unknown, duplicate, missing, null or cross-task recipe fields are errors;
trailing JSON is rejected. Before reading, the binder checks captured node kind
and length. It rejects early EOF and excess bytes, verifies the captured
artifact nodes and revalidates the tree before publishing a binding.

`decode_manifest(bytes, limits, check)` applies the same metadata schema to
explicit server configuration without manufacturing a PROJECT snapshot. It
provides no artifact verification or execution authority. Both local clients
subsequently stream and verify their actual artifact through
`model_artifact::read_artifact` under the same inference context.

Errors preserve typed stage/cause, real I/O kind and native code, and safe JSON
category/location. Display and Debug contain no manifest input, raw parser
message or secret. Cancellation/deadline and integrity failures are terminal.
The removed ONNX reducers and external tokenizer schema are not active APIs.
Native output validation belongs to the [embedding](rust_native_embedding.md)
and [paired reranking](rust_gguf_rerank.md) producers.

Ten current binding tests cover both families, strict recipes and selection,
paths/roles/digests, optional absence versus mismatch, exact limits and original
context/revalidation. Their integrity fixtures are deliberately metadata-only;
they do not qualify native model execution.
