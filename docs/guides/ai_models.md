---
title: AI Models, Providers, and Agent CLIs
type: guide
updated: 2026-10-29
tags: [ai, models, completions, embeddings, rerank, agents, broker]
references:
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
  - target: {type: task, id: tsk-170b}
    relation: implements
  - target: {type: task, id: tsk-28ee}
    relation: implements
  - target: {type: task, id: tsk-0af7}
    relation: implements
  - target: {type: task, id: tsk-be6e}
    relation: uses
  - target: {type: task, id: tsk-9953}
    relation: implements
  - target: {type: task, id: tsk-a9c4}
    relation: supports
  - target: {type: task, id: tsk-6365}
    relation: supports
  - target: {type: task, id: tsk-ec15}
    relation: implemented_by
  - target: {type: task, id: tsk-b74a}
    relation: uses
  - target: {type: task, id: tsk-247e}
    relation: implemented_by
  - target: {type: task, id: tsk-0f32}
    relation: implemented_by
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-c847
    relation: supports
  - target: {type: task, id: tsk-13b9}
    relation: supports
  - target: {type: task, id: tsk-61a0}
    relation: derived_from
  - target: {type: task, id: tsk-ec10}
    relation: derived_from
  - target: {type: task, id: tsk-27c1}
    relation: implemented_by
  - target: {type: task, id: tsk-5f7d}
    relation: relates_to
  - target: {type: task, id: tsk-9db8}
    relation: implements
  - target: {type: task, id: tsk-e6ff}
    relation: specified_by
  - target: {type: task, id: tsk-7f7c}
    relation: specified_by
  - target: {type: task, id: tsk-625d}
    relation: implements
---

# AI Models, Providers, and Agent CLIs

Rust Knowledge Ask and Incoming Conversation target [OpenCode 2 via ACP protocol 1](../specs/rust_opencode_completion.md). OpenCode 1 and Gemini CLI support are removed; use Antigravity through `agy`. Google Gemini API providers and models remain supported. Tests use controlled peers and real Graphit consumers without requiring a vendor binary, account or plan.

Embedding and rerank resolve through the active Auth provider/profile. A Local
provider selects each service independently as `local`, `broker`, `direct` or
`disabled`. A Broker provider defaults both services to `broker` and permits
only `broker` or explicit `disabled`; `local` and `direct` are rejected before
persistence and when reading a stored document. Agent completion uses its
separate CLI selection.

| Layer | Purpose | Local model | Configuration |
|---|---|---|---|
| Agent completion | Synthesis and agent work | Selected agent CLI | agent/CLI settings |
| Embedding | Semantic vectors for AST, Knowledge and Memory | EmbeddingGemma 2 GGUF BF16 | provider `embedding` plus `daemon.models` slot `embedding` |
| Rerank | Optional second-stage relevance | BAAI BGE Reranker V2 M3 GGUF F16 | provider `rerank`, slot `rerank`, and `search.rerank` |

Local device selection defaults to CPU/device 0. Embedding and rerank have
separate device choices, and selecting a rerank provider or installing its model
does not enable reranking. `search.rerank` defaults to `false`; disabled rerank
does not discover a rerank endpoint, prepare, download, load or run a reranker.

The [captured-model binder](../specs/rust_model_bindings.md) validates the GGUF
manifest, recipe and artifact digest under the original request. Installation,
Auth service admission and actual inference remain distinct. A valid model file
or profile does not grant PROJECT access.

## Local provider and device selection

Create and activate a Local profile with explicit CPU choices:

```bash
graphit provider add workstation --type local \
  --embedding-mode local --embedding-device cpu --embedding-device-id 0 \
  --rerank-mode local --rerank-device cpu --rerank-device-id 0
graphit login --provider workstation --profile workstation --username workstation
```

`login` activates the profile. Select an already stored profile with
`graphit account use workstation`; inspect its provider with
`graphit provider show workstation`. Provider updates preserve omitted fields
and invalidate credentials bound to the old provider revision: log in again
after an update.

For example, use CPU embedding and CUDA rerank independently:

```bash
graphit provider update workstation \
  --embedding-device cpu --embedding-device-id 0 \
  --rerank-device cuda --rerank-device-id 0
graphit login --provider workstation --profile workstation --username workstation
```

Use `--embedding-device auto` or `--rerank-device auto` independently when GPU
selection may fall back to CPU. Explicit `cuda` requires the selected CUDA
backend/device; it reports an error rather than silently choosing CPU. `auto`
records the actual device and a fallback reason if no accelerator is available.
`metal` selects the local Metal backend on macOS. CPU uses device ID 0; the
accelerator ID selects the device within that backend. Availability depends on
the native package and driver, not just the CLI flag.

The private Auth service representation is `execution`, for example:

```json
{
  "embedding": {"mode": "local", "execution": {"device": "cpu", "device_id": 0}},
  "rerank": {"mode": "local", "execution": {"device": "cuda", "device_id": 0}}
}
```

This is a topology fragment, not a complete Auth document. Configure it through
the CLI. `provider show` returns a redacted view with flattened `device` and
`device_id`; it is not the persisted document codec. Hardware flags apply only
to Local services; Broker and Direct services reject local device options.

## Agent completion

Completion is separate from embedding/rerank. Graphit captures `ai.cli` if explicitly configured,
otherwise the selected native agent binding's executable. It does not search unrelated installed
CLIs as fallback. Missing/unsupported executables remain unavailable. Actual use needs that CLI's
own model/account configuration; compatibility tests use controlled documented peers without
requiring a vendor binary, account or paid plan.

```sh
graphit config --global set ai.cli claude
# Restart the daemon to reattach Ask with this captured completion binding.
graphit daemon restart
```

`ai.agent_args` and `ai.agent_args.*` are rejected configuration namespaces. Configure supported
host-native options through that host's maintained files and Graphit's explicit request contracts;
do not add unsupported permission flags to an obsolete configuration key.

| Product | Executable | Current protocol contract |
|---|---|---|
| Claude Code | `claude` | [Structured print stream and Incoming continuation](../specs/rust_claude_completion.md) |
| Antigravity | `agy` | [Current AGY stream/continuation](../specs/rust_antigravity_completion.md) |
| Grok CLI | `grok` | [Structured Ask/Incoming sessions](../specs/rust_grok_completion.md) |
| Cursor Agent | `cursor-agent` | [Ask and Incoming continuation](../specs/rust_cursor_completion.md) |
| Codex | `codex` | [Thread stream and exec resume](../specs/rust_codex_completion.md) |
| OpenCode 2 | `opencode` | [ACP protocol 1](../specs/rust_opencode_completion.md) |
| Kiro V3 | `kiro-cli` | [ACP engine V3](../specs/rust_kiro_completion.md) |
| GitHub Copilot CLI | `copilot` | [Prompt JSONL and Incoming](../specs/rust_copilot_completion.md) |
| Qwen Code | `qwen` | [Structured stream/continuation](../specs/rust_qwen_completion.md) |
| Kimi Code | `kimi` | [Native REST/WebSocket execution](../specs/rust_kimi_completion.md) |

Completion executables and installable host bindings are different catalogs: not every completion
CLI is a native agent-file destination. Kiro V2/preview, OpenCode 1 and Gemini CLI are retired;
Google Gemini API/model protocols remain supported independently. `deepseek` is not a CLI alias.

Knowledge Ask accepts a literal requested model and either explicit page slugs or ranked retrieval.
Conversation retains the exact native session identity with its project/user/CLI binding; it
cannot silently resume under another CLI or invent a session on failure. Native clear/compact
availability is host-specific, not a generic command every protocol implements. Final delivery
requires current authority, actual native success/EOF and joined process ownership; partial
progress is not a successful answer. See [Incoming conversation](../specs/rust_knowledge_conversation_app.md).

Live captures the workspace-selected binding and its own permissions. Dream uses its scoped
Memory workflow and explicit program/model, not a reusable global completion session or an old
`ai.Conversation` object. Native tool policy is not an OS sandbox. See
[Dream](../specs/dream_module.md) and [Live](../architecture/rust-knowledge-ai-live.md).

## Embedding modes

| Mode | Execution | Provider fields | Profile credential |
|---|---|---|---|
| `local` | Installed EmbeddingGemma 2 GGUF | local device/device ID | Local profile |
| `direct` | Selected remote embedding API | protocol, full endpoint, model, model revision, recipe, dimensions | `--embedding-api-key` |
| `broker` | Discovered Graphit embedding service | Broker endpoint | Broker login token or Local profile `--broker-key` |
| `disabled` | Explicitly unavailable | none | none |

Direct embedding example; replace the declared model/revision/recipe and width
with the actual deployment's values:

```bash
graphit provider add direct-ai --type local \
  --embedding-mode direct --embedding-protocol openai-compatible \
  --embedding-endpoint https://ai.example/v1/embeddings \
  --embedding-model your-embedding-model \
  --embedding-model-revision deployment-2026-10 \
  --embedding-recipe retrieval-v1 --embedding-dimensions 768 \
  --rerank-mode disabled
graphit login --provider direct-ai --profile direct-ai --username alice \
  --embedding-api-key "$EMBEDDING_API_KEY"
```

The endpoint is the complete request URL; Graphit does not append
`/embeddings` to it. Embedding protocols are `openai`, `openai-compatible`,
`cohere`, `voyage`, `google-gemini-1` and `google-gemini-2`. Google also requires
its explicit matching `--embedding-batch-endpoint` and a `models/...` model ID.
Protocol-specific request dimensions are separate from the declared output
width and are accepted only when that protocol/model supports them.

`model_revision` and `recipe` are explicit, nonempty deployment/model identity
and reproducible input/output recipe declarations. They are not inferred from
a URL or replaced with a fabricated model-file hash.

## Broker services

For Broker-managed embedding and rerank, use native Broker login:

```bash
graphit provider add managed-ai --type broker \
  --broker-endpoint https://broker.example \
  --embedding-mode broker --rerank-mode broker
graphit login --provider managed-ai --profile managed-ai
```

A provider with `--type broker` uses the Broker exclusively for every enabled
embedding or rerank service. Missing discovery, authentication, advertised
service or inference failures are explicit errors; Graphit does not fall back
to Local or Direct execution and does not prepare or load their local models.
Rerank remains opt-in (`search.rerank=false` by default), so configuring a
Broker rerank service alone does not discover or invoke it.

A Local provider can instead carry a Broker endpoint and authenticate its
services with a Local profile's `--broker-key`. Choose each service mode
independently. A Local provider may keep one purpose Local or disabled and
select Broker for the other. A Broker service requires a configured Broker
endpoint. Ordinary updates preserve omitted settings; explicitly changing a
provider's type selects compatible service defaults for that type while
preserving disabled services. Incompatible service flags still produce errors.

Discovery supplies the complete service endpoint, protocol, model,
`model_revision`, recipe and service revision. Embedding additionally advertises
dimensions and request limits; a native reranker does not advertise vector
dimensions. Graphit uses `graphit-embeddings-v1` and `graphit-rerank-v1`, with
query/document input semantics preserved. Service rollout revision is distinct
from model revision. Do not configure guessed `/v1/embeddings` paths or local
hardware flags for a Broker service.

## Vector compatibility

Equal vector width does not establish equal semantics. Local vectors use the
validated artifact and recipe identity. Direct vectors use the selected
protocol, declared model revision, recipe and dimensions. Broker vectors use
the discovered protocol/model/revision/recipe/dimensions, independently of the
service's endpoint or rollout revision.

Change the declared model revision or recipe when the deployment changes its
vector space. Incompatible indexed vectors are rejected rather than padded or
mixed; the authorized indexers must publish the matching space before semantic
queries can use it.

## Rerank modes and activation

| Mode | Execution | Required configuration |
|---|---|---|
| `local` | BGE Reranker V2 M3 GGUF F16, one classifier logit | installed `rerank` slot and independent local device |
| `direct` | Native Cohere, Voyage or Jina rerank | protocol, full endpoint, model, model revision and recipe |
| `broker` | Discovered Graphit rerank service | Broker endpoint and profile credential |
| `disabled` | Explicitly unavailable when requested | none |

The productive AST/Knowledge Direct rerank protocols are `cohere`, `cohere-v2`, `voyage`, `voyage-v1`,
`jina` and `jina-v1`. There is no implicit embedding/cosine fallback. Direct Auth
configuration requires `--rerank-dimensions 0` for these native scorers; zero
denotes no vector output in this configuration field, not an advertised
dimension of the reranker. Direct Auth rejects embedding protocols and cosine
similarity as rerank configurations; they do not implement these query/document
scorers.

For Local embedding with a Direct reranker:

```bash
graphit provider add remote-rerank --type local \
  --embedding-mode local --embedding-device cpu --embedding-device-id 0 \
  --rerank-mode direct --rerank-protocol cohere-v2 \
  --rerank-endpoint https://rerank.example/v2/rerank \
  --rerank-model your-reranker --rerank-model-revision deployment-2026-10 \
  --rerank-recipe native-logit-v1 --rerank-dimensions 0
graphit login --provider remote-rerank --profile remote-rerank --username alice \
  --rerank-api-key "$RERANK_API_KEY"
```

Choosing a backend does not enable the second stage. Enable it globally or for
the current PROJECT with the maintained configuration command:

```bash
graphit config --global set search.rerank true
graphit config set search.rerank true
```

Use the scope you need; a PROJECT value overrides the global value. To disable
it in that scope, set the same key to `false`. Per-query flags override either
configuration, including an explicit `false`:

```bash
graphit config --global set search.rerank false
graphit ast search "parser cancellation" --mode hybrid --rerank=true
graphit knowledge search "parser cancellation" --mode hybrid --rerank=false
```

Omitting `--rerank` follows configuration, whose default is `false`.
`--rerank=false` skips reranking even when configuration enables it. Browser
search uses the same Use configuration/Off/On choice. The backend preserves the
retrieval scores and returns a separate `rerank_score`; provider failures remain
visible. Local rerank preparation/loading occurs only when reranking is effectively
enabled. Broker/Direct rerank do not require local weights.

## Rust local model install and select

Install the pinned embedding model, or provide its already downloaded GGUF:

```bash
graphit models install embeddinggemma2
# Offline alternative for the same pinned artifact:
graphit models install embeddinggemma2 --file prepared/embeddinggemma-2-BF16.gguf
```

Install the default reranker from its pinned public F16 artifact:

```bash
graphit models install bge-reranker-v2-m3
# Offline alternative for the same pinned bytes:
graphit models install bge-reranker-v2-m3 --file prepared/bge-reranker-v2-m3-F16.gguf
```

The download uses [the Sinjab distribution on Hugging Face](https://huggingface.co/sinjab/bge-reranker-v2-m3-F16-GGUF),
fixed at distribution commit `1458e3304258a8a11773000c7d0c192abfe78291`.
Its model card identifies BAAI's original model, Apache-2.0 licensing and
conversion with llama.cpp through GGUF-my-repo. The publisher does not record a
converter revision. This distribution contains the exact F16 bytes already
pinned and qualified by Graphit; the upstream model revision below remains
unchanged. Installation verifies the complete SHA-256 and byte count, not the
publisher's filename alone.

Graphit's reproducible preparation recipe remains in
`graphit-libs/models/bge-reranker-v2-m3/build.py`, with upstream source digests,
converter and Python dependency pins, model license and converter notice. It uses
the official llama.cpp converter at
`4fbc76dec51d0add466f0210855c0596589b60d4` with `--outtype f16`.
That is Graphit's preparation recipe, not an assertion about the publisher's
unrecorded converter version.

| Model | Revision | Artifact | Bytes | SHA-256 |
|---|---|---|---|---|
| EmbeddingGemma 2 | `bfcd298762cc34d0357ece5ebdd31791a3a374d8` | `embeddinggemma-2-BF16.gguf` | 557950176 | `68bae29d62fb8c7d23e98d21fd4662753ddd636e6b62b8a70dfe059a9844f216` |
| BAAI BGE Reranker V2 M3 | `953dc6f6f85a1b2dbfca4c34a2796e7dde08d41e` | `bge-reranker-v2-m3-F16.gguf` | 1159774912 | `3c2de408d2c0a85a9472dc09f9d5a22c9b73743c6343952c15053299c777c298` |

Installation verifies the captured file, exact digest, size and manifest before
publishing it to the installation's model store. An installed valid bundle is
reused; conflicting contents are not overwritten. `--file` resolves relative to
the command's working directory, and `--global-dir` chooses the installation.
Installation and selection do not create or change an Auth profile.

Each task has its canonical `daemon.models` slot: `embedding` or `rerank`.
Install preserves any existing selection for that task and an explicit
`slots: []` opt-out. To select an installed model explicitly:

```bash
graphit models select embeddinggemma2-bfcd298762cc34d0357ece5ebdd31791a3a374d8/model-128.json
graphit models select bge-reranker-v2-m3-953dc6f6f85a1b2dbfca4c34a2796e7dde08d41e/model.json
```

These paths are relative to the installation's model directory. Selecting one
task preserves the other task's slot. EmbeddingGemma's `model.json` selects 768
dimensions; `model-128.json`, `model-256.json` and `model-512.json` select explicit
MRL recipes. All use the same BF16 weights, embedded tokenizer, 8192-token
context and F32 L2 normalization. MRL truncates the output and normalizes it
again; a changed recipe requires the matching indexed vector space.

BGE V2 M3 uses its embedded XLM-RoBERTa query/document pair, noncausal RANK
pooling and original single-output classifier. Its default score is the raw
logit. The 8192-token limit includes four special tokens; truncation removes
from the longer side on the right. Sigmoid requires an explicitly selected
recipe. The upstream BGE model is licensed under Apache-2.0.

Provisioning a reranker does not enable search reranking. See
[Rerank activation](#rerank-modes-and-activation) for configuration and request
overrides. Detailed binding, installation and qualification evidence lives in
[Rust CLI models](../specs/rust_cli_models.md) and the
[model service contract](../specs/rust_model_service.md).

## Local model manifests

Local models use the closed [`graphit-model/v1` manifest](../specs/rust_model_manifest.md)
with `gguf-embeddinggemma2` or `gguf-bge-reranker-v2-m3`. Each has its own task
and recipe, an embedded tokenizer, required relative artifact paths and SHA-256
digests. Unknown or cross-backend fields fail. The
[captured-source binder](../specs/rust_model_bindings.md) validates the artifact;
there is no local ONNX Runtime backend.

Select installed manifests through `graphit models select`, which updates the
canonical `daemon.models` slot for the task. Auth holds the independent Local
execution device; it does not choose a model by an arbitrary first slot.
The [model service](../specs/rust_model_service.md) consumes only effective Local
purposes. Remote/disabled purposes do not capture a local bundle or start a
model process. Native libraries follow the
[runtime packaging](../development/native-runtime-packaging.md) layout.

Local embedding and rerank weights load on their first effective use and unload
after five minutes without inference. To change that installation-wide interval:

```bash
graphit config --global set models.idle_timeout_seconds 300
```

The value is a positive number of seconds; zero and unsupported durations fail
validation. The interval starts when actual model work completes. Active work
finishes before unloading, including native cleanup after a cancelled request.
Cache hits do not extend residency. The next uncached operation reloads the
selected model; unloading does not remove installed files, cached results or
indexed vectors, or change model identity and generation. The model-service
process stays available while the weights are unloaded. Broker and Direct
services manage their own residency; this local setting does not control them.

## Data and network boundaries

- Local embedding/rerank keeps query and indexed text on the machine after model download.
- Direct providers receive text and authenticate with the active profile's API key.
- Broker providers receive text and the active Broker access token; local providers can use a static Broker key. Broker upstream
  AI keys never enter the Graphit client.
- Completion CLIs receive prompts/retrieved context and may send them to their own provider.
- BM25, exact Cypher, source slicing and deterministic coordination do not require a model.

Provider/account storage and non-interactive examples are in [Authentication](authentication.md).
Managed AI plus ACL/S3 details are in [Graphit Broker](auth-broker.md). Internal contracts are
in the [AI Engine specification](../specs/ai_engine.md).


## Configuration reload and diagnosis

The daemon observes global configuration/control changes and refreshes local model selection;
a blanket restart after every `models select` is unnecessary. Already admitted work retains its
captured binding until its operation finishes. `ai.cli` is different: Ask captures it at attachment,
so restart to change that completion binding. Provider updates preserve omitted fields but change
provider revision; obtain a valid profile/session for the new provider generation.

GPU memory is not the process's entire RAM. GGUF mappings, tokenizer, contexts, staging and CPU
metadata can remain in system memory even with GPU offload. Diagnose actual selected device and
fallback reason instead of assuming a nonzero RSS proves CPU inference. Explicit GPU needs the
matching packaged backend plus host driver; `auto` can legitimately choose CPU and must report why.
There is no local ONNX Runtime dependency to install or repair.

[Configuration](configuration.md) documents default values and capture/reload boundaries;
[packaging](../development/native-runtime-packaging.md) defines redistribution and platform limits.
