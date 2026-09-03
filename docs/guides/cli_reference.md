---
title: "CLI Command Reference"
description: "Reference manual detailing every subcommand, flag, and configuration override option in the Graphit CLI."
content-type: reference
audience: developers
keywords:
  - CLI
  - commands
  - reference
  - arguments
  - flags
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/user_manual.md"
---

# Command Line Interface Reference

The `graphit` command-line tool acts as the control center for configuring project registries, indexing code structure, query-tuning, and running background daemon tasks.
This document lists every command and flag available in the CLI.

---

## Global Flags

These flags can be appended to any command:

| Flag | Shorthand | Description | Default |
|---|---|---|---|
| `--verbose` | `-v` | Enables detailed debug logs on stdout. | `false` |
| `--config` | `-c` | Override a configuration value inline (e.g. `-c ide=cursor`). | `nil` |

---

## Project Lifecycle Commands

These commands initialize and maintain Graphit Code environments:

### `setup`
Initializes the global directory (`~/.graphit`), collects machine-wide defaults, verifies
the configured S3-compatible Hub, and downloads shared runtime assets.
```bash
graphit setup
```

**Every question also has a flag, and a question whose flag is supplied is not asked.** Answer one
thing on the command line and setup asks about the rest; answer everything it reaches and it needs
no terminal at all, which is how it runs in a container or a pipeline. There is no separate
non-interactive switch — silence is the consequence of having answered, not a mode.

```bash
# one answer given, the rest still asked
graphit setup --ide cursor

# nothing left to ask: a local-only hub, so region, endpoint and credentials are
# never reached, and both providers are local
graphit setup --hub-bucket "" --ide cursor --cli cursor-agent \
  --embedding-provider local --rerank-provider local
```

An empty value **is** an answer: it clears that key. Omitting the flag leaves the key untouched. The
distinction matters for the credential pair — `--hub-access-key-id ""` clears both halves, while
passing neither credential flag leaves a stored pair alone.

**Flags:**

| Flag | Sets | Notes |
|---|---|---|
| `--hub-bucket <string>` | `hub.bucket` | Empty selects local-only mode, which also skips the region, endpoint and credential questions |
| `--hub-region <string>` | `hub.region` | |
| `--hub-endpoint <string>` | `hub.endpoint` | For MinIO and other S3-compatible servers |
| `--hub-access-key-id <string>` | `hub.access_key_id` | Both credential flags are needed; either one empty clears the pair |
| `--hub-secret-access-key <string>` | `hub.secret_access_key` | Prefer `GRAPHIT_HUB_SECRET_ACCESS_KEY` — a value passed here is stored in plain text |
| `--ide <string>` | `ide` | Default IDE |
| `--cli <string>` | `cli` | Default CLI for the AI fallback |
| `--embedding-provider <string>` | `ai.embedding.provider` | `local`, `openai`, `openai-compatible`, `cohere`, `voyage`, `google` |
| `--embedding-model <string>` | `ai.embedding.model` | Empty for the provider's own default |
| `--embedding-base-url <string>` | `ai.embedding.base_url` | Required by `--embedding-provider openai-compatible` |
| `--embedding-api-key <string>` | `ai.embedding.api_key` | Prefer `GRAPHIT_AI_EMBEDDING_API_KEY` |
| `--rerank-provider <string>` | `ai.rerank.provider` | `local`, `cohere`, `voyage`, `jina` |
| `--rerank-model <string>` | `ai.rerank.model` | Empty for the provider's own default |
| `--rerank-api-key <string>` | `ai.rerank.api_key` | Prefer `GRAPHIT_AI_RERANK_API_KEY` |

A provider other than `local` reaches three further questions — model, base URL, API key — so a
fully scripted run with a remote provider has to answer all three. Passing an api-key flag as the
empty string is how to say "store no key here" without being asked; the provider then reads its own
environment variable at run time.

Nothing is softened by answering in advance. An unreachable hub bucket and, for the local embedding
provider, a failed model download both fail the command rather than leaving a half installation
reporting success. See [Run as a Server in a Container](container.md) for a complete scripted
invocation.

When a Hub bucket is entered, setup optionally asks for an S3 access key and secret. A
complete pair is saved globally; leaving either prompt blank removes both explicit keys
and keeps the AWS SDK provider chain active. The secret is not echoed, but it is stored as
plain text in the owner-only global config file. Prefer profiles or workload roles when
possible. See [S3 Credentials and UI Network Configuration](s3-and-ui-network.md).

Its final step downloads the embedding model (~132 MB) into
`~/.graphit/models/coderankembed/`, showing a progress bar on a terminal and reporting
in tenths when the output is redirected. The model is not built into the binary.

**The command fails if that download fails** — a non-zero exit, because an
installation without the model cannot answer a semantic query. It is the last step, so
every setting collected before it is already saved and re-running `setup` after fixing
the network loses nothing.
See [AI Engine](../specs/ai_engine.md#-model-manager-downloaded-once-shared-by-everything).

### `init`
Initializes a new project workspace.
It creates a project-local `graphit.lock.json` file, registers the project under the global active tracker, generates rules or skills files, and maintains a generated block in the project's `.gitignore`. The block ignores `**/.graphit/runtime/` and `**/.graphit/grammars/`; project query YAMLs and rule overrides remain versionable. See [Storage Layout](../architecture/storage_layout.md#inside-a-projects-brand-directory).
```bash
graphit init --ide <ide_name> [flags]
```
**Flags:**
- `--ide <string>`: Targets a specific model coding assistant (e.g. `cursor`, `claude`, `kiro`, `gemini`, `antigravity`).
- `--id <string>`: Explicitly overrides or sets the project ULID metadata.
- `--name <string>`: Sets the human-readable project name.
- `--description <string>`: Sets the project description inline to skip prompts.

### `sync`
Forces a complete synchronization of the project state.
```bash
graphit sync [flags]
```
**Flags:**
- `--no-background`: Prevents spawning background tasks asynchronously. Both phases (sync and heavy indexing/processing) execute synchronously inside the terminal process.
- `--heavy`: Runs only Phase 2 tasks (generating embeddings and memory consolidation).

### `update`
Checks for updates to registered artifacts (rules, skills, prompts) in the decentralized Hub registry and pulls down fresh copies.
```bash
graphit update --ide <ide_name>
```

### `remove`
Deletes project-local configuration, rules files, git hooks, and ignorer configurations. Does not affect source code or the global database.
```bash
graphit remove --ide <ide_name>
```

### `uninstall`
Cleans up all global parameters, schedules, background daemons, and deletes the `~/.graphit` workspace directory.
```bash
graphit uninstall
```

### `self-update`
Self-downloads and replaces the current `graphit` CLI binary with the latest release published on GitHub.
```bash
graphit self-update
```

---

## Configuration Management

### `config`
Gets, sets, or unsets parameters inside your active configuration file.
By default, targets your project-local `graphit.lock.json`.
```bash
graphit config [key] [value] [flags]
```
**Flags:**
- `--global`: Reads or updates parameters in `~/.graphit/config.json` instead.
- `--get`: Retrieves the value of a key.
- `--unset`: Deletes a key from the configuration.
- `--list`: Prints all keys and values.
- `--secret`: Reads a value securely from stdin without echoing it on terminal logs.

**Examples:**
```bash
graphit config ide cursor
graphit config --global ide cursor
graphit config --get ide
graphit config --unset ide
graphit config --list
printf '%s' "$S3_SECRET_ACCESS_KEY" | graphit config --global --secret hub.secret_access_key
```

**AST Cluster Configuration:**
```bash
# Set cluster mapping for multi-domain monorepos (persisted to graphit.lock.json)
graphit config ast.cluster_map "backend/=python,frontend/=javascript,shared/=typescript"

# Set default cluster for unmatched paths
graphit config ast.cluster default-cluster
```
The `ast.cluster_map` accepts comma-separated `path=cluster` pairs. Paths are directory prefixes (trailing slash optional). When using `graphit ast index` or `graphit ast watch` with `--cluster-path`, the mapping is automatically persisted.

`hub.secret_access_key` is shown as `[REDACTED]` by `--get` and `--list`; redaction does
not encrypt the value on disk. Project values override global values, while matching
`GRAPHIT_*` environment variables override both.

---

## Dashboard Interface

### `ui`
Launches the embedded unified web application server and automatically selects a free port.
Allows you to explore the AST code database in 3D, chat with the wiki knowledge, and view memories.
```bash
graphit ui [--repo <path>]
```

The server binds to `ui.host` (`127.0.0.1` by default). Browser origins use the exact,
comma-separated `ui.allowed_origins` policy; without an override, only same-origin and
localhost loopback origins are accepted. The server has no authentication, so a reachable
instance needs a firewall, VPN, or authenticated reverse proxy. `--repo` selects the
repository to visualize; there is no fixed-port flag. See
[S3 Credentials and UI Network Configuration](s3-and-ui-network.md).

---

## MCP Integration

### `mcp`
Starts an MCP (Model Context Protocol) server.
Allows AI tools to consume AST querying, memory search, and wiki indexes via standardized MCP actions.

The MCP server runs inside the daemon process, exposed via HTTP on a dynamic port with Bearer token authentication.
The `--stdio` flag starts a lightweight proxy that relays JSON-RPC messages between stdin/stdout and the daemon's HTTP endpoint.

```bash
graphit mcp [flags]
```
**Flags:**
- `--stdio`: Starts the MCP stdio proxy for IDE integration (used by Claude Code, Cursor, Gemini, etc.).

**Without flags:** Displays the MCP HTTP endpoint URL and auth information.

**Architecture:**
- The daemon listens on `127.0.0.1:<dynamic-port>/mcp` (Streamable HTTP transport)
- Authentication: Bearer token stored in `~/.graphit/daemon/mcp.key`
- Port: Written to `~/.graphit/daemon/mcp.port`
- The stdio proxy auto-recovers if the daemon restarts (re-reads port/key files)

---

## Subsystem Commands

### `ast`
Directly indexes, queries, and manages the abstract syntax tree.
```bash
graphit ast <subcommand> [flags]
```
**Subcommands:**
- `index [path...]`: Parses source code and builds the AST knowledge graph.
  - `--reset`: Wipe database before indexing.
  - `--reindex`: Wipe only this repo's data before re-indexing.
  - `--cluster <name>`: Logical cluster tag for queries (fallback for unmatched paths).
  - `--cluster-path <path=cluster>`: Tag nodes under <path> with <cluster> (repeatable). Paths are directory prefixes; most specific match wins.
  - `--workers <int>`: Worker thread count.
  - `--no-source`: Skip storing raw source code inside nodes.
- `watch [path]`: Watch directory for file changes and re-index incrementally.
  - `--cluster <name>`: Logical cluster tag (fallback).
  - `--cluster-path <path=cluster>`: Tag nodes under <path> with <cluster> (repeatable).
  - `--workers <int>`: Worker thread count.
- `query <cypher-query | natural-language-question>`: Execute graph query.
  - `--ai`: Generate Cypher from natural language via AI.
  - `--cypher`: Print generated Cypher without executing (requires `--ai`).
  - `--ai-optimized`: Output tabular representation for AI agent tokens.
  - `--hybrid`: Perform combined BM25 + semantic vector search (RRF).
  - `--top <int>`: Limit results count.
  - `--context <name>`: Query an imported context instead of project.
- `schema`: Print the AST node properties and labels schema.
  - `--context <name>`: Context name.
- `install <path> --context <name>`: Import external AST database into named context.
  - `--reset`: Wipe context before importing.
  - `--list`: List imported contexts.
  - `--workers <int>`: Thread count.
- `remove`: Wipe project AST or context.
  - `--context <name>`: Context name.
- `sync`: Re-sync imported context from cache.
- `export`: Export AST graph to Obsidian vault or `.ast` bundle.
  - `--format <format>`: Format: `obsidian` or `bundle`.
  - `--output <dir>`: Output path; defaults to `.graphit/runtime/ast/export/`.
  - `--no-sources`: Exclude source code contents.
- `list`: List all installed AST contexts.
- `source <relative-path>`: Show stored source code for a file.
  - `--entity <name>`: Extract specific entity range.
  - `--entity-type <type>`: Class context type.
  - `--head <int>`: Show first N lines.
  - `--tail <int>`: Show last N lines.
  - `--start <int>`: Start line.
  - `--end <int>`: End line.
  - `--pattern <string>`: Grep-like match.
  - `--regex`: Treat pattern as regex.
  - `--before <int>`: Context before match.
  - `--after <int>`: Context after match.
  - `--line-numbers`: Print line prefixes.
  - `--context <name>`: Context name.
- `embed`: Generate vector embeddings for semantic search.
- `rule`: Manage the AST module rule.
  - `--unset`: Remove customization.
  - `--default`: Show default rule.

### `knowledge`
Manages project documentation wiki and contexts.
```bash
graphit knowledge <subcommand> [flags]
```
**Subcommands:**
- `index [path]`: Scan `knowledge.docs_dir` (default `docs/`) plus the root README, and compile the wiki index. A `path` argument overrides both and indexes that directory wholesale.
  - `--reset`: Clear knowledge graph first.
  - `--louvain`: Detect community structures.
  - `--workers <int>`: Thread count.
  - `--context <name>`: Re-index context.
- `watch [path]`: Watch the project and incrementally recompile from the same scope. A `path` argument watches and indexes that directory wholesale.
  - `--louvain`: Detect community structures.
- `query <text>`: Search the knowledge wiki.
  - `--context <name>`: Search context.
- `lint`: Audit wiki files for link defects.
  - `--fix`: Fix broken backlinks.
  - `--deep`: Enable AI contradiction audit.
  - `--stale-days <int>`: Age threshold.
  - `--context <name>`: Context name.
- `schema`: Print knowledge graph schema.
- `install <name>`: Fetch knowledge context.
- `remove`: Clear knowledge graph or context.
  - `--context <name>`: Context name.
- `sync`: Re-sync context.
- `export`: Export wiki DB.
- `list`: List contexts.
- `rule`: Manage knowledge rule.

### `memory`
Manipulates persistent agent memories.
```bash
graphit memory <subcommand> [flags]
```
**Subcommands:**
- `index`: Scan memories and regenerate wiki.
  - `--user`: Target user scope.
  - `--louvain`: Run clustering.
  - `--context <name>`: Re-index context.
- `watch`: Watch memory changes.
  - `--user`: Target user scope.
  - `--louvain`: Run clustering.
- `query <question>`: Query memories using AI.
  - `--user`: Target user scope.
  - `--context <name>`: Context name.
- `schema`: Show memory graph schema.
- `install <project-id-or-name>`: Fetch external memory context.
- `remove`: Remove memory graph.
  - `--context <name>`: Context name.
- `sync`: Sync cache.
- `export`: Export memory wiki database.
- `list`: List memories.
  - `--user`: List user scope.
- `insert <title>`: Add new memory entry.
  - `--content <body>`: Memory details.
  - `--user`: Save to user scope.
  - `--project`: Link user memory to active project.
  - `--important`: Surface in IDE rules.
  - `--mandatory`: Require unconditional session-start recall.
  - `--type <type>`: convention, correction, decision, tension, fact, or skill.
  - `--tags <list>`: Comma-separated tags.
  - `--context <name>`: Target context.
- `update <id>`: Modify existing memory.
  - `--content <body>`: New body.
  - `--title <title>`: New title.
  - `--user`: User scope.
- `delete <slug>`: Remove memory.
  - `--user`: User scope.
  - `--context <name>`: Context name.
- `search <term>`: Grep-like memory search.
  - `--user`: Target user scope.
- `important`: List important memories.
  - `--user`: User scope.
- `mandatory`: List every mandatory memory with complete content, without search.
  - `--user`: User scope.
- `promote <id>`: Mark memory as important.
  - `--user`: User scope.
- `demote <id>`: Remove important status.
  - `--user`: User scope.
- `mark-mandatory <id>`: Require unconditional recall for a memory.
  - `--user`: User scope.
- `unmark-mandatory <id>`: Stop unconditional recall when the requirement no longer applies.
  - `--user`: User scope.
- `consolidate`: Find and resolve duplicate, contradicting and stale memories.
  - `--user`: User scope.
  - `--dry-run`: Show the plan only, change nothing (default `true`).

The analysis runs on the agent CLI from `ai.cli`; every change is then applied in Go
under invariants the analysis cannot override — content is always carried into a
surviving memory before anything is removed, importance, mandatory status, and classification
survive a merge, an important or mandatory memory is never deleted outright, the last memory in a scope is
never deleted, and every refusal is reported with its reason. Without an AI CLI, only
the deterministic staleness check runs.

This is the same pass the [dream module](../specs/dream_module.md) performs on idle.
Run it here to have it now, or when the dream module is off.

> `gc` was removed. Collecting memories by age answers the wrong question: age says a
> memory has not been revised, not that it is wrong. Consolidation reasons about
> content instead, and carries it forward instead of deleting it.
  - `--stale-days <int>`: Expiry threshold.
- `rule`: Manage memory rule.

### `wiki`
AI multi-wiki explorer.
```bash
graphit wiki <subcommand> [flags]
```
**Subcommands:**
- `search <query>`: Search multiple wiki sources.
  - `--wiki <refs>`: Sources to search: `project`, `memory`, or project IDs.
  - `--hub <refs>`: Registry knowledge artifacts.
  - `--session <name>`: Session identifier.
  - `--continue`: Resume last session.
  - `--top-k <int>`: BM25 filter limit.
- `chat`: Multi-turn chat over wiki context.
  - `--session <id>`: Session ID.
  - `--continue`: Resume last session.
- `sessions`: List or delete wiki sessions.
  - `--delete <id>`: Delete session ID.

### `daemon`
Controls background service lifecycle.
```bash
graphit daemon <subcommand> [flags]
```
**Subcommands:**
- Foreground daemon start: `graphit daemon`
  - `--no-embedding`: Disable embedding server.
  - `--no-dream`: Disable dream runner.
  - `--log <path>`: Log file.
- `stop`: Terminate running daemon.
- `status`: Show daemon status and log tail.
- `restart`: Restart daemon.
- `scheduler <install|remove|status>`: Manage OS system launchers (cron, launchd, task scheduler).

### `dream`
Controls autonomous skill generation and knowledge mining.
```bash
graphit dream <subcommand> [flags]
```
**Subcommands:**
- `status`: Show dream state (active/idle/exhausted), report count, and timing configuration.
- `reports`: List dream session reports.
  - `--all`: Show all reports.

The default reports vault is `.graphit/runtime/dream/`, which is covered by the generated
`.gitignore`. Set `dream.reports_dir` to a versioned directory such as `docs/dream` when
reports are intended to be reviewed and committed. Existing `.graphit/dream/` reports are
not moved or deleted automatically.

### `task`
Manages deterministic project work in the shared LanceDB task store.
```bash
graphit task <subcommand> [flags]
```
**Subcommands:**
- `batch <file|->`: Run 1-100 ordered mutations from a JSON object with `operations` and optional default `lease`; `-` reads standard input. Every item reports success or an explicit error, and the command exits non-zero if any item fails.
- `create <title>`: Create an idempotent task with required description, acceptance criteria, and tests; `--parent` creates a subtask.
- `list` / `ready`: List tasks or only dependency-ready work; filter by status, owner, or parent.
- `get`, `search`: Retrieve authoritative history or search task/comment text.
- `claim`, `heartbeat`, `release`: Own or hand off work with a fenced lease.
- `progress`, `comment`, `check`: Record checkpoints, typed context, and acceptance/test evidence.
- `flag`, `unflag`: Add or resolve a completion gate with a reason.
- `dependency add|remove`: Maintain explicit blocking edges.
- `complete`: Finish only after every check and subtask passes and no flag remains.
- `cancel`: Preserve an obsolete task as an audited terminal record with a required reason.
- `remove` / `rm`: Hard-delete certainly erroneous work with `--confirm <exact-id>` and `--reason`; referenced tasks are refused.

Claims default to one hour. Renewing through heartbeat, progress, checks, comments, or lifecycle
hooks never shortens a longer active lease.

Open, unclaimed tasks are the backlog; no Markdown task files are created. On a direction change,
cancel or remove obsolete work immediately instead of leaving task garbage. See
[Task Module](../specs/task_module.md).

### `cluster`
Manages project grouping.
```bash
graphit cluster [key] [value] [flags]
```
Without subcommands:
- `graphit cluster <key> <value>`: Set cluster label.
- `--get <key>`: Read label.
- `--list`: List all labels.
- `--unset <key>`: Remove label.

**Subcommands:**
- `projects`: List sibling projects.
