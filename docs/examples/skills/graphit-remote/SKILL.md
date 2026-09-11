---
name: graphit-remote
description: Use a projectless Graphit MCP server to retrieve versioned code, knowledge, shared skills, and durable user memory without assuming access to a source checkout.
---

# Graphit Remote

Use Graphit as the authoritative context and coordination layer when its MCP tools are available.

## Bootstrap

Before the first Graphit action in a session, call `graphit_mandates` with no arguments. Match the
current action against the returned module triggers. When a trigger matches, call
`graphit_module_skill` with that module and read the complete `content` before using its tools. Do
not pass `project_dir` to either call on an artifact-only server.

The core module names are `task`, `memory`, `ast`, `hub`, and `knowledge`. The returned `enabled`
field reflects the server's resolved module configuration. If it is `false`, do not assume that
module is available merely because its skill source was returned.

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
  `id: "id@version"`. Follow its `canonical_path`; use `path` for an individual file.
- Read Graphit's own core module instructions with `graphit_module_skill`, not
  `graphit_hub_content`.

Install a missing artifact globally by calling `graphit_hub_install` without `project_dir` and with
an exact `id@version`. Do not silently upgrade it during the task.

## Project-bound operations

Do not call tools that require a real project for indexing, synchronization, project Task state, or
project Memory when the server is artifact-only. User-scoped Memory remains valid because it belongs
to the server user rather than to an invented checkout. If the requested operation truly needs a
project, explain that a checkout must be mounted and initialized on the server, then request its real
server-side path.
