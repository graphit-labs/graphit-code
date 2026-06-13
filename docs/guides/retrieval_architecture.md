# Retrieval Architecture

> Complete guide to the wiki, knowledge, and memory retrieval system.

This document explains how data flows through the retrieval pipeline, which tools to use in each scenario, and how scope parameters control what gets searched.

---

## 1. Three-Tier Architecture

The retrieval system is organized into three tiers of increasing sophistication:

### Tier 1: Raw File Search

**Tools:** `graphit_memory_search`

Direct text matching (grep-style) on raw Markdown files stored in `.graphit/memory/{project|user}/raw/*.md`. This tier is:

- **Lightweight** — no indexes, no databases
- **No AI** — pure text matching
- **Instant** — O(n) scan over small files
- **Best for** — quick keyword lookups, checking if a memory exists

```
.graphit/
└── memory/
    ├── project/
    │   └── raw/
    │       ├── decision-001.md
    │       └── context-002.md
    └── user/
        └── raw/
            ├── preference-001.md
            └── workflow-002.md
```

### Tier 2: Compiled Wiki (FTS5 / BM25 / Semantic)

**Tools:** `graphit_knowledge_search`, `graphit_wiki_search`, `graphit_wiki_browse`

Operates on compiled wiki artifacts — either `.md` files or a `wiki.db` SQLite database:

| Tool | Backend | What it searches |
|------|---------|-----------------|
| `knowledge_search` | BM25 on `.md` files | `.graphit/knowledge/{project\|CONTEXT}/wiki/*.md` |
| `wiki_search` | FTS5 + semantic on `wiki.db` | `.graphit/knowledge/project/wiki/wiki.db` or `.graphit/memory/project/wiki/wiki.db` |
| `wiki_browse` | SQLite catalog on `wiki.db` | Same as `wiki_search` |

```
.graphit/
├── knowledge/
│   ├── project/
│   │   └── wiki/
│   │       ├── index.md
│   │       ├── page-001.md
│   │       └── wiki.db        ← FTS5/semantic index
│   └── <hub-artifact>/
│       └── wiki/
│           ├── index.md
│           └── page-001.md
└── memory/
    └── project/
        └── wiki/
            └── wiki.db        ← FTS5/semantic index
```

### Tier 3: AI Synthesis

**Tools:** `graphit_knowledge_query`, `graphit_memory_query`

Uses an AI model to synthesize answers from wiki pages found via BM25 retrieval. Supports multi-turn conversations for iterative exploration.

- **AI-powered** — LLM reads retrieved pages and synthesizes a coherent answer
- **Multi-turn** — follow-up queries refine the answer
- **Higher latency** — involves an LLM inference call
- **Best for** — complex questions that span multiple documents

---

## 2. Tool Differentiation Matrix

| Tool | Module | Searches | Backend | AI? | Scope Params |
|------|--------|----------|---------|-----|-------------|
| `graphit_knowledge_search` | knowledge | project knowledge wiki | BM25 on `.md` files | No | `context` (empty = project, named = hub import) |
| `graphit_knowledge_query` | knowledge | project knowledge wiki | AI + BM25 multi-turn | Yes | `context` |
| `graphit_wiki_search` | wiki | multiple wikis simultaneously | FTS5/semantic on `wiki.db` | Semantic mode only | `wikis[]` (project, memory), `hub_refs[]` |
| `graphit_wiki_browse` | wiki | single wiki catalog | SQLite `wiki.db` | No | `wiki` (project or memory) |
| `graphit_wiki_xrefs` | wiki | single wiki cross-refs | SQLite `wiki.db` | No | `wiki` (project or memory) |
| `graphit_wiki_log` | wiki | single wiki sync history | SQLite `wiki.db` | No | `wiki` (project or memory) |
| `graphit_memory_search` | memory | raw memory files | text matching (grep) | No | `scope` (project or user) |
| `graphit_memory_query` | memory | memory wiki | AI + BM25 | Yes | `scope`, `context` |

> [!NOTE]
> All tools that return structured data support `ai_optimized: true` to return token-efficient, pre-summarized output optimized for LLM consumption.

---

## 3. Scope & Context Parameters

### Memory: `scope` parameter

Controls which pool of raw memory files is searched.

| Value | Description | Storage Path |
|-------|-------------|-------------|
| `"project"` (default) | Project-specific memories | `.graphit/memory/project/raw/` |
| `"user"` | Personal cross-project memories | `.graphit/memory/user/raw/` |

```jsonc
// Search project memories (default)
graphit_memory_search(query: "auth flow", scope: "project")

// Search personal memories across all projects
graphit_memory_search(query: "preferred patterns", scope: "user")
```

### Knowledge: `context` parameter

Controls which knowledge wiki is searched. An empty context targets the local project wiki compiled from `docs/`. A named context targets a hub-imported knowledge artifact.

| Value | Description | Storage Path |
|-------|-------------|-------------|
| `""` (default) | Local project wiki from `docs/` | `.graphit/knowledge/project/wiki/` |
| `"<name>"` | Hub-imported knowledge artifact | `.graphit/knowledge/<name>/wiki/` |

```jsonc
// Search local project knowledge
graphit_knowledge_search(query: "deployment config", context: "")

// Search hub-imported knowledge (e.g., a framework's docs)
graphit_knowledge_search(query: "middleware setup", context: "nextjs-docs")
```

### Wiki: `wikis`, `hub_refs`, and `wiki` parameters

The wiki module provides the most flexible search surface, supporting simultaneous multi-wiki queries.

**`wikis[]` — for `wiki_search` (multi-scope)**

| Value | Description | wiki.db Location |
|-------|-------------|-----------------|
| `["project"]` | Search the knowledge wiki | `.graphit/knowledge/project/wiki/wiki.db` |
| `["memory"]` | Search the memory wiki | `.graphit/memory/project/wiki/wiki.db` |
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

## 4. Filesystem Path Resolution

Every tool resolves to a specific filesystem path based on its scope parameters:

| Tool | Scope | Resolves To |
|------|-------|------------|
| `knowledge_search` | no context | `.graphit/knowledge/project/wiki/*.md` |
| `knowledge_search` | `context: "X"` | `.graphit/knowledge/X/wiki/*.md` |
| `knowledge_query` | no context | `.graphit/knowledge/project/wiki/*.md` |
| `knowledge_query` | `context: "X"` | `.graphit/knowledge/X/wiki/*.md` |
| `wiki_search` | `wikis: ["project"]` | `.graphit/knowledge/project/wiki/wiki.db` |
| `wiki_search` | `wikis: ["memory"]` | `.graphit/memory/project/wiki/wiki.db` |
| `wiki_browse` | `wiki: "project"` | `.graphit/knowledge/project/wiki/wiki.db` |
| `wiki_browse` | `wiki: "memory"` | `.graphit/memory/project/wiki/wiki.db` |
| `wiki_log` | `wiki: "project"` | `.graphit/knowledge/project/wiki/wiki.db` |
| `wiki_log` | `wiki: "memory"` | `.graphit/memory/project/wiki/wiki.db` |
| `wiki_xrefs` | `wiki: "project"` | `.graphit/knowledge/project/wiki/wiki.db` |
| `wiki_xrefs` | `wiki: "memory"` | `.graphit/memory/project/wiki/wiki.db` |
| `memory_search` | `scope: "project"` | `.graphit/memory/project/raw/*.md` |
| `memory_search` | `scope: "user"` | `.graphit/memory/user/raw/*.md` |
| `memory_query` | `scope: "project"` | `.graphit/memory/project/wiki/*.md` |
| `memory_query` | `scope: "user"` | `.graphit/memory/user/wiki/*.md` |

> [!IMPORTANT]
> Never read `.graphit/knowledge/*/index.md` or `.graphit/memory/*/index.md` directly. Always use the MCP tools — they provide compiled, BM25-ranked, pre-summarized output that is far more token-efficient.

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
// Either of these works:
graphit_hub_install(id: "nextjs-docs")
graphit_knowledge_install(name: "nextjs-docs")

// Installs to: .graphit/knowledge/nextjs-docs/
```

After installation, the artifact's wiki files are available at `.graphit/knowledge/nextjs-docs/wiki/`.

### Step 4: Search the Installed Artifact

Use the `context` parameter to target the installed artifact:

```jsonc
// BM25 keyword search
graphit_knowledge_search(query: "middleware configuration", context: "nextjs-docs")

// AI-synthesized answer
graphit_knowledge_query(query: "How do I set up middleware?", context: "nextjs-docs")
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
│  └─► graphit_memory_query(query: "...", scope: "project")
│
├─ Quick keyword search in project docs?
│  └─► graphit_knowledge_search(query: "...", ai_optimized: true)
│
├─ AI-synthesized answer from project docs?
│  └─► graphit_knowledge_query(query: "...")
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
| "Explain how X works from my notes" | `memory_query` | `scope` |
| "Find docs mentioning X" | `knowledge_search` | `context` |
| "Explain X from the project docs" | `knowledge_query` | `context` |
| "Search everything for X" | `wiki_search` | `wikis: ["project", "memory"]` |
| "Find semantically similar content" | `wiki_search` | `mode: "semantic"` |
| "What docs exist?" | `wiki_browse` | `wiki` |
| "What links to this page?" | `wiki_xrefs` | `wiki` |
| "Search NextJS docs artifact" | `knowledge_search` | `context: "nextjs-docs"` |

> [!TIP]
> Always pass `ai_optimized: true` when calling from an agent context. This returns BM25-ranked, pre-summarized output that costs ~500 tokens versus grep scanning all files.
