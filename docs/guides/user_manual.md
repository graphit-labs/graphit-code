# User Manual

Graphit is a context system for coding agents. It keeps four signals distinct—AST, knowledge, memory, and Hub artifacts—then makes them available through CLI commands, MCP tools, and the Graphite Observatory.

## Mental model

| Module | Source of truth | Best question |
|---|---|---|
| AST | Indexed project source | “What calls this?”, “What imports that?”, “What would this change affect?” |
| Knowledge | Maintained project documentation | “How is this feature intended to work?”, “What decision explains it?” |
| Memory | Structured project or user records | “What did we learn or correct before?” |
| Hub | Published reusable artifacts | “Does the ecosystem already provide this rule, skill, language, AST, or documentation?” |
| Live Search | A temporary agent workspace assembled for a question | “What answer needs evidence from several selected artifacts?” |

Use the narrowest source that can answer the question. A text search finds candidates; a graph traversal settles structural relationships; a source or wiki read supplies the actual content.

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

The daemon watches registered projects and schedules incremental AST and wiki work. It also supports background services such as shared local embeddings and Dream sessions.

## AST workflow

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

When a correction supersedes an existing memory, update it in place instead of leaving contradictory records. Search results are titles; read the selected memory before acting on it.

![Graphit Memory Explorer showing the Graphite Observatory decision](../site/assets/observatory-memory-explorer.jpg)

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

The temporary workspace owns no project data store after the session ends. Selected artifacts and the prompt define its scope.

## Dream and Task

Dream runs during configured idle periods to analyze conversation history and improve project knowledge or reusable agent artifacts. It is a knowledge-improvement process, not a task scheduler.

Task is separate. Open, unclaimed LanceDB tasks are backlog; agents claim, checkpoint, comment,
verify checks, hand off, and complete them through Graphit Task rather than host-native TODO tools.
When direction changes, agents cancel tasks whose history remains useful or explicitly remove
certainly erroneous, unreferenced tasks; superseded open/flagged garbage is forbidden. Dream does
not consume or execute tasks.

## Observatory navigation

The Graphite Observatory groups routes by intent:

- **Live Search** — multi-source agent runs;
- **Hub** — registry, project artifacts, and upload;
- **Knowledge** — project and imported documentation contexts;
- **AST** — project and imported code contexts;
- **Memory** — project and user scopes;
- **System** — daemon, Dream, and ecosystem state.

Always verify the active workspace before interpreting explorer data. The selected workspace is persisted between browser sessions.

Light and dark modes share the same semantic hierarchy. On mobile, the global navigation becomes a drawer and explorer rails can be collapsed.

## Local and shared data

Compiled graphs, wikis, memory stores, search indexes, models, and caches live once in the global Graphit directory, keyed by identity. A project checkout keeps source, documentation, its lockfile, and small project records—not duplicate compiled stores.

See [Storage Layout](../architecture/storage_layout.md) for the exact structure and [S3 Credentials and UI Network Configuration](s3-and-ui-network.md) for shared storage.

## Network and security

The UI is designed for local operation and has no built-in authentication. Its bind host and exact-origin CORS policy are configurable, but CORS does not protect non-browser clients.

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
