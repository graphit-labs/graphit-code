# User Manual

Graphit is the context and control plane around coding agents. It keeps structural code, maintained
knowledge, persistent memory, deterministic work state, and reusable ecosystem artifacts distinct,
then exposes them through CLI commands, MCP tools, native agent hooks, and the Graphit Observatory.

## Mental model

| Module | Source of truth | Best question |
|---|---|---|
| AST | Indexed project source | “What calls this?”, “What imports that?”, “What would this change affect?” |
| Knowledge | Maintained project documentation | “How is this feature intended to work?”, “What decision explains it?” |
| Memory | Structured project or user records | “What did we learn or correct before?” |
| Task | Authoritative project work tables | “Who owns this?”, “What blocks it?”, “What evidence makes it complete?” |
| Hub | Published reusable artifacts | “Does the ecosystem already provide this rule, skill, language, AST, or documentation?” |
| Live Search | A temporary agent workspace assembled for a question | “What answer needs evidence from several selected artifacts?” |

Use the narrowest source that can answer the question. Full-text or semantic search finds
candidates; a graph traversal settles structural relationships; a source or wiki read supplies the
actual content; Task controls project mutation and completion.

## Teams, agents, and software ecosystems

Graphit separates identity and sharing so one workflow can scale without merging unrelated state:

| Boundary | Graphit behavior |
|---|---|
| Several agents in one project | One Task queue, atomic ownership, fenced mutations, explicit dependencies, and resumable checkpoints. |
| One person across projects | User memory follows `unit.id`; project memory remains isolated by repository identity. |
| Several repositories on one machine | The ecosystem registry resolves each sibling's own AST and wiki instead of copying its checkout. |
| Several machines or teammates | Optional S3-compatible storage hosts versioned Hub artifacts and authoritative shared Memory/Task tables. |
| Several coding assistants | Each adapter writes its native project-local MCP and lifecycle-hook format; the MCP server remains the shared capability surface. |
| Several external systems | Hub contexts are addressed by artifact ID and version, then queried in place through the same AST or Wiki contracts. |

This is deterministic coordination around nondeterministic models. Graphit never promises that two
models will produce the same patch; it guarantees that claims, revisions, dependencies, evidence,
and completion transitions follow the same rules.

## Project lifecycle

### Register and configure

Run from the repository root:

```bash
graphit init --ide codex
```

Initialization registers the project, records the selected adapter, and installs managed instructions and MCP configuration. Re-run it when changing adapters or refreshing generated integration files.

### Build a verified snapshot

```bash
graphit sync
```

Sync coordinates the active project indexes and shared registry state. Use it for the first build, after bulk changes that occurred while the daemon was unavailable, and before a decision that requires a verified current snapshot.

### Keep indexes current

```bash
graphit daemon
```

The daemon watches registered projects and schedules incremental AST and wiki work. It also owns
shared local or remote embeddings, memory maintenance, the authenticated MCP HTTP endpoint, and
optional Dream and Observatory services. It starts automatically before ordinary CLI/MCP work
unless `modules.daemon=false`; an OS scheduler can keep it alive independently. See
[Daemon Operations and Monitoring](daemon_operations.md) for every start path, watched signal,
module loop, runtime file, and recovery behavior.

## AST workflow

To add a language, override extraction, select a dialect, or understand every grammar
YAML field, use [AST Grammars and Parser Extensibility](ast_extensibility.md).

AST exploration has three stages:

1. inspect the current schema before writing Cypher;
2. search for candidate names when the exact entity is unknown;
3. query the graph for the relationship or aggregate that answers the question.

Example:

```cypher
MATCH (caller)-[:CALLS]->(target:Function {name: 'RunSync'})
RETURN caller.name, caller.path
```

Use source retrieval after the graph identifies the relevant file or entity. Do not infer callers, imports, inheritance, or impact from text matches alone.

AST search supports three retrieval modes:

- `fts` for precise identifiers and terms;
- `semantic` for meaning rather than spelling;
- `hybrid` (default) for BM25 and vector candidates fused with reciprocal rank fusion.

The graph is stored as Icebug files and attached to an in-memory LadybugDB catalog when queried.
That on-the-fly catalog makes local and published contexts portable without a separate graph-server
deployment.

The AST Explorer presents the same process visually: schema and type controls on the rail, Cypher and AI-assisted query modes above, and the graph canvas in the main workspace. Relationship names are friendly names resolved from the active store's `graph.icebug/icebug.json` manifest; physical edge-table names are storage details.

![Graphit AST Explorer using graphit-code as the active project](../site/assets/observatory-ast-explorer.jpg)

## Knowledge workflow

Graphit compiles `docs/` and the root `README.md` into a source-backed wiki.

1. Search by topic to receive ranked page titles.
2. Choose the relevant page.
3. Read the page content and frontmatter.
4. Follow cross-references when the answer spans documents.
5. Check provenance when exact source wording or freshness matters.

The Knowledge Explorer exposes the same page index, keyword and AI-assisted search modes, confidence, provenance, outbound links, and update history.

Direct keyword and semantic retrieval do not require a coding-agent CLI. AI synthesis does and is
controlled by `modules.agent`. Search returns candidate titles; `wiki_source` is the evidence read.

![Graphit Knowledge Explorer showing the project architecture](../site/assets/observatory-knowledge-explorer.jpg)

## Memory workflow

Memory stores context the code and current documentation cannot reconstruct.

- **Project memory**: repository-specific conventions, corrections, decisions, facts, tensions, and reusable debugging skills.
- **User memory**: preferences and techniques that should follow one person across projects.

Good memories state:

- what happened;
- why it matters;
- how the team should act on it;
- what it changes or constrains.

Mandatory memories are loaded without a query at session start. Contextual search excludes those
already-loaded records and returns candidate titles; read the selected memory before acting on it.
Importance and mandatory recall are independent flags. When a correction supersedes an existing
memory, update it in place so the revision chain remains searchable instead of leaving contradictory
records.

![Graphit Memory Explorer showing a persistent project decision](../site/assets/observatory-memory-explorer.jpg)

## Hub and ecosystem workflow

The ecosystem registry identifies local sibling projects. Each registered project retains its own AST, knowledge, and project memory; cross-project work should query the sibling's own context rather than copying its repository into the current one.

The Hub distributes reusable artifacts:

- rules;
- skills;
- agents;
- commands;
- MCP servers;
- powers;
- languages;
- AST bundles;
- knowledge bundles.

Optional S3-compatible storage holds published catalog and artifact data. Installing or publishing is an explicit action. Read the relevant artifact metadata and understand its scope before applying it to a project.

## Live Search

Live Search assembles selected artifacts into an ephemeral workspace and runs a supported coding agent against a concrete question.

Use it when the answer genuinely spans several sources. Do not use it in place of a direct AST traversal, a single wiki read, or a known CLI action.

The temporary workspace is not registered as a normal project. Selected artifacts and the prompt
define its scope; deleting the session removes its ephemeral project data. Live Search requires an
installed coding-agent CLI and is unavailable when `modules.agent=false`.

## Dream and Task

Dream runs during configured idle periods to analyze conversation history and improve project knowledge or reusable agent artifacts. It is a knowledge-improvement process, not a task scheduler.

Task is separate. Open, unclaimed LanceDB tasks are backlog. Dependencies decide readiness; an
atomic claim returns a fencing token that every owner mutation must present. Agents checkpoint the
exact next step, attach typed decisions/problems/lessons, and record pass/fail evidence against
structured acceptance and test checks. Releases and expired leases preserve enough state for safe
takeover.

Completion fails closed when any active check is pending or failed, a flag remains, or a nested
subtask is incomplete. Scope changes require the current task revision and preserve immutable
before/after history; obsolete checks are superseded rather than rewritten. When direction changes,
agents cancel work whose history remains useful or explicitly remove certainly erroneous,
unreferenced work. Dream never consumes the Task backlog.

Open **Task → _project name_** in the Observatory to inspect this state without creating a second
task model. The catalogue searches IDs, titles, specifications, ownership, progress, and next steps,
and filters both stored lifecycle states and the derived blocked/flagged views. It requests bounded
pages and loads more only on demand, so routine browsing does not transfer every task's audit data.
Selecting a task loads its authoritative complete record: specification, claim metadata, progress, checks and
evidence, dependencies, subtasks, comments, events, and specification revisions. The project and
task download actions save the same versioned JSON returned by the CLI and MCP surfaces.

Use the complete export when another tool needs a portable machine-readable snapshot:

```bash
# Every task in the current project.
graphit task export > tasks.json

# One exact task plus all of its recursive subtasks.
graphit task export tsk-abcd > tsk-abcd.json
```

The MCP equivalent is `graphit_task_export`: omit `id` for the project document or pass one exact
task ID. Both surfaces return arrays for task snapshots, dependency and check projection records,
lifecycle events, comments, and specification revisions in deterministic order. Claim fencing
tokens and internal scheduler-control rows remain private and are never part of the export.

## Retrieval and reranking

Graphit's retrieval surfaces are deliberately different:

| Need | Use |
|---|---|
| Exact term or identifier | BM25/FTS search |
| Similar concept with different wording | semantic vector search |
| Strong general recall | hybrid BM25 + vector search with RRF |
| Proven code relationship | AST Cypher query |
| Exact content | AST source or Wiki source slicing |
| Several selected contexts plus synthesis | Live Search or an AI wiki query |

Graphit also implements a bounded optional second-stage reranker with local, Cohere, Voyage AI, and
Jina backends. It widens retrieval before reordering and uses deterministic tie-breaking. Current
public CLI, MCP, and UI searches do not yet attach that stage, so `search.rerank` is
integration-ready rather than active on those entry points. See
[Retrieval Architecture](retrieval_architecture.md) and the [AI Engine](../specs/ai_engine.md).
For model selection, CLI fallback and invocation protocols, credentials, dimensions, and local or
remote data boundaries, use [AI Models, Providers, and Agent CLIs](ai_models.md).

## Observatory navigation

The Graphit Observatory groups routes by intent:

- **Live Search** — multi-source agent runs;
- **Hub** — registry, project artifacts, and upload;
- **Knowledge** — project and imported documentation contexts;
- **AST** — project and imported code contexts;
- **Memory** — project and user scopes;
- **Task** — deterministic project backlog and complete lifecycle records;
- **System** — daemon, Dream, and ecosystem state.

Always verify the active workspace before interpreting explorer data. The selected workspace is persisted between browser sessions.

Light and dark modes share the same semantic hierarchy. On mobile, the global navigation becomes a drawer and explorer rails can be collapsed.

## Local and shared data

Compiled graphs, wikis, memory stores, search indexes, models, and caches live once in the global Graphit directory, keyed by identity. A project checkout keeps source, documentation, its lockfile, and small project records—not duplicate compiled stores.

See [Storage Layout](../architecture/storage_layout.md) for the exact structure and [S3 Credentials and UI Network Configuration](s3-and-ui-network.md) for shared storage.

## Network and security

The UI is designed for local operation and has no built-in authentication. Its bind host and exact-origin CORS policy are configurable, but CORS does not protect non-browser clients. The daemon's MCP HTTP endpoint is a separate listener and requires its active bearer key. Open **System → Daemon** to see the endpoint and copy the full key from the masked **MCP bearer key** control. The default key is regenerated on every daemon start; set the secret global `mcp.api_key` or service environment variable `GRAPHIT_MCP_API_KEY` for a stable value, then restart the daemon.

Before remote access:

- bind only to an intended interface;
- restrict traffic with a firewall or VPN;
- place the service behind an authenticated TLS proxy when appropriate;
- keep credentials out of repository configuration.

## Operational checklist

When something looks wrong:

1. confirm the active project and working directory;
2. confirm the daemon is running;
3. distinguish a transient store lock during reindex from a missing index;
4. run an explicit sync after bulk external changes;
5. check the relevant module guide or specification before deleting or rebuilding data.

Continue with [Troubleshooting](troubleshooting.md), the [CLI Reference](cli_reference.md), or the [MCP Tools Reference](mcp_tools_reference.md).

For every default, module switch, provider, and environment override, use the
[Configuration Reference](configuration.md).
For repository files, generated state, agent adapter layouts, and filesystem change detection, see
[Filesystem, State, and Watchers](filesystem_contract.md).
