---
title: "Configuration Reference"
description: "Complete user-facing reference for Graphit configuration layers, feature switches, providers, storage, networking, indexing, automation, and runtime resource controls."
content-type: reference
audience: users
keywords:
  - configuration
  - modules
  - environment variables
  - embedding
  - rerank
  - S3
  - daemon
  - security
related:
  - "docs/guides/getting_started.md"
  - "docs/guides/user_manual.md"
  - "docs/specs/config_module.md"
  - "docs/specs/ai_engine.md"
  - "docs/guides/ai_models.md"
  - "docs/guides/daemon_operations.md"
---

# Configuration Reference

Graphit can run entirely on one workstation, coordinate several agents against shared S3-backed
state, or serve reusable contexts to a team. The same configuration system covers those modes.
This page lists every supported user-facing key, its default, and the boundary it changes.

## Resolution and scope

Most keys use the first non-empty value in this order:

1. one-command override: `graphit -c key=value ...`;
2. environment variable;
3. project config in `graphit.lock.json`;
4. global config in `~/.graphit/config.json`;
5. a value compiled into a private distribution;
6. the code default described below.

Every ordinary key gets an environment variable mechanically: uppercase the key, replace dots
with underscores, and prefix it with `GRAPHIT_`. For example, `knowledge.docs_dir` becomes
`GRAPHIT_KNOWLEDGE_DOCS_DIR` and `modules.task` becomes `GRAPHIT_MODULES_TASK`.

```bash
# Persist for this project.
graphit config knowledge.docs_dir documentation

# Persist for every project on this machine.
graphit config --global ai.embedding.provider local

# Override one command.
graphit -c ast.grammars_blacklist=yaml ast index

# Override through the environment.
GRAPHIT_UI_HOST=0.0.0.0 graphit ui

# Inspect or remove values.
graphit config --list
graphit config --get knowledge.docs_dir
graphit config --unset knowledge.docs_dir
```

Use `--secret` to read a value from standard input without putting it in shell history:

```bash
printf '%s' "$PROVIDER_KEY" | graphit config --global --secret ai.embedding.api_key
```

Graphit redacts `hub.secret_access_key`, `ai.embedding.api_key`, `ai.rerank.api_key`, and
`mcp.api_key` in CLI configuration output. The global file is owner-readable only but stores
configured secrets as plain text; environment variables or a deployment secret store are
preferable.

Two compatibility keys do not use the normal layered resolver: `ai.cli` and `ai.agent_args` are
read from the global configuration used by the completion client. Set them with `--global`.

## Deployment profiles

| Profile | Essential settings | Result |
|---|---|---|
| Local workstation | no Hub bucket; local embedding; default modules | Sources, graphs, wikis, memory, and tasks stay on the machine. |
| Shared team state | `hub.bucket`, region/endpoint/credentials as needed | Hub artifacts plus authoritative Memory and Task tables can be shared by agents and machines. |
| CI artifact publisher | Hub and remote provider settings; agent, Dream, daemon, watcher, and hooks disabled | An ephemeral runner explicitly builds and publishes current AST and knowledge contexts without an agent CLI or background process. |
| Headless server | `modules.agent=false`, `modules.daemon_ui=true`, fixed `mcp.port`; usually remote storage | One daemon serves the authenticated MCP endpoint and the unauthenticated Observatory UI. |
| Private distribution | build-time `COMPILE_CONFIG` and brand variables | Defaults and identity ship with an internally distributed launcher. |

See [Publishing Graphit artifacts from GitHub Actions](github-actions-artifacts.md) for a complete
environment-only production publisher. Its setup flags suppress prompts while secrets continue to
resolve directly from the runner environment instead of being persisted in the ephemeral global
configuration. That profile requires an S3-backed Hub and a remote embedding provider with an
explicit model and API key; `ai.embedding.provider=local` is not supported there.

## General and agent selection

| Key | Default | Scope and effect |
|---|---|---|
| `ide` | `opencode` fallback | Selects the project adapter used for native MCP and lifecycle-hook files. `graphit init --ide` wins for that command. |
| `cli` | mapped from `ide`, then `opencode` | Default coding-agent CLI used when Graphit needs text synthesis or a Live Search worker. |
| `ai.cli` | unset | Global compatibility override checked before the normal CLI fallback chain. |
| `ai.agent_args` | unset | Global arguments appended only to streamed agentic runs with tool use enabled, currently Live Search and Dream. |
| `ai.agent_args.<binary>` | unset | Global agentic-run arguments for one executable, such as `ai.agent_args.claude`; wins over the generic value. Values split on whitespace, without shell quoting. |
| `unit.id` | generated ULID | Machine/user identity for user-scope memory. Set the same value on trusted machines to address the same user scope. |

Supported IDE adapters are `antigravity`, `claude`, `codex`, `cursor`, `gemini`, `kiro`, and
`opencode`. The adapter owns the project-local MCP and lifecycle-hook format; Graphit does not
copy instructions into generic repository prompt files.

There is no Graphit chat-completion model key: the selected CLI owns its provider and model. See
[AI Models, Providers, and Agent CLIs](ai_models.md) for the complete fallback order, prompt
transport used by every supported executable, session/stream behavior, and completion consumers.

## Embeddings and hybrid retrieval

Graphit's current production retrieval combines lexical and semantic candidates. AST search uses
BM25 full-text search plus vector similarity fused with reciprocal rank fusion (RRF); wiki search
can run lexical, semantic, or multi-wiki retrieval. The embedding provider controls how vectors
are produced, not whether exact graph traversal or BM25 search exists.

| Key | Default | Values and effect |
|---|---|---|
| `ai.embedding.provider` | `local` | `local`, `openai`, `openai-compatible`, `cohere`, `voyage`, or `google`. Remote providers send indexed text to that provider. |
| `ai.embedding.model` | provider default | Overrides the provider model. Required for `openai-compatible`. |
| `ai.embedding.base_url` | provider endpoint | Required for `openai-compatible`; optional endpoint override for named remote providers. |
| `ai.embedding.api_key` | provider-native environment | Graphit-managed API key. Native fallbacks are `OPENAI_API_KEY`, `COHERE_API_KEY`, `VOYAGE_API_KEY`, `GOOGLE_API_KEY`, and `GEMINI_API_KEY`. |
| `ai.embedding.dimensions` | model-known width; local is `768` | Positive vector width for custom or truncated models. Required when Graphit cannot infer the model width. |

Provider defaults:

| Provider | Default model | Dimensions |
|---|---|---:|
| Local ONNX | CodeRankEmbed-137M-INT8 | 768 |
| OpenAI | `text-embedding-3-small` | 1536 |
| Cohere | `embed-english-v3.0` | 1024 |
| Voyage AI | `voyage-3` | 1024 |
| Google | `text-embedding-004` | 768 |
| OpenAI-compatible | none | set `ai.embedding.dimensions` |

Changing provider, model, or vector width requires rebuilding embeddings because a Lance vector
column has a fixed width. The local embedding model is downloaded during `graphit setup`; remote
providers skip that download.

## Second-stage reranking

Graphit contains a bounded two-stage reranking engine: the first stage widens retrieval to 50
candidates by default, then a cross-encoder or remote rerank provider reorders the same candidate
set and trims it to the requested limit. Tie-breaking is deterministic.

| Key | Default | Values and effect |
|---|---|---|
| `search.rerank` | `false` | Enables the second stage for a caller that supplies the configured reranker. |
| `ai.rerank.provider` | `local` | `local`, `cohere`, `voyage`, or `jina`. |
| `ai.rerank.model` | provider default | Overrides the remote rerank model. |
| `ai.rerank.base_url` | provider endpoint | Overrides the remote rerank endpoint. |
| `ai.rerank.api_key` | provider-native environment | Graphit-managed key; native fallbacks include `COHERE_API_KEY`, `VOYAGE_API_KEY`, and `JINA_API_KEY`. |

The local provider downloads `bge-reranker-base` lazily on first use. Remote defaults are
`rerank-english-v3.0` for Cohere, `rerank-2` for Voyage AI, and
`jina-reranker-v2-base-multilingual` for Jina.

> [!IMPORTANT]
> The provider adapters and reranking pipeline are implemented, but the current CLI, MCP, and UI
> search entry points do not yet attach a reranker to production queries. Those entry points use
> hybrid BM25/vector RRF today. Treat `search.rerank` as integration-ready infrastructure, not as
> evidence that an existing search command has applied a second stage.

## Hub, shared state, and published artifacts

| Key | Default | Effect |
|---|---|---|
| `hub.bucket` | empty unless compiled | Enables S3-compatible Hub storage and remote authoritative Memory/Task tables. Empty means local-only. |
| `hub.region` | SDK/provider default unless compiled | Region for the S3 client. |
| `hub.endpoint` | AWS endpoint unless compiled | Custom endpoint for MinIO or another S3-compatible service. |
| `hub.prefix` | empty unless compiled | Slash-normalized namespace inside the bucket. |
| `hub.access_key_id` | AWS provider chain | Optional explicit identifier; active only with the secret. |
| `hub.secret_access_key` | AWS provider chain | Optional explicit secret; active only with the identifier. |
| `hub.icebug.reverse_edges` | `true` | Set explicit `false` to omit reverse CSR data from published AST artifacts. |
| `task.prefix` | `tasks` | Namespace for authoritative project task tables under the Hub prefix. It changes location; it does not migrate existing tables. |

Hub artifact types are `knowledge`, `ast`, `rule`, `skill`, `command`, `agent`, `mcp`, `power`,
and `language`. Installed artifacts are version-pinned in `graphit.lock.json`; large AST and
knowledge stores remain mounted at their global or S3 location instead of being copied into each
checkout.

## Network listeners

| Key | Default | Effect |
|---|---|---|
| `ui.host` | `127.0.0.1` | Interface used by `graphit ui` and daemon-hosted UI. The UI has no built-in authentication. |
| `ui.allowed_origins` | same-origin and loopback origins | Comma-separated exact CORS allowlist. A configured list replaces the loopback defaults; `*` allows any browser origin. |
| `mcp.host` | `127.0.0.1` | Interface for the daemon's streamable HTTP MCP listener. |
| `mcp.port` | `0` | Fixed port, or `0` for an OS-assigned port written to the daemon runtime directory. Invalid values fall back to `0`. |
| `mcp.api_key` | empty | Secret bearer key for the daemon MCP listener. Empty means generate a fresh random key on each start; a non-empty value is used exactly and remains stable until changed. |

Because the daemon is machine-global, set its listener keys in global configuration or the
environment. Configure a stable secret without placing it in shell history, then restart:

```bash
openssl rand -hex 32 | graphit config --global --secret mcp.api_key
graphit daemon restart
```

The same key is `GRAPHIT_MCP_API_KEY` in containers and service managers. `graphit config --get`
and `--list` redact it; copy the active value from **System → Daemon** or read
`~/.graphit/daemon/mcp.key`, which is written with mode `0600`. Unset the key and restart to return
to per-start random rotation:

```bash
graphit config --global --unset mcp.api_key
graphit daemon restart
```

The UI listener does not authenticate users. CORS is not authorization. Keep both listeners on
loopback unless a firewall, private network, or authenticated reverse proxy defines the remote
trust boundary.

## Knowledge indexing

| Key | Default | Effect |
|---|---|---|
| `knowledge.docs_dir` | `docs` | Project-relative documentation tree. `.` indexes the whole project. An explicit path passed to `knowledge index` wins. |
| `knowledge.include_readme` | `true` | Adds the first supported root README to the wiki even when it is outside the docs tree. |
| `knowledge.extensions` | see below | Comma-separated extension allowlist; leading dots are optional. |
| `wiki.version_retention` | `15m` | Minimum retention window for old wiki table versions. Values below one second fall back to the default. |

The default knowledge extensions are `md`, `markdown`, `mdx`, `txt`, `adoc`, `rst`, `puml`,
`plantuml`, `yaml`, `yml`, `json`, `proto`, `graphql`, `gql`, `wsdl`, and `xml`.

## AST indexing and language control

For the complete grammar-file schema, query selectors, merge semantics, binary
resolution, and new-parser workflow, see [AST Grammars and Parser Extensibility](ast_extensibility.md).

| Key | Default | Effect |
|---|---|---|
| `ast.index_source` | `true` | Stores source text needed for source reads and the full-text sidecar. |
| `ast.index_docs` | `false` | Adds `knowledge.docs_dir` to the code graph. Use only for code-shaped documentation such as schemas. |
| `ast.queries_dir` | `.graphit/ast/queries` | Project-relative, versionable directory for grammar query overrides. |
| `ast.grammar` | empty | Comma-separated `.extension=grammar` bindings. This is how exclusive SQL dialect grammars become reachable. |
| `ast.grammars_blacklist` | empty | Disables matching language/grammar names. |
| `ast.grammars_whitelist` | empty | When non-empty, enables only matching names; the blacklist still wins. |
| `ast.cluster_map` | empty | Comma-separated `path=cluster` prefixes. The longest matching path wins. |

`graphit ast index --cluster <name>` applies a default cluster to that invocation, and
`--cluster-path path=name` adds path mappings. The command persists an `ast.cluster` field for
historical compatibility, but automatic indexing does not currently resolve that field; use
`ast.cluster_map` for persistent automated routing and `--cluster` for an explicit invocation.

## Module switches

Every `modules.<name>` value is `true` or `false`. Core and process modules are on when absent,
except `dream` and `daemon_ui`, which are opt-in. A switch controls the orchestration paths that
consult it; it is not a blanket authorization layer unless stated below.

| Key | Default | What the switch controls |
|---|---|---|
| `modules.task` | on | Task mandate, lifecycle reconciliation, and all Task service operations. Disabled operations fail closed. |
| `modules.memory` | on | Memory mandate/bootstrap, synchronization, and daemon maintenance. |
| `modules.ast` | on | AST mandate plus lifecycle/daemon indexing. Direct AST commands remain explicit operations. |
| `modules.knowledge` | on | Knowledge mandate plus lifecycle/daemon indexing. Direct Knowledge commands remain explicit operations. |
| `modules.hub` | on | Hub routing in injected agent context and artifact preparation flows. |
| `modules.daemon` | on | Automatic daemon startup from ordinary CLI commands and setup. Manual daemon commands remain available. |
| `modules.sync` | on | Daemon filesystem synchronization module. `false` removes the recursive project watcher and incremental AST/Knowledge reactions. An explicit `graphit sync` remains available. |
| `modules.embedding` | on | Background and heavy-checkpoint embedding work. Exact graph and lexical operations remain available. |
| `modules.hooks` | on | Graphit's Git hook installation during synchronization. |
| `modules.agent` | on | Natural-language Cypher generation, AI wiki answers, and Live Search; graph, BM25, vector, and hybrid retrieval remain available. |
| `modules.dream` | off | Autonomous idle Dream cycles. |
| `modules.daemon_ui` | off | Long-running UI hosted by the daemon, primarily for server/container deployments. |

To stop filesystem watching for one project while keeping the daemon and manual synchronization:

```bash
cd /path/to/project
graphit config modules.sync false
graphit daemon restart
```

Use `graphit config --global modules.sync false` to disable watchers for every registered project,
or start the daemon with `GRAPHIT_MODULES_SYNC=false`. Set the value back to `true` and restart to
restore watching. The daemon selects the project module set when it constructs a supervisor, so a
running watcher is not removed immediately when the lockfile changes. `modules.sync` affects only
the daemon's incremental watcher: `graphit sync`, `graphit ast index`, and `graphit knowledge index`
remain explicit operations.

The five agent-routing modules—Task, Memory, AST, Hub, and Knowledge—also determine which
semantic mandates are injected at supported lifecycle boundaries. Native hooks reload current
configuration for the active project rather than baking checkout paths into generated commands.

## Daemon, Dream, and retention

| Key | Default | Effect |
|---|---|---|
| `daemon.activity_window` | `30m` | How recently a registered project must change to stay supervised. `0` disables parking. Invalid/negative values use the default. |
| `dream.idle_timeout` | `7200` seconds | Idle time before an enabled Dream cycle starts. |
| `dream.max_duration` | `28800` seconds | Maximum Dream session duration; `0` means unlimited. |
| `dream.reports_dir` | `.graphit/runtime/dream` | Project-relative report location. Move under `docs/` only when reports are intentionally versioned. |
| `memory.version_retention` | `720h` (30 days) | Minimum retention for old authoritative memory-table versions. Values below one second use the default. |

Dream improves memory and documentation during idle time but never consumes the Task backlog.
Task ownership and completion remain explicit, fenced actions.

## Runtime-only environment controls

These variables are operational overrides rather than dot-notation config keys:

| Variable | Effect |
|---|---|
| `GRAPHIT_GLOBAL_DIR` | Replaces the default global Graphit data directory. |
| `GRAPHIT_MODEL_CACHE` | Replaces the local embedding-model cache root. |
| `GRAPHIT_MAX_WORKERS` | Caps the shared CPU budget between 1 and the available CPU count. |
| `GRAPHIT_HEAVY_SLOTS` | Allows more than one CPU-saturating job per process, capped by the CPU budget. Default `1`; higher values trade peak memory for throughput. |
| `GRAPHIT_DB_THREADS` | Overrides the LadybugDB worker-thread budget. |
| `GRAPHIT_DB_BUFFER_MB` | Overrides the LadybugDB buffer pool in MiB. |
| `GRAPHIT_EMBED_THREADS` | Overrides local ONNX embedding intra-op threads. |
| `GRAPHIT_ANTLR_HEAP_MB` | Overrides the ANTLR sidecar heap budget in MiB. |
| `GRAPHIT_ANTLR_RESET_FILES` | Number of parsed files between ANTLR cache-pressure checks. Default `250`; positive integers only. |
| `GRAPHIT_ANTLR_SLL=1` | Forces the SLL-first parser path, including for grammars normally kept in LL mode. Use only for parser diagnosis or measured tuning. |
| `GRAPHIT_ANTLR_LL_ONLY=1` | Forces LL parsing and takes precedence when both prediction overrides are `1`. Use only for parser diagnosis or measured tuning. |
| `GRAPHIT_AGENT_SESSION_ID` | Supplies stable task attribution when a host-specific session ID is unavailable. Graphit otherwise recognizes `CODEX_THREAD_ID`, `CODEX_SESSION_ID`, `CLAUDE_SESSION_ID`, `CURSOR_SESSION_ID`, `GEMINI_SESSION_ID`, `OPENCODE_SESSION_ID`, and `KIRO_SESSION_ID`, in that order. |

The launcher sets `GRAPHIT_LAUNCHER_PATH` for internal process replacement;
`GRAPHIT_SUBAGENT_PROTOCOL_V1` is a generated protocol marker rather than an input; and
`GRAPHIT_TEST_HOME_ROOT` belongs only to hermetic test infrastructure. They are intentionally not
user configuration. Do not persist or override them in normal operation.

## Installer and image-build controls

These values select what gets installed; they are not read by the installed Graphit runtime and do
not participate in the configuration precedence chain.

| Surface | Control | Default | Effect |
|---|---|---|---|
| Linux/macOS `install.sh` | `--dir <path>` | `$HOME/.local/bin` | Launcher destination. |
| Linux/macOS `install.sh` | `--version <tag>` or `VERSION=<tag>` | latest release | Pins the release archive. The flag wins over the environment. |
| Windows `install.ps1` | `-Dir <path>` or `GRAPHIT_INSTALL_DIR=<path>` | `%LOCALAPPDATA%\Programs\graphit` | Launcher destination. The parameter wins over the environment. |
| Root `Dockerfile` | build argument `GRAPHIT_VERSION` | `latest` | Pins the Graphit release installed in the image. |
| Root `Dockerfile` | build arguments `BASE_IMAGE`, `EMBEDDING_PROVIDER`, `EMBEDDING_MODEL`, `EMBEDDING_BASE_URL`, `HTTP_PROXY`, `HTTPS_PROXY` | documented in the container guide | Selects the image base and build-time embedding preparation/network. Credentials remain runtime inputs. |

The PowerShell installer currently always selects the latest release; use the release archive
directly when Windows needs an exact version. See [Getting Started](getting_started.md) and
[Running Graphit Code as a server in a container](container.md).

## Verify an effective setup

```bash
graphit config --list
graphit config --list --global
graphit daemon status
graphit dream status
graphit ast schema
graphit knowledge list
graphit memory mandatory
graphit task list --ready
```

For implementation details and invalid-value behavior, continue with the
[Configuration Module Specification](../specs/config_module.md). For provider internals and
data-transfer boundaries, see [AI Models, Providers, and Agent CLIs](ai_models.md) and the
[AI Engine Specification](../specs/ai_engine.md). For lifecycle and monitoring behavior, see
[Daemon Operations and Monitoring](daemon_operations.md).
