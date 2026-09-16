---
name: graphit-remote
description: Use a projectless Graphit MCP server to retrieve versioned code, knowledge, shared skills, and durable user memory without assuming access to a source checkout.
---

# Graphit Remote

Use Graphit as the authoritative context and coordination layer when its MCP tools are available.

## Bootstrap

If the host has not supplied Graphit mandates, call `graphit_mandates` once with no arguments.
Match the next action against those triggers. Immediately before the first matching action, read
that module through `graphit_module_skill`; reuse instructions still retained for this scope.
Reload only when lost after compaction or when scope/overrides change. Do not preload other modules
or pass `project_dir` on an artifact-only server.

The core module names are `task`, `memory`, `ast`, `hub`, and `knowledge`. The returned `enabled`
field reflects the server's resolved module configuration. If it is `false`, do not assume that
module is available merely because its skill source was returned.

## Recall when questions arise during work

At any stage, a question about system behavior, rationale or earlier work may need Memory facts
and lessons or Task investigations, decisions and validation results. Read the matching module
skill and retrieve only the missing topic: known ID directly to source/get, otherwise focused
search followed by selected records. Do not wait for restart, a new plan or a blocker. Reuse
sufficient context, and compare historical findings with current AST/Knowledge evidence.
These rules do not make unavailable scopes available: an artifact-only server still has no
project Task/Memory; use the supported sources and report a needed history gap.

## Load detailed examples at their named boundary

The main skill response includes `references`, a list of exact resource paths. Follow the skill's
reading trigger: planning a delivery, writing domain documentation, investigating code impact or
handling an unfamiliar discovery/correction flow may need a detailed guide. Do not load every
reference simply because it is listed. On an artifact-only server, for example:

```json
{"module":"task"}
```

When the returned Task skill calls for its planning reference, use the exact returned path:

```json
{"module":"task","reference":"references/planning.md"}
```

Both payloads call `graphit_module_skill`. The second response contains the selected reference in
`content`, with its path in `reference`; it does not repeat the skill or load other examples.
Keep `project_dir` on both calls only when addressing a real project resolved on the current server.
It is a runtime argument: never save that server's checkout root in shared docs, tasks, memories or
handoffs. Save project identity, relative paths/slugs and revision; resolve the root again per host. Read Graphit's built-in
references through this tool, not as Hub artifacts or by opening server filesystem paths. Reuse
them until lost or the relevant scope/instructions change. Skill overrides still determine the main
instructions; the listed references are framework resources and do not override project guidance.

## Address remote content

An artifact-only server has no project checkout and no meaningful local project path. Never invent,
infer, or send `project_dir` in this mode.

Discover content with `graphit_hub_search`, `graphit_hub_list`, and `graphit_hub_show`. An
unqualified ID may be used during discovery, but after selecting an artifact resolve its version and
refer to it as `id@version` on every read, install, query, and handoff. This preserves provenance and
makes results reproducible when the Hub's latest version changes.

- Pass `id@version` as `context` to AST and Knowledge tools.
- Pass `id@version` in `hub_refs` when searching Hub knowledge with `graphit_wiki_search`.
- Read AST source with `graphit_ast_source` and Knowledge source with `graphit_wiki_source`.
- Read installed Hub `skill`, `rule`, `command`, or `agent` files with `graphit_hub_content`, using
  `id: "id@version"` and the selected artifact-relative `path` (for a skill, start at `SKILL.md`).
  Omitting `path` returns every file; use that only when the whole artifact is required.
- Read Graphit's own core module instructions with `graphit_module_skill`, not
  `graphit_hub_content`.

Install a missing artifact globally by calling `graphit_hub_install` without `project_dir` and with
an exact `id@version`. Do not silently upgrade it during the task.

Pass `ai_optimized: true` where supported. Start with a narrow search; read selected sources before
making claims and stop when the question is resolved. Reuse resolved IDs, versions and evidence
across modules instead of repeating discovery. Separate verified behavior from inference or gaps.

## Project-bound operations

When the MCP server exposes initialized projects, a cluster result is also a managed Graphit
target. Discover with the current project's `project_dir`, then use the selected returned `dir`
as `project_dir` on that neighbor's AST/Knowledge/Memory/Task reads as needed. Read each needed
target skill with `graphit_module_skill` and that path; respect `enabled` and target overrides.
Use AST and wiki source tools even if a local filesystem happens to expose the neighbor. Do not
switch to grep, glob or directory walks because it is outside the working directory. Keep the
coordinating Task in its owning project and preserve evidence by logical identity/relative paths;
read access does not itself authorize modifications. This branch requires a real server-resolved
project and does not apply to artifact-only content.

Do not call tools that require a real project for indexing, synchronization, project Task state, or
project Memory when the server is artifact-only. User-scoped Memory remains valid because it belongs
to the server user rather than to an invented checkout. If the requested operation truly needs a
project, explain that a checkout must be mounted and initialized on the server, then request its real
server-side path.
