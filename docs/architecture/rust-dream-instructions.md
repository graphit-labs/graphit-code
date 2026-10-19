---
title: Rust Dream consolidation instructions
type: reference
status: partial
updated: 2026-10-13
references:
  - target: {type: task, id: tsk-edf3}
    relation: implements
  - target: {type: task, id: tsk-289f}
    relation: uses
  - target: {type: task, id: tsk-e643}
    relation: uses
  - target: {type: task, id: tsk-9509}
    relation: derived_from
  - target: {type: task, id: tsk-8c6a}
    relation: relates_to
  - target: {type: task, id: tsk-f2ba}
    relation: implements
  - target: {type: task, id: tsk-6aa2}
    relation: implemented_by
---

# Rust Dream consolidation instructions

`policy_prompt(PolicyPrompt::DreamConsolidation)` selects the maintained PROJECT Memory recipe prompt in `templates/agent-instructions/policy/prompts/dream-consolidation.md`. Rendering uses the existing [policy catalog](rust-policy-instructions.md) and [instruction engine](rust-agent-instruction-templates.md). It produces instructions and data; it does not run a model, grant authority or prove a Memory effect.

The productive App composition is documented in [Rust PROJECT Dream caller](../specs/rust_project_dream.md). It binds the selected Foreground PROJECT/version, actual paired Memory admissions, native proposal, complete recipe/application and retained cleanup. Those responsibilities belong to the caller rather than this pure renderer.

## Input and identity

The caller supplies five required Text values:

| Slot | Caller-owned value |
|---|---|
| `dream_session_id` | Actual attempt identity; not a Task session claim |
| `dream_agent` | Selected installed agent identity |
| `project_id` | Captured PROJECT identity |
| `project_version` | Captured project version |
| `memories_json` | Complete admitted corpus projected to IDs, revisions, title/body and attributes |

The selected templates have CompiledDefault provenance. Captured overrides, positive engine limits and the original SnapshotContext remain explicit. Values are literal, including braces and Unicode. Missing variables, malformed selected overrides and duplicate templates/variables fail without fallback or partial output. The renderer checks cancellation/deadline and UTF-8 output bounds. Its cooperative checks and SDK fuel are not a preemptive deadline or hard RSS guarantee.

## Recipe and effects

The model returns one JSON object with `actions`. An empty array is a valid noop. A fold contains `action: "fold"`, `kind: "merge"` or `"conflict"`, `survivor` and nonempty `members` with actual `id` and positive `expected_revision`, optional `title`, and nonblank `body` and `reason`. A delete contains `action: "delete"`, actual `id`, `expected_revision` and `reason`.

The caller validates the entire proposal before applying any action: closed fields and action kinds, byte bounds, IDs from the supplied corpus, exact revisions and no repeated IDs. Runtime supplies operation keys; the model cannot supply credentials, grants, keys, content hashes or authorization. The text is a proposal, not a tool call or commit receipt.

The canonical prompt permits only the supplied PROJECT Memory data. It excludes USER, Task, Knowledge, AST, Hub and other-domain reads or writes. It retains the shared persistence boundary and treats corpus content as evidence rather than instructions. It prohibits shell/filesystem writes, generated reports and configuration changes.

Application belongs to the actual paired ReadMemory/WriteFencedMemory admissions and automatic Records operations. Automatic folds preserve strongest classification, mandatory and important attributes. Automatic bare deletion protects mandatory/important records and the last active record. This differs from the manual role tool contract. CAS failures require reobservation and reassessment. Earlier committed receipts survive a later failure; a sequence is not a global transaction.

Core Module::Dream remains unsupported by generic module mandate/skill APIs. This dedicated prompt does not create a module skill. Native readonly enforcement, complete Codex EOF/process success, actual Auth/Records effects and retained cleanup belong to the caller; instructions cannot establish them.

## Verification and qualification

`crates/graphit-runtime/tests/dream_instructions.rs` compares full fixtures for graphit and orbit, the PROJECT recipe rules, five required slots, literal substitution, malformed/duplicate inputs, original cancellation/deadline and UTF-8 bounds. These tests qualify rendering only. Task `tsk-f2ba` completed the coupled Foreground PROJECT job with seven active checks passed, reusing the eight qualified `tsk-6aa2` caller cases and the current Source/CLI human-activity composition. Those local Linux execution proofs are separate from this renderer and do not establish installed vendor authentication or live commercial inference. No fixture qualifies Kiro, native vendor credentials, Windows or macOS.

The Foreground caller under `tsk-6aa2` passed eight unique Linux controller/SDK tests (0 failed/ignored, 180 filtered; 104.01 s suite, 147.25 s total) and strict App `--lib` Clippy (exit 0, 81.26 s). Exact commands, tested paths, historical failures and limits are recorded in the [caller specification](../specs/rust_project_dream.md#source-and-qualification-boundary). Qualification remains partial for the broader integration: renderer tests do not qualify execution, and caller fixtures do not establish commercial Codex/Kiro authentication or live inference, Main CLI, Windows/macOS, all MCP effects, universal Host repair or all failure-path joins. Leaf final verification and parent/session closure remain separate.

```sh
cargo test --locked --offline -j4 -p graphit-runtime --test dream_instructions
cargo clippy --locked --offline -j4 -p graphit-runtime --lib --test dream_instructions -- -D warnings
rustfmt --edition 2024 --check crates/graphit-runtime/tests/dream_instructions.rs
```
