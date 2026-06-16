---
title: "MCP Tools Reference"
description: "Complete reference of all MCP tools available to AI agents via the Graphit Code stdio server."
content-type: reference
audience: developers, ai-agents
keywords:
  - mcp
  - tools
  - api
  - reference
  - ai agents
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/user_manual.md"
  - "docs/guides/troubleshooting.md"
---

# MCP Tools Reference

This document provides a complete reference for all MCP (Model Context Protocol) tools exposed by the Graphit Code stdio server. AI agents use these tools to interact with the Graphit platform — indexing code, querying graphs, managing memories, searching knowledge, and more.

The tools are organized by module. Every tool name follows the pattern `graphit_<module>_<action>` (e.g., `graphit_ast_query`).

---

## Retrieval Tools Overview

The platform provides multiple retrieval tools across three tiers. Use this matrix to choose the right tool:

| Tool | Backend | AI? | Scope Params | Best for |
|------|---------|-----|-------------|----------|
| `graphit_memory_search` | Text match on raw `.md` files | No | `scope` (project/user) | Quick keyword match in memories — instant, lightweight |
| `graphit_knowledge_search` | BM25 on wiki `.md` pages | No | `context` (empty=project, named=hub import) | Keyword search in project docs |
| `graphit_wiki_search` | FTS5 + semantic on `wiki.db` | Semantic mode | `wikis[]` (project, memory), `hub_refs[]` | Multi-source search, semantic search |
| `graphit_wiki_browse` | SQLite `wiki.db` catalog | No | `wiki` (project/memory) | Listing all documents with filters |
| `graphit_knowledge_query` | AI + BM25 multi-turn | Yes | `context` | Deep AI-synthesized answer from project docs |
| `graphit_memory_query` | AI + compiled wiki | Yes | `scope`, `context` | AI-synthesized answer from memories |

**Key parameter differences:**
- **`scope`** (Memory tools): `"project"` (default) = project-specific memories, `"user"` = personal cross-project memories
- **`context`** (Knowledge/Memory tools): empty = local project, `"<name>"` = hub-imported context at `.graphit/knowledge/<name>/`
- **`wikis`** (Wiki search): `["project"]` = knowledge wiki, `["memory"]` = memory wiki, both for multi-source
- **`hub_refs`** (Wiki search): `["artifact-id@version"]` to include hub knowledge artifacts

> For the full retrieval architecture guide with filesystem paths and decision trees, see [Retrieval Architecture](retrieval_architecture.md).

---

## Table of Contents

- [Lifecycle Tools](#lifecycle-tools)
- [AST Tools](#ast-tools)
- [Knowledge Tools](#knowledge-tools)
- [Memory Tools](#memory-tools)
- [Hub Tools](#hub-tools)
- [Wiki Tools](#wiki-tools)
- [Dream Tools](#dream-tools)
- [Daemon Tools](#daemon-tools)
- [Cluster Tools](#cluster-tools)
- [Improvements Tools](#improvements-tools)

---

## Lifecycle Tools

Tools for project initialization, syncing, updating, removing, and configuration management.

### `graphit_init`

**Description:** Initialize a new project in the given project directory, creating project identity and lockfiles.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | The directory of the project to initialize |
| `ide` | string | | Target IDE (claude, cursor, gemini, etc.) |
| `id` | string | | Project ID (ULID) override |
| `name` | string | | Project name override |
| `description` | string | | Project description |

---

### `graphit_sync`

**Description:** Sync and reindex all local modules, AST DB, memory wikis, and update IDE rules.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory to sync |
| `ide` | string | | Target IDE |

This tool performs a full sync cycle:
1. AST indexing (if the `ast` module is enabled)
2. Knowledge indexing (if the `knowledge` module is enabled)
3. Memory cycle (project and user)
4. Hub git store sync
5. IDE rule installation
6. IDE adapter sync

---

### `graphit_update`

**Description:** Update all installed hub artifacts to their latest version and refresh rules.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory to update |
| `ide` | string | | Target IDE |

---

### `graphit_remove`

**Description:** Uninstall and remove Graphit from the current project.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory to remove from |
| `ide` | string | | Target IDE |

---

### `graphit_config_set`

**Description:** Set a configuration key to the specified value globally or locally.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | | Project directory (ignored if `global` is `true`) |
| `key` | string | ✅ | Configuration key (e.g., `ide`, `cli`, `hub.repo`) |
| `value` | string | ✅ | Configuration value |
| `global` | boolean | | Save to global configuration instead of project |

---

### `graphit_config_get`

**Description:** Get the value of a configuration key.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | | Project directory (ignored if `global` is `true`) |
| `key` | string | ✅ | Configuration key to retrieve |
| `global` | boolean | | Load from global configuration instead of project |

---

### `graphit_config_unset`

**Description:** Unset a configuration key.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | | Project directory (ignored if `global` is `true`) |
| `key` | string | ✅ | Configuration key to unset |
| `global` | boolean | | Remove from global configuration instead of project |

---

### `graphit_config_list`

**Description:** List all configuration keys and their values.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | | Project directory |
| `global` | boolean | | List global configuration |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_version`

**Description:** Get the current version of the Graphit CLI and MCP server.

_No parameters._

---

## AST Tools

Tools for building, querying, and managing the AST code graph database.

### `graphit_ast_index`

**Description:** Index files in the project to build the AST code graph database.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory to index |
| `path` | string | | Target path to index (defaults to `project_dir`) |
| `workers` | integer | | Number of parallel worker threads |
| `reset` | boolean | | Reset database before indexing |
| `reindex` | boolean | | Force reindexing of unchanged files |
| `cluster` | string | | Optional cluster label for grouping |
| `no_source` | boolean | | Do not index file source contents |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

**Returns:** JSON with indexing statistics (files processed, entities found, etc.).

---

### `graphit_ast_query`

**Description:** Execute a Cypher query against the AST code graph database.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `query` | string | ✅ | Cypher query to execute against the AST graph database |
| `context` | string | | Named imported context to query instead of the default project |
| `ai_optimized` | boolean | | Optimize the Cypher query execution for AI context |

> **Tip:** Always set `ai_optimized: true` for AI agent usage. This returns results in a compact, token-efficient format (TOON) rather than raw JSON.

---

### `graphit_ast_query_ai`

**Description:** Convert a natural language question about the codebase into a Cypher query using AI, execute it, and return results.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `query` | string | ✅ | Natural language question about the codebase to convert to Cypher |
| `context` | string | | Named imported context to query |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_ast_schema`

**Description:** Return the AST graph database schema: node labels, properties, and relationship types.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `context` | string | | Named imported context |

---

### `graphit_ast_install`

**Description:** Import another local repository's code graph as a named context.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `path` | string | ✅ | Absolute path to the source project to import |
| `context` | string | ✅ | Name of the context to assign to the imported project |
| `reset` | boolean | | Reset the context database before importing |
| `workers` | integer | | Number of parallel worker threads |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_ast_remove`

**Description:** Remove an imported context or clear the main project code graph.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `context` | string | | Name of the imported context to remove. If empty, clears the main project graph. |

---

### `graphit_ast_list`

**Description:** List all imported AST contexts and their repository paths.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_ast_source`

**Description:** Retrieve source code from the indexed code graph with support for head/tail, line ranges, entity extraction, and pattern search with context.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `path` | string | ✅ | Relative path to the file |
| `context` | string | | Named imported context where the file resides |
| `entity` | string | | Entity name (function, class, etc.) to extract source using its line range from the graph |
| `entity_type` | string | | Entity type for disambiguation: Function, Class, Method, Struct, etc. |
| `head` | integer | | Show only the first N lines |
| `tail` | integer | | Show only the last N lines |
| `start_line` | integer | | Start line number (1-indexed) |
| `end_line` | integer | | End line number (1-indexed, inclusive) |
| `pattern` | string | | Search for a pattern (literal text or regex if `regex=true`) |
| `regex` | boolean | | Treat pattern as a regular expression |
| `before` | integer | | Number of context lines before each pattern match |
| `after` | integer | | Number of context lines after each pattern match |
| `line_numbers` | boolean | | Include line numbers in the output |

---

### `graphit_ast_export`

**Description:** Export the AST database to Obsidian markdown format or an archive bundle.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `format` | string | ✅ | Export format: `obsidian` or `bundle` |
| `output` | string | ✅ | Output directory path where files will be exported |
| `no_sources` | boolean | | Do not include file source contents in bundle |

---

### `graphit_ast_embed`

**Description:** Run embedding cycle to precompute or update semantic embeddings.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `context` | string | | Named imported context |

---

### `graphit_ast_search`

**Description:** Hybrid search combining BM25 full-text and semantic vector search with Reciprocal Rank Fusion (RRF). Supports three modes: hybrid (default, best results), fts (keyword only), semantic (vector only).

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `query` | string | ✅ | Search query (keywords, natural language, or code identifiers) |
| `top_k` | integer | | Maximum number of results (default: 15) |
| `mode` | string | | Search mode: `hybrid` (default), `fts` (BM25 only), `semantic` (vector only) |
| `context` | string | | Named imported context to search |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

## Knowledge Tools

Tools for indexing, querying, and managing the project documentation knowledge graph.

### `graphit_knowledge_index`

**Description:** Index `docs/` into the knowledge graph and regenerate the wiki.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory to index |
| `path` | string | | Target path to index (defaults to `docs/`) |
| `workers` | integer | | Number of parallel worker threads |
| `reset` | boolean | | Clear graph and re-index from scratch |
| `louvain` | boolean | | Use Louvain community detection |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_knowledge_query`

**Description:** Search the project knowledge wiki using AI-powered retrieval.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `query` | string | ✅ | Natural language question to search the project knowledge wiki |
| `context` | string | | Named imported context to search instead of the default project |

---

### `graphit_knowledge_search`

**Description:** Search the project knowledge wiki using BM25 keyword ranking.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `query` | string | ✅ | Keywords to search for in the knowledge wiki using BM25 |
| `top_k` | integer | | Maximum number of results (0 = no limit) |
| `context` | string | | Named imported context to search |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_knowledge_schema`

**Description:** Show the knowledge graph schema and node properties.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `context` | string | | Named imported context |

---

### `graphit_knowledge_lint`

**Description:** Audit the knowledge wiki for structural issues.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `deep` | boolean | | Enable AI-assisted contradiction detection |
| `fix` | boolean | | Auto-repair fixable issues (backlinks) |
| `stale_days` | integer | | Mark pages older than N days as stale |
| `context` | string | | Lint an imported context by name |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_knowledge_install`

**Description:** Import an external knowledge context from the hub.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `name` | string | ✅ | Name of the knowledge context to import from the hub |

---

### `graphit_knowledge_remove`

**Description:** Remove the project knowledge graph or an imported context.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `context` | string | | Name of the imported context to remove. If empty, clears local project knowledge wiki. |

---

### `graphit_knowledge_sync`

**Description:** Re-sync an imported context from the global cache or rebuild local wiki.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `context` | string | | Sync a specific imported context by name. If empty, syncs local `docs/` index. |

---

### `graphit_knowledge_export`

**Description:** Export the project knowledge wiki and graph to the hub.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |

---

### `graphit_knowledge_list`

**Description:** List all articles in the local knowledge wiki.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

## Memory Tools

Tools for managing the project and user persistent memory store.

### `graphit_memory_insert`

**Description:** Add a new memory to the project or user memory store.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `title` | string | ✅ | Memory title |
| `content` | string | ✅ | Detailed memory content |
| `type` | string | | Memory type: `convention`, `correction`, `decision`, `tension`, `fact`, or `skill` |
| `scope` | string | | Scope: `project` (default) or `user` |
| `link_project` | boolean | | Link user memory to project identity |
| `important` | boolean | | Mark as important |
| `tags` | string | | Comma-separated tags |

---

### `graphit_memory_update`

**Description:** Update the title or content of an existing memory.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `id` | string | ✅ | Memory ID to update |
| `content` | string | | New content |
| `title` | string | | New title |
| `scope` | string | | Scope: `project` (default) or `user` |

---

### `graphit_memory_delete`

**Description:** Delete a memory entry by ID.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `id` | string | ✅ | Memory ID to delete |
| `scope` | string | | Scope: `project` (default) or `user` |

---

### `graphit_memory_list`

**Description:** List all memories in the project or user store.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `scope` | string | | Scope: `project` (default) or `user` |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_memory_search`

**Description:** Search for text matching in raw memory files.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `query` | string | ✅ | Text query to search |
| `scope` | string | | Scope: `project` (default) or `user` |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_memory_query`

**Description:** Search memories with AI Consultation and return a synthesized response.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `query` | string | ✅ | Natural language query |
| `scope` | string | | Scope: `project` (default) or `user` |
| `context` | string | | Named imported context |

---

### `graphit_memory_important`

**Description:** List all memories marked as important.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `scope` | string | | Scope: `project` (default) or `user` |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_memory_promote`

**Description:** Promote a memory to important status.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `id` | string | ✅ | Memory ID to promote |
| `scope` | string | | Scope: `project` (default) or `user` |

---

### `graphit_memory_demote`

**Description:** Demote a memory from important status.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `id` | string | ✅ | Memory ID to demote |
| `scope` | string | | Scope: `project` (default) or `user` |

---

### `graphit_memory_consolidate`

**Description:** Analyze memory wiki for staleness, duplicates, contradictions, and suggestions.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `scope` | string | | Scope: `project` (default) or `user` |
| `apply` | boolean | | Apply proposed changes |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_memory_gc`

**Description:** Garbage collect inactive or stale memories.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `scope` | string | | Scope: `project` (default) or `user` |
| `dry_run` | boolean | | Only scan, do not delete |
| `stale_days` | integer | | Days of inactivity before memory is stale |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_memory_index`

**Description:** Regenerate the semantic wiki index of the memory store.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `scope` | string | | Scope: `project` (default) or `user` |

---

### `graphit_memory_export`

**Description:** Index and sync project memories back to the local git repository.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |

---

### `graphit_memory_schema`

**Description:** Show the memory graph database schema details.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |

**Returns:** Text describing node labels (`Document`, `Section`), edge labels (`REFERENCES`, `CONTAINS`), and their properties.

---

### `graphit_memory_remove`

**Description:** Remove a memory context sync connection.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `context` | string | ✅ | Named context to remove |

---

### `graphit_memory_sync`

**Description:** Sync memories from an external context.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `context` | string | ✅ | Named context to sync |

---

## Hub Tools

Tools for interacting with the Graphit Hub artifact registry.

### `graphit_hub_list`

**Description:** List available artifacts in the Graphit Hub registry.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `type` | string | | Filter by artifact type: `knowledge`, `ast`, `rule`, `skill`, `command`, `agent`, `mcp`, `power` |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_hub_search`

**Description:** Search the Graphit Hub registry for artifacts by name, ID, or description.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | ✅ | Search term to find artifacts |
| `type` | string | | Filter by artifact type |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_hub_show`

**Description:** Show detailed information about a specific artifact in the Graphit Hub.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `id` | string | ✅ | Artifact ID to show details for |
| `type` | string | | Artifact type (helps disambiguate) |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_hub_install`

**Description:** Install an artifact from the Graphit Hub into the current project.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `id` | string | ✅ | Artifact ID to install. Supports `@version` suffix for version pinning |
| `type` | string | | Artifact type |
| `ide` | string | | Target IDE (claude, cursor, gemini, etc.) |
| `alias` | string | | Alias to assign to installed artifact |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_hub_uninstall`

**Description:** Remove an installed artifact from the current project.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `id` | string | ✅ | Artifact ID to uninstall |
| `type` | string | | Artifact type |
| `ide` | string | | Target IDE |

---

### `graphit_hub_update`

**Description:** Update installed hub artifacts in the current project.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `id` | string | | Artifact ID to update. If omitted, updates all artifacts |
| `type` | string | | Artifact type |
| `ide` | string | | Target IDE |

---

### `graphit_hub_submit`

**Description:** Publish a local artifact to the hub.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `id` | string | ✅ | Artifact ID to publish |
| `local_path` | string | ✅ | Local directory path to artifact source |
| `version` | string | | Artifact version (defaults to `1.0.0`) |
| `name` | string | | Display name override |
| `description` | string | | Detailed description |
| `type` | string | | Artifact type (defaults to `rule`) |
| `tags` | string | | Comma-separated tags |

---

### `graphit_hub_link`

**Description:** Link a local project's artifacts into the current project via symlinks.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `name` | string | ✅ | Name of the linked artifact |
| `source_path` | string | ✅ | Path to local source project to link |
| `type` | string | ✅ | Artifact type: `ast`, `knowledge`, `rule`, `skill`, `command`, `agent`, `mcp` |
| `ide` | string | | Target IDE |

---

### `graphit_hub_unlink`

**Description:** Remove a linked artifact from the current project.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `name` | string | ✅ | Name of linked artifact to remove |
| `type` | string | ✅ | Artifact type |
| `ide` | string | | Target IDE |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_hub_projects`

**Description:** List registered projects in the global lock.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

## Wiki Tools

Tools for multi-source wiki search with AI-powered retrieval and chat sessions.

### `graphit_wiki_search`

**Description:** Search across multiple wiki sources using AI-powered retrieval.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | ✅ | Natural language question to search across multiple wikis |
| `wikis` | array[string] | | Wiki sources to search (`project`, `memory`, or project IDs from ecosystem) |
| `hub_refs` | array[string] | | Hub knowledge artifact references to include (format: `artifact-id@version`) |
| `session_id` | string | | Session ID to continue an existing conversation |
| `top_k` | integer | | BM25 results per wiki source (0 = no limit) |
| `project_dir` | string | ✅ | Project directory |
| `ai_optimized` | boolean | | Optimize output for AI context — returns compact, token-efficient format (TOON) instead of raw JSON |

> **Tip:** Always set `ai_optimized: true` for AI agent usage. This returns results in a compact, token-efficient format (TOON) rather than raw JSON.

**Returns:** AI-synthesized answer with a session ID for follow-up chat.

---

### `graphit_wiki_browse`

**Description:** Browse wiki documents in a structured format. Lists chunks/documents from the WikiDB with optional filtering by type.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `wiki` | string | | Wiki scope: `project`, `memory` (default: `project`) |
| `doc_type` | string | | Filter by document type (e.g., `specification`, `architecture`, `decision`) |
| `limit` | integer | | Max results (default: 100) |
| `ai_optimized` | boolean | | Optimize output for AI context — returns compact, token-efficient format (TOON) instead of raw JSON |

> **Tip:** Always set `ai_optimized: true` for AI agent usage.

---

### `graphit_wiki_xrefs`

**Description:** Show cross-references for a wiki entity. Returns inbound and outbound references with configurable graph traversal depth.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `query` | string | ✅ | Entity slug or name to find cross-references for |
| `wiki` | string | | Wiki scope: `project`, `memory` (default: `project`) |
| `depth` | integer | | Depth of graph traversal (default: 1, max: 3) |
| `ai_optimized` | boolean | | Optimize output for AI context — returns compact, token-efficient format (TOON) instead of raw JSON |

> **Tip:** Always set `ai_optimized: true` for AI agent usage.

---

### `graphit_wiki_log`

**Description:** Show wiki sync history. Returns a timeline of sync operations showing what was added, updated, and deleted.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `wiki` | string | | Wiki scope: `project`, `memory` (default: `project`) |
| `limit` | integer | | Max log entries (default: 10) |
| `ai_optimized` | boolean | | Optimize output for AI context — returns compact, token-efficient format (TOON) instead of raw JSON |

---

### `graphit_wiki_embed`

**Description:** Run embedding cycle to precompute or update semantic embeddings for wiki documents.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `wiki` | string | | Wiki scope: `project`, `memory` (default: `project`) |

---

### `graphit_wiki_chat`

**Description:** Continue a wiki chat session started by `graphit_wiki_search`.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `session_id` | string | ✅ | Chat session ID to continue |
| `message` | string | ✅ | User message to send |

---

### `graphit_wiki_sessions`

**Description:** List or delete wiki chat sessions.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `action` | string | ✅ | Action: `list` or `delete` |
| `session_id` | string | | Session ID for delete action |
| `project_dir` | string | ✅ | Project directory for listing |

---

## Dream Tools

Tools for managing the autonomous dream module — skill generation and knowledge mining during idle periods.

### `graphit_dream_status`

**Description:** Show status and configuration of the dream module.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

**Returns:** JSON with fields:
- `enabled` — whether dreaming is active
- `daemon_running` — whether the background daemon is running
- `status` — current status: `dreaming`, `deep sleep`, `standby`, or `inactive`
- `idle_timeout` / `max_duration` — dream timing configuration
- `total_reports` — number of dream session reports
- `pending_subjects` — queued dream subjects

---

### `graphit_dream_reports`

**Description:** List dream session reports.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `all` | boolean | | Show all reports (not just new ones) |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_dream_subject_list`

**Description:** List dream subjects — instructions left for future dream sessions.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_dream_subject_add`

**Description:** Add a new dream subject for a future dream session.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `title` | string | ✅ | Subject title/description |
| `body` | string | | Detailed instructions for the dream agent |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_dream_subject_remove`

**Description:** Remove a dream subject by slug.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `slug` | string | ✅ | Subject slug to remove |

---

## Daemon Tools

Tools for managing the global background daemon process.

### `graphit_daemon_status`

**Description:** Check status of the global background daemon process.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

**Returns:** JSON with fields:
- `pid` — process ID
- `running` — whether the daemon is alive
- `started_at` / `uptime_seconds` — uptime information
- `pid_file_path` — path to the PID file
- `scheduler_status` — OS scheduler status (systemd, launchd, or schtasks)
- `recent_logs` — last 10 lines from the daemon log

---

### `graphit_daemon_stop`

**Description:** Stop the running global daemon process.

_No parameters._

Sends `SIGTERM` first and waits up to 10 seconds. Falls back to `SIGKILL` if the daemon does not stop gracefully.

---

## Cluster Tools

Tools for managing project cluster labels in the Graphit ecosystem.

### `graphit_cluster_set`

**Description:** Set a cluster label for grouping the project in the ecosystem.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `key` | string | ✅ | Cluster label key |
| `value` | string | ✅ | Cluster label value |

---

### `graphit_cluster_get`

**Description:** Get a specific cluster label value, or all cluster labels set on the project.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `key` | string | | Cluster label key to retrieve. If empty, retrieves all labels. |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

### `graphit_cluster_unset`

**Description:** Remove a cluster label from the project.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `key` | string | ✅ | Cluster label key to remove |

---

### `graphit_cluster_projects`

**Description:** List all projects in the same cluster as the current project (including itself). Optionally filter by a specific cluster label key.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_dir` | string | ✅ | Project directory |
| `label` | string | | Optional cluster label key to filter by |
| `ai_optimized` | boolean | | Set to `true` for compact TOON output instead of JSON |

---

## Improvements Tools

Tools for code improvement analysis and methodology rules.

### `graphit_improvements_rules`

**Description:** Output the resolved code improvement analysis methodology rules.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `default` | boolean | | Return compiled-in default rules ignoring any customization |

---

## Common Parameters

Most tools accept the following common parameters:

| Parameter | Description |
|-----------|-------------|
| `project_dir` | **Required on nearly every tool.** Absolute path to the project directory. The MCP server uses this to resolve the project lockfile, configuration, and local data stores. |
| `scope` | Used by Memory tools. Either `project` (default) or `user`. Controls which memory store is targeted. |
| `context` | Used by AST, Knowledge, and Memory tools. Names an imported context (external project) to operate on instead of the default project. |
| `ide` | Target IDE adapter (e.g., `claude`, `cursor`, `gemini`, `windsurf`). Affects rule installation format. |
| `ai_optimized` | Available on all tools that return structured JSON data. Set to `true` to receive output in compact TOON (Token-Optimized Object Notation) format instead of verbose JSON. Reduces token consumption by ~60-80%. |

## Error Handling

All MCP tools follow a consistent error pattern:
- On success, tools return a `CallToolResult` with `TextContent` or JSON-serialized content.
- On error, tools return a Go error that is propagated as an MCP error response.
- Every tool handler is wrapped in `safeTool()`, which adds panic recovery and automatic daemon autostart.
