---
title: Rust local GGUF execution
description: Local GGUF execution, model recipes and caller-owned context.
type: specification
updated: 2026-10-27
references:
  - target: {type: task, id: tsk-170b}
    relation: implements
  - target:
      type: task
      id: tsk-658a
    relation: implements
  - target:
      type: task
      id: tsk-94c3
    relation: derived_from
  - target:
      type: task
      id: tsk-204c
    relation: derived_from
---

# Rust local GGUF execution

This reference is for local inference consumers of `graphit-ai`. Execution data
lives in `local_execution`; verified streaming reads and `GgufModelSource` live in
`model_artifact`. The synchronous [embedding](rust_native_embedding.md) and
[paired reranking](rust_gguf_rerank.md) clients use the existing CXX bridge and
pinned llama.cpp SDK. The former ONNX executor, external tokenizer loader and
tensor reducers have been removed. Historical ONNX qualifications do not qualify
these GGUF paths.

## Canonical device selection

Auth supplies separate `LocalExecution { device, device_id }` values for
`provider.embedding` and `provider.rerank`. CPU/index zero remains the profile
default. CUDA accepts a nonnegative runtime index; Metal requires macOS/index
zero. Auto discovers a supported accelerator at the requested index and reports
CPU/index zero only when none is registered. CoreML and the old ONNX profile
shape are unsupported; no alias decoder is maintained.

`LocalDeviceRegistration` reports requested/effective provider and index plus a
finite `AcceleratorUnavailable` reason for Auto selecting CPU. Explicit accelerator
selection cannot return CPU success. This is registration and selected model
offload, not proof that every graph operation executed on GPU. Model integrity,
model loading, cancellation and deadline errors never retry on CPU.
Auto also preserves allocation failures when a registered accelerator lacks
memory; it does not retry the operation on CPU or change the configured device.

The existing SDK registry is discovered only for Auto/CUDA/Metal demand. CPU
selection does not call plugin discovery. Discovery is serialized within the
shared native kernel; each owner retains a null-terminated list selecting one
actual accelerator. GPU model loading requests all layers on that device; CPU
uses zero GPU layers. Packaging owns backend modules, ABI and relative loader
paths, with its own qualification.

## Verified artifacts and original context

`GgufModelSource` contains model/revision, bundle/path, minimum/maximum byte counts
and SHA-256. It is data, without diagnostic/serialization traits or authority.
Callers first authorize and bind the actual captured manifest. `capture_gguf`
reuses the streaming reader, validates the portable relative path and canonical
bundle, checks the original cancellation/deadline between chunks, enforces the
observed byte bound and verifies the digest before SDK load.

Verified bytes are staged in an owned `.gguf-*` directory under the installation's
private `tmp/` directory. The model child resolves this parent from its bootstrapped
GLOBAL installation; it never derives a write destination from a model bundle or
an RPC request. The service and workers carry the existing Core captured directory
as local infrastructure, separate from model identity, recipe and configuration IDs.
The shared capture producer checks its private handle with the caller's original
cancellation/deadline before and after staging, and uses `tempdir_in` without a
system-temporary fallback. An unavailable, replaced or nonprivate parent fails the
load. This keeps a full verified GGUF copy off a system `/tmp` that may be tmpfs;
the filesystem backing GLOBAL still determines its physical storage.

A native owner drops its mappings before its temporary directory is removed.
Embedding and rerank share this producer and preserve SHA-256, EOF, byte limits,
Original, cancellation and errors. Model identity and CPU/GPU selection do not
change; no re-embedding follows from the staging location. Remote Broker/direct
execution does not create a local snapshot. No periodic cleanup or environment
override controls this path.
Synchronous filesystem calls and native callbacks are cooperative boundaries,
not hard preemption. Real IO/SDK causes stay local; public diagnostics expose
controlled kinds and native status without model text, private paths or secrets.

`InferenceContext` keeps the caller's same cancellation and absolute deadline
across preparation, admission, native load, processing and delivery. The worker
retains actual joins after caller timeout/Drop. See the
[shared service](rust_model_service.md) and [worker lifecycle](rust_native_embedding_worker.md).
