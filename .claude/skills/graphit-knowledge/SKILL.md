---
name: graphit-knowledge
description: 'Knowledge-first: project documentation, wiki retrieval, task logs, architecture, decisions, specifications, provenance, and backlog; use wiki tools before reading documentation files.'
---

# Graphit Knowledge

Use this skill for project documentation, architecture, decisions, specifications, task logs, backlog, provenance, or another project's wiki. Code-structure questions belong to Graphit AST; external systems must be resolved through Graphit Hub first.

## Task log

For implementation work, create `docs/tasks/<task>.md` before the first task action. Record objective, reasoning, approach, task breakdown, acceptance criteria, affected files, trade-offs, debt, and system knowledge. Update it when a step lands, direction changes, a blocker appears, and after every code change. On resume, read the existing log before acting. The log must let another agent continue without conversation history.

Do not manufacture a task log for a read-only factual answer. If the request becomes an implementation task, open the log at that transition.

## Reading knowledge

1. Search with `graphit_knowledge_search` for the current project or `graphit_wiki_search` across selected wikis. Search returns titles, not evidence.
2. Pick the smallest relevant set and read it with `graphit_wiki_source`; use a pattern or line slice for long pages. Use `preview: true` only when titles cannot disambiguate.
3. Use `graphit_wiki_xrefs` for provenance/relationships, `graphit_wiki_log` for change history, and `graphit_wiki_browse` or `graphit_knowledge_list` for catalogues.

A different project's documentation is queried with its returned `project_dir`; never walk or grep its docs tree. A missing result is not proof that a page is absent—use the catalogue or refine once before concluding.

## Maintenance

The daemon indexes `docs/` after writes. Use `graphit_knowledge_sync` only for knowledge-only freshness and `graphit_sync` when all module indexes must be aligned. Check `graphit_daemon_status` on stale/locked reads. `graphit_knowledge_lint`, `schema`, `export`, `install`, and `remove` are administrative operations used only when the task calls for them; `graphit_wiki_embed` repairs semantic coverage.

Use `graphit_backlog_list`, `add`, and `remove` for deferred work. Backlog is independent of Dream state; do not smuggle future work into the current change.

Tool index: `graphit_knowledge_search`, `graphit_wiki_search`, `graphit_wiki_browse`, `graphit_wiki_xrefs`, `graphit_wiki_log`, `graphit_wiki_source`, `graphit_wiki_embed`, `graphit_knowledge_list`, `graphit_knowledge_lint`, `graphit_knowledge_schema`, `graphit_knowledge_export`, `graphit_knowledge_install`, `graphit_knowledge_remove`, `graphit_knowledge_sync`, `graphit_backlog_list`, `graphit_backlog_add`, `graphit_backlog_remove`, `graphit_cluster_projects`, `graphit_daemon_status`, `graphit_daemon_stop`, `graphit_sync`.
