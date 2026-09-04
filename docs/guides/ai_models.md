---
title: AI Models, Providers, and Agent CLIs
type: guide
updated: 2026-09-03
tags: [ai, models, completions, embeddings, rerank, agents]
---

# AI Models, Providers, and Agent CLIs

Graphit uses AI in three independent layers. Choosing a coding-agent CLI does not choose an
embedding model, and choosing a reranker does not enable reranking.

| Layer | Purpose | Default | Configuration |
|---|---|---|---|
| Agent completion | Synthesis, analysis, and autonomous tool use | First available supported CLI | `ai.cli`, `ai.agent_args*`, `modules.agent` |
| Embedding | Semantic vectors for code, knowledge, and memory | Local CodeRankEmbed ONNX model | `ai.embedding.*`, `modules.embedding` |
| Rerank | Optional relevance scoring after first-stage retrieval | Local cross-encoder provider, stage off | `search.rerank`, `ai.rerank.*` |

This separation lets a team keep retrieval local while using any supported coding agent, use a
hosted embedding API with a local agent, or run lexical and graph operations without any model.

## Agent completion: Graphit delegates to your CLI

Graphit has no direct chat-completion HTTP provider and no `ai.model` key. It starts an installed,
authenticated agent CLI and sends it a non-interactive prompt. The CLI owns the provider, account,
model name, rate limits, and model-specific settings. Select those in that CLI's own configuration
or with CLI arguments supported by that product.

```bash
graphit config --global ai.cli claude
graphit config --global ai.agent_args.claude "--permission-mode acceptEdits"
```

`ai.agent_args` is only added to streamed **agentic** runs in which Graphit explicitly allows tool
use, currently Live Search and the agent phase of Dream. Ordinary completions such as AI wiki
answers, natural-language Cypher, and memory analysis do not receive these arguments. A
binary-specific `ai.agent_args.<binary>` value wins over the generic value. Arguments are split on
whitespace, not parsed by a shell; quoting and shell expansion are not supported.

### CLI resolution order

`ai.cli` is a preference, not an exclusive requirement. Graphit tries:

1. `ai.cli`, or the configured/default CLI when the key is empty;
2. the CLI mapped from the configured IDE, when different;
3. `opencode`, `agy`, `gemini`, `claude`, `codex`, `grok`, `kiro-cli`, `cursor-agent`, `agent`,
   `copilot`, `cline`, `goose`, then `openhands`.

The first executable found on `PATH` is used. If `ai.cli` names an absolute or custom executable
that exists on `PATH`, unknown CLIs use the generic stdin protocol with `-` as their argument.
Graphit does not verify authentication until the CLI runs.

### Supported invocation protocols

| Binary | Prompt transport | Arguments added by Graphit | Session resume | Structured stream |
|---|---|---|---|---|
| `claude` | stdin | `-p -` | `--resume <id>` | Yes: text, thinking, tool use/results, session |
| `gemini` | stdin | `-p -` | `--resume <id>` | Text stream |
| `agy` | stdin | `-p -` | `--conversation <id>` | Text stream |
| `grok` | stdin | `-p -` | No | Text stream |
| `cursor-agent` | stdin | `-p -` | No | Text stream |
| `agent` | stdin | `-p -` | No | Text stream |
| `codex` | stdin | `exec -` | No | Text stream |
| `opencode` | final argument | `run <prompt>` | `-s <id>` | Text stream |
| `kiro-cli` | stdin | `chat --no-interactive -` | No | Text stream |
| `copilot` | stdin | `-p -` | `--resume <id>` | Text stream |
| `cline` | final argument | `<prompt>` | No | Text stream |
| `goose` | mode-`0600` temporary file | `run -i <file>` | No | Text stream |
| `openhands` | mode-`0600` temporary file | `--headless -f <file>` | No | Text stream |

Every child receives `NO_COLOR=1` and `TERM=dumb`. Temporary prompts use
`$XDG_RUNTIME_DIR/graphit` when available and the operating-system temporary directory otherwise.
Graphit removes the file after the CLI exits.

### Non-agentic and agentic safety modes

Ordinary completions receive a preamble that forbids questions, interactive interfaces, approval-
requiring actions, and tool-based mutations. Agentic streaming receives a different preamble: the
agent may use its tools inside the prepared workspace, must investigate before answering, cannot
wait for approval, and must end with prose. Graphit does not add permission-bypass flags. Whether a
CLI can edit files autonomously depends on that CLI and the operator-supplied `ai.agent_args`.

Only Claude currently exposes a structured event stream to Graphit. Other CLIs still stream text,
but Graphit cannot observe their thinking, tool calls, tool results, or newly allocated session IDs.
Session flags in the table are used only when Graphit has a real CLI-issued ID; Graphit never
invents one.

### Completion consumers and their gates

| Consumer | Why it calls the CLI | Gate |
|---|---|---|
| Natural-language Cypher | Generate a query that is then validated before execution | `modules.agent` |
| AI wiki/memory answer | Synthesize an answer from retrieved wiki sources | `modules.agent` |
| Live Search | Work in a prepared ephemeral multi-artifact workspace with tools enabled | `modules.agent` |
| `memory consolidate` | Judge possible duplicates, contradictions, and suggestions; Go validates and applies the plan | `modules.memory`; explicit command |
| Dream memory phase | Make the same bounded judgments before deterministic application | `modules.dream` and `modules.memory` |
| Dream agent phase | Generate or improve skills, rules, commands, and memories in the project | `modules.dream` |

`modules.agent=false` disables the three interactive product surfaces in the first three rows. It
does not disable deterministic graph queries, BM25, semantic retrieval, hybrid RRF, complexity
analysis, dead-code analysis, explicit memory operations, or an independently enabled Dream run.

Memory consolidation illustrates Graphit's boundary around probabilistic output: memory order and
batches are deterministic, the model returns constrained JSON, IDs and actions are sanitized, and
the Go service owns every mutation. If the AI analysis fails, deterministic stale-memory checks can
still run and the failure is reported rather than mistaken for “nothing to do.”

## Embedding models

Embedding powers semantic and hybrid retrieval. Normal callers first try the daemon's Unix socket;
if it is unavailable, they construct the configured provider directly. The daemon uses the same
provider resolver behind a lazy shared client, so the API and dimensions do not change when a
request goes through the proxy.

### Providers, defaults, and dimensions

| Provider | `ai.embedding.provider` | Default model | Native dimensions | Credential fallback |
|---|---|---|---:|---|
| Local ONNX | `local` or empty | CodeRankEmbed-137M-INT8 | 768 | None |
| OpenAI | `openai` | `text-embedding-3-small` | 1536 | `OPENAI_API_KEY` |
| OpenAI-compatible | `openai-compatible` | None | Set explicitly | API key optional |
| Cohere | `cohere` | `embed-english-v3.0` | 1024 | `COHERE_API_KEY` |
| Voyage AI | `voyage` | `voyage-3` | 1024 | `VOYAGE_API_KEY` |
| Google | `google` | `text-embedding-004` | 768 | `GOOGLE_API_KEY`, then `GEMINI_API_KEY` |

Known alternate models include OpenAI `text-embedding-3-large` (3072) and
`text-embedding-ada-002` (1536); Cohere English/multilingual v3 models (1024) and light variants
(384); Voyage `voyage-3-lite` (512), `voyage-code-3` (1024), and `voyage-large-2` (1536); and
Google `gemini-embedding-001` (3072).

`ai.embedding.dimensions` overrides the known-model table. It is required for an unknown model and
for `openai-compatible`. For OpenAI `text-embedding-3-*`, a non-native value is sent as the API's
server-side `dimensions` parameter. Graphit refuses to construct a remote client when the width is
unknown instead of guessing a vector schema.

`ai.embedding.api_key` takes precedence over native environment variables. `ai.embedding.base_url`
overrides a named provider's endpoint and is required for `openai-compatible`. That protocol works
with any service that implements the OpenAI `/v1/embeddings` request and response shape; Graphit
does not claim compatibility with a product merely because it exposes some other OpenAI API.

### Local model lifecycle

The local model consists of `model.onnx` and `tokenizer.json`. Resolution order is:

1. a `models/` directory beside the Core binary, for private or air-gapped packaging;
2. the shared cache at `~/.graphit/models/coderankembed/`, or `GRAPHIT_MODEL_CACHE`;
3. a one-time download into that cache.

`graphit setup` downloads the model only when the selected embedding provider is local and fails if
the required asset cannot be obtained. The model is not embedded in release binaries. Inputs are
capped at 512 tokens; query text receives the model's retrieval prefix. ONNX intra-op work follows
the shared CPU budget and can be overridden with `GRAPHIT_EMBED_THREADS`.

### Daemon proxy and lazy loading

The daemon listens on `~/.graphit/daemon/embed.sock`. The protocol is newline-delimited JSON:
`{"texts":[...]}` or `{"query":"..."}` in, and `{"vectors":[[...]]}` out. There is no
`embed.port`, HTTP endpoint, or health URL.

The daemon client is lazy: constructing the daemon does not open an ONNX session or call a remote
API. The first embedding request resolves the backend, and the result or construction error is
memoized. Short-lived CLI and MCP processes use the socket automatically when it answers and fall
back to the direct provider otherwise.

### Changing provider, model, or width

Lance vector columns have a fixed width. After changing the provider, model, or dimensions, rebuild
the affected embeddings. AST shard caches record their width and invalidate incompatible entries,
but an existing table cannot accept vectors of another width.

```bash
graphit config --global ai.embedding.provider voyage
graphit config --global ai.embedding.model voyage-code-3
graphit ast embed
graphit wiki embed
graphit wiki embed --wiki memory
```

The daemon also runs AST and wiki/memory embedding loops every two minutes for supervised projects
when `modules.embedding=true`. Use explicit commands when you need a known-complete checkpoint now.

## Rerank models

Reranking is an optional second stage after candidate retrieval. Two settings are necessary:

- `search.rerank=true` enables the stage;
- `ai.rerank.provider` selects its implementation.

| Provider | Value | Default model | Credential fallback |
|---|---|---|---|
| Local cross-encoder | `local` or empty | `bge-reranker-base` | None |
| Cohere | `cohere` | `rerank-english-v3.0` | `COHERE_API_KEY` |
| Voyage AI | `voyage` | `rerank-2` | `VOYAGE_API_KEY` |
| Jina | `jina` | `jina-reranker-v2-base-multilingual` | `JINA_API_KEY` |

The local reranker downloads about 1.04 GiB on first construction and caches it separately from the
embedding model. Remote providers use `ai.rerank.api_key`, `ai.rerank.model`, and optionally
`ai.rerank.base_url`. Their dedicated APIs return input indexes; Graphit maps every score back to
the original candidate before applying the shared rerank adapter.

> Current product boundary: provider clients, the local cross-encoder, the search-layer adapter,
> and the `search.rerank` switch are implemented, but current CLI, MCP, and Observatory search
> entry points do not attach a reranker. Production searches therefore stop after hybrid RRF even
> when `search.rerank=true`. The setting is integration-ready, not an active user-visible stage yet.

## Configuration recipes

Local retrieval with a preferred agent CLI:

```bash
graphit config --global ai.cli codex
graphit config --global ai.embedding.provider local
graphit config --global modules.agent true
```

OpenAI-compatible embeddings without a bearer key:

```bash
graphit config --global ai.embedding.provider openai-compatible
graphit config --global ai.embedding.base_url http://127.0.0.1:11434/v1
graphit config --global ai.embedding.model your-embedding-model
graphit config --global ai.embedding.dimensions 768
```

Lexical and structural operation without agent synthesis or background embeddings:

```bash
graphit config --global modules.agent false
graphit config --global modules.embedding false
graphit ast search "ResolveConfig" --mode fts
```

## Data and network boundaries

- Local embeddings keep indexed text on the machine after the one-time model download.
- Remote embedding and rerank providers receive the text being scored.
- Completion CLIs receive Graphit's prompts and retrieved context and may send them to the CLI's
  configured provider.
- BM25, exact Cypher, source slicing, deterministic Task control, and non-AI memory operations do
  not require a remote model.
- The daemon socket is local filesystem state. The authenticated MCP HTTP listener is a separate
  service and does not expose the embedding socket protocol.

For every key and precedence rule, see [Configuration Reference](configuration.md). For how these
models interact with BM25, hybrid RRF, graph traversal, and source selection, see
[Retrieval Architecture](retrieval_architecture.md). For implementation contracts, see the
[AI Engine Specification](../specs/ai_engine.md).
