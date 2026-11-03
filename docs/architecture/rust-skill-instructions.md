---
title: Rust skill instructions
type: reference
status: current
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-2e3f}
    relation: implements
  - target: {type: task, id: tsk-289f}
    relation: uses
  - target: {type: task, id: tsk-e643}
    relation: uses
  - target: {type: task, id: tsk-84f3}
    relation: uses
  - target: {type: task, id: tsk-49b5}
    relation: uses
  - target: {type: task, id: tsk-800a}
    relation: derived_from
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# Rust skill instructions

A caller can select complete maintained skill bodies, descriptions and their companion reference templates for Task, Memory, AST, Hub and Knowledge. The content producer is pure: select a set, supply actual effective values and captured overrides, then render the body and each needed reference through [the instruction engine](rust-agent-instruction-templates.md) with the original request. The existing Hub SkillDescriptor/render_skill encodes SKILL.md; the application publisher described below owns companion materialization.

## Metadata and source catalog

`crates/graphit-runtime/src/skill_instructions.rs` exports `skill_templates(Core Module) -> Result<SkillTemplateSet, UnsupportedSkillModule>`. The immutable set exposes `body_entry()`, `description()`, `references()` and `candidates()`. Each SkillReferenceTemplate has public static `relative_path` and `entry` metadata. These are logical companion names, not native filesystem destinations, authority or grants. Daemon, Sync, Embedding, Hooks, Agent, Dream and DaemonUi return explicit UnsupportedSkillModule errors from this factory. This is its five-module API boundary, not a promise of a compiled skill for every orchestration switch; Dream prompts and lifecycle hooks have their own template producers. Historical refinement labels do not establish an additional current command or completion gate.

Descriptions are normative Markdown assets alongside each entrypoint, using the maintained package metadata wording. description() strips only terminal LF padding; it does not template, discover a brand or manufacture frontmatter. There are no artifact/agent parameters or Memory/Knowledge context snapshot inputs. Those concerns belong to actual callers. Installed .agents/.codex files are generated projections, not authoritative sources.

| Module | Body entry | References |
|---|---|---|
| ast | `skills/ast/entry.md` | `references/impact-assessment.md` |
| hub | `skills/hub/entry.md` | `references/discovery-cases.md` |
| knowledge | `skills/knowledge/entry.md` | `references/documentation-design.md`, `references/worked-domain.md` |
| memory | `skills/memory/entry.md` | `references/durable-context.md` |
| task | `skills/task/entry.md` | `references/execution.md`, `references/planning.md`, `references/session.md`, `references/worked-feature.md`, `references/worked-system.md` |

The 20 maintained sources under `templates/agent-instructions/skills/` comprise five bodies, ten references and five descriptions. All sets reuse the six shared reference/persistence sources from [policy instructions](rust-policy-instructions.md). Task additionally reuses the existing protocol.md for its body and standalone references, reconciling legacy own-Task claims/releases with current coordinator-only lifecycle.

## Effective values

brand_display labels the authored body/reference text. Every mcp_tool_<operation> receives the actual catalog name, which may differ from any brand-derived name. Names and their policy validation come from the caller. There is no inferred MCP registration, availability, prefix or authorization. PROJECT and runtime Knowledge rendering require explicit docs_dir from effective validated configuration: no Rust default to docs and no filesystem/config lookup. A Global physical instruction instead resolves DOCS_ROOT from the active PROJECT at execution time; publication_scope=global selects that guidance without embedding an origin PROJECT path or configuration. Graphit/docs and Orbit/handbook are illustrative fixture values, not hidden defaults.

| Body | Direct MCP variable slots in the current entry template |
|---|---|
| ast | `ast_embed`, `ast_index`, `ast_list`, `ast_query`, `ast_schema`, `ast_search`, `ast_source`, `daemon_status`, `module_skill`, `sync` |
| hub | `cluster_get`, `cluster_projects`, `cluster_set`, `cluster_unset`, `config_get`, `config_list`, `config_set`, `config_unset`, `hub_content`, `hub_forget`, `hub_install`, `hub_install_local`, `hub_install_native`, `hub_list`, `hub_projects`, `hub_recover`, `hub_remove`, `hub_search`, `hub_show`, `hub_sync`, `hub_type_path`, `hub_unpublish`, `hub_update`, `module_skill`, `sync` |
| knowledge | `daemon_status`, `knowledge_export`, `knowledge_index`, `knowledge_lint`, `knowledge_references`, `knowledge_remove`, `knowledge_schema`, `knowledge_search`, `knowledge_sync`, `module_skill`, `sync`, `task_get`, `task_search`, `wiki_embed`, `wiki_log`, `wiki_source` |
| memory | `cluster_projects`, `memory_delete`, `memory_demote`, `memory_important`, `memory_index`, `memory_insert`, `memory_list`, `memory_mandatory`, `memory_mark_mandatory`, `memory_promote`, `memory_remove`, `memory_schema`, `memory_search`, `memory_source`, `memory_sync`, `memory_unmark_mandatory`, `memory_update`, `module_skill` |
| task | `task_commit_set`, `task_get`, `task_list`, `task_query`, `task_schema`, `task_search`, `task_session_get`, `task_session_list` |

Task, Memory and Knowledge include shared query instructions requiring mcp_tool_references_query/reconcile. All bodies include the maintained persistence boundary: shared records contain only necessary non-sensitive project facts, decisions and evidence; durable user-only preferences belong only in private user memory. Generic context instructions never incorporate actual standing-memory titles, contents or runtime snapshots.

| Companion entry | Direct variables in the current source |
|---|---|
| `skills/ast/references/impact-assessment.md` | `brand_display`, `mcp_tool_ast_query`, `mcp_tool_ast_schema`, `mcp_tool_ast_search`, `mcp_tool_ast_source` |
| `skills/hub/references/discovery-cases.md` | `brand_display`, `mcp_tool_cluster_projects`, `mcp_tool_hub_content`, `mcp_tool_hub_forget`, `mcp_tool_hub_install`, `mcp_tool_hub_install_native`, `mcp_tool_hub_list`, `mcp_tool_hub_projects`, `mcp_tool_hub_recover`, `mcp_tool_hub_remove`, `mcp_tool_hub_search`, `mcp_tool_hub_show`, `mcp_tool_hub_type_path`, `mcp_tool_hub_unpublish`, `mcp_tool_module_skill` |
| `skills/knowledge/references/documentation-design.md` | `mcp_tool_knowledge_search`, `mcp_tool_module_skill`, `mcp_tool_wiki_source` |
| `skills/knowledge/references/worked-domain.md` | `brand_display`, `mcp_tool_knowledge_search`, `mcp_tool_wiki_source` |
| `skills/memory/references/durable-context.md` | `brand_display`, `mcp_tool_memory_delete`, `mcp_tool_memory_insert`, `mcp_tool_memory_mark_mandatory`, `mcp_tool_memory_search`, `mcp_tool_memory_source`, `mcp_tool_memory_update`, `mcp_tool_task_get`, `mcp_tool_task_search` |
| `skills/task/references/execution.md` | `mcp_tool_task_claim`, `mcp_tool_task_commit_set`, `mcp_tool_task_create`, `mcp_tool_task_progress`, `mcp_tool_task_revise` |
| `skills/task/references/planning.md` | None |
| `skills/task/references/session.md` | `mcp_tool_task_session_complete`, `mcp_tool_task_session_create`, `mcp_tool_task_session_revise` |
| `skills/task/references/worked-feature.md` | None |
| `skills/task/references/worked-system.md` | None |

These tables describe template inputs, **not announced tools**. Some slots name capabilities
not exposed by the current Root server, including historical cluster or Memory CRUD names.
The actual [MCP catalog](../guides/mcp_tools_reference.md) determines availability; supplying a
string variable cannot register a tool, grant authority or turn an illustrative workflow into an API.
Shared includes such as `protocol.md` add their own variables. The template engine evaluates the
selected content and reports missing required values; do not infer availability from the union of
all possible slots. Editorial IDs, paths and test runners remain illustrative.

## Render, links and errors

For a selected set, combine its captured compiled defaults with explicit project/global/Hub overrides. Call `render_instruction(entry, candidates, variables, limits, &original_snapshot)` for its body_entry and every reference entry required by the caller. Shared sources are registered in the same bundle. CompiledDefault provenance describes source selection, not SourceRef validation or authority. Override priority, default wrapping, required-variable errors, duplicate rejection and selected syntax failure follow the engine. A selected malformed companion source fails validation even when the chosen body does not include it; there is no lower-origin fallback.

References remain separate documents, and body references/*.md links resolve to the set's exact companion list. A caller must materialize them with Core/Hub owned publication and preserve foreign files; render_skill has no companion argument or multi-file publication result. It receives an already rendered body and metadata via SkillDescriptor and returns only native SKILL.md bytes and its logical entry path. Alias/name/frontmatter encoding is delegated to that existing native renderer. A selected Hub Skill retains its optional license, compatibility and metadata fields when its body and description are rendered.

Use the original SnapshotContext across all composition stages. Cancellation and deadline are cooperative engine checks; positive byte/value/template limits, fuel and recursion must be supplied by the caller. The caller retains the same deadline and cancellation across source capture, rendering and publication. Concrete template and output limits bound those individual inputs and results. The engine returns no partial body on error. Logical caps and SDK controls do not promise hard RSS bounds or preemptive cancellation.

Coordinator workflows retain specification, readiness, private claims, checkpoints, revision, dependency/correction ordering, cancellation and explicit closure. Delegated roles instead read/report or transcribe expressly decided content, without acquiring Task/session claims, completing/cancelling/releasing records or revising coordination. Claimed writes require returning a draft to the coordinator. These boundaries match [managed-role context](../specs/managed_role_context.md); a role's text never grants capability.

## Physical project install and sync

The application install/sync consumer materializes the five maintained skill sets and their ten companions directly through the Core generated publication writer, with the installed package as canonical source and origin/version/content hashes in its registry. `graphit roles install` and `graphit roles sync` use the selected project's effective Brand and `knowledge.docs_dir`; `--skill-alias MODULE=NAME` retargets the module's existing publication. The native SkillDescriptor renderer owns frontmatter/name encoding, while references remain separate files beneath the same skill directory. Codex and Antigravity share the `.agents/skills` publication identity, so switching between those bindings does not claim the same directory under two owners.

The Role and Skill publications retain their separate native capabilities: installing skills does not bypass an unsupported Role selection. The qualified45b three-role fixture used Kiro V3; its current Codex follow-up passed in the five managed_roles cases of run-app-consumers47g, and the shared Codex/Antigravity skill fixture explicitly selects Scribe; Codex Scout and Tracker preserve their read-only instructions and the official `features.shell_tool` field. They inherit the parent sandbox, as documented by the official [subagent configuration contract](https://learn.chatgpt.com/docs/agent-configuration/subagents#approvals-and-sandbox-controls); the renderer does not invent a profile-local sandbox or reject that instruction intent. Graphit MCP authorization remains independent. This Codex correction has its own affected validation; the existing CP1 skill/Scribe proof stays valid.

The install/sync publisher acquires `ArtifactStore::writer_with_context` with the
same captured Snapshot on Linux, macOS and Windows. Its operational I/O accepts
an absent deadline while the Original remains valid and uncancelled; a finite
deadline keeps its original absolute bound. It does not manufacture a timeout
or a replacement cancellation token. An ingress that requires a finite deadline
keeps that separate requirement. Native platform execution remains separately
qualified.

An explicit `--skill-bundle MODULE=ID@VERSION` selects a locally installed Hub Skill at an exact version. The consumer reads the maintained `SKILL.md` plus canonical reference paths, retaining source observations and current registry membership through publication. Project and Global raw overrides retain their normal priority over that selected Hub source and the compiled defaults. Malformed selected body or companion input fails without adopting a lower-priority source. Equal foreign bytes do not establish ownership, and a refused retarget preserves the previous owned publication.

A pre-existing skill directory without an entry for the selected publication key is
also foreign on the first install/sync. The Core publication check returns
`Conflict: AlreadyExists` before creating a destination publication intent; the
application failure names the module and destination while retaining that cause.
The existing skill, companions, extra files and raw overrides are preserved. Earlier Role effects remain visible in the result; no rendered payload cache
or pin is created. Repeating sync does not adopt the directory;
neither a compiled version label nor matching content establishes ownership.

When an operator authorizes a one-time replacement of an unowned projection,
preserve the entire directory outside the publication destination before invoking
the canonical publisher. Retain all files, including unknown inline edits, and
verify preservation; do not manufacture an ownership entry or import the old
projection. The normal publisher can then create its own entry at the vacant
destination. This procedure adds no automatic adoption or migration policy.

`roles sync` and `skills sync` publish instruction files only. General `graphit sync` and Runtime `graphit_sync` also compose the Coordinator publisher, which reconciles owned hooks and MCP configuration. `hooks sync` is available independently. Saved logical publication choices are reused from ConfigStore; user-modified files remain explicit conflicts, and Hub versions stay pinned until an explicit `hub update`.

Selected remote FILE bundles use the existing service capture, Native verifier cache and FileTransport admission. Their exact installed origin and producer remain authoritative selectors only after the actual credential and PROJECT read checks succeed. The existing installation-owned cache receipt is verified before capture and after physical publication; a read never downloads or repairs an absent or changed cache. Leases and transport proofs remain alive through raw-source/registry revalidation and the existing generated-payload writer. Standalone identity selection is explicit and does not replace storage or PROJECT authorization.

## Physical Global install and sync

It uses captured native agents/skills directories, the same Core writer and the concrete runtime/native-instructions control namespace. Builtins and Global raw instructions require no PROJECT; --source-project explicitly selects an authorized origin for raw overrides or an installed Hub bundle. Global profiles and Knowledge guidance retain no origin PROJECT path/version/docs_dir, StandingContext or mandatory Memory. Shared native targets, aliases and foreign files keep existing ownership semantics. The actual ModelProvisioner moves to the concrete runtime/models namespace to resolve its demonstrated root-set collision with FILE/cache; no importer or relaxed scope is added. Run continues to require PROJECT.

## Verification and external producers

The focused test suite uses complete static fixtures with exact unique coverage of all 20 documents, two validated brands, distinct effective aliases, and docs/handbook roots. It checks the ten companion link sets, five descriptions, seven unsupported modules, current delegation/privacy, default wrapping, required docs_dir, duplicate variables/templates, selected malformed references, original cancellation between body/reference and expired deadline, and UTF-8 output equality/overflow for real body/reference text. Real Hub SkillDescriptor parse/render_skill and native reparse/roundtrip cover each of the five bodies with the current Kiro V3 Markdown profile, with explicit alias, metadata and preserved body. Encoding is not native model delivery.

Portable focused commands use Cargo/rustfmt on PATH:

```sh
cargo test -p graphit-runtime --test skill_instructions --offline --locked -j1
cargo clippy -p graphit-runtime --lib --test skill_instructions --offline --locked -j1 -- -D warnings
rustfmt --edition 2024 --check crates/graphit-runtime/src/skill_instructions.rs crates/graphit-runtime/tests/skill_instructions.rs
```

Pure renderer results belong to Task 2e3f. The registered MCP catalog (47fd), module_skill endpoint, captured roots/overrides/Auth/SourceRef, runtime9bd4 lifecycle/live context and native invocation have their own application consumers and evidence; fixture aliases alone do not prove those integrations. The physical project publisher above is qualified by its real CLI/HTTP consumers. The selected remote consumer is also qualified by its actual Broker/standalone journeys and current quality checks; a physical Global destination and native three-platform publication remain open. See [native bindings](../development/native-host-bindings.md). Source edf3 produces the separate Dream consolidation prompt; its runner is outside this skill API. The broad migration remains open.

The Root MCP guidance consumer has its own [module skill contract](../specs/rust_root_module_skill.md), including current scope selection, authority, captured templates and qualification status. Its application transport evidence is separate from the pure renderer and native host/model delivery.
