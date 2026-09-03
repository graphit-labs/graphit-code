---
name: graphit-knowledge
description: 'Knowledge-first: project documentation, wiki retrieval, architecture, decisions, specifications, and provenance; use wiki tools before reading documentation files.'
---

# Graphit Knowledge

Use this skill for project documentation, architecture, decisions, specifications, provenance, or another project's wiki. Task lifecycle and backlog belong to Graphit Task; code structure belongs to Graphit AST; external systems must be resolved through Graphit Hub first.

## Reading knowledge

1. Search with `graphit_knowledge_search` for the current project or `graphit_wiki_search` across selected wikis. Search returns titles, not evidence.
2. Pick the smallest relevant set and read it with `graphit_wiki_source`; use a pattern or line slice for long pages. Use `preview: true` only when titles cannot disambiguate.
3. Use `graphit_wiki_xrefs` for provenance/relationships, `graphit_wiki_log` for change history, and `graphit_wiki_browse` or `graphit_knowledge_list` for catalogues.

A different project's documentation is queried with its returned `project_dir`; never walk or grep its docs tree. A missing result is not proof that a page is absent—use the catalogue or refine once before concluding.

## Maintenance

The daemon indexes `docs/` after writes. Use `graphit_knowledge_sync` only for knowledge-only freshness and `graphit_sync` when all module indexes must be aligned. Check `graphit_daemon_status` on stale/locked reads. `graphit_knowledge_lint`, `schema`, `export`, `install`, and `remove` are administrative operations used only when the task calls for them; `graphit_wiki_embed` repairs semantic coverage.

Tool index: `graphit_knowledge_search`, `graphit_wiki_search`, `graphit_wiki_browse`, `graphit_wiki_xrefs`, `graphit_wiki_log`, `graphit_wiki_source`, `graphit_wiki_embed`, `graphit_knowledge_list`, `graphit_knowledge_lint`, `graphit_knowledge_schema`, `graphit_knowledge_export`, `graphit_knowledge_install`, `graphit_knowledge_remove`, `graphit_knowledge_sync`, `graphit_cluster_projects`, `graphit_daemon_status`, `graphit_daemon_stop`, `graphit_sync`.
