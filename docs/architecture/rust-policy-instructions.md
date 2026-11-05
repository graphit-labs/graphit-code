---
title: Rust policy instructions
type: reference
status: partial
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-be6e}
    relation: implements
  - target: {type: task, id: tsk-289f}
    relation: implements
  - target: {type: task, id: tsk-e643}
    relation: uses
  - target: {type: task, id: tsk-49b5}
    relation: uses
  - target: {type: task, id: tsk-84f3}
    relation: uses
  - target: {type: task, id: tsk-800a}
    relation: derived_from
  - target: {type: task, id: tsk-ec11}
    relation: uses
---

# Rust policy instructions

A caller can select generic normative Markdown for Task, Memory, AST, Hub and Knowledge, and 25 bootstrap/routing/worker fragments, without reading configuration or files. This is the pure content stage in Runtime. The caller supplies effective catalog values, captures any overrides, and renders with the original request through [the instruction engine](rust-agent-instruction-templates.md). It then uses the existing Hub descriptor/native renderer if native encoding is needed. Capture, Auth, SourceRef proofs, publication and hook/model delivery remain separate producers.

The Rust catalog does not select a per-tool unit-completion reminder. Bootstrap
mandates and lifecycle routing remain selected by their actual consumers. Its
Kiro and Codex support custom hooks and Stop without default per-tool or Task
reminders.

## Typed selection

The implementation is in `crates/graphit-runtime/src/policy_instructions.rs`; normative sources are exclusively under `templates/agent-instructions/policy/` and `shared/`. The existing `protocol.md` remains the worker protocol's single source.

- `module_mandate(Module, MandateAudience) -> Result<PolicyTemplateSet, UnsupportedPolicyModule>` consumes Core's existing Module enum. Audience is Coordinator or Delegate; only Task's text changes.
- `policy_prompt(PolicyPrompt) -> PolicyTemplateSet` selects one of 28 closed variants: 25 routing/bootstrap/worker fragments, PROJECT and USER Dream consolidation, and PROJECT hook context.
- The immutable set exposes `entry()`, `candidates()` and `module_tag()`. Candidates are compiled defaults, including six reusable shared policy sources and required includes. Selection does not render, discover availability or grant permissions.

Task has tag `task_rule`, Memory `mem_rule`, AST `ast_rule`, Hub `hub_rule`, and Knowledge `doc_rule`. Daemon, Sync, Embedding, Hooks, Agent, Dream and DaemonUi return UnsupportedPolicyModule with their actual Core module identity. This factory supports those five module mandates; orchestration switches do not create new mandate variants. Dream consolidation and PROJECT hook context use their separate typed prompts. Historical refinement labels are not additional commands or validation gates.

The variable catalog below names template inputs, not MCP availability. Use the actual
[Root tool catalog](../guides/mcp_tools_reference.md) and selected caller metadata. In particular,
a historical slot such as `mcp_tool_task_session_search` does not expose a Session search API.

## Values and composition

Variables use explicit `InstructionVariable` values; strings are Text and the router list is TextList. All effective MCP names are supplied as `mcp_tool_<operation>`. Skill names are `skill_name_task/memory/ast/hub/knowledge`. A caller obtains actual aliases from its catalog; deriving a name from a brand does not establish that a tool exists. `brand_display`, `brand_marker` and effective role names come from validated Core/managed-role inputs. Text metadata never establishes grants or availability.

Mandate template operation-name slots are TaskCoordinator: task_session_get/list/create/claim, task_get/query/create; TaskDelegate: task_get/search/progress/comment_add; Memory: memory_search/source/update/insert/query; AST: ast_source/search/schema/query/fts_schema/fts_query; Hub: cluster_projects, hub_projects/search/show, config_get; Knowledge: wiki_source, knowledge_search/query. All five use their effective skill name; all use brand_display. Shared query instructions use references_query/reconcile.

| Current template under `templates/agent-instructions/policy/` | Direct named inputs |
|---|---|
| `hooks/antigravity-compensation.md` | None |
| `hooks/bootstrap-header.md` | `brand_display` |
| `hooks/bootstrap-memory-empty.md` | None |
| `hooks/bootstrap-memory-fetch.md` | `mcp_tool_memory_mandatory`, `skill_name_memory` |
| `hooks/bootstrap-memory-loaded-notice.md` | `mcp_tool_memory_mandatory` |
| `hooks/bootstrap-memory-recall.md` | `mcp_tool_memory_search`, `mcp_tool_memory_source`, `skill_name_memory` |
| `hooks/bootstrap-task-lifecycle.md` | `mcp_tool_task_session_checkpoint`, `mcp_tool_task_session_complete`, `mcp_tool_task_session_revise` |
| `hooks/bootstrap-task-planning.md` | None |
| `hooks/bootstrap-task-recall.md` | `mcp_tool_task_get`, `mcp_tool_task_search`, `skill_name_task` |
| `hooks/bootstrap-task-session.md` | `brand_display`, `mcp_tool_task_session_create`, `mcp_tool_task_session_get`, `mcp_tool_task_session_list`, `mcp_tool_task_session_search`, `skill_name_task` |
| `hooks/core-invariant.md` | `brand_display`, `project_version` |
| `hooks/cursor-compensation.md` | `brand_display` |
| `hooks/documentation-consistency-reminder.md` | None |
| `hooks/module-wrapper.md` | `module_content`, `module_tag` |
| `hooks/project-hook-context.md` | `project_memory_json`, `project_memory_state` |
| `hooks/project-version-notice.md` | `project_version` |
| `hooks/router-preamble.md` | `brand_display`, `project_version`, `role_name_scout`, `role_name_scribe`, `role_name_tracker` |
| `hooks/router.md` | `brand_marker`, `module_mandates` |
| `hooks/routing-fallback.md` | `mcp_tool_mandates`, `mcp_tool_module_skill` |
| `hooks/worker-delegated-contract.md` | None |
| `hooks/worker-delivery-validation.md` | None |
| `hooks/worker-memory-fetch.md` | `mcp_tool_memory_mandatory`, `skill_name_memory` |
| `hooks/worker-memory-loaded-notice.md` | `mcp_tool_memory_mandatory` |
| `hooks/worker-memory-recall.md` | `mcp_tool_memory_search`, `mcp_tool_memory_source`, `skill_name_memory` |
| `hooks/worker-protocol-header.md` | `protocol_marker`, `role_name` |
| `hooks/worker-task-recall.md` | `mcp_tool_task_get`, `mcp_tool_task_search` |
| `mandates/ast.md` | `brand_display`, `mcp_tool_ast_fts_query`, `mcp_tool_ast_fts_schema`, `mcp_tool_ast_query`, `mcp_tool_ast_schema`, `mcp_tool_ast_search`, `mcp_tool_ast_source`, `skill_name_ast` |
| `mandates/hub.md` | `brand_display`, `mcp_tool_cluster_projects`, `mcp_tool_config_get`, `mcp_tool_hub_projects`, `mcp_tool_hub_search`, `mcp_tool_hub_show`, `skill_name_hub` |
| `mandates/knowledge.md` | `brand_display`, `mcp_tool_knowledge_query`, `mcp_tool_knowledge_search`, `mcp_tool_wiki_source`, `skill_name_knowledge` |
| `mandates/memory.md` | `brand_display`, `mcp_tool_memory_insert`, `mcp_tool_memory_query`, `mcp_tool_memory_search`, `mcp_tool_memory_source`, `mcp_tool_memory_update`, `skill_name_memory` |
| `mandates/task-coordinator.md` | `brand_display`, `mcp_tool_task_commit_set`, `mcp_tool_task_create`, `mcp_tool_task_get`, `mcp_tool_task_query`, `mcp_tool_task_session_claim`, `mcp_tool_task_session_create`, `mcp_tool_task_session_get`, `mcp_tool_task_session_list`, `skill_name_task` |
| `mandates/task-delegate.md` | `brand_display`, `mcp_tool_task_comment_add`, `mcp_tool_task_get`, `mcp_tool_task_progress`, `mcp_tool_task_search`, `skill_name_task` |
| `prompts/dream-consolidation.md` | `dream_agent`, `dream_session_id`, `project_version` |
| `prompts/user-dream-consolidation.md` | `dream_session_id` |

Inputs in an unselected conditional branch need not be evaluated. Includes add their own
dependencies; the table lists direct source references rather than a new validation schema.
The 25 historical bootstrap/worker fragments remain selectable library content. Current PROJECT
hooks use `ProjectHookContext`; their presence in this catalog does not make every fragment
part of a current host hook, nor expose a referenced unavailable tool.

Router additionally includes RouterPreamble and requires its brand_display and role_name_scout/tracker/scribe when a nonempty module list is rendered. WorkerDelegatedContract includes the unchanged protocol.md. Six shared sources cover typed-reference contract/example/Knowledge frontmatter/mandate/query and the persistence boundary, and are reused by the [skill producer](rust-skill-instructions.md).

`DreamConsolidation` selects `policy/prompts/dream-consolidation.md`. It requires `dream_session_id`, `dream_agent`, `dream_project_id`, `dream_project_version` and `dream_memories_json`; it reuses the persistence boundary. See [Dream instructions](rust-dream-instructions.md) for the PROJECT-only data recipe and its native qualification boundary. This specific prompt does not introduce a Dream module mandate or skill.

`UserDreamConsolidation` selects `policy/prompts/user-dream-consolidation.md` for the separately
admitted USER lane. It does not reuse PROJECT authority or create a generic Dream module skill.
[Dream records](../specs/rust_dream_records.md) define the paired write and recovery boundary.

`ProjectHookContext` selects `policy/hooks/project-hook-context.md`, including Header, ProjectVersionNotice, Router/preamble and CoreInvariant. Its App consumer supplies reserved `module_mandates`, `project_memory_state` and `project_memory_json` from the actual PROJECT admission and Memory read. The production hook renders the current project and global overrides, the active Broker layer and the compiled defaults. See [PROJECT hook context](../specs/rust_project_hook_context.md).

The caller renders each selected module mandate, wraps its already rendered body with ModuleWrapper, and supplies the list in Task → Memory → AST → Hub → Knowledge order to Router. This stage preserves supplied order; it does not consult ModuleSettings or silently filter unsupported modules. Empty module_mandates renders an empty body. module_content is literal data, so native tokens or braces inside it are not evaluated again.

WorkerProtocolHeader always receives protocol_marker and role_name; an explicitly empty role_name omits the role field, while a missing required value fails. Runtime resolves the logical marker and role identity. ProjectVersionNotice receives an explicitly selected project_version. Bootstrap/worker loaded notices contain only generic acknowledgement: runtime may append live Memory context at its authorized boundary. No standing-memory records, personal context or runtime snapshots are compiled into these sources. Conditional selection, numbering, scope resolution, repeat/dedup and Task/Memory disabling belong to the runtime consumer.

## Overrides, integrity and lifecycle

Combine the selected candidates with captured project/global/Hub overrides and call `render_instruction(set.entry(), candidates, variables, limits, &original_request)`. Engine precedence is project → global → Hub → compiled. An override can include `defaults/<entry>` to wrap its maintained default. A selected invalid override fails; it does not fall back to a lower origin. Losing candidates are still charged to limits; all registered sources must have valid syntax. Required variables missing from evaluated expressions are SDK UndefinedError.

The original SnapshotContext and its cancellation/deadline are preserved. The caller budgets the entire capture/composition/publication request, including intermediate bodies and repeated render stages; individual positive engine limits do not reset that outer budget. Engine byte limits, fuel and recursion are logical controls, not a hard RSS cap or preemptive cancellation guarantee. No partial body is returned on failure. Provenance CompiledDefault is not SourceRef validation or authorization.

The coordinator owns acceptance, checkpoints, Task/session claims, revisions, completion, cancellation and release. Delegates do not acquire a claim, including their own Task, and return a draft when a write requires that claim. Progress/comment tool names are catalog metadata and do not grant write authority.Shared project persistence permits only necessary non-sensitive project facts, decisions and evidence; durable user-only preferences belong only in private user memory.

## Validation and remaining integration

The focused suite in `crates/graphit-runtime/tests/policy_instructions.rs` compares complete fixtures for all six mandate variants, 25 prompts and six shared sources with two validated brands and intentionally different effective MCP/skill aliases. It also protects all seven unsupported modules, coordinator/delegate boundaries, empty/one/five-module routing, literal data, missing/duplicate/malformed inputs, default wrapping, discarded versus selected invalid overrides, original cancellation/deadline, and Unicode output-byte equality/overflow. Current Hub AgentDescriptor/render_agent uses Kiro V3 Markdown body/grant encoding. Earlier V2/preview fixture receipts are historical and do not qualify the current host; controlled contract tests require no vendor account or plan.

Portable focused commands, using Cargo/rustfmt on PATH:

```sh
cargo test -p graphit-runtime --test policy_instructions --test kiro_hook_plans --offline --locked -j4
cargo clippy -p graphit-runtime --lib --test policy_instructions --test kiro_hook_plans --offline --locked -j4 --no-deps -- -D warnings
rustfmt --edition 2024 --config skip_children=true --check crates/graphit-runtime/src/policy_instructions.rs crates/graphit-runtime/tests/policy_instructions.rs crates/graphit-runtime/tests/kiro_hook_plans.rs
```

Record actual platform/results in Task 289f and the Dream extension in edf3. W CLI/registered MCP supply captured inputs and effective catalog values; 9bd4 supplies runtime lifecycle and live context; HK supplies native hook invocation/publication. See [native bindings](../development/native-host-bindings.md). None of those integrations is certified by pure content tests. Skills/references are supplied by Source 2e3f, the Dream consolidation prompt by Source edf3, and each other instruction consumer retains its own documented producer.
