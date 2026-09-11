---
title: AI Models, Providers, and Agent CLIs
type: guide
updated: 2026-09-08
tags: [ai, models, completions, embeddings, rerank, agents, broker]
---

# AI Models, Providers, and Agent CLIs

Graphit uses AI in three independent layers:

| Layer | Purpose | Default | Configuration owner |
|---|---|---|---|
| Agent completion | Synthesis and autonomous agent work | First supported CLI on `PATH` | global/project `cli` and `ai.agent_args*` |
| Embedding | Semantic vectors for code, knowledge, memory | Local `coderankembed` manifest | active provider service plus `models.embedding.id` for the local artifact |
| Rerank | Optional second-stage relevance | Local `bge-reranker-base` manifest; stage off | active provider service, `models.rerank.id`, and `search.rerank` |

`graphit setup` selects runtime, agent adapter, and agent CLI, ensures the persisted default
provider `local`, and provisions any selected local bundle whose manifest uses the `setup` fetch
policy. Custom embedding/rerank topology and local ONNX execution belong to `graphit provider`,
local manifest selection belongs to global `models.*.id` keys, and account API or broker keys
belong to `graphit login`.

With no active profile, Graphit uses the real `local` provider from `auth.json`: local embedding and
rerank backends, ONNX `auto`/`0`, and no broker. If absent, the AI resolver recreates it before use.
Identity remains anonymous and reranking stays off until `search.rerank=true`. AST, Knowledge,
Memory, Task, and `graphit init` remain available. Update `local` for non-default execution; create
and activate another provider/profile for named identity or remote services.

Agent completion remains independent from the named provider's AI service topology. Embedding and
rerank may be selected separately only on a provider without broker configuration. Once a broker
is configured, provider validation requires both service modes to be `broker`; `local`, `direct`,
`disabled`, and omitted modes are rejected for either service.

## Agent completion

Graphit does not contain a chat-completion HTTP provider or model key. It starts an installed,
authenticated coding-agent CLI and sends a non-interactive prompt. The CLI owns its provider,
account, model and rate limits.

Graphit groups related completions in an `ai.Conversation`. A conversation fixes the canonical
project working directory and the effective CLI, serializes its turns, and reuses the native agent
session when that CLI exposes a verified create/capture/resume protocol. A failed resume clears the
native ID. A session is never resumed after the project directory or effective CLI changes.
Consumers without a multi-turn flow still perform one non-interactive invocation.

```bash
graphit config --global ai.cli claude
graphit config --global ai.agent_args.claude "--permission-mode acceptEdits"
```

Resolution tries the explicit CLI, the agent-mapped CLI, then these supported products and
executables:

| Product | Executable | Native session continuity |
|---|---|---|
| Claude Code | `claude` | yes: Graphit creates/captures the ID and resumes it |
| Gemini CLI | `gemini` | yes: Graphit creates/captures the ID and resumes it |
| Antigravity | `agy` | yes: `conversation_id` is captured and resumed |
| Grok CLI | `grok` | no verified create/capture/resume protocol |
| Cursor Agent CLI | `cursor-agent` | no verified ID capture in the implemented protocol |
| Codex CLI | `codex` | yes: `thread.started` is captured and `exec resume` is used |
| OpenCode | `opencode` | yes: the structured session ID is captured and resumed |
| Kiro CLI | `kiro-cli` | one-shot only: resume is available upstream, but initial ID capture is not verified |
| GitHub Copilot CLI | `copilot` | no verified create/capture/resume protocol |
| Qwen Code | `qwen` | yes: Graphit supplies/captures the session ID and resumes with `--resume` |
| Kimi Code | `kimi` | yes: the `session.resume_hint` ID is captured and resumed with `--session` |
| Deep Code | `deepcode` | one-shot in Graphit: upstream resume exists, but structured initial-ID capture is not verified |

The matrix contains twelve executable names for twelve products. `deepcode` is the Deep Code agent
recommended by DeepSeek; `deepseek` is not a CLI alias. Graphit uses each CLI's
non-interactive stdin/argument protocol. `ai.agent_args` is appended only to explicitly
agentic Live Search/Dream work; it is split on whitespace and is never evaluated by a shell.

Saved Graphit chats keep both `agent_session_id` and `agent_cli`, so a later turn can resume only
the matching native agent under the same project directory. Live Search, wiki retries, memory
consolidation batches, Dream, and multi-step AST generation use the same conversation within their
logical flow. The Cypher generation API also returns `agent_session_id` and `agent_cli` for
observability. Native sessions stay in the selected CLI's own storage; for example, inspect a Codex
execution with:

```bash
codex resume --include-non-interactive <agent_session_id>
```

`modules.agent=false` disables synthesis consumers. It does not disable exact graph queries,
BM25, vector retrieval, deterministic Task operations or independently configured embedding.

## Embedding modes

Normal callers first try the daemon's Unix socket, then resolve the active provider directly. The
daemon uses a lazy client and rebuilds it when the active provider/profile revision changes.

| Mode | Behavior | Required provider fields | Login secret |
|---|---|---|---|
| `local` | Manifest-selected ONNX embedding model | none | none |
| `direct` | Graphit calls a remote API | protocol, endpoint, model, dimensions | `--embedding-api-key` |
| `broker` | Graphit discovers and calls Graphit Broker | `--broker-endpoint` | OIDC access token or `--broker-key` |
| `disabled` | Embedding construction fails explicitly | none | none |

Local mode defaults to the built-in `coderankembed` manifest. Select another bundle independently
from provider or broker routing:

```bash
graphit config --global models.embedding.id my-embedder
```

The selected manifest lives at `~/.graphit/models/<id>/manifest.json`, or under
`GRAPHIT_MODEL_CACHE/<id>/manifest.json` when that root is overridden. See
[Local model manifests](#local-model-manifests).

Direct example:

```bash
graphit provider add direct-ai --type local \
  --embedding-mode direct \
  --embedding-protocol openai-compatible \
  --embedding-endpoint http://127.0.0.1:11434/v1 \
  --embedding-model your-embedding-model \
  --embedding-dimensions 768 \
  --rerank-mode disabled

graphit login --provider direct-ai --profile workstation \
  --username alice --embedding-api-key "$EMBEDDING_API_KEY"
```

Supported direct protocols are OpenAI/OpenAI-compatible (`/embeddings` appended to the configured
base URL), Cohere, Voyage and Google (Gemini API). Dimensions are explicit provider topology so Graphit never
guesses a Lance vector schema. A direct key is required even for a local compatible endpoint; use a
synthetic endpoint-specific credential if that service requires no real authentication.

Google document embeddings use `RETRIEVAL_DOCUMENT`, while query embeddings use
`RETRIEVAL_QUERY`. For `gemini-embedding-2`, which replaced task types with text instructions,
Graphit formats the corresponding asymmetric retrieval prefixes instead. The configured dimensions
are sent as `outputDimensionality`, so the API request and the vector schema cannot silently disagree.

Broker example:

```bash
graphit provider add managed-ai --type local \
  --broker-endpoint https://broker.example.com \
  --embedding-mode broker --rerank-mode broker
graphit login --provider managed-ai --profile alice \
  --username alice --broker-key "$BROKER_KEY"
```

The broker advertises only a public route, revision and dimensions; it chooses upstream provider,
model, credentials, cache and optimization internally. Client-side protocol, endpoint, model and
dimensions are rejected in broker mode.

## Vector compatibility

Lance vector columns are fixed-width, but equal width does not prove equal vector semantics.
Graphit fingerprints:

- local model identity and width;
- direct protocol, endpoint, model and width;
- broker endpoint, public route, broker revision and width.

It also validates broker revision/dimensions on every embedding response. After any effective
model/vector-space change, increment the broker revision (for broker mode) and rebuild:

```bash
graphit ast embed
graphit wiki embed
```

Incompatible cached shards are rejected instead of padded or silently mixed with new vectors.

## Rerank modes and activation

Rerank backend mode is stored beside embedding in the active provider:

| Mode | Implementation | Provider topology | Login secret |
|---|---|---|---|
| `local` | Manifest-selected ONNX cross-encoder | none | none |
| `direct` | Native rerank API or embedding-simulated cosine scorer | protocol, endpoint, model; dimensions for simulated mode | `--rerank-api-key` |
| `broker` | Graphit rerank v1 | broker endpoint | OIDC token or broker key |
| `disabled` | Explicitly unavailable | none | none |

These four choices apply independently only when the provider has no broker configuration. A
broker-configured provider must store `broker` for both embedding and rerank. This routing rule is
separate from activation: `search.rerank=false` keeps the second stage off and makes no rerank call.

Direct protocols are selected explicitly:

| Protocol | Strategy | Dimensions |
|---|---|---|
| `cohere`, `cohere-v2` | Native Cohere rerank | rejected |
| `voyage`, `voyage-v1` | Native Voyage rerank | rejected |
| `jina`, `jina-v1` | Native Jina rerank | rejected |
| `openai`, `openai-compatible`, `openai-embeddings-v1` | Cosine similarity over query/document embeddings | required with `--rerank-dimensions` |
| `google` | Cosine similarity over Gemini query/document embeddings | required with `--rerank-dimensions` |

The simulated mode exists because OpenAI-compatible and Gemini embedding APIs do not expose the
same native rerank contract as Cohere, Voyage, or Jina. It embeds the query and all widened
candidates, validates vector cardinality and width, then uses normalized cosine similarity. Its
reported model name ends in `@embedding-simulated`, so telemetry and errors do not imply a native
cross-encoder. Empty candidate sets make no API call; invalid or zero-norm vectors fail explicitly.

For example, the same OpenAI embedding model can power both indexing and simulated rerank. The two
profile secrets remain separate fields; when they are the same provider key, pass the same value to
both login flags:

```bash
graphit provider add openai-search --type local \
  --embedding-mode direct --embedding-protocol openai \
  --embedding-endpoint https://api.openai.com/v1 \
  --embedding-model text-embedding-3-small --embedding-dimensions 1536 \
  --rerank-mode direct --rerank-protocol openai \
  --rerank-endpoint https://api.openai.com/v1 \
  --rerank-model text-embedding-3-small --rerank-dimensions 1536

graphit login --provider openai-search --profile openai-search \
  --username alice \
  --embedding-api-key "$OPENAI_API_KEY" \
  --rerank-api-key "$OPENAI_API_KEY"
```

For Gemini, use protocol `google`, the Gemini API base URL, an embedding model such as
`gemini-embedding-001` or `gemini-embedding-2`, and its selected output width.

Choosing a backend does not enable the second stage. Turn it on separately:

```bash
graphit config --global search.rerank true
```

When enabled, AST and Knowledge keyword/hybrid searches widen first-stage retrieval to at least 50
candidates, send readable candidate text to the active reranker, require the same candidate set
back, then trim to the requested size. Broker revision is validated. Local mode defaults to
`bge-reranker-base`; select another manifest with `models.rerank.id`. Its fetch policy controls whether
artifacts are provisioned during setup/startup, downloaded on demand, or required to exist locally.

Disable all calls immediately with:

```bash
graphit config --global search.rerank false
```

## Local model manifests

A local bundle is selected by ID and describes its files rather than relying on fixed filenames.
Every required operator-defined artifact needs a SHA-256 digest. A source URL is optional; HTTPS is
required except for loopback HTTP used by local development servers. `setup` fetches during
`graphit setup` and daemon startup, `on_demand` fetches when the capability is first constructed,
and `never` performs no network access.

```json
{
  "schema_version": 1,
  "id": "my-embedder",
  "name": "My embedding model",
  "revision": "2026-09-07",
  "task": "embedding",
  "runtime": {
    "engine": "onnx",
    "entrypoints": { "model": "weights" }
  },
  "fetch_policy": "on_demand",
  "artifacts": [
    {
      "role": "weights",
      "path": "onnx/model.onnx",
      "sources": [{ "url": "https://models.example/my-embedder.onnx" }],
      "sha256": "<64 lowercase hex characters>",
      "min_size": 1000000
    },
    {
      "role": "tokenizer",
      "path": "tokenizer.json",
      "format": "huggingface-json",
      "sources": [{ "url": "https://models.example/tokenizer.json" }],
      "sha256": "<64 lowercase hex characters>"
    }
  ],
  "tokenizer": { "artifact": "tokenizer", "format": "huggingface-json" },
  "text": {
    "max_tokens": 512,
    "query_prefix": "query: ",
    "document_prefix": "document: ",
    "truncation": "longest-first",
    "padding": "longest"
  },
  "inference": {
    "inputs": { "input_ids": "input_ids", "attention_mask": "attention_mask" },
    "output": "sentence_embedding",
    "pooling": "none",
    "normalize": true,
    "dimensions": 768
  }
}
```

Embedding supports `none`, `mean`, and `cls` pooling. Rerank manifests use the same artifact,
tokenizer, text, input, and output fields, set `task` to `rerank`, and can declare `max_batch`,
`score_transform` (`auto`, `none`, `sigmoid`, or `softmax`), and `positive_class`. ONNX input/output
names and tensor facts are discovered from the model; the manifest supplies semantic choices that
ONNX does not encode. Unsupported tokenizer formats, tensor signatures, pooling, paths, hashes, or
missing artifacts fail explicitly, with no fallback to a different model.

The manifest describes the portable model contract only. It never contains a device, GPU ID,
CUDA/CoreML setting, or provider-library path. Execution belongs to each provider AI
service whose mode is `local` and is independent for embedding and rerank:

```bash
graphit --non-interactive provider add workstation --type local \
  --embedding-mode local --embedding-device cuda --embedding-device-id 0 \
  --rerank-mode local --rerank-device cpu --rerank-device-id 0
graphit --non-interactive login \
  --profile workstation --provider workstation --username alice
```

| `device` | Behavior |
|---|---|
| `auto` | macOS tries CoreML then CPU; Linux and Windows try CUDA then CPU. Local embeddings also fall back to CPU after an accelerator resource failure and periodically retry the accelerator with backoff. |
| `cpu` | Forces CPU even when acceleration is available. |
| `cuda` | Requires CUDA and prevents daemon startup/client initialization without falling back. `device_id` selects the zero-based GPU. |
| `coreml` | Requires macOS and prevents daemon startup/client initialization without falling back. `device_id` must be `0`. |

Both services default to `auto` and `device_id=0` on the persisted `local` provider used when no
profile is active. An invalid device, negative ID, unsupported
CoreML platform, missing provider, or incompatible model fails with an actionable initialization
error. In `auto`, failure to register the preferred provider or create its model session retries the
same manifest on CPU. During local embedding inference, accelerator OOM/resource exhaustion also
retries the same request on CPU. CPU is a recovery state: later embedding requests periodically
probe a fresh accelerator session with bounded backoff and return to it only after a successful
inference. Explicit `cpu`, `cuda`, and `coreml` selections never migrate automatically.

### Device selection on a provider

When a service mode is `local`, interactive `provider add` or `provider update` shows its effective
values in brackets:

```text
Enter embedding ONNX device (auto/cpu/cuda/coreml) [auto]:
Enter embedding ONNX device ID [0]:
Enter rerank ONNX device (auto/cpu/cuda/coreml) [auto]:
Enter rerank ONNX device ID [0]:
```

Pressing Enter keeps the value shown. The provider's current service value is preselected; a new
local service starts at `auto` and `0`. Embedding and rerank are stored separately, so selecting
CUDA for one does not change the other. A service routed to `direct`, `broker`, or `disabled` has no
ONNX block, its local-device questions are omitted, and local device flags are rejected.

The equivalent unattended invocation is:

```bash
graphit --non-interactive provider add workstation --type local \
  --embedding-mode local \
  --embedding-device=auto --embedding-device-id=0 \
  --rerank-mode local \
  --rerank-device=auto --rerank-device-id=0
```

In non-interactive provider commands, omitted local device values preserve the current value or use
`auto`/`0` for a new service. Values are normalized and checked by the same validator used when ONNX
sessions are created. `auto` is not preflighted because its CPU fallback is resolved when the model
session opens. Explicit `cuda` and `coreml` are preflighted when the daemon starts and fail closed if
their provider cannot initialize.

Release launchers embed and extract the provider-capable ONNX Runtime beside the core executable:

| Platform | Embedded libraries |
|---|---|
| Linux | `libonnxruntime.so.1.29.0`, `libonnxruntime_providers_shared.so`, `libonnxruntime_providers_cuda.so` |
| Windows | `onnxruntime.dll`, `onnxruntime_providers_shared.dll`, `onnxruntime_providers_cuda.dll` |
| macOS | `libonnxruntime.1.29.0.dylib` |

CoreML is compiled into the macOS dylib; there is no separate `providers_coreml` library.

## Data and network boundaries

- Local embedding/rerank keeps query and indexed text on the machine after model download.
- Direct providers receive text and authenticate with the active profile's API key.
- Broker providers receive text and the active OIDC access token/static broker key; their upstream
  AI keys never enter the Graphit client.
- Completion CLIs receive prompts/retrieved context and may send them to their own provider.
- BM25, exact Cypher, source slicing and deterministic coordination do not require a model.

Provider/account storage and non-interactive examples are in [Authentication](authentication.md).
Managed AI plus ACL/S3 details are in [Graphit Broker](auth-broker.md). Internal contracts are
in the [AI Engine specification](../specs/ai_engine.md).
