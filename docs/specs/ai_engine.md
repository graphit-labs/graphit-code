---
title: AI Engine Specification
description: "Provider/profile resolution, local/direct/broker embedding, rerank, compatibility, and daemon contracts."
type: specification
updated: 2026-09-08
---

# AI Engine Specification

## Ownership model

Completion, embedding and rerank are separate systems. Completion delegates to an external coding
agent CLI. Embedding and rerank resolve from the active authentication snapshot:

```text
provider (non-secret topology)
  ├─ AI embedding mode/protocol/endpoint/model/dimensions or local ONNX execution
  ├─ AI rerank mode/protocol/endpoint/model, dimensions for embedding simulation, or local ONNX execution
  └─ optional broker endpoint/audience/resource

profile (account secret/session)
  ├─ OIDC access/refresh/ID tokens
  ├─ static broker/MCP keys
  └─ direct embedding/rerank API keys
```

Setup ensures the canonical persisted provider `local` with explicit local embedding/rerank and
ONNX `cpu`/`0`; it does not prompt for AI topology or create a profile. It may provision a selected
local bundle when its manifest uses the `setup` fetch policy. Provider add/update owns subsequent
service topology and independent ONNX settings. With no active profile, AI resolution atomically
ensures and uses `local`, recreating it if absent. A configured profile is resolved atomically and
wins over the fallback; secrets cannot be filled from another profile or old configuration.

## Agent completion sessions

`ai.NewConversation` is the common boundary for related coding-agent calls. It canonicalizes and
fixes the project working directory, binds any native session ID to the effective CLI, and holds a
mutex across a turn so concurrent prompts cannot interleave in one agent session. Creation and
resume both run with that same cwd. `ResumeConversation` discards a stored ID when the current
client cannot identify its CLI, when the CLI changed, or when the stored binding is incomplete.
A completion error clears the in-memory native ID before another turn.

The supported executable matrix is `claude`, `gemini`, `agy`, `grok`, `cursor-agent`, `codex`,
`opencode`, `kiro-cli`, `copilot`, `qwen`, and `kimi`. Session continuity is declared only for
`claude`, `gemini`, `agy`, `codex`, `opencode`, `qwen`, and `kimi`, whose structured protocols provide a deterministic initial ID and a real resume
operation. Other supported CLIs remain one-shot until both sides of that contract are verified;
Graphit does not return an invented persistence ID for them.

`deepseek` is a provider/model name and is deliberately not accepted as a CLI alias.

Persisted Graphit chat metadata stores the native `agent_session_id` beside `agent_cli`. Consumers
that span HTTP requests resume through both values; in-process multi-step consumers retain one
`Conversation`. AST Cypher responses expose the pair for observability. These IDs belong to the
external CLI and are separate from Graphit chat IDs, Live Search workspace IDs, provider profiles,
and MCP host-session identity.

AST query generation must follow the [canonical traversal contract](ast_module.md#the-rules-and-what-each-refusal-says): name a logical relationship, filter an anchor and project the reached endpoint with `DISTINCT`. Wildcard relationships and type alternation are refused with the current Icebug 0.19 engine because multi-table scans can return incorrect endpoints. Treat this diagnostic as a query failure, never as an empty result; the UI Relationship map uses a separate bounded, typed sample.

## Live CLI execution boundary

`ai.NewClientForAgent` resolves only `config.CLIForAgent(agent)` on `PATH`, with the corresponding
configured agent arguments. Unsupported agents and missing executables return errors without
automatic fallback. `livesearch.NewManagerFromConfig` resolves that client per session before
workspace creation or adapter preparation; the session retains it across turns. `NewManager`
continues to accept an injected client for embedded callers and tests. General completion's
configuration-based resolution is unchanged.

`StreamRequest.AllowNonGitWorkspace` defaults to false and requires an explicit `WorkDir` when
enabled. Live enables it for every turn in its prepared workspace. Only the Codex adapter translates
it into `--skip-git-repo-check`, for both initial and resumed execution, independently of `AllowTools`.
It does not replace the working directory, trust another directory, or alter sandbox/approval
settings. `AllowTools` selects the prompt and configured agent arguments; the external CLI remains
responsible for enforcing permissions. Other adapters receive no Codex-specific flag.

## Tool activity correlation

Public agent progress and Live events carry optional `tool_call_id`, copied from the native tool
identity. It is independent of event sequence numbers and the private agent session ID. Live
preserves this field in its event log, replay and SSE; older events without it remain valid.
Tool input/output payloads are preserved within the stream reader’s existing line-size limit;
compact diagnostic display limits belong to the UI, not the protocol parsers.
Consumers group input and result only within the same turn. They must not merge concurrent calls
merely because the tool names match.

| CLI protocol | Correlation source | Result handling |
| --- | --- | --- |
| Claude | `tool_use.id` / `tool_result.tool_use_id` | Message content blocks retain input and output |
| Codex | `command_execution` item `id` | Started/completed items share the ID |
| Gemini | `tool_id` | `parameters` is input; `output` is the result, which need not repeat the tool name |
| OpenCode | `part.callID` | A `tool_use` snapshot with completed/error state includes both input and result |
| Qwen | Claude-compatible block IDs | User message tool-result blocks are activity, not assistant answer text |
| Kimi | `tool_calls[].id` / `tool_call_id` | Assistant function calls and tool-role responses share the ID |
| Antigravity | Present `step_index`, normalized as `agy-step:N` | ACTIVE/DONE describe the same step; absent index remains absent |

These adapters follow the native contracts documented by [Claude](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls),
[Codex](https://github.com/openai/codex/blob/main/sdk/typescript/src/items.ts),
[Gemini](https://github.com/google-gemini/gemini-cli/blob/main/packages/cli/src/__snapshots__/nonInteractiveCli.test.ts.snap),
[OpenCode](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/cli/cmd/run.ts),
[Qwen](https://github.com/QwenLM/qwen-code/blob/main/packages/cli/src/nonInteractive/io/BaseJsonOutputAdapter.ts),
[Kimi](https://moonshotai.github.io/kimi-cli/en/customization/print-mode.html), and
[Antigravity](https://www.antigravity.google/docs/cli/headless/#tool-calls-in-the-stream).
Unsupported or unidentified calls remain uncorrelated; no synthetic session-based ID is used.

## Assistant text boundaries

Public `EventText` chunks concatenate to exactly `StreamResult.Text`. Structured CLI
adapters normalize confirmed independent messages into paragraphs before emitting or
accumulating text. The adapter adds only the missing line breaks at a boundary; it
does not trim source text or insert spaces between arbitrary deltas. Live persists
these normalized chunks, so new-run replay and live output have identical spacing.

Codex completed `agent_message` items, OpenCode completed text parts, and Qwen/Kimi
assistant messages start independent text. Multiple content blocks in one Qwen/Kimi
message remain contiguous. Claude reads the nested `stream_event.event` envelope:
`message_start` marks a pending boundary, and `content_block_delta.delta.text` supplies
the text. Empty/tool-only messages do not add blank output; the boundary applies to
the next nonempty text. Complete Claude assistant snapshots remain suppressed because
partial-message streaming already supplied their text. Legacy flat Claude deltas are
still accepted. See the [Claude streaming contract](https://code.claude.com/docs/en/agent-sdk/streaming-output).

Gemini message chunks, Antigravity `text_delta`, and unstructured stdout remain exact
concatenations. The internal boundary marker is not part of the public event schema.
Consumers must not guess boundaries from punctuation, tool activity, or event timing.
Previously persisted chunks without boundaries cannot be safely repaired by a viewer;
the normalization applies to new executions, including new turns in existing sessions.

## Service modes

`auth.AIServiceConfig.Mode` is one of:

- `local`: use the framework-owned implementation; endpoint/model/key fields are forbidden and an
  optional ONNX execution block selects device/device ID;
- `direct`: require explicit protocol, endpoint and model; embedding and embedding-simulated rerank
  require positive dimensions, while native rerank forbids dimensions; the corresponding API key
  is required in the login profile;
- `broker`: require provider broker topology; downstream model is deliberately absent;
- `disabled`: explicit failure when a caller requests the capability.

Provider validation runs before persistence. OIDC providers using both the daemon MCP listener and
broker relay with non-empty audiences must configure the same audience because one profile owns one
refreshable access token.

Broker topology is an exclusive boundary for the two retrieval services. If `Provider.Broker` is
present, both `AI.Embedding.Mode` and `AI.Rerank.Mode` must be `broker`; omitted, `local`, `direct`,
and `disabled` values are invalid. Conversely, either `broker` mode without `Provider.Broker` is
invalid. This coupling does not apply to agent completion, whose CLI owns its provider and model,
and does not enable the rerank stage.

## Embedding resolution

`ai.NewEmbeddingClientFromConfig` first probes the daemon Unix socket and falls back to
`newDirectEmbeddingClientFromConfig`. The daemon wraps the latter in `LazyEmbeddingClient`, keyed
by active provider revision/profile, so it never dials itself and rebuilds on account changes.

### Local

`models.embedding.id` selects a versioned manifest from the global model catalog, defaulting to
`coderankembed`. The ONNX adapter discovers input/output names and tensor facts, while the manifest
declares tokenizer artifact, text limits and prefixes, output override, pooling, normalization, and
dimensions. Required artifacts resolve only inside the bundle directory and follow their explicit
fetch policy; custom downloads require integrity metadata and are published atomically.

`AIServiceConfig.ONNX` selects execution independently of the manifest for each local provider
service. Setup and no-profile resolution materialize a missing block on the default `local`
provider as `cpu`/`0`. `auto` tries CoreML then CPU on macOS and CUDA then CPU on Linux/Windows. Explicit
`cpu` registers no accelerator; explicit `cuda` or `coreml` has no fallback. CoreML is macOS-only
and requires ID 0; CUDA IDs are zero-based. Provider registration and model-session creation are
both inside the fallback attempt, so an acceleration failure cannot leave a partially configured
session. A local embedding session selected through `auto` also treats accelerator OOM/resource
exhaustion during inference as recoverable: it repeats that request on CPU, then probes a fresh
accelerator session on later requests with bounded backoff. Promotion requires a successful
inference on the candidate session; another resource failure returns execution to CPU. Daemon
startup preflights explicitly selected local accelerator providers before exposing services; auto
and CPU remain lazy because auto has the defined CPU fallback.

Interactive provider add/update reads the provider's effective device/device ID for each local
service and presents it as the selected answer; missing values become `cpu`/`0`. The two selections
are prompted and persisted independently. Non-interactive provider commands accept
`--embedding-device`, `--embedding-device-id`, `--rerank-device`, and `--rerank-device-id`; omitted
values preserve the current selection or use the default for a new local service. Provider parsing
uses the same rules as session construction. Direct, broker, and disabled services store no ONNX
execution block, skip prompts, and reject local device flags.

### Direct

Adapters support:

| Protocol | Request family |
|---|---|
| `openai`, `openai-compatible`, `openai-embeddings-v1` | OpenAI embeddings shape |
| `cohere` | Cohere embed |
| `voyage` | Voyage embeddings |
| `google` | Google Gemini embedding API with model-aware retrieval tasks/instructions and requested output dimensions |

Endpoint, model and dimensions come from the provider. The key comes only from
`Profile.EmbeddingAPIKey`. Constructors reject unknown width instead of guessing the Lance schema.

### Broker

The client calls unauthenticated `GET /.well-known/graphit-broker`, requires discovery version 1
and `openai-embeddings-v1`, then constructs an exact-path client using the advertised route,
revision, dimensions and max batch. Each request resolves/refreshed the active credential and
sends `Authorization: Bearer` with the Broker-issued access token, direct OIDC access token, or
static broker credential. For an explicitly
anonymous provider/profile it omits the header; the server must still grant `anonymous` or
`global` access, and an invalid present token is never downgraded.

Every response must repeat the expected Graphit revision and dimensions. A mismatch fails before
vectors reach storage. User model input is not used for routing.

## Embedding compatibility and artifacts

`ConfiguredEmbeddingIdentity` returns non-secret compatibility inputs:

| Mode | Identity inputs |
|---|---|
| local | manifest ID + revision + compatibility hash + declared dimensions |
| direct | protocol + normalized endpoint + model + configured dimensions |
| broker | broker endpoint + public route + broker revision + discovered dimensions |
| disabled | provider name + zero dimensions |

Hub Lance history fingerprints that identity. AST shard metadata records dimensions. A mismatch
prevents selecting an old published base or reusing an incompatible shard. Operators must rebuild
AST/wiki embeddings after an effective vector-space change. Equal width is not sufficient, hence
the broker revision requirement.

## Rerank resolution

`NewRerankerFromConfig` resolves the active provider to `RerankAdapter`:

| Mode | Implementation |
|---|---|
| local | manifest-selected ONNX cross-encoder; defaults to `bge-reranker-base` |
| direct | Native Cohere/Voyage/Jina scorer, or OpenAI/OpenAI-compatible/Google embedding-simulated cosine scorer |
| broker | Graphit rerank v1 scorer discovered from broker |
| disabled | explicit error |

`models.rerank.id` selects the local manifest independently from provider/broker topology. Its ONNX
signature is discovered and its manifest controls tokenizer artifacts, text limits, batch size,
output selection, score transform, and positive class. Direct keys come from
`Profile.RerankAPIKey`. Broker calls resolve the current bearer token, require a complete score set
and validate the response revision.

An embedding-simulated scorer requires positive configured dimensions, embeds candidates in one
provider-bounded batch flow, and embeds the query through `QueryEmbedder` where available. It
rejects response-count mismatches, vector-width mismatches, non-finite values, and zero-norm
vectors before returning one cosine score per input candidate. Native rerank protocols reject a
dimensions value. Google uses `RETRIEVAL_QUERY` and `RETRIEVAL_DOCUMENT` for embedding models that
support task types; `gemini-embedding-2` instead receives the documented textual retrieval
instructions. Every Google request carries `outputDimensionality` from provider configuration.

The model manifest is deliberately hardware-neutral. Runtime payload assembly, not the manifest,
ships the native execution providers. Linux and Windows launchers carry the ONNX Runtime core,
shared-provider bridge, and CUDA provider; macOS carries the versioned CoreML-enabled core dylib
without a separate CoreML provider library.

`search.rerank` is the independent activation switch. When false, no ranker is constructed and no
network/model work occurs. When true, `ConfiguredLanceRerank` supplies the ranker to AST and
Knowledge keyword/hybrid Lance queries. First-stage candidate limit defaults to 50 and is never
less than requested result count. Candidate text excludes AST gram bags, includes human-readable
identity/doc/path, and includes wiki title/summary/body. The adapter must return the identical hit
set; changing cardinality is an error. Scores replace first-stage scores and ordering uses a stable
tiebreak.

## Broker contracts

The framework accepts discovery version `1`:

- embeddings: protocol `openai-embeddings-v1`, path, route, revision, dimensions, max batch;
- rerank: protocol `graphit-rerank-v1`, path, route, revision, max documents;
- temporary storage credentials: protocol `graphit-s3-credentials-v2`, path and authorization revision.

The broker chooses actual upstream provider/model. This makes routing, central API keys, cache,
rate policy and optimizations an operator concern while preserving a stable client contract.

## Daemon protocol

The embedding daemon listens on the mode-restricted local socket
`~/.graphit/daemon/embed.sock`. Requests are newline-delimited JSON with `texts` or `query`; replies
contain vectors. This is not HTTP and is not exposed by MCP. The proxy reports active dimensions
without eagerly loading a model. Failure to connect falls back to direct active-provider resolution.

## Failure behavior

- no active profile: the persisted `local` provider is ensured for AI while anonymous local-only identity/storage remain available;
- broker configuration with either AI mode not set to `broker`: provider validation fails before persistence;
- stale provider revision: login/session resolution fails and requires a new login;
- refresh failure: fail closed without using a stale broker token;
- missing direct/broker secret: login or client construction fails explicitly;
- broker discovery/protocol/revision mismatch: fail before using output;
- vector dimension mismatch: reject before storage;
- rerank failure: Lance returns first-stage trimmed hits plus an error; public callers report the
  failed search rather than claiming rerank succeeded.

## Security boundary

Provider topology is non-secret except an optional OIDC client secret. Direct AI keys and static
broker/MCP keys reside in the mode-0600 auth file and are redacted. Broker configuration ensures
organizational AI and cloud-storage keys never enter the client. Remote direct/broker AI modes send
candidate/indexed text to their configured service; local mode does not after model acquisition.

See [Authentication](../guides/authentication.md), [Graphit Broker](../guides/auth-broker.md),
and [AI Models](../guides/ai_models.md).
