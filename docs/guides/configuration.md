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
  - "docs/guides/github-actions-artifacts.md"
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
graphit provider add workstation --type local --embedding-mode local --rerank-mode local

# Override one command.
graphit -c ast.grammars_blacklist=yaml ast index

# Override through the environment.
GRAPHIT_UI_HOST=0.0.0.0 graphit ui

# Inspect or remove values.
graphit config --list
graphit config --get knowledge.docs_dir
graphit config --unset knowledge.docs_dir
```

`--secret` remains available for supported configuration secrets such as `client.secret`.
Authentication, broker and AI keys do not live in configuration; provider/account output redacts
them from the restricted auth store.

Two compatibility keys do not use the normal layered resolver: `ai.cli` and `ai.agent_args` are
read from the global configuration used by the completion client. Set them with `--global`.

## Deployment profiles

| Profile | Essential settings | Result |
|---|---|---|
| Local workstation | local auth provider without broker storage; local embedding; default modules | Sources, graphs, wikis, memory, and tasks stay on the machine while profiles remain isolated. |
| Shared S3 state | local provider with direct S3, OIDC provider with web-identity STS, or S3-enabled Broker provider with Broker-issued STS | Hub artifacts and authenticated Memory/Task tables use direct S3 access; Broker-routed AI remains independently selectable. |
| CI artifact publisher | non-interactive provider creation and login; agent, Dream, daemon, watcher, and hooks disabled | An ephemeral runner explicitly builds and publishes current AST and knowledge contexts without prompts. |
| Headless server | `modules.agent=false`, `modules.daemon_ui=true`, fixed `mcp.port`; usually remote storage | One daemon serves the authenticated MCP endpoint and the unauthenticated Observatory UI. |
| Private distribution | build-time `COMPILE_CONFIG` and brand variables | Defaults and identity ship with an internally distributed launcher. |

See [Publishing Graphit artifacts from GitHub Actions](github-actions-artifacts.md) for a complete
production publisher. `setup --non-interactive` configures runtime preferences; separate
non-interactive provider/login commands establish AI topology, local ONNX execution, the account,
and the S3 boundary. See also
[Authentication providers and account profiles](authentication.md).

## General and agent selection

| Key | Default | Scope and effect |
|---|---|---|
| `agent` | `opencode` fallback | Selects the project adapter used for native MCP and lifecycle-hook files. `graphit init --agent` wins for that command. |
| `cli` | mapped from `agent`, then `opencode` | Default coding-agent CLI used when Graphit needs text synthesis or a Live Search worker. |
| `ai.cli` | unset | Global compatibility override checked before the normal CLI fallback chain. |
| `ai.agent_args` | unset | Global arguments appended only to streamed agentic runs with tool use enabled, currently Live Search and Dream. |
| `ai.agent_args.<binary>` | unset | Global agentic-run arguments for one executable, such as `ai.agent_args.claude`; wins over the generic value. Values split on whitespace, without shell quoting. |
| `unit.id` | generated ULID | Local installation identity provisioned during setup for attribution and diagnostics. A lazy fallback covers non-CLI callers that bypass setup. It does not select user memory and is not trusted remote Hub identity. |

Supported agent adapters are `antigravity`, `claude`, `codex`, `cursor`, `gemini`, `kiro`,
`opencode`, `qwen`, `kimi`, and `deepcode`. The adapter owns the native MCP and lifecycle format.
Most are project-scoped; Kimi hooks are global and reference-counted across Graphit projects, while
Deep Code uses its project `AGENTS.md` plus the native completion `notify` callback because it has no
general lifecycle-hook API.

There is no Graphit chat-completion model key: the selected CLI owns its provider and model. See
[AI Models, Providers, and Agent CLIs](ai_models.md) for the complete fallback order, prompt
transport used by every supported executable, session/stream behavior, and completion consumers.

## Embeddings and hybrid retrieval

Graphit's current production retrieval combines lexical and semantic candidates. AST search uses
BM25 full-text search plus vector similarity fused with reciprocal rank fusion (RRF); wiki search
can run lexical, semantic, or multi-wiki retrieval. The embedding provider controls how vectors
are produced, not whether exact graph traversal or BM25 search exists.

The active named provider selects `local`, `direct`, `broker`, or `disabled`. With no active
profile, AI resolution uses the persisted provider named `local`, creating it with explicit local
embedding/rerank and `auto`/`0` ONNX execution if absent. Identity remains anonymous; local init,
AST, Knowledge, Memory, and Task remain available, while remote Hub is unavailable. Direct
endpoint/model/dimensions are provider topology and the API key is a login secret. Local reads the
`models.embedding.id` global key and obtains dimensions and semantics from that manifest. Broker
discovery supplies route/revision/dimensions; Graphit fingerprints and validates them. If the
provider has broker configuration, both embedding and rerank modes must be `broker`; mixed,
disabled, or omitted service modes are rejected. See [AI Models](ai_models.md) and [Broker](auth-broker.md).

| Key | Default | Values and effect |
|---|---|---|
| `models.embedding.id` | `coderankembed` | Selects a built-in or operator-defined local embedding manifest under the global `models/` directory. |

Local execution is not a layered config key. Each provider service in `local` mode stores
an `onnx` block with `device` and `device_id`. Provider add/update exposes independent embedding and
rerank flags and, interactively, preselects the provider's current values or `auto`/`0`. `auto`
tries CoreML then CPU on macOS and CUDA then CPU on Linux/Windows. Explicit accelerators fail rather
than falling back; CUDA IDs are zero-based, and CoreML is macOS-only with ID `0`. For local
embeddings, `auto` also recovers from accelerator OOM/resource failures by repeating the request on
CPU and periodically retrying a fresh accelerator session with bounded backoff. It returns to the
accelerator only after a successful inference; explicit device selections never switch at runtime.

## Second-stage reranking

Graphit contains a bounded two-stage reranking engine: the first stage widens retrieval to 50
candidates by default, then a cross-encoder, native remote rerank provider, or embedding-simulated
cosine scorer reorders the same candidate set and trims it to the requested limit. Tie-breaking is
deterministic.

| Key | Default | Values and effect |
|---|---|---|
| `search.rerank` | `false` | Enables the second stage in AST and Knowledge keyword/hybrid searches. |
| `models.rerank.id` | `bge-reranker-base` | Selects a built-in or operator-defined local rerank manifest under the global `models/` directory. It is independent from `search.rerank`, which still controls activation. |

In non-interactive provider commands, `--embedding-device*` and `--rerank-device*` apply only to a
corresponding local service. Omitted values preserve the provider's current value or use `auto`/`0`
for a new local service. Direct, broker, and disabled services store no ONNX block and reject these
flags. This rule follows service mode regardless of whether provider authentication is local,
direct OIDC, or Broker-managed.

On providers without broker configuration, the active provider selects local/direct/broker/disabled
for this backend independently. A broker-configured provider must use broker mode for both embedding
and rerank. Local resolves `models.rerank.id` according to the selected manifest's fetch policy;
direct keys are login secrets; broker model selection is server-side. The stage widens retrieval,
preserves the same candidate set, and then
trims to the requested limit. `search.rerank=false` still prevents construction and calls even though
the configured backend mode is broker.

Direct rerank accepts native `cohere`/`cohere-v2`, `voyage`/`voyage-v1`, and `jina`/`jina-v1`
protocols. It also accepts `openai`, `openai-compatible`, `openai-embeddings-v1`, and `google` as
embedding-simulated protocols. Simulated protocols require `--rerank-dimensions` and rank by cosine
similarity between the query embedding and batched document embeddings; native protocols reject
that flag. The endpoint, embedding model and dimensions belong to the provider, while the key is
stored as `Profile.RerankAPIKey`. Google means the Gemini API; Graphit applies the model-appropriate
query/document retrieval task or textual instruction and requests the configured output width.

## Hub, shared state, and published artifacts

Hub storage and identity are not layered configuration keys. A local provider may own direct S3
topology and choose login credentials, an AWS profile, or the allowed AWS chain. A direct OIDC
provider owns S3 topology plus an STS role and exchanges web identity. A first-class Broker provider
resolves topology and a restricted temporary session in memory for each project/user/Hub scope when discovery
advertises `graphit-s3-credentials-v2`; when the capability is absent because Broker S3 is disabled,
the profile remains authenticated and storage is local. A profile owns identity, service keys and
direct-provider renewable sessions; Broker S3 sessions are process-local. See [Authentication](authentication.md).

| Runtime key | Default | Effect |
|---|---|---|
| `hub.events.anonymize` | `false` | When `true`, Hub event payloads replace `project_id` and `user_id` with SHA-256 `project_hash` and `user_hash` values salted by the installation's `client.secret`. |
| `client.secret` | generated ULID | Installation salt provisioned during setup for anonymized event IDs. A lazy fallback covers non-CLI callers that bypass setup. Keep it stable to preserve hash continuity; changing it rotates every project/user hash. It is redacted as a secret. |
| `hub.icebug.reverse_edges` | `true` | Set explicit `false` to omit reverse CSR data from published AST artifacts. |
| `task.prefix` | `tasks` | Final namespace for authoritative Task tables in the filesystem fallback or S3. It changes location; it does not migrate existing tables. |

Hub artifact types are `knowledge`, `ast`, `rule`, `skill`, `command`, `agent`, `mcp`, `power`,
and `language`. Installed artifacts are version-pinned in `graphit.lock.json`; Hub AST and Knowledge
native stores are mounted directly through `s3://`. File artifacts remain managed local files.

The Hub uses the project's immutable ULID for logical keys and treats its globally unique mutable
name as discovery metadata. In Broker mode, Graphit sends its bearer only when resolving access or
renewing credentials; the Broker derives identity and session policy, and object traffic then goes
directly to S3. See
[Project Identity](../specs/project_identity.md) and
[Hub Access Control](../specs/hub_access_control.md).

`~/.<brand>/hub/cache/` is a bounded, lazy metadata cache isolated by Hub and subject. It has no
user-facing authority switch: clearing it is safe, and no configuration may make stale cache data
authorize remote content or mounts. File artifacts required by adapters are managed separately
under `~/.<brand>/artifacts/modules/`.

The unattended artifact-publisher profile creates a named provider and logs in with explicit
flags under global `--non-interactive`. Its event privacy switch is
`GRAPHIT_HUB_EVENTS_ANONYMIZE`; absence or any value other than `true`
keeps explicit identifiers. Its embedding topology is created by `graphit provider add`, while a
direct API key or broker key/token is supplied by `graphit login`. See
[Publishing Graphit artifacts from GitHub Actions](github-actions-artifacts.md) for the complete
validation and noninteractive setup sequence.

## Network listeners

| Key | Default | Effect |
|---|---|---|
| `ui.host` | `127.0.0.1` | Interface used by `graphit ui` and daemon-hosted UI. The UI has no built-in authentication. |
| `ui.allowed_origins` | same-origin and loopback origins | Comma-separated exact CORS allowlist. A configured list replaces the loopback defaults; `*` allows any browser origin. |
| `mcp.host` | `127.0.0.1` | Interface for the daemon's streamable HTTP MCP listener. |
| `mcp.port` | `0` | Fixed port, or `0` for an OS-assigned port written to the daemon runtime directory. Invalid values fall back to `0`. |

The daemon writes a fresh local runtime key to `~/.graphit/daemon/mcp.key` with mode `0600` on
each start. Static MCP keys are local-provider credentials. With an active direct OIDC or
Broker-managed provider, the listener also verifies each caller's access token; Broker-managed
tokens use the audience and JWKS discovered from the Broker, while direct OIDC checks
`oidc.mcp_audience`. Broker calls made by
that request use the same bearer (`relay`) or, for direct OIDC, a request-scoped RFC 8693 exchanged
token. A direct HTTP caller must supply its own identity. The stdio bridge always targets this local
daemon and deliberately uses the renewable access token of its active OIDC/Broker session, resolving
it again before every request; local/no-profile use falls back to a static local MCP key or the
current runtime key. `oidc.mcp_audience` and `oidc.mcp_resource` configure only the direct OIDC token
requested for this daemon listener, never a remote MCP endpoint.

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
| `memory.version_retention` | `720h` (30 days) | Minimum retention for old authoritative memory versions. Values below one second use the default. |

Dream improves memory and documentation during idle time but never consumes the Task backlog.
Task ownership and completion remain explicit, fenced actions.

## Runtime-only environment controls

These variables are operational overrides rather than dot-notation config keys:

| Variable | Effect |
|---|---|
| `GRAPHIT_GLOBAL_DIR` | Replaces the default global Graphit data directory. |
| `GRAPHIT_MODEL_CACHE` | Replaces the global root containing all local model bundles and manifests. |
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
| Root `Dockerfile` | build arguments `BASE_IMAGE`, `ANONYMIZE_EVENTS`, `EMBEDDING_PROVIDER`, `EMBEDDING_MODEL`, `EMBEDDING_BASE_URL`, `HTTP_PROXY`, `HTTPS_PROXY` | documented in the container guide | Selects the image base, event identifier privacy, and build-time embedding preparation/network. Credentials remain runtime inputs. |

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
