<p align="center">
  <img src="docs/site/assets/logo.svg" width="72" height="72" alt="Graphit Code">
</p>

<h1 align="center">Graphit Code</h1>

<p align="center"><strong>The context and control plane for AI software engineering.</strong></p>

<p align="center">
  <a href="https://github.com/graphit-labs/graphit-code/releases/latest"><img src="https://img.shields.io/github/v/release/graphit-labs/graphit-code?style=flat-square&color=b9fb63&labelColor=101311" alt="Latest release"></a>
  <a href="https://github.com/graphit-labs/graphit-code/actions"><img src="https://img.shields.io/github/actions/workflow/status/graphit-labs/graphit-code/release.yml?style=flat-square&labelColor=101311" alt="Build status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/graphit-labs/graphit-code?style=flat-square&labelColor=101311" alt="MIT license"></a>
  <a href="https://github.com/sponsors/lainosantos"><img src="https://img.shields.io/badge/Sponsor-Graphit-db61a2?style=flat-square&logo=github-sponsors&labelColor=101311" alt="Sponsor Graphit"></a>
</p>

<p align="center">
  <a href="https://graphit-labs.github.io/graphit-code">Website</a> ·
  <a href="#install">Install</a> ·
  <a href="#first-run">First run</a> ·
  <a href="docs/guides/configuration.md">Configure</a> ·
  <a href="docs/README.md">Documentation</a> ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

![Graphit AST Explorer analyzing the graphit-code repository](docs/site/assets/observatory-ast-explorer.jpg)

## AI agents need more than a prompt

Models are probabilistic. Engineering work cannot be.

Coding agents usually enter a repository with four blind spots:

- source text does not tell them the exact structural relationships in the code;
- a new session does not remember yesterday's correction or architectural decision;
- concurrent agents can duplicate work, overwrite ownership, or stop without a resumable checkpoint;
- documentation, sibling projects, and reusable agent tooling live in disconnected places.

Graphit closes those gaps with one local-first system that works across agents, IDEs, repositories,
machines, and model providers:

| Signal | What Graphit provides | What the agent can do |
|---|---|---|
| **AST** | Language-aware entities, source, and exact graph relationships | Find candidates with FTS + vectors, then prove callers, imports, inheritance, dependencies, and impact with Cypher |
| **Knowledge** | A compiled wiki built from maintained project documentation | Search pages, read only the relevant source, follow cross-references, and verify provenance |
| **Memory** | Durable project and user scopes with revision history | Carry corrections, conventions, decisions, and learned procedures across sessions and repositories |
| **Task** | A shared LanceDB scheduler with fenced claims, dependencies, checks, comments, immutable audit history, and complete JSON export | Coordinate parallel agents, make takeover safe, inspect work in the Task Explorer, and make incomplete work impossible to close |
| **Hub** | A versioned registry for reusable agent capabilities and contexts | Share rules, skills, agents, commands, MCP servers, languages, ASTs, and knowledge across systems |
| **Observatory** | One operational workspace over the same stores agents use | Explore code, docs, memory, live runs, daemon state, Dream, and ecosystem projects without a second data model |

Graphit does not make a language model deterministic. It puts deterministic discovery, ownership,
validation, persistence, and completion gates around whichever coding agent you choose.

## Built for teams, agents, and software ecosystems

- **One project, many agents.** Atomic claims and fencing tokens prevent stale writers; checkpoints,
  typed decisions, and `next_step` let another agent resume without reconstructing the work.
- **One engineer, many systems.** Project memory stays repository-specific while user memory follows
  personal conventions across projects. Registered sibling projects retain independent stores.
- **One team, many machines.** Optional S3-compatible storage makes Hub artifacts plus Memory and Task
  tables directly shareable without copying them into every checkout.
- **One framework, many assistants.** Native adapters support Codex, Claude Code, Cursor, Gemini CLI,
  Kiro, OpenCode, and Antigravity; any MCP client can use the server endpoint.
- **One query, several retrieval modes.** BM25 full-text search, semantic vectors, hybrid reciprocal
  rank fusion, exact graph traversal, and source slicing serve different evidence needs.

## The Graphit Observatory

The web UI is an operational view over the same project context exposed to agents.

| Knowledge Explorer | Memory Explorer |
|---|---|
| ![Graphit Knowledge Explorer showing project architecture](docs/site/assets/observatory-knowledge-explorer.jpg) | ![Graphit Memory Explorer showing a design decision](docs/site/assets/observatory-memory-explorer.jpg) |

These screenshots use Graphit Code itself as the example project. The same Observatory also includes
a Task Explorer with a paginated task catalogue, server-side status/search filters, complete specs,
checks, dependencies, subtasks, comments, lifecycle events, revision history, and JSON download.

## Install

Prebuilt releases support Linux, macOS, and Windows.

### Linux or macOS

```bash
curl -fsSL https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.sh | sh
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.ps1 | iex
```

The installers detect the platform, download the latest archive, verify its SHA-256 checksum, and install the launcher in a user directory. On the next invocation, the launcher extracts a changed Core, the daemon replaces itself, and the stdio MCP proxy asks connected clients to refresh their tool catalog through the protocol's list-change notification. Pin a release with `--version <tag>`. See the [getting started guide](docs/guides/getting_started.md) for manual downloads, custom paths, and source builds.

### Run it as a server for any MCP agent

The root `Dockerfile` builds a server: the daemon as PID 1, publishing an **MCP endpoint** and the UI.

**Any MCP-capable AI agent connects to it** — Claude Code, Codex, Gemini, Cursor, OpenCode, Copilot, Kiro, or your own client. The agent runs wherever the developer is and brings its own model; the server supplies the code graphs, documentation wikis and memory it reasons over. One container serves a team, and nobody indexes anything locally.

```bash
docker build -t graphit-code .

docker run -d --name graphit \
  -p 127.0.0.1:8080:8080 \
  -p 127.0.0.1:8081:8081 \
  -v graphit-global:/opt/graphit \
  graphit-code
```

Point a client at `http://your-server:8081/mcp` with `Authorization: Bearer <key>`. In the UI, open **System → Daemon** to copy the full active key from **MCP bearer key** and confirm the endpoint. The server holds no source checkouts and needs none—it answers about Hub artifacts addressed reproducibly as `id@version`.

Remote agents can load the server's current routing contract with `graphit_mandates` and fetch the
complete source of any core module skill with `graphit_module_skill`. Start from the copy-ready
[remote agent skill](docs/examples/skills/graphit-remote/SKILL.md).

The MCP endpoint uses a fresh random bearer key on every daemon start by default. For a stable
server credential, set the secret `mcp.api_key` globally or supply `GRAPHIT_MCP_API_KEY`; the exact
value remains active across restarts until you change it and restart the daemon. Configuration
output redacts it, while the active key remains available from **System → Daemon** and the
mode-`0600` runtime key file. The UI has no built-in authentication, and CORS is not authorization,
so keep both ports on a trusted
network or put an authenticated proxy in front. Read
[Running Graphit Code as a server in a container](docs/guides/container.md) before exposing them.

## First run

Run Graphit from the repository it should understand:

```bash
cd your-project
graphit setup
graphit init --ide codex
graphit sync
graphit ui
```

`graphit setup` prepares machine-wide providers and the local model when selected. `graphit init`
creates the project identity, installs the selected IDE's native MCP/hooks, and performs the first
synchronization. `graphit sync` is the explicit all-system checkpoint; the daemon keeps incremental
indexes current afterwards.

Use the exact IDE identifier supported by your environment; `graphit init --help` lists the available values.

## What agents gain

### Find broadly, then prove structurally

Graphit indexes declarations and relationships from Tree-sitter and ANTLR grammars into an
Icebug/LadybugDB graph. A separate Lance sidecar combines BM25 full-text and semantic vector results
with reciprocal rank fusion. Agents use ranked search to find the likely entity, exact Cypher to
establish relationships, and a source call to read only the relevant lines.

```cypher
MATCH (caller)-[:CALLS]->(target:Function {name: 'RunSync'})
RETURN caller.name, caller.path
```

The graph opens on the fly from Icebug files into an in-memory catalog, so published contexts remain
portable without running a separate graph server. Optional second-stage rerank providers are
implemented for local, Cohere, Voyage AI, and Jina; current public search entry points use hybrid RRF
while production rerank wiring remains pending. See the [AI Engine specification](docs/specs/ai_engine.md).

### Source-backed knowledge

The knowledge module compiles `docs/` and the root README into a searchable wiki. Pages retain their source, confidence, links, and update history; agents read the selected page after search instead of treating a ranked title as the answer.

### Memory that survives the session

Project memory captures repository-specific decisions and corrections. User memory captures portable personal conventions. Both are stored outside the checkout and exposed through the same search-and-read workflow.

### Deterministic control that survives the agent

Graphit Task replaces host-native TODO lists and repository Markdown task logs with one project task database. Agents search prior work, atomically claim a ready task, checkpoint progress and decisions, revise scope through expected-revision fencing, supersede obsolete checks without erasing history, verify active acceptance/test checks with evidence, and release or complete through fenced transitions. Dependencies and nested subtasks gate readiness and completion; flags carry a reason and block completion until resolved. Task IDs are compact hashes that lengthen only on a detected collision, while conditional writes prevent one task from overwriting another. Direction changes deterministically cancel useful history or remove certainly erroneous, unreferenced tasks so no obsolete work is left open. The Observatory discovers work through a lightweight paginated catalogue and loads the same versioned complete JSON export as CLI and MCP only for exact detail or an explicit project download. With S3 configured, every project agent reads and writes the same LanceDB tables directly.

### Reusable context across systems

Registered sibling projects keep their own AST, wiki, and memory. Hub artifacts package reusable capabilities when a project or team intentionally publishes them. Optional S3-compatible storage supports shared catalogs and published contexts; everyday local operation does not require a hosted database.

### Ephemeral synthesis across several sources

Live Search prepares a throwaway project from selected Hub artifacts, installs the requested agent
environment, streams a bounded agent session, and removes the project data when the session is
deleted. It is the on-the-fly path for questions that genuinely span several codebases or knowledge
bundles; direct AST, Wiki, Memory, and Task tools remain the cheaper path for focused questions.

## Configure the operating model

| Goal | Setting |
|---|---|
| Keep everything local | leave `hub.bucket` empty (the default) |
| Share Hub, Memory, and Task state | configure `hub.bucket` and its S3-compatible boundary |
| Run without an installed coding-agent CLI | `modules.agent=false` |
| Keep autonomous Dream work off/on | `modules.dream=false` (default) or `true` |
| Serve the Observatory from the daemon | `modules.daemon_ui=true` |
| Disable the daemon filesystem watcher | `modules.sync=false` (manual `graphit sync` still works) |
| Disable background embedding work | `modules.embedding=false` |
| Select a remote embedding backend | `ai.embedding.provider=<provider>`: `openai`, `cohere`, `voyage`, `google`, or `openai-compatible` |
| Restrict indexed languages | `ast.grammars_whitelist` / `ast.grammars_blacklist` |
| Move or narrow the documentation tree | `knowledge.docs_dir`, `knowledge.extensions`, `knowledge.include_readme` |

Every normal key can be set per command, environment, project, global installation, or private-build
default. The [complete configuration reference](docs/guides/configuration.md) documents every key,
default, switch, provider, network boundary, and runtime resource control.

## Security boundary

- Mutable project sources and compiled local stores remain on the machine by default.
- Hub publication and S3-compatible storage are optional and explicitly configured; shared Task storage uses the configured S3 location directly.
- The UI binds according to `ui.host` and has no built-in authentication layer.
- Remote UI access requires an appropriate firewall, VPN, or authenticated reverse proxy; CORS is not authorization.

See [S3 credentials and UI network configuration](docs/guides/s3-and-ui-network.md) before exposing the UI or configuring shared storage.

## Documentation

Start with the document that matches your intent:

- [Getting started](docs/guides/getting_started.md) — install and initialize a project.
- [User manual](docs/guides/user_manual.md) — daily workflows and operational concepts.
- [Configuration reference](docs/guides/configuration.md) — every setting, default, feature switch, provider, and environment override.
- [AI models, providers, and agent CLIs](docs/guides/ai_models.md) — completion delegation, every CLI protocol, embedding models, credentials, dimensions, rerank, and local/remote boundaries.
- [Daemon operations and monitoring](docs/guides/daemon_operations.md) — start paths, schedulers, watched signals, module loops, MCP service, logs, parking, and recovery.
- [Capability and surface matrix](docs/guides/capability_matrix.md) — every module, CLI/MCP/UI exposure, gate, and current limitation.
- [Filesystem, state, and watchers](docs/guides/filesystem_contract.md) — special files, generated state, adapter layouts, and change detection.
- [AST grammars and parser extensibility](docs/guides/ast_extensibility.md) — every YAML field, selector, parser extension path, precedence rule, and validation workflow.
- [CLI reference](docs/guides/cli_reference.md) — commands and flags.
- [MCP tools reference](docs/guides/mcp_tools_reference.md) — agent-facing tool contracts.
- [Architecture overview](docs/architecture/architecture_overview.md) — system boundaries and data flow.
- [Storage layout](docs/architecture/storage_layout.md) — what lives in a project and what lives globally.
- [Task module](docs/specs/task_module.md) — shared lifecycle, ordered batches, durable claims, checks, hooks, and takeover guarantees.
- [UI specification](docs/specs/ui_dashboard.md) — Observatory behavior and backend contract.
- [Documentation hub](docs/README.md) — the complete maintained documentation map.

Task history lives in the authoritative LanceDB tables; changelogs and accepted decisions remain documentation evidence. The documentation hub separates historical records from current operational guidance.

## Build from source

Source builds require Go 1.26.6+, Node.js 22+, Make, and a C/C++ toolchain. The normal build downloads an immutable, checksum-verified native dependency bundle from [`graphit-labs/graphit-code-lib`](https://github.com/graphit-labs/graphit-code-lib); Rust is needed only for the explicit maintainer command `make fetch-lancedb`.

```bash
git clone https://github.com/graphit-labs/graphit-code.git
cd graphit-code
make install
graphit setup
```

Platform-specific targets and development checks are documented in [Getting Started](docs/guides/getting_started.md) and [Contributing](https://github.com/graphit-labs/graphit-code/blob/main/CONTRIBUTING.md).

## Project status

Graphit Code is under active development. Interfaces, storage formats, and supported integrations may evolve between releases. Prefer the documentation on the same branch or release as the binary you are using.

## License

Licensed under the [MIT License](https://github.com/graphit-labs/graphit-code/blob/main/LICENSE).

If Graphit improves your agent workflow, consider [sponsoring its development](https://github.com/sponsors/lainosantos).
