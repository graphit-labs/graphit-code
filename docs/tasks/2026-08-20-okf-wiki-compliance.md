---
title: Wiki 100% OKF (Open Knowledge Format) Compliance
status: done
created: 2026-08-20
updated: 2026-08-20
tags: [wiki, knowledge, memory, okf, refactor, mcp]
---

# Wiki 100% OKF (Open Knowledge Format) Compliance

## Objective
Refactor the Graphit wiki generator (for knowledge wiki, memory wiki, index.md, and log.md) to strictly adhere to the Open Knowledge Format (OKF v0.2) open specification published by Google Cloud. Replace Obsidian/Karpathy `[[wikilinks]]` and non-standard frontmatter with OKF frontmatter (`type`, `generated.at`, `sources`, `description`, etc.) and standard Markdown graph links (`[Title](slug.md)`). Maintain 100% functionality for SQLite database indexing (`wiki.db`) and MCP search/retrieval tools (`graphit_knowledge_search`, `graphit_memory_search`, `graphit_wiki_search`, `graphit_wiki_source`, `graphit_wiki_browse`, `graphit_wiki_xrefs`, `graphit_wiki_log`).

## Plan & Task Breakdown
- [x] **T1 — Update OKF Frontmatter & Markdown Generator (`internal/knowledge/wiki.go`)**: Implemented `type`, `generated.at`, `sources`, `description`, `id`, `tags` in `knowledgeEntityPage()`, `knowledgeIndexPage()`, and `appendKnowledgeLog()`.
- [x] **T2 — Update OKF Frontmatter in Memory Generator (`internal/memory/wiki.go`)**: Updated `memoryEntityPageWithHash()` to emit OKF frontmatter (`type`, `generated.at`, `sources`, `id`, `description`, `tags`).
- [x] **T3 — Dual Link Parsing & Standard Link Generation (`internal/wiki/crossref.go`, `resolve.go`, `autolink.go`)**: Supported extracting cross-references and resolving both OKF standard links `[label](target.md)` and legacy `[[wikilinks]]`.
- [x] **T4 — Database Indexing & MCP Compatibility (`internal/wiki/store.go`, `fts.go`, `search.go`, `internal/mcpstdio/`)**: Guaranteed `WikiDB` indexes OKF frontmatter fields and all MCP search/navigation tools function seamlessly.
- [x] **T5 — Frontend UI Link Parsing (`internal/ui/src/components/wiki/WikiMarkdown.tsx`)**: Supported rendering standard OKF markdown links as `wiki://` protocol in UI.
- [x] **T6 — Test Verification & Validation**: Ran full non-cached test suite (`go test -count=1 -tags fts5 ...`) across all internal packages with 100% pass rate.

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/knowledge/wiki.go` | Modified | Updated knowledge wiki pages, index.md and log.md to OKF v0.2 format |
| `internal/memory/wiki.go` | Modified | Updated memory wiki pages to OKF v0.2 format |
| `internal/wiki/crossref.go` | Modified | Supported standard Markdown links alongside wikilinks in crossref extraction and backlink injection |
| `internal/wiki/resolve.go` | Modified | Resolved wikilinks and Markdown links to canonical slugs |
| `internal/wiki/autolink.go` | Modified | Generated standard OKF Markdown links `[label](slug.md)` |
| `internal/ui/src/components/wiki/WikiMarkdown.tsx` | Modified | Rendered standard Markdown links to `wiki://` protocol in UI |
| `internal/knowledge/knowledge_test.go` | Modified | Updated test assertions for OKF links |
| `internal/knowledge/knowledge_coverage_test.go` | Modified | Updated test assertions for OKF frontmatter and links |
| `internal/memory/memory_test.go` | Modified | Updated test assertions for OKF frontmatter |
| `internal/wiki/crossref_test.go` | Modified | Updated test assertions for OKF links |

## Key Decisions
- **OKF v0.2 Standard Compliance**: Adopted mandatory `type` field, `generated.at` (ISO 8601), `sources` array, `id`, and `description` in YAML frontmatter.
- **Dual Link Resolution**: Generated standard Markdown links `[Title](slug.md)` while maintaining backward-compatible parsing for any legacy `[[wikilinks]]` in raw user source files.
- **Full MCP Tool Support**: Kept `WikiDB` FTS indexing and MCP tools intact so AI agents can query and retrieve source exclusively via MCP tools.
