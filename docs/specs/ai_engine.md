---
title: AI Engine Specification
description: "Provider/profile resolution, local/direct/broker embedding, rerank, compatibility, and daemon contracts."
type: specification
updated: 2026-10-29
---

# AI Engine Specification

This is the current Rust boundary between completion, embeddings and rerank. The
[user configuration guide](../guides/ai_models.md) gives executable examples, protocols and pinned
artifacts. A configured model/backend is not an access grant and does not enable unrelated work.

## Ownership model

| Concern | Owner | Identity |
|---|---|---|
| Agent completion | Captured native CLI and its account/model configuration | PROJECT/USER, selected binding and native session |
| Embedding | Active Auth provider/profile plus admitted service and selected model recipe | Protocol/model/revision/dimensions/recipe, not URL alone |
| Rerank | Independent admitted service and query/document scoring model | Native rerank model/revision/recipe; not an embedding substitute |
| Local execution | Existing model process, llama.cpp/cxx and independent devices | Verified GGUF artifact/recipe and slot generation |

Auth stores provider topology separately from account secrets/tokens. Ordinary config controls
module/search choices, native bindings, model slots and runtime policy. It rejects obsolete
`ai.embedding.*`, `ai.rerank.*` and `ai.agent_args` namespaces instead of reading them as aliases.

## Service selection

A Local provider independently chooses `local`, `direct`, `broker` or `disabled` for embedding
and rerank. Local executes the installed task slot; Direct uses its exact configured full endpoint,
protocol, declared model/revision/recipe and the selected account key. Broker uses only authenticated
discovery advertisements. Disabled requests fail explicitly.

A Broker provider accepts **only Broker or disabled** for both services, defaulting enabled
services to Broker. Local/Direct is rejected at create/update/decode. An endpoint, a local model
slot or a profile's spare API key cannot override that rule. Missing discovery/service, authorization,
invalid response, cancellation or deadline never triggers fallback to another provider or CPU model.

Credential/discovery leases are scoped to the actual identity, provider revision, service and
Original request. Reuse valid metadata/credentials; do not perform needless discovery or login for
every operation. Revalidate admission across inference and delivery. Credential validity and a
corpus snapshot's version are separate facts.

## Local model contract

Local embedding is EmbeddingGemma2 GGUF BF16; local rerank is BAAI/bge-reranker-v2-m3 GGUF F16.
The closed [manifest](rust_model_manifest.md) and [binder](rust_model_bindings.md) verify task,
relative artifacts, size/digest, backend, recipe and execution compatibility. Embedded tokenizers
and model-specific preparation are retained. There is no ONNX backend, second FFI or second local
server. Both tasks use the existing llama.cpp/cxx process.

`daemon.models` stores canonical `embedding`/`rerank` slots. `models install` preserves existing
selection and explicit `slots:[]`; `models select` changes the selected task explicitly. Models
load on demand. Each unloads after `models.idle_timeout_seconds` without actual inference
(default 300 seconds), after active work/native cleanup finishes. Cache hits do not extend weight
residency. Unloading preserves installed artifacts, vector identities and indexed embeddings.

## CPU and accelerators

Each local service has independent `execution:{device,device_id}`. CPU/device 0 is the default;
CUDA/Metal selection is explicit and requires an available native backend/driver/device. Explicit
unavailable acceleration fails clearly. Auto attempts the available accelerator and reports the
actual device and any CPU fallback. A packaged GPU backend does not opt a CPU provider into GPU.
Remote execution rejects local hardware options because the remote service chooses its hardware.
See [GGUF worker](rust_native_embedding_worker.md) and [model service](rust_model_service.md).

Background workers use proportional native runtime/thread defaults and low-priority background
operation. They must not retain whole corpora or initialize disabled purposes. This is not a
machine-wide reservation/quotas framework or a promise that all memory is GPU memory.

## Embedding and index consistency

Query/document roles, prefixes, tokenization, truncation, pooling, dimensions and normalization
belong to the model/protocol recipe. Equal width does not establish compatibility. A model or
recipe change invalidates incompatible vectors and follows normal background reindexing;
authoritative text/history and compatible rows remain available.

AST, Knowledge, Memory and Task attach vectors incrementally to their existing searchable corpus.
Recheck current identity/content fingerprint and vector-space metadata before attaching an
inference result. Superseded input cannot become current. Committed compatible partial coverage
can be searched while remaining inference runs. FTS/graph/Source do not wait for embedding.
`modules.embedding=false` preserves those operations and prevents embedding preparation/loading/work.
Requested semantic service failures are explicit, not successful lexical fallbacks.

## Rerank modes and activation

`search.rerank` defaults false. Request true/false overrides effective configuration; selecting or
installing a rerank model does not activate it. Disabled rerank performs no discovery, download,
preparation, model loading or inference for rerank. Embedding enablement is independent.

Local BGE scores real query/document pairs with noncausal RANK pooling and the original classifier.
Default output is the raw logit, with F16 precision explicitly pinned. It is neither EmbeddingGemma
nor cosine similarity. The documented recipe controls context/truncation and optional sigmoid.
Direct native protocols are Cohere/Voyage/Jina variants; Broker discovery advertises its endpoint
and identity. No implicit embedding-simulation fallback occurs.

AST/Knowledge retrieval preserves candidate identity and score-index correspondence, then applies
returned rerank order with pagination under that query's contract. Original retrieval scores and
rerank scores have distinct meanings. Missing required Source or an invalid/out-of-range/duplicate
score association is an explicit error, not an excuse to substitute unrelated text or silently
skip a configured service. [Rerank consumer](rust_search_rerank_consumer.md) defines the adapters.

## Completion and sessions

`ai.cli` explicitly selects a completion executable; otherwise use the effective native binding.
A missing selection/program is unavailable, not a reason to probe unrelated CLIs. No general
`ai.Conversation` Go implementation remains. Each Rust adapter uses its documented protocol and
owned Process lifecycle; native session identities stay bound to their actual project/user/CLI.

Knowledge Ask, Incoming Conversation, Live and Dream retain their own admission and final-answer
gates. Preserve Unicode/text chunk boundaries and native tool-call correlation. Progress is not
a committed tool effect or final answer. Native success, process exit/EOF, cancellation/remaining
cleanup and current authorization govern delivery. Do not resume a failed session by silently
choosing another identity, CLI or model.

The [completion matrix](../guides/ai_models.md#agent-completion) links exact protocol specifications.
Kiro V3 and OpenCode 2 are current; AGY replaces Gemini CLI. Compatibility tests follow documented
contracts with controlled local peers and do not require vendor installations/accounts/plans.

## Errors and observability

Preserve protocol/auth/domain/transport/native/cancellation/deadline errors and actual confirmed
publication effects. No tool invocation counter proves a record committed. Public diagnostics
must not contain account keys, bearer tokens, private native session IDs or provider response
bodies. Model generations, selected device/fallback and source/semantic coverage explain operation
without inventing a general benchmark campaign. Platform/GPU support is qualified only where
executed; prepared builds and historical ONNX receipts do not qualify current GGUF execution.
