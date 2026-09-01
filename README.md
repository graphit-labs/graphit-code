<p align="center">
  <img src="docs/site/assets/logo.svg" width="72" height="72" alt="Graphit Code">
</p>

<h1 align="center">Graphit Code</h1>

<p align="center"><strong>A system of code intelligence, durable memory, and connected knowledge for coding agents.</strong></p>

<p align="center">
  <a href="https://github.com/graphit-labs/graphit-code/releases/latest"><img src="https://img.shields.io/github/v/release/graphit-labs/graphit-code?style=flat-square&color=b9fb63&labelColor=101311" alt="Latest release"></a>
  <a href="https://github.com/graphit-labs/graphit-code/actions"><img src="https://img.shields.io/github/actions/workflow/status/graphit-labs/graphit-code/release.yml?style=flat-square&labelColor=101311" alt="Build status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/graphit-labs/graphit-code?style=flat-square&labelColor=101311" alt="Apache-2.0 license"></a>
  <a href="https://github.com/sponsors/lainosantos"><img src="https://img.shields.io/badge/Sponsor-Graphit-db61a2?style=flat-square&logo=github-sponsors&labelColor=101311" alt="Sponsor Graphit"></a>
</p>

<p align="center">
  <a href="https://graphit-labs.github.io/graphit-code">Website</a> ·
  <a href="#install">Install</a> ·
  <a href="#first-run">First run</a> ·
  <a href="docs/README.md">Documentation</a> ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

![Graphit AST Explorer analyzing the graphit-code repository](docs/site/assets/observatory-ast-explorer.jpg)

## Why Graphit

Coding agents usually enter a repository with three blind spots:

- source text does not tell them the exact structural relationships in the code;
- a new session does not remember yesterday's correction or architectural decision;
- documentation, sibling projects, and reusable agent tooling live in disconnected places.

Graphit closes those gaps with one local-first harness:

| Signal | What Graphit provides | What the agent can do |
|---|---|---|
| **AST** | Language-aware entities and graph relationships | Query callers, imports, inheritance, dependencies, source, and complexity |
| **Knowledge** | A compiled wiki built from maintained project documentation | Search pages, follow cross-references, and verify provenance |
| **Memory** | Durable project and user scopes | Reuse corrections, conventions, decisions, and discoveries |
| **Hub** | A registry for shareable agent artifacts and contexts | Install rules, skills, agents, MCP servers, languages, ASTs, and knowledge |
| **Observatory** | One visual workspace | Explore code, docs, memory, daemon state, Dream, and ecosystem projects |

Graphit does not replace your coding agent. It gives the agent a dependable way to inspect the system it is changing.

## The Graphite Observatory

The web UI is an operational view over the same project context exposed to agents.

| Knowledge Explorer | Memory Explorer |
|---|---|
| ![Graphit Knowledge Explorer showing project architecture](docs/site/assets/observatory-knowledge-explorer.jpg) | ![Graphit Memory Explorer showing a design decision](docs/site/assets/observatory-memory-explorer.jpg) |

These screenshots use Graphit Code itself as the example project.

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

The installers detect the platform, download the latest archive, verify its SHA-256 checksum, and install the launcher in a user directory. Pin a release with `--version <tag>`. See the [getting started guide](docs/guides/getting_started.md) for manual downloads, custom paths, and source builds.

### Run it as a server, for any AI agent

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

Point a client at `http://your-server:8081/mcp` with `Authorization: Bearer <key>`; the UI's daemon page shows the key with a copy button. The server holds no source checkouts and needs none — it answers about Hub artifacts, addressed by name and version.

Neither port is authenticated, so publish both to the host loopback as above and read [Running Graphit Code as a server in a container](docs/guides/container.md) before exposing them.

## First run

Run Graphit from the repository it should understand:

```bash
cd your-project
graphit setup
graphit init --ide codex
graphit sync
graphit ui
```

`graphit setup` prepares the machine-wide runtime and local embedding model. `graphit init` installs the project-facing instructions and MCP configuration for the selected IDE. `graphit sync` builds the current AST, knowledge, and memory indexes. The daemon keeps them current afterwards.

Use the exact IDE identifier supported by your environment; `graphit init --help` lists the available values.

## What agents gain

### Structural code queries

Graphit indexes declarations and relationships from Tree-sitter and ANTLR grammars into an Icebug/LadybugDB graph. Agent-facing tools can search for names and then execute exact Cypher traversals for structural answers.

```cypher
MATCH (caller)-[:CALLS]->(target:Function {name: 'RunSync'})
RETURN caller.name, caller.path
```

### Source-backed knowledge

The knowledge module compiles `docs/` and the root README into a searchable wiki. Pages retain their source, confidence, links, and update history; agents read the selected page after search instead of treating a ranked title as the answer.

### Memory that survives the session

Project memory captures repository-specific decisions and corrections. User memory captures portable personal conventions. Both are stored outside the checkout and exposed through the same search-and-read workflow.

### Reusable ecosystem context

Registered sibling projects keep their own AST, wiki, and memory. Hub artifacts package reusable capabilities when a project or team intentionally publishes them. Optional S3-compatible storage supports shared catalogs and published contexts; everyday local operation does not require a hosted database.

## Security boundary

- Mutable project sources and compiled local stores remain on the machine by default.
- Hub publication and S3-compatible storage are optional and explicitly configured.
- The UI binds according to `ui.host` and has no built-in authentication layer.
- Remote UI access requires an appropriate firewall, VPN, or authenticated reverse proxy; CORS is not authorization.

See [S3 credentials and UI network configuration](docs/guides/s3-and-ui-network.md) before exposing the UI or configuring shared storage.

## Documentation

Start with the document that matches your intent:

- [Getting started](docs/guides/getting_started.md) — install and initialize a project.
- [User manual](docs/guides/user_manual.md) — daily workflows and operational concepts.
- [CLI reference](docs/guides/cli_reference.md) — commands and flags.
- [MCP tools reference](docs/guides/mcp_tools_reference.md) — agent-facing tool contracts.
- [Architecture overview](docs/architecture/architecture_overview.md) — system boundaries and data flow.
- [Storage layout](docs/architecture/storage_layout.md) — what lives in a project and what lives globally.
- [UI specification](docs/specs/ui_dashboard.md) — Observatory behavior and backend contract.
- [Documentation hub](docs/README.md) — the complete maintained documentation map.

Task logs, changelogs, and accepted decisions are retained as historical evidence. The documentation hub separates those records from current operational guidance.

## Build from source

Source builds require Go 1.23+, Node.js 22+, Make, a C/C++ toolchain, and Rust for the native LanceDB build.

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

Licensed under the [Apache License 2.0](https://github.com/graphit-labs/graphit-code/blob/main/LICENSE).

If Graphit improves your agent workflow, consider [sponsoring its development](https://github.com/sponsors/lainosantos).
