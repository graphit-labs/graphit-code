---
title: "Persisted record relationships"
description: "Typed relationships across Task, Session, Memory and Knowledge, including authoring, queries and recovery."
content-type: reference
audience: developers, ai-agents
---

# Persisted record relationships

Graphit stores relationships between engineering records so agents and engineers can inspect provenance, dependencies and reuse across tasks, sessions, memories and knowledge pages. A relationship is data in the database. Writing an identifier in a description or rendering a Markdown link does not create one.

## Write explicit references

When a record relies on another record, the agent must resolve its real ID and send `references` in the write input. The source is the record being created or updated. Each reference contains:

| Field | Meaning |
| --- | --- |
| `target.type` | `task`, `session`, `memory` or `knowledge` |
| `target.id` | Actual Task/Session/Memory ID or Knowledge page slug |
| `target.scope` | `project` or `user`; defaults to the source scope |
| `target.scope_id` | Logical project or user identity; defaults only within the source scope |
| `target.context` | Knowledge context; inherits only within the same source namespace |
| `relation` | Semantic relation, for example `supports`, `implements`, `derived_from` or `relates_to` |
| `field` | Optional provenance within the source record |

Illustrative fragment for a Task write, after resolving the memory ID:

```json
{
  "references": [
    {
      "target": {"type": "memory", "id": "<verified-memory-id>"},
      "relation": "supports"
    }
  ]
}
```

Send the complete intended explicit list when changing relations. Omit `references` to preserve existing links; send `references: []` to clear explicit links. Read the current list before replacing it. Structural Task links (`session_id`, `parent_id`, `depends_on`) remain managed by their existing fields and are not cleared by an empty explicit list.

Task create/revise/progress/comment/check/completion and related text mutations, Session create/revise/checkpoint/completion and related text mutations, Memory insert/update, and Task batch operations accept structured references. The Memory HTTP create/update endpoints accept the same field. Existing claim, revision and scope checks still apply. A relationship never grants access to its target.

For Knowledge, put the same structure in the authoritative Markdown's existing YAML frontmatter:

```yaml
references:
  - target:
      type: memory
      id: <verified-memory-id>
      scope: project
      scope_id: <logical-project-id>
    relation: derived_from
```

Indexing validates the complete batch before changing the compiled corpus. Invalid types, missing IDs and a non-list `references` value fail validation. Omission preserves already indexed authored references; `references: []` clears them. Keep references in the source for reproducible indexing into a new database. Existing wiki cross-links are persisted separately as `wiki-link` relations with `field: wiki-links`.

## Read and analyze

`graphit_references_query` reads current persisted edges from the authorized project's Task, Memory and Knowledge stores. It supports `source_type`/`source_id`, `target_type`/`target_id`, `relation`, source `scope`/`scope_id`/`context`, and cursor pagination. For backlinks, filter the target; `target_scope`, `target_scope_id` and `target_context` disambiguate its namespace. `include_user: true` also reads the current authenticated caller's personal memory; it is false by default. Imported Knowledge comes only from the project's registered contexts.

The result contains `relations.results`, optional `relations.next_cursor`, `complete` and `warnings`. Qualify identities when different scopes or contexts can contain the same ID. A stored target may be unavailable or outside the current read scope; its identity remains usable for analysis. The UI only attaches a navigation URL after resolving an exact authorized target. It shows outgoing references and incoming backlinks, and marks unresolved targets without inventing a local link.

The UI endpoint is `GET /api/references?project_dir=...&kind=task&id=...`, with optional `scope` and `context`. It reads stored relationships; opening or refreshing a viewer does not create them. Knowledge targets use the selected context and a `page` query parameter to open the actual page.

## Storage and recovery

`record_relations` uses the same schema in each source module's authorized LanceDB store. Each row records typed source and target identities, scope/context, relation, field, origin, source revision and source snapshot hash. Knowledge's containing store supplies its project/import context namespace when composing queries; the index remains portable.

A complete immutable generation, including an empty-generation marker, is merged in one relation-table commit. Readers match generations against authoritative current heads before choosing current edges. Older replayed generations cannot restore cleared links. Task and Session carry explicit reference lists in their existing durable snapshots. Memory writes the relation generation before committing the matching head; Knowledge validates metadata before corpus changes. These are separate table commits, not a cross-table transaction. Failed or missing projections are reported as incomplete instead of silently presenting stale edges.

`graphit_references_reconcile` replays known structured metadata and existing structural links in writable project stores; `include_user` optionally includes the caller's memory. It does not infer references from prose, alter record bodies, or rewrite imported Knowledge. It reports module failures individually and can be repeated. Normal Knowledge indexing also repairs missing projections on unchanged-content paths. Memory reconciliation writes only relation projections, so it cannot replace a concurrently edited memory head.

Existing records without explicit references retain their content. Task structural links and existing wiki cross-links can be projected; arbitrary mentions require an explicit author update. Historic relation generations remain in the database, while the query and UI above return current edges. Deletion removes the source from current reads; references from other records can remain as unresolved provenance.

Native Knowledge packages carry their indexed relationship metadata along with their corpus. They do not include the referenced Task or Memory record bodies. A target remains subject to the consumer's own access and namespace resolution.

## Implementation and validation

The storage contract lives in `internal/relations`; authorized composition and filtering live in `internal/references`. Producers are Task, Memory and Wiki services. MCP tools and UI handlers share those services. Agent skill and lifecycle mandate generators include the explicit write contract, examples and query route.

Native storage tests cover reopens, omission versus clearing, stale-generation replay, invalid metadata, Memory body edits, Knowledge reconciliation and namespace isolation. MCP schema tests assert the nested typed write contract and agent instructions; UI/API tests verify exact authorized navigation and backlinks.
