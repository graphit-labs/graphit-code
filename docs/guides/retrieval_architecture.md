# Retrieval Architecture

> Complete guide to the wiki, knowledge, and memory retrieval system.

This document explains how data flows through the retrieval pipeline, which tools to use in each scenario, and how scope parameters control what gets searched.

---

## Contextual recall throughout the work

Session hooks load standing memory; they do not limit later retrieval. During questions,
exploration, coding, debugging, documentation or review, a new uncertainty about the system
is a trigger to consult the relevant source before guessing or repeating an investigation.
Memory supplies durable facts, decisions and lessons; Task supplies specifications, prior
investigations, attempts, decisions and validation results. No new session, plan, scope change
or blocker is required. A known ID goes straight to memory_source/task_get; otherwise use a
focused search and read selected results. Reuse sufficient retained evidence and query only
the missing topic, rather than calling both modules mechanically.

For example, an unexpected retry guard found midway through implementation can prompt Memory
recall of the provider constraint and Task retrieval of the experiment that established it.
Compare their recorded scope/revision with current AST code and Knowledge contracts before
claiming that the historical explanation still applies. A second resolved question needs no
new call; a different question in the same session may need one.

## 1. Three-Tier Architecture

The retrieval system is organized into three tiers of increasing sophistication:

### Tier 1: Direct memory retrieval

**Tools:** `graphit_memory_mandatory`, `graphit_memory_search`

Indexed matching directly over each scope's authoritative memory table. Matches use the canonical
Memory order—mandatory, important, normal—then newest first inside each group. This tier is:

- **Lightweight** — indexed table access without source-file scanning
- **No AI** — engine-matched text retrieval with deterministic domain ordering
- **Fast** — no source-file scan
- **Best for** — standing session context plus focused lookup whenever a question needs it

```
~/.graphit/memory/                   authoritative tables in local-only mode
├── memory-project-<id>/
└── memory-user-<hash>/
```

### Tier 2: Compiled Wiki (BM25 / Semantic, LanceDB)

**Tools:** `graphit_knowledge_search`, `graphit_wiki_search`, `graphit_wiki_browse`, `graphit_task_search`

Operates on `index.lance/` stores locally or at a mounted Hub `s3://` URI:

| Tool | Backend | What it searches |
|------|---------|-----------------|
| `knowledge_search` | LanceDB BM25 | local project `index.lance/` or a mounted Hub artifact |
| `wiki_search` | BM25 + semantic on `index.lance/` | Knowledge wikis only |
| `wiki_browse` | LanceDB catalog on `index.lance/` | Knowledge wikis only |
| `task_search` | LanceDB BM25 | Authoritative project task specs/check evidence and typed comment bodies |

Knowledge and Task complement each other. Reuse Task specifications and prior findings already
retrieved for the current question. When implementation history or decisions are still needed, use a
focused `graphit_task_search` and read selected hits with `graphit_task_get`; search metadata is not
evidence. Follow `next_cursor` only while a concrete gap remains. Task outcomes and check evidence
supplement authoritative pages read with `graphit_wiki_source`. If Task is disabled or unavailable,
continue Knowledge retrieval without inventing project task state.

```
~/.graphit/wiki/knowledge/project/<project-id>/
└── index.lance/                  ← complete local knowledge wiki
```

> There is no wiki inside a project. Every one of them lives once, in the global brand
> directory, keyed by an id — see [Storage Layout](../architecture/storage_layout.md).
> Use the known or cluster-resolved absolute `project_dir` to reach a local project's wiki.
> Without a local checkout, a resolved installed artifact can instead be addressed by qualified
> `id@version` with `project_dir` omitted. Neither wiki is readable with a file tool.

#### Hybrid ranking and semantic confidence

AST hybrid search delegates fusion and ranking to the search engine. There is no Go-side
semantic-channel weight to tune. Measurements with uniform semantic weights `0.8`, `1.2`,
`1.5`, and `2.0` produced the same ordering: when two documents were plausible, the semantic
channel returned both at adjacent ranks, so uniformly scaling that channel did not reorder
them. Lowering a local weight would therefore add a knob without changing the measured result.

The live Go-side control is the semantic confidence floor. Neighbours below cosine `0.20` do
not vote, because short or weak queries otherwise receive arbitrary nearest neighbours that
can drown exact lexical matches. This threshold is a relevance gate, not a fusion weight.

### Tier 3: AI Synthesis

**Surfaces:** `graphit knowledge ask`, `graphit memory ask`, Observatory AI search, and
`graphit live`

Uses a locally installed coding-agent CLI to synthesize answers from retrieved wiki pages or a
temporary multi-artifact workspace. These are CLI/UI workflows, not stdio MCP tools. They require
`modules.agent=true` and an authenticated agent CLI on `PATH`.

- **AI-powered** — LLM reads retrieved pages and synthesizes a coherent answer
- **Synthesized** — the agent reads retrieved evidence and produces one coherent answer
- **Higher latency** — involves an LLM inference call
- **Best for** — complex questions that span multiple documents

---

## 2. Tool Differentiation Matrix

| Tool | Module | Searches | Backend | AI? | Scope Params |
|------|--------|----------|---------|-----|-------------|
| `graphit_knowledge_search` | knowledge | project or Hub knowledge wiki | LanceDB BM25 | No | `context` (empty = project, named = installed Hub artifact) |
| `graphit_wiki_search` | wiki | multiple Knowledge wikis | LanceDB BM25/semantic | Semantic mode only | `wikis[]`, `hub_refs[]` |
| `graphit_wiki_browse` | wiki | Knowledge catalog | LanceDB | No | `context` |
| `graphit_wiki_xrefs` | wiki | Knowledge-wiki cross-refs | LanceDB `index.lance/` | No | `context` |
| `graphit_wiki_log` | wiki | Knowledge-wiki sync history | LanceDB `index.lance/` | No | `context` |
| `graphit_memory_search` | memory | authoritative memory table | LanceDB FTS/vector | No | `scope` (project or user) |
| `graphit_memory_source` | memory | current or historical row | LanceDB key lookup | No | `scope`, `path` |
| `graphit_memory_mandatory` | memory | authoritative live memory table | LanceDB filter, no ranking | No | `scope` (project or user) |
| `graphit_task_search` | task | current/prior task specs and comments | LanceDB BM25 | No | project identity |
| `graphit_task_query` | task | any Task table | LanceDB predicate + projection | No | `table`, `filter`, `columns` |
| `graphit_memory_query` | memory | authoritative memory table | LanceDB predicate + projection | No | `scope`, `filter`, `columns` |
| `graphit_knowledge_query` | knowledge | index tables, including `xrefs` | LanceDB predicate + projection | No | `table`, `filter`, `columns` |
| `graphit_ast_fts_query` | ast | `entities` and `files` | LanceDB predicate + projection | No | `table`, `filter`, `columns` |
| `graphit knowledge ask` | CLI | project or imported knowledge wiki | agent CLI + retrieved pages | Yes | `--context` |
| `graphit memory ask` | CLI | project/user/imported authoritative memory table | agent CLI + retrieved records | Yes | `--user`, `--context` |
| `graphit live` | CLI/UI | selected Hub artifacts in an ephemeral workspace | coding-agent session | Yes | artifact IDs and versions |

> [!NOTE]
> Pass `ai_optimized: true` where supported for compact structured results; source tools return
> selected text directly. Search results identify sources to read, not synthesized evidence.

The four `*_query` surfaces answer a different shape of question from the searches above them.
A search ranks by relevance when the target is unknown; a query filters a table by predicate and
projects a few columns when the records are already nameable — the status of a set of ids, which
pages are stale, what links to a slug. They are **not SQL**: the filter is a WHERE clause, and the
engine offers no SELECT, JOIN, GROUP BY or ORDER BY. Rows come back in storage order, which is not
guaranteed stable across compaction, so filter on a key range when order matters. Each `*_query`
has a `*_schema` companion that lists the tables and columns to filter on.

A `*_schema` answer names the store it read (`store`) and flags the columns that behave specially.
A `redacted` column is refused in both the projection and the filter, so a credential cannot be
read back or guessed one predicate at a time; a `heavy` column stays out of the default projection
and is returned when named, so a listing does not pay for prose; a `vector` column is never read
at all and comes back as `<vector dim=N; values not returned>`, which keeps an embedding off the
wire rather than merely out of the answer. `remote: true` marks a store reached over S3 instead of
the local filesystem, and **only Task and Memory can report it** — `internal/task/paths.go` and
`internal/memory/paths.go` are the only two places that build an `s3store.URI`. Knowledge and AST
index locally under the global directory even when the account profile configures S3, and reach
the Hub as published artifacts rather than as live tables, so their schema answers never carry the
field. Reading it as "this module has no remote store" is correct; reading it as "S3 is not
configured" is not.

Memory list and search surfaces always order logical memories by category first—`mandatory`,
`important`, `normal`—and by descending `updated_at` inside each category. Search scores identify
match strength but do not change that presentation order; limits and pagination are applied after
the canonical order.

---

## 3. Scope & Context Parameters

### Memory: `scope` parameter

Controls which authoritative memory scope is searched.

| Value | Description | Storage Path |
|-------|-------------|-------------|
| `"project"` (default) | Project-specific memories | local or S3 `v2/projects/<project-ulid>/memory/` table |
| `"user"` | Personal cross-project memories | local or S3 `v2/users/<trusted-user-id>/memory/` table |

```jsonc
// Fallback only when hooks have not already supplied these scopes
graphit_memory_mandatory(scope: "project", ai_optimized: true)
graphit_memory_mandatory(scope: "user", ai_optimized: true)

// Search project memories (default)
graphit_memory_search(query: "auth flow", scope: "project", exclude_mandatory: true, top_k: 5, ai_optimized: true)

// Search personal memories across all projects
graphit_memory_search(query: "preferred patterns", scope: "user", exclude_mandatory: true, top_k: 5, ai_optimized: true)
```

### Knowledge: `context` parameter

Controls which knowledge wiki is searched. An empty context targets the local project wiki compiled from `docs/`.
For a known local checkout, pass its absolute `project_dir`; an installed artifact can be selected by its resolved
project alias. Only without a local checkout, omit `project_dir` and use a resolved globally installed qualified
`id@version` as `context`. Do not guess project paths, aliases or artifact IDs.

| Value | Description | Storage Path |
|-------|-------------|-------------|
| `""` (default) | Local project wiki from `docs/` | `~/.graphit/wiki/knowledge/project/<project-id>/` |
| `"<resolved-alias>"` with `project_dir` | Project-installed Hub knowledge artifact | versioned `s3://` LanceDB mount |
| `"<qualified-id>@<version>"` without `project_dir` | Globally installed artifact, no local checkout | versioned `s3://` LanceDB mount |

```jsonc
// Search local project knowledge
graphit_knowledge_search(project_dir: "<known-absolute-path>", query: "deployment config", top_k: 5, ai_optimized: true)

// Search an artifact already installed for this project
graphit_knowledge_search(project_dir: "<known-absolute-path>", query: "middleware setup", context: "<resolved-alias>", top_k: 5, ai_optimized: true)
```

### Wiki: `wikis` and `hub_refs` parameters

The wiki module searches Knowledge sources only and supports simultaneous multi-wiki queries.

**`wikis[]` — for `wiki_search` (multi-scope)**

| Value | Description | Index Location |
|-------|-------------|-----------------|
| `["project"]` | Search the knowledge wiki | `~/.graphit/wiki/knowledge/project/<project-id>/index.lance/` |

**`hub_refs[]` — for `wiki_search` (hub artifacts)**

| Value | Description |
|-------|-------------|
| `["artifact-id@version"]` | Include a hub knowledge artifact in the search |

```jsonc
// Semantic search across project docs
graphit_wiki_search(project_dir: "<known-absolute-path>", query: "authentication flow", wikis: ["project"], mode: "semantic", top_k: 5, ai_optimized: true)

// Include a resolved installed Hub artifact
graphit_wiki_search(project_dir: "<known-absolute-path>", query: "API reference", wikis: ["project"], hub_refs: ["<qualified-id>@<version>"], top_k: 5, ai_optimized: true)

// Get cross-references for project wiki
graphit_wiki_xrefs(project_dir: "<known-absolute-path>", query: "authentication-flow", ai_optimized: true)
```

---

## 4. Store resolution

Every tool resolves to a local directory or a version-addressed, consumer-read-only object-store URI
based on its scope:

| Tool | Scope | Resolves To |
|------|-------|------------|
| `knowledge_search` | no context | `~/.graphit/wiki/knowledge/project/<project-id>/` |
| `knowledge_search` | `context: "X"` | versioned Hub `s3://…/index.lance` |
| `wiki_search` | `wikis: ["project"]` | `~/.graphit/wiki/knowledge/project/<project-id>/index.lance/` |
| `wiki_browse` | project/context | `~/.graphit/wiki/knowledge/project/<project-id>/index.lance/` or a mounted Knowledge artifact |
| `wiki_log` | project/context | the selected Knowledge `index.lance/` |
| `wiki_xrefs` | project/context | the selected Knowledge `index.lance/` |
| `memory_search` | `scope: "project"` | local table or `s3://.../v2/projects/<id>/memory/` |
| `memory_search` | `scope: "user"` | local table or `s3://.../v2/users/<id>/memory/` |
| `memory_source` | project/user | the same authoritative table, by current or revision key |

> [!IMPORTANT]
> You cannot read these stores as workspace files. Read knowledge pages with `graphit_wiki_source`
> and memory records with `graphit_memory_source`; both support bounded retrieval, while search
> returns ordered titles for selective reading. Memory uses priority/recency order; Knowledge uses
> relevance ranking.

---

## 5. Project Resolution and Optional Hub Contexts

For a named unfamiliar ecosystem project, reuse its known local path or call
`graphit_cluster_projects(project_dir: "<current-absolute-path>")` first. If found, pass its returned
path as `project_dir` to the relevant module tools. Only if absent, discover it through
`graphit_hub_projects` and inspect the relevant published artifacts. If no current project exists,
do not invent a path for cluster discovery. Reuse a resolution already established in this context.
Read the Hub skill before unresolved project discovery; a tool failure does not establish absence.

Every cluster result is a Graphit-managed project, including a neighboring checkout. Discovery
uses the current project's path; evidence reads then use the selected project's returned `dir` as
`project_dir`. Use AST search/source/schema/query for its code, Knowledge search/wiki source for
its documentation, and Memory/Task tools for its facts and prior work when needed. Being outside
the agent's working directory is never a reason to switch to native grep, glob or repository walks.
Read only the needed module skill for that target (including its overrides and enabled state),
and keep schemas, evidence and project-scoped mandatory memories associated with their own target.
The active coordinating Task stays in the delivery's owning project; looking up a neighbor's
history does not transfer its claim or authorize mutations there. Save logical project identity,
relative references and revision in shared findings; the next host resolves its own local root.
The Hub skill's discovery reference provides complete payloads for this transition and return.

When the answer needs an uninstalled Hub artifact, announce the preparation and install Knowledge
for documentation or AST for implementation, then query the corresponding module. The user's question
authorizes this necessary preparation; do not ask for redundant permission. Public technologies such
as React, Next.js, languages and frameworks do not require Hub discovery: use known knowledge and
official documentation, verifying current details. A published docs artifact is an optional source
when relevant, not a prerequisite for using those technologies.

The lifecycle below illustrates that optional artifact path; identifiers are placeholders from discovery.

The transport first establishes a trusted subject and resolves global, authenticated, user, and
team project grants; anonymous instead resolves global and `v2/anonymous/projects.json`. Every step below operates only
within that authorized project set. Discovery caches may
avoid repeated metadata transfer, but they do not authorize details, installation, search, or page
reads.

### Step 1: Discover Available Artifacts

```jsonc
graphit_hub_list(type: "knowledge", page_size: 20, ai_optimized: true)
// Returns one authorized page plus an opaque continuation cursor
```

### Step 2: Inspect Artifact Details

```jsonc
graphit_hub_show(id: "<project-ulid>/knowledge/nextjs-docs", ai_optimized: true)
// Returns metadata: description, version, size, contents summary
```

### Step 3: Install the Artifact

```jsonc
// Known local consumer project: retain its path
graphit_hub_install(project_dir: "<known-absolute-path>", id: "<project-ulid>/knowledge/nextjs-docs@<version>", alias: "nextjs-docs", ai_optimized: true)

// Records the selected version in graphit.lock.json. The index remains on S3.

// Alternative only without a local checkout: install globally, without a project lock
graphit_hub_install(id: "<project-ulid>/knowledge/nextjs-docs@<version>", ai_optimized: true)
```

The wiki itself is shared: a second project installing the same artifact adds a claim and copies
nothing. Its rows are read through the knowledge/wiki tools, never as files.

The claim records selection and version; it does not freeze permission. Opening the mounted S3
index revalidates access to the publishing project and fails closed after revocation or an
authorization-backend failure.

### Step 4: Search the Installed Artifact

Use the resolved `context` and preserve its project scope. Search titles identify a page; read the
selected returned slug with `graphit_wiki_source` using the same `project_dir`/`context`:

```jsonc
// Artifact installed for a local consumer project
graphit_knowledge_search(project_dir: "<known-absolute-path>", query: "middleware configuration", context: "nextjs-docs", top_k: 5, ai_optimized: true)

// Globally installed context without a local checkout
graphit_knowledge_search(query: "middleware configuration", context: "<project-ulid>/knowledge/nextjs-docs@<version>", top_k: 5, ai_optimized: true)
```

### Step 5: Alternative — Search via `wiki_search`

You can also include hub artifacts directly in `wiki_search` using `hub_refs`:

```jsonc
graphit_wiki_search(
  project_dir: "<known-absolute-path>",
  query: "middleware setup",
  wikis: ["project"],
  hub_refs: ["<project-ulid>/knowledge/nextjs-docs@<version>"],
  top_k: 5,
  ai_optimized: true
)
```

This searches the project wiki **and** the hub artifact simultaneously.

> [!TIP]
> Reuse evidence already retrieved from an installed artifact. Search and read additional pages only
> when the integration decision needs them; installing an artifact does not require querying it on every action.

---

## 6. Decision Guide: When to Use Which Tool

### Decision Tree

Reuse known Task context with these Knowledge operations. Add a focused `graphit_task_search` and
selected `graphit_task_get` only for unresolved earlier work, decisions or plan context. CLI synthesis
entries describe separate user workflows; agents use MCP rather than substituting the Graphit CLI.
Project branches use the known or cluster-resolved absolute `project_dir` as illustrated above.

```
What do you need?
│
├─ Quick keyword match in memories?
│  └─► graphit_memory_search(query: "...", scope: "project")
│
├─ AI-synthesized answer from memories?
│  └─► graphit memory ask "..." [--user | --context <name>]
│
├─ Quick keyword search in project docs?
│  └─► graphit_knowledge_search(query: "...", ai_optimized: true)
│
├─ AI-synthesized answer from project docs?
│  └─► graphit knowledge ask "..." [--context <name>]
│
├─ Semantic (vector) search?
│  └─► graphit_wiki_search(query: "...", wikis: ["project"], mode: "semantic")
│
├─ Catalog all documents in a wiki?
│  └─► graphit_wiki_browse(ai_optimized: true)
│
├─ Find cross-references between documents?
│  └─► graphit_wiki_xrefs(query: "...", ai_optimized: true)
│
├─ Search hub-imported knowledge?
│  ├─► graphit_knowledge_search(project_dir: "<known-path>", query: "...", context: "<resolved-alias>")
│  └─► graphit_wiki_search(query: "...", hub_refs: ["<qualified-id>@<version>"])
│
└─ Check sync history?
   └─► graphit_wiki_log()
```

### Quick Reference

| Scenario | Tool | Key Parameter |
|----------|------|--------------|
| "Did I save a memory about X?" | `memory_search` | `scope` |
| "Are these five tasks done?" | `task_query` | `table: tasks`, `filter: id IN (...)`, `columns: [id, status]` |
| "Which docs pages are stale?" | `knowledge_query` | `table: chunks`, `filter: stale_since != ''` |
| "Every entity in this file" | `ast_fts_query` | `table: entities`, `filter: path = '...'` |
| "Explain how X works from my notes" | `graphit memory ask` | `--user` / `--context` |
| "Find docs mentioning X" | `knowledge_search` → `wiki_source` | Knowledge `context` and resolved `project_dir` |
| "Explain X from the project docs" | `graphit knowledge ask` (CLI workflow) | Knowledge `--context` |
| "Search docs and memory for X" | `wiki_search` → `wiki_source`; `memory_search` → `memory_source` | Retrieve each required scope; reuse known Task context |
| "Find semantically similar content" | `wiki_search` → `wiki_source` | Wiki `mode: "semantic"` |
| "What docs exist?" | `wiki_browse` | `context` |
| "What links to this page?" | `wiki_xrefs` | `query`, `context` |
| "Search an installed Next.js docs artifact" | `knowledge_search` → `wiki_source` | Resolved local alias with `project_dir`, or global qualified context without a checkout |

> [!TIP]
> Pass `ai_optimized: true` where supported, begin with small `top_k` and read selected sources.
> Expand retrieval only while the question has an unresolved evidence gap.

## 7. Maintained Domain Documentation

Knowledge maintains documentation that readers and consuming projects can use without the author's
conversation. Organize it around business domains, audiences and journeys in the configured `docs_dir`
and existing project structure. A whole-system documentation request begins with a domain/interaction
inventory and coverage map: actor/journey → rule or contract → implementation evidence → page/section.
A directory tree, page count or three general overview pages cannot demonstrate coverage of unrelated
domains. Split or combine pages by reader needs, contractual boundaries and maintenance ownership.

Provide navigable entry points from the documentation root to domain overviews and relevant user,
technical and operational pages. User guides explain prerequisites, actions, observable outcomes and
errors/recovery. Technical documentation describes interfaces appropriate to the product (CLI, API,
library, grammar or UI), architecture boundaries, data/state rules, supported versions, source provenance
and operations. Keep examples consistent and link shared facts to their authoritative definition.
Empty scaffolds do not satisfy coverage; no fixed number of documents is required.

The generated Knowledge skill preserves retrieval and work-unit invariants in `SKILL.md`. It loads
`references/documentation-design.md` before designing/reorganizing a set or reviewing coverage, and
`references/worked-domain.md` before substantive domain authoring. The latter demonstrates a complete
illustrative set with navigation, user steps, integration/state contracts, operations and both drift
directions. Its product, stack and rules are examples, not requirements for the consumer. References
are installed beside the skill and can also be retrieved individually with `graphit_module_skill`
using `module: "knowledge"` and the selected `reference`; retrieval-only sessions need not load them.

For **every code work unit**, inspect and update affected user/technical documentation, examples and
navigation in the same unit. For **every documentation work unit**, inspect the corresponding
implementation and validate its claims. Authorized maintenance requires no additional permission.
Record inspected code/page targets, applicable version, actual evidence and any no-impact rationale
in Task before completing the unit. Review both directions: rule → source/test → page, and changed
page → implementation and consumer expectations. A documentation-quality review does not establish
that runtime tests passed.

Resolve stale pages or authorized implementation defects. Preserve future specifications as explicitly
proposed intent; keep current limitations truthful and unresolved implementation work in Task. Do not
weaken an accepted requirement to match a bug or implement a new feature merely because an unsupported
claim appeared in a draft. Report source inspection, executed checks and unavailable validation distinctly.

To support other projects through Hub, include scope/version, prerequisites, vocabulary, public
contracts, examples, operations and stable provenance. Use relative links and repository-relative source
references, not the author's private checkout paths. Publication is a separate authorized Hub action;
updating the authoritative documents does not automatically publish them or imply deployed behavior.
