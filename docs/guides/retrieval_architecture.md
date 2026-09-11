# Retrieval Architecture

> Complete guide to the wiki, knowledge, and memory retrieval system.

This document explains how data flows through the retrieval pipeline, which tools to use in each scenario, and how scope parameters control what gets searched.

---

## 1. Three-Tier Architecture

The retrieval system is organized into three tiers of increasing sophistication:

### Tier 1: Direct memory retrieval

**Tools:** `graphit_memory_mandatory`, `graphit_memory_search`

Indexed matching directly over each scope's authoritative memory table. Matches use the canonical
Memory order—mandatory, important, normal—then newest first inside each group. This tier is:

- **Lightweight** — indexed table access without source-file scanning
- **No AI** — engine-matched text retrieval with deterministic domain ordering
- **Fast** — no source-file scan
- **Best for** — unconditional session context followed by quick contextual lookup

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

Knowledge and Task are separate sources but a single agent retrieval workflow. Every project or imported
Knowledge search is paired with one focused `graphit_task_search` for the same question when Task is
enabled and available. Follow `next_cursor`, then read every relevant hit with `graphit_task_get`; Task
search metadata alone is not the reusable record. Use the task specification, analytical outcomes,
progress, comments, check evidence, next steps, and audit history alongside pages read with
`graphit_wiki_source`. If Task is disabled or unavailable, continue the Knowledge search.

```
~/.graphit/wiki/knowledge/project/<project-id>/
└── index.lance/                  ← complete local knowledge wiki
```

> There is no wiki inside a project. Every one of them lives once, in the global brand
> directory, keyed by an id — see [Storage Layout](../architecture/storage_layout.md).
> This is why every tool below takes `project_dir`: it is how a sibling project's wiki
> is reached, and why no wiki is readable with a file tool.

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

**Surfaces:** `graphit knowledge query`, `graphit memory query`, Observatory AI search, and
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
| `graphit knowledge query` | CLI | project or imported knowledge wiki | agent CLI + retrieved pages | Yes | `--context` |
| `graphit memory query` | CLI | project/user/imported authoritative memory table | agent CLI + retrieved records | Yes | `--user`, `--context` |
| `graphit live` | CLI/UI | selected Hub artifacts in an ephemeral workspace | coding-agent session | Yes | artifact IDs and versions |

> [!NOTE]
> All tools that return structured data support `ai_optimized: true` to return token-efficient, pre-summarized output optimized for LLM consumption.

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
// Phase 1: load every unconditional memory, with no query
graphit_memory_mandatory(scope: "project")

// Search project memories (default)
graphit_memory_search(query: "auth flow", scope: "project", exclude_mandatory: true)

// Search personal memories across all projects
graphit_memory_search(query: "preferred patterns", scope: "user")
```

### Knowledge: `context` parameter

Controls which knowledge wiki is searched. An empty context targets the local project wiki compiled from `docs/`. A named context targets a hub-imported knowledge artifact.

| Value | Description | Storage Path |
|-------|-------------|-------------|
| `""` (default) | Local project wiki from `docs/` | `~/.graphit/wiki/knowledge/project/<project-id>/` |
| `"<name>"` | Installed Hub knowledge artifact | versioned `s3://` LanceDB mount |

```jsonc
// Search local project knowledge
graphit_knowledge_search(query: "deployment config", context: "")

// Search hub-imported knowledge (e.g., a framework's docs)
graphit_knowledge_search(query: "middleware setup", context: "nextjs-docs")
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
graphit_wiki_search(query: "authentication flow", wikis: ["project"], mode: "semantic")

// Include hub artifact in search
graphit_wiki_search(query: "API reference", wikis: ["project"], hub_refs: ["express-docs@1.0"])

// Get cross-references for project wiki
graphit_wiki_xrefs(query: "authentication-flow", ai_optimized: true)
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

## 5. Hub Knowledge Context Lifecycle

Hub knowledge artifacts provide pre-built documentation for external libraries and frameworks. Here is the complete lifecycle:

The transport first establishes a trusted subject and resolves global, authenticated, user, and
team project grants; anonymous instead resolves global and `v2/anonymous/projects.json`. Every step below operates only
within that authorized project set. Discovery caches may
avoid repeated metadata transfer, but they do not authorize details, installation, search, or page
reads.

### Step 1: Discover Available Artifacts

```jsonc
graphit_hub_list(type: "knowledge", limit: 20)
// Returns one authorized page plus an opaque continuation cursor
```

### Step 2: Inspect Artifact Details

```jsonc
graphit_hub_show(id: "<project-ulid>/knowledge/nextjs-docs")
// Returns metadata: description, version, size, contents summary
```

### Step 3: Install the Artifact

```jsonc
graphit_hub_install(id: "<project-ulid>/knowledge/nextjs-docs")

// Records the selected version in graphit.lock.json. The index remains on S3.
```

The wiki itself is shared: a second project installing the same artifact adds a claim and copies
nothing. Its rows are read through the knowledge/wiki tools, never as files.

The claim records selection and version; it does not freeze permission. Opening the mounted S3
index revalidates access to the publishing project and fails closed after revocation or an
authorization-backend failure.

### Step 4: Search the Installed Artifact

Use the `context` parameter to target the installed artifact:

```jsonc
// BM25 keyword search
graphit_knowledge_search(query: "middleware configuration", context: "nextjs-docs")

// AI-synthesized answer from the installed context (CLI)
graphit knowledge query "How do I set up middleware?" --context nextjs-docs
```

### Step 5: Alternative — Search via `wiki_search`

You can also include hub artifacts directly in `wiki_search` using `hub_refs`:

```jsonc
graphit_wiki_search(
  query: "middleware setup",
  wikis: ["project"],
  hub_refs: ["nextjs-docs@1.0"]
)
```

This searches the project wiki **and** the hub artifact simultaneously.

> [!TIP]
> After installing a hub knowledge artifact, always search its wiki via MCP **before** writing integration code. The artifact contains API patterns, gotchas, and best practices that prevent common mistakes.

---

## 6. Decision Guide: When to Use Which Tool

### Decision Tree

Every Knowledge branch below includes the paired `graphit_task_search` plus relevant `graphit_task_get`
reads described above. The diagram names the primary Knowledge operation rather than repeating the pair
on every branch.

```
What do you need?
│
├─ Quick keyword match in memories?
│  └─► graphit_memory_search(query: "...", scope: "project")
│
├─ AI-synthesized answer from memories?
│  └─► graphit memory query "..." [--user | --context <name>]
│
├─ Quick keyword search in project docs?
│  └─► graphit_knowledge_search(query: "...", ai_optimized: true)
│
├─ AI-synthesized answer from project docs?
│  └─► graphit knowledge query "..." [--context <name>]
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
│  ├─► graphit_knowledge_search(query: "...", context: "artifact-id")
│  └─► graphit_wiki_search(query: "...", hub_refs: ["artifact-id@version"])
│
└─ Check sync history?
   └─► graphit_wiki_log()
```

### Quick Reference

| Scenario | Tool | Key Parameter |
|----------|------|--------------|
| "Did I save a memory about X?" | `memory_search` | `scope` |
| "Explain how X works from my notes" | `graphit memory query` | `--user` / `--context` |
| "Find docs mentioning X" | `knowledge_search` + `task_search`/`task_get` | Knowledge `context`; same-question Task lookup |
| "Explain X from the project docs" | `graphit knowledge query` + `task_search`/`task_get` | Knowledge `--context`; same-question Task lookup |
| "Search docs and memory for X" | `wiki_search` + `task_search`/`task_get` plus `memory_search` | Pair the Knowledge lookup with same-question Task history; run Memory independently |
| "Find semantically similar content" | `wiki_search` + `task_search`/`task_get` | Wiki `mode: "semantic"`; same-question Task lookup |
| "What docs exist?" | `wiki_browse` | `context` |
| "What links to this page?" | `wiki_xrefs` | `query`, `context` |
| "Search NextJS docs artifact" | `knowledge_search` + `task_search`/`task_get` | Knowledge `context: "nextjs-docs"`; current-project Task lookup |

> [!TIP]
> Always pass `ai_optimized: true` when calling from an agent context. This returns BM25-ranked, pre-summarized output that costs ~500 tokens versus grep scanning all files.
