---
name: graphit-remote
description: Read admitted Graphit content through the actual MCP catalog without inventing a source checkout or authorization.
---

# Graphit Remote

This is an example host skill for the current Rust Root MCP contract. An artifact-only server
has no implied PROJECT, checkout, Task corpus or USER identity. Inspect its actual `tools/list`
and use only announced operations with the server's authentication and admitted sources.

## Load the instruction needed for the next action

When the host has not already supplied the relevant instructions, use `graphit_module_skill`:

```json
{"module":"knowledge","ai_optimized":false}
```

Follow a reference only at its reading boundary, using its exact returned path:

```json
{"module":"knowledge","reference":"references/documentation-design.md","ai_optimized":false}
```

The module names are `task`, `memory`, `ast`, `hub` and `knowledge`. Reuse retained instructions
for the same scope; respect disabled modules and project overrides. There is no Root
`graphit_mandates` tool. Do not preload every module or replace unavailable tools with guessed names.

## Resolve exact artifact provenance

Use `graphit_hub_search` for an authorized remote catalog, with its real `transport`, `provider`,
`producer` and positive `limit`. `graphit_hub_list` lists installed metadata and requires explicit
`global`. These are different operations; there is no Root `graphit_hub_show` alias.
Retain producer, type, logical ID, concrete version and origin. Native AST/Knowledge installation
uses `graphit_hub_install_native` with its typed `installation`; authored artifact installation
uses `graphit_hub_install` with complete `selection` and `global` fields. Read the actual
[schema](../../../reference/mcp-input-schemas.json) before performing a mutation. Installing an
artifact does not grant its data permissions, and an exact-version sync does not upgrade it.

An imported Knowledge request uses its typed source rather than `hub_refs` or flat `context`:

```json
{"search":{"query":"authentication","mode":"fts","top_k":5,"page_size":5,"rerank":false},"source":{"kind":"imported_knowledge","artifact":{"id":"example/auth-guide","version":"1.2.0"}},"ai_optimized":false}
```

Read a returned slug with `graphit_wiki_source` and that same exact source. The IDs/version here
are illustrative: select an actually installed and admitted artifact. AST imported reads retain
their own literal `context` selector as documented by their schema; do not apply one domain's
selection shape to every tool. Source is original content; search previews only locate evidence.

For installed authored content, `graphit_hub_content` uses a nested `installed` request:

```json
{"installed":{"artifact_type":"skill","id":"example/review","version":"1.2.0","global":true,"path":"SKILL.md"},"ai_optimized":false}
```

Remote content instead uses `remote:{selection,path}`. Exactly one variant is required.
`id@version` is not a substitute for separate ID/version fields. The framework's own module
references come from `graphit_module_skill`, not from a guessed Hub artifact.

## Work with a real PROJECT only when supplied

Use the admitted project's directory returned by supported discovery, never a reconstructed host
path. Pass it only as a live `project_dir` argument. Resolve each neighbor independently and read
its enabled module instructions. No cluster discovery tool is implied merely by historical docs;
if the endpoint lacks it, record that limitation and request supported discovery from the host.
Project reads and writes require their own authority, even when content is locally visible.

Task coordination remains with the owning coordinator and existing Session. Use exact Task reads
for known IDs and focused search only when advertised. Recall Memory facts/lessons or Task evidence
when a question actually needs them. Missing PROJECT or USER admission remains an explicit gap;
it is never inferred from an artifact, path, profile name or public catalog entry.

## Output, paging and evidence

Structured tools default to TOON text. Use top-level `ai_optimized:false` for machine consumers
requiring JSON, not inside `input` or `search`. AST Source and SessionHook preserve original text;
Knowledge Source wraps the unchanged Markdown string. Continue opaque cursors with unchanged
selection/query through EOF when a complete traversal is needed. Preserve authorization,
cancellation, deadlines and explicit errors. Do not retry a timed-out write blindly.

Persist only project-inherent, non-sensitive findings with logical identities, exact versions and
repository-relative references. Never store credentials, host checkout paths or live mandatory
memory snapshots in shared artifacts. Separate inspected source, executed behavior and unresolved
limitations. See the [Root MCP guide](../../../guides/mcp_tools_reference.md) for complete contracts.
