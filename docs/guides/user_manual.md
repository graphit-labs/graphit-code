# User manual

Graphit connects code, documentation, durable Memory, shared Tasks and reusable agent artifacts.
Use this manual after [getting started](getting_started.md). Choose your goal below rather than
learning internal stores first. [Capabilities](capability_matrix.md) distinguishes CLI, MCP and
Browser; [configuration](configuration.md) explains exact defaults and opt-ins.

## Establish the project context

Open the Browser with `graphit ui` (or print its URL with `graphit ui --no-open`). Sign in through
the configured Broker, then select Project, version and Agent in the shared header. Project shows
the authorized name; IDs remain the durable identity and disambiguate equal names. A Hub target
is a remote artifact/project, not a fabricated local checkout. Loading/absent/denied selections
are not permission to silently choose a different project.

The Browser reads the same domain services as other clients. A selected URL/ID is not access.
A valid JWT does not grant PROJECT permission. Local CLI and Browser identities may differ; check
the intended profile and provider before interpreting a result.

## Inspect code

1. Use AST search to find candidate names/paths. Choose FTS for literal terms or semantic/hybrid
   when the configured embedding service is appropriate.
2. Read the selected Source or entity; do not infer callers from similar text.
3. For relationships, inspect the schema, anchor an exact entity and query typed edges with
   distinct reached results. The relationship map is a separate bounded visualization.
4. Keep source identity/fingerprint and current project/version in the result. A concurrent edit
   may require a new read rather than reuse of stale coordinates.

```sh
graphit ast search 'checkout' --mode fts --rerank=false
graphit ast source Cargo.toml --head 20 --line-numbers
graphit ast schema
graphit ast query --help
```

AST uses SCIP by default, specialized Docker indexers and generic Tree-sitter/ANTLR fallback.
Custom images map file extensions through CSV configuration. [Parser extensibility](ast_extensibility.md)
explains how to build an image and produce valid SCIP. Known Source paths can fall back to the
authorized selected checkout even for unsupported extensions; no Git command is required.

## Read and maintain Knowledge

Maintain source documents under the configured documentation root (`docs` by default); README
inclusion is on by default. `.wikiignore`, extensions and profile/format choices define selection.
The daemon compiles changed files progressively. Source metadata, references and text become
queryable before all embeddings finish. Indexing is not a generated Markdown rewrite of your
source files.

```sh
graphit knowledge search 'configuration' --mode fts --rerank=false
graphit knowledge source '<returned-slug>' --head 40
graphit knowledge log --help
```

Use an exact slug from search. Source supports line/pattern slices; the slug is not a filesystem
path. Explicit typed frontmatter references create cross-module relationships, while normal wiki
links keep their own document-link meaning. Keep broken links/invalid metadata visible and fix
the authored source instead of editing the generated store.

Ask is optional agent synthesis over selected pages or retrieval:

```sh
graphit knowledge ask --query 'How is configuration resolved?' \
  --retrieval fts --top-k 5 --model '<model-supported-by-your-agent>'
```

The completion CLI owns its model account and must be available for actual use. Retrieval/model
or native-session errors are explicit; Graphit does not silently switch CLIs. Conversation/Live
have their own state and cancellation rules. A successful synthetic test does not imply a vendor
account is configured. [AI models](ai_models.md) describes those independent services.

## Preserve durable Memory

Use Memory for confirmed reusable facts, corrections, conventions, decisions and procedures.
PROJECT is shared and versioned; USER is private to the verified identity/installation and is
independent of project Git branches. Keep Task progress and temporary hypotheses in Task, not as
mandatory Memory. Never place secrets or personal details in shared records.

The Memory Explorer reads current records and explicit revision history and provides admitted
CRUD actions with CAS/revision checks. A stale edit conflicts instead of overwriting another
writer. Mandatory and important are independent flags; mandatory content loads at the configured
agent bootstrap boundary. Search supports FTS, semantic and hybrid with partial compatible vectors.
The current Root MCP catalog exposes search/index; do not guess old CRUD tool names.

```sh
graphit memory index --scope project
graphit memory index --scope user
```

`--refresh` is an explicit lexical-index replacement, not required for every read. Background
updates reuse compatible embeddings and persist progressively in the same corpus.
[Memory contract](../specs/memory_module.md) specifies scope, revisions and storage.

## Dream and Task

Task coordinates shared work deterministically. Session preserves the evolving demand; Tasks
have specifications, hierarchy/dependencies, dates, checks and current claims. Read current state
before claiming and keep private returned tokens out of shared content. Gantt uses explicit dates;
progress excludes cancelled tasks. Completed/cancelled descendants are terminal but the parent
must still satisfy its own required work.

```sh
graphit task session-heads
graphit task schema
graphit task search --text parser --mode fts --page-size 10 --top-k 10
```

Exact reads initially load current/creation/last-event, not every historical event or comment.
Use explicit pinned pages through EOF for full evidence/export. The Browser uses the same audit
contracts; there is no `task export`/`graphit_task_export` command or generic force-takeover tool.
After an authorized commit/amend, update included Tasks with the full actual SHA via
`task set-task-commit`. [Session guide](task-sessions.md) gives exact mutation envelopes.

Dream is an explicit background Memory workflow, not a source of automatic task completion.
Enable both its module and `dream.enabled`, choose `dream.program` and `dream.model`, and use its
scoped permissions. It does not authorize arbitrary project writes. [Dream](../specs/dream_module.md)
contains prerequisites and lifecycle; leave it off when not wanted.

## Versions and Git

Project stores are keyed by stable ID/corpus. Versions are native Lance branches sharing
fragments; worktrees do not create full private copies merely because their paths differ.
Product Git hooks snapshot PROJECT Task/Memory for observed commits, restore on checkout and
merge independent edits with explicit conflicts. They never include USER Memory. A failed
post-commit hook cannot undo the Git commit: inspect the error and retry supported recovery.
Projects without Git continue normal version operation. [Git records](../specs/git_records.md)
defines snapshots, tags, rewrites and pre-install historical limits.

## Teams, agents and software ecosystems

The Hub separates project discovery, authored resource review, artifact publication and consumer
installation. Capture authored Source through the authorized PROJECT/version, review its exact
kind/ID/artifact version, then publish/install using the provider's grants. A local source link
and a remote versioned artifact have different provenance.

`hub install` installs an exact artifact; `hub install-local` uses an explicit authorized local
source. Native AST/Knowledge imports can be queried on-the-fly without a repository clone. Local
Icebug reads Parquet through in-memory Ladybug; remote Icebug reads authorized S3 ranges through
the same path. An artifact version is distinct from a project version and from SCIP image
compatibility aliases.

```sh
graphit hub authored list --project-dir . --limit 20
graphit hub list --help
graphit hub install --help
graphit hub update --help
```

General sync reapplies exact recorded versions and Graphit's installed agent files:

```sh
graphit sync
graphit skills sync
graphit roles sync
graphit hooks sync --binding codex
graphit coordinator sync --binding codex
graphit hub sync
```

It does not upgrade artifacts, clone repositories, hydrate by Git ancestry or wait for all
indexing. `hub update` explicitly chooses newer remote releases. User modifications remain
protected by origin/version/hash ownership; a conflict needs reconciliation, not deletion.
[Hub](../specs/hub_collaboration.md) and [filesystem](filesystem_contract.md) explain lifecycle.

## Choose inference deliberately

Embedding is enabled by the module default but still needs a usable service/model. To disable its
background work use `modules.embedding=false`; CRUD/FTS remains usable. Local embedding uses
EmbeddingGemma2 GGUF BF16. Local rerank uses BGE V2 M3 GGUF F16 and is off by default
(`search.rerank=false`). Installing a reranker does not enable it.

CPU is default for each local purpose. Choose CPU, explicit accelerator or auto independently on
the provider; explicit unavailable GPU fails, auto reports selection/fallback. Local models load
on demand and unload after the configured idle timeout (300 seconds default). Remote modes need
no local model; with an active Broker provider both services are Broker-only or disabled.
[AI configuration](ai_models.md) contains exact examples and artifact publication limitations.

## Operate and troubleshoot

```sh
graphit daemon status
graphit daemon install --help
graphit daemon login --help
graphit ui --no-open
```

Service login startup is explicit. Linux tray requires the desktop bus and StatusNotifier host;
headless services do not. The Browser requires its trusted Broker configuration/login, not a
`ui.auth.enabled` switch. Port zero selects a real ephemeral listener, so use the returned URL
rather than bookmarking one old port. Restarting/replacing a daemon can invalidate an unfinished
login challenge; start a fresh login from Graphit.

Errors can preserve confirmed partial effects; check current records before retry. Do not delete
stores/credentials, remove live lock files or switch silently to local storage to hide a failure.
[Daemon operations](daemon_operations.md), [troubleshooting](troubleshooting.md) and
[storage layout](../architecture/storage_layout.md) are the next steps.
