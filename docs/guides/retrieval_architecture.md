# Retrieval Architecture

> Complete guide to the wiki, knowledge, and memory retrieval system.

This document explains how data flows through the retrieval pipeline, which tools to use in each scenario, and how scope parameters control what gets searched.

---

## 1. Three-Tier Architecture

The retrieval system is organized into three tiers of increasing sophistication:

### Tier 1: Direct ranked search

**Tools:** `graphit_memory_mandatory`, `graphit_memory_search`

Ranked matching over the compiled memory wiki, which lives once in the global brand directory. This tier is:

- **Lightweight** — one indexed query
- **No AI** — engine-ranked text matching
- **Fast** — no source-file scan
- **Best for** — unconditional session context followed by quick contextual lookup

```
~/.graphit/
├── memory-table/                    authoritative tables in local-only mode
│   ├── memory-project-<id>/
│   └── memory-user-<hash>/
└── wiki/memory/                     compiled query projections
    ├── project/<id>/
    └── user/<hash>/
```

### Tier 2: Compiled Wiki (BM25 / Semantic, LanceDB)

**Tools:** `graphit_knowledge_search`, `graphit_wiki_search`, `graphit_wiki_browse`, `graphit_task_search`

Operates on `index.lance/` stores locally or at a mounted Hub `s3://` URI:

| Tool | Backend | What it searches |
|------|---------|-----------------|
| `knowledge_search` | LanceDB BM25 | local project `index.lance/` or a mounted Hub artifact |
| `wiki_search` | BM25 + semantic on `index.lance/` | `~/.graphit/wiki/knowledge/project/<project-id>/index.lance/` or `~/.graphit/wiki/memory/project/<project-id>/index.lance/` |
| `wiki_browse` | LanceDB catalog on `index.lance/` | Same as `wiki_search` |
| `task_search` | LanceDB BM25 | Authoritative project task specs/check evidence and typed comment bodies |

```
~/.graphit/wiki/
├── knowledge/
│   └── project/<project-id>/
│       └── index.lance/      ← complete local knowledge wiki
└── memory/project/<project-id>/
    └── index.lance/          ← complete memory query projection
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
| `graphit_wiki_search` | wiki | multiple wikis simultaneously | BM25/semantic on `index.lance/` | Semantic mode only | `wikis[]` (project, memory), `hub_refs[]` |
| `graphit_wiki_browse` | wiki | single wiki catalog | LanceDB `index.lance/` | No | `wiki` (project or memory) |
| `graphit_wiki_xrefs` | wiki | single wiki cross-refs | LanceDB `index.lance/` | No | `wiki` (project or memory) |
| `graphit_wiki_log` | wiki | single wiki sync history | LanceDB `index.lance/` | No | `wiki` (project or memory) |
| `graphit_memory_search` | memory | compiled memory wiki | LanceDB BM25 | No | `scope` (project or user) |
| `graphit_memory_mandatory` | memory | authoritative live memory table | LanceDB filter, no ranking | No | `scope` (project or user) |
| `graphit_task_search` | task | current/prior task specs and comments | LanceDB BM25 | No | project identity |
| `graphit knowledge query` | CLI | project or imported knowledge wiki | agent CLI + retrieved pages | Yes | `--context` |
| `graphit memory query` | CLI | project/user/imported memory wiki | agent CLI + retrieved pages | Yes | `--user`, `--context` |
| `graphit live` | CLI/UI | selected Hub artifacts in an ephemeral workspace | coding-agent session | Yes | artifact IDs and versions |

> [!NOTE]
> All tools that return structured data support `ai_optimized: true` to return token-efficient, pre-summarized output optimized for LLM consumption.

---

## 3. Scope & Context Parameters

### Memory: `scope` parameter

Controls which authoritative memory scope is projected and searched.

| Value | Description | Storage Path |
|-------|-------------|-------------|
| `"project"` (default) | Project-specific memories | local or S3 `memory/project/<project-id>` table |
| `"user"` | Personal cross-project memories | local or S3 `memory/user/<hash>` table |

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

### Wiki: `wikis`, `hub_refs`, and `wiki` parameters

The wiki module provides the most flexible search surface, supporting simultaneous multi-wiki queries.

**`wikis[]` — for `wiki_search` (multi-scope)**

| Value | Description | Index Location |
|-------|-------------|-----------------|
| `["project"]` | Search the knowledge wiki | `~/.graphit/wiki/knowledge/project/<project-id>/index.lance/` |
| `["memory"]` | Search the memory wiki | `~/.graphit/wiki/memory/project/<project-id>/index.lance/` |
| `["project", "memory"]` | Search both simultaneously | Both databases |

**`hub_refs[]` — for `wiki_search` (hub artifacts)**

| Value | Description |
|-------|-------------|
| `["artifact-id@version"]` | Include a hub knowledge artifact in the search |

**`wiki` — for `wiki_browse`, `wiki_log`, `wiki_xrefs` (single-scope)**

| Value | Description |
|-------|-------------|
| `"project"` | Browse/inspect the knowledge wiki |
| `"memory"` | Browse/inspect the memory wiki |

```jsonc
// Search both project docs and memory at once
graphit_wiki_search(query: "error handling", wikis: ["project", "memory"])

// Semantic search across project docs
graphit_wiki_search(query: "authentication flow", wikis: ["project"], mode: "semantic")

// Include hub artifact in search
graphit_wiki_search(query: "API reference", wikis: ["project"], hub_refs: ["express-docs@1.0"])

// Browse all documents in memory wiki
graphit_wiki_browse(wiki: "memory", ai_optimized: true)

// Get cross-references for project wiki
graphit_wiki_xrefs(wiki: "project", ai_optimized: true)
```

---

## 4. Store resolution

Every tool resolves to a local directory or immutable object-store URI based on its scope:

| Tool | Scope | Resolves To |
|------|-------|------------|
| `knowledge_search` | no context | `~/.graphit/wiki/knowledge/project/<project-id>/` |
| `knowledge_search` | `context: "X"` | versioned Hub `s3://…/index.lance` |
| `wiki_search` | `wikis: ["project"]` | `~/.graphit/wiki/knowledge/project/<project-id>/index.lance/` |
| `wiki_search` | `wikis: ["memory"]` | `~/.graphit/wiki/memory/project/<project-id>/index.lance/` |
| `wiki_browse` | `wiki: "project"` | `~/.graphit/wiki/knowledge/project/<project-id>/index.lance/` |
| `wiki_browse` | `wiki: "memory"` | `~/.graphit/wiki/memory/project/<project-id>/index.lance/` |
| `wiki_log` | `wiki: "project"` | `~/.graphit/wiki/knowledge/project/<project-id>/index.lance/` |
| `wiki_log` | `wiki: "memory"` | `~/.graphit/wiki/memory/project/<project-id>/index.lance/` |
| `wiki_xrefs` | `wiki: "project"` | `~/.graphit/wiki/knowledge/project/<project-id>/index.lance/` |
| `wiki_xrefs` | `wiki: "memory"` | `~/.graphit/wiki/memory/project/<project-id>/index.lance/` |
| `memory_search` | `scope: "project"` | `~/.graphit/wiki/memory/project/<project-id>/` |
| `memory_search` | `scope: "user"` | `~/.graphit/wiki/memory/user/<hash>/` |

> [!IMPORTANT]
> You cannot read these files, and that is deliberate: every wiki lives once in the global brand directory, outside any project. `graphit_wiki_source` is how a page is read — it takes the project as a parameter and slices, so a long page costs only the part you asked for. The other tools return compiled, BM25-ranked, pre-summarized output.

---

## 5. Hub Knowledge Context Lifecycle

Hub knowledge artifacts provide pre-built documentation for external libraries and frameworks. Here is the complete lifecycle:

### Step 1: Discover Available Artifacts

```jsonc
graphit_hub_list(type: "knowledge")
// Returns a list of available knowledge artifacts with IDs and descriptions
```

### Step 2: Inspect Artifact Details

```jsonc
graphit_hub_show(id: "nextjs-docs")
// Returns metadata: description, version, size, contents summary
```

### Step 3: Install the Artifact

```jsonc
graphit_hub_install(id: "nextjs-docs")

// Records the selected version in graphit.lock.json. The index remains on S3.
```

The wiki itself is shared: a second project installing the same artifact adds a claim and copies
nothing. Its rows are read through the knowledge/wiki tools, never as files.

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
├─ Search BOTH knowledge + memory at once?
│  └─► graphit_wiki_search(query: "...", wikis: ["project", "memory"])
│
├─ Semantic (vector) search?
│  └─► graphit_wiki_search(query: "...", wikis: ["project"], mode: "semantic")
│
├─ Catalog all documents in a wiki?
│  └─► graphit_wiki_browse(wiki: "project", ai_optimized: true)
│
├─ Find cross-references between documents?
│  └─► graphit_wiki_xrefs(wiki: "project", ai_optimized: true)
│
├─ Search hub-imported knowledge?
│  ├─► graphit_knowledge_search(query: "...", context: "artifact-id")
│  └─► graphit_wiki_search(query: "...", hub_refs: ["artifact-id@version"])
│
└─ Check sync history?
   └─► graphit_wiki_log(wiki: "project")
```

### Quick Reference

| Scenario | Tool | Key Parameter |
|----------|------|--------------|
| "Did I save a memory about X?" | `memory_search` | `scope` |
| "Explain how X works from my notes" | `graphit memory query` | `--user` / `--context` |
| "Find docs mentioning X" | `knowledge_search` | `context` |
| "Explain X from the project docs" | `graphit knowledge query` | `--context` |
| "Search everything for X" | `wiki_search` | `wikis: ["project", "memory"]` |
| "Find semantically similar content" | `wiki_search` | `mode: "semantic"` |
| "What docs exist?" | `wiki_browse` | `wiki` |
| "What links to this page?" | `wiki_xrefs` | `wiki` |
| "Search NextJS docs artifact" | `knowledge_search` | `context: "nextjs-docs"` |

> [!TIP]
> Always pass `ai_optimized: true` when calling from an agent context. This returns BM25-ranked, pre-summarized output that costs ~500 tokens versus grep scanning all files.
