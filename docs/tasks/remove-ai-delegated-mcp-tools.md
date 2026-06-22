---
title: Remove AI-Delegated MCP Tools and Teach Agent Self-Sufficiency
status: done
created: 2026-06-22
updated: 2026-06-22
tags: [mcp, agent, ai, refactor]
---

# Remove AI-Delegated MCP Tools

## Objective

Remove MCP tools that delegate generative AI reasoning to a sub-AI via CLI. The calling agent (Claude, Gemini, etc.) IS an AI — delegating reasoning to another AI via `ai.NewClientFromConfig()` is redundant and wasteful. Instead, teach the agent to achieve the same results by combining the non-AI tools (search → read → synthesize).

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/mcpstdio/tools_ast.go` | Removed `ast_query_ai` tool + `astAIQueryInput` struct | Delegates Cypher generation to sub-AI; agent can write Cypher directly |
| `internal/mcpstdio/tools_knowledge.go` | Removed `knowledge_query` tool + struct; removed `ai` import | Delegates wiki consultation to sub-AI; agent can search+read+synthesize |
| `internal/mcpstdio/tools_memory.go` | Removed `memory_query` + `memory_consolidate` tools + structs; removed `ai`/`wiki` imports | Same reasoning; consolidation is agent-level analysis |
| `internal/mcpstdio/tools_wiki.go` | Removed `wiki_chat` + `wiki_sessions` tools + structs; removed `chat`/`wikisvc` imports; updated `wiki_search` description | Chat delegation removed; wiki_search kept (BM25/hybrid, no gen-AI) |
| `internal/memory/rule.go` | Removed refs to `memory_query`/`memory_consolidate`; added agentic workflow instructions | Teaches agent: search → browse → follow wikilinks → synthesize |
| `internal/knowledge/rule.go` | Removed refs to `knowledge_query`/`wiki_chat`; added agentic workflow instructions | Same pattern: search → read pages → xrefs → synthesize yourself |

## Key Decisions

- **MCP-only removal**: CLI commands kept for human terminal use (e.g., `graphit wiki chat` REPL). Only the MCP agent-facing tools were removed.
- **Embedding tools kept**: `ast_embed`, `ast_search` (semantic), `wiki_embed`, `wiki_search` (semantic) use embedding vectors — pre-computation, not AI reasoning delegation.
- **wiki_search kept**: Despite misleading "AI-powered" description, it's actually BM25/FTS/hybrid search. Description updated to "BM25 full-text and optional semantic search."
- **memory_gc kept**: Pure Go staleness detection, no generative AI.
- **knowledge_lint kept**: Deep AI flag declared but never implemented; no actual AI dependency.

## Tools Removed (6)

| Tool | Internal AI Usage |
|---|---|
| `graphit_ast_query_ai` | `ai.GenerateAICypher()` — natural language to Cypher |
| `graphit_knowledge_query` | `wiki.SearchWiki()` with `ai.NewClientFromConfig()` — multi-turn AI |
| `graphit_memory_query` | `wiki.SearchWiki()` with `ai.NewClientFromConfig()` — multi-turn AI |
| `graphit_wiki_chat` | `chat.ChatEngine` with `aiClient` — interactive AI chat |
| `graphit_wiki_sessions` | Session management for `wiki_chat` |
| `graphit_memory_consolidate` | `memory.RunConsolidation()` with `aiClient` — AI duplicate detection |

## Tools Kept (embedding-only)

| Tool | Usage |
|---|---|
| `graphit_ast_embed` | `ai.NewEmbeddingClientFromConfig()` — vector pre-computation |
| `graphit_ast_search` | Hybrid BM25 + pre-computed embeddings |
| `graphit_wiki_embed` | `ai.NewEmbeddingClientFromConfig()` — vector pre-computation |
| `graphit_wiki_search` | BM25 FTS + optional semantic (embeddings) |

## Notes

- All 26 lint warnings in CI output are pre-existing UI warnings, unrelated to this change
- Zero remaining references to removed tools across `internal/` and `cmd/`
- `go build ./internal/mcpstdio/...`, `go build ./internal/memory/`, `go build ./internal/knowledge/` all pass

## Progress Log

### 2026-06-22
- Identified all 6 AI-delegated MCP tools via source analysis
- Removed tools from 4 MCP server files
- Updated 2 rule.go files with agentic workflow equivalents
- Verified with `make ci` — 0 errors
