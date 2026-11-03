---
title: Rust agent instruction templates
type: reference
status: partial
updated: 2026-10-15
references:
  - target: {type: task, id: tsk-e643}
    relation: implements
  - target: {type: task, id: tsk-84f3}
    relation: uses
  - target: {type: task, id: tsk-49b5}
    relation: uses
  - target: {type: task, id: tsk-800a}
    relation: relates_to
  - target: {type: task, id: tsk-d6e4}
    relation: uses
  - target: {type: task, id: tsk-8c6f}
    relation: implements
---

# Rust agent instruction templates

A caller with already captured instruction sources can render Markdown with its effective brand, artifact, agent and MCP tool names. The pure Runtime stage expands text; the existing Hub descriptor/renderer then encodes it for the selected native host. This stage does not read overrides, publish files or deliver a mandate to a running agent.

The maintained default roles use the same stage. Their normative source is Markdown under `templates/agent-instructions/`: `protocol.md`, `roles/header.md`, the three role bodies and their summaries. Rust retains role identity, modules and descriptor intent. The public three-argument `compose_role` remains default-only, and `StaticRole` has no arbitrary-body constructor or mutable body. See [Managed role context](../specs/managed_role_context.md).

## Captured inputs and selection

`graphit_runtime::instructions::render_instruction` receives an entry name, a slice of `CapturedInstruction`, explicit `InstructionVariable` values, `InstructionLimits` and the original borrowed Core `SnapshotContext`. Templates cannot discover filesystem, environment, credentials, Memory, cwd or a tool catalog.

Each candidate has a logical relative Markdown name, an `InstructionOrigin` and its UTF-8 body. Names must end in `.md`; absolute paths, empty/dot/parent components, backslashes, colon and controls fail. `defaults/` is reserved. Duplicate name+origin fails. A missing candidate is absence; empty Markdown is an explicit value.

For each name, precedence is project → global CLI → Hub → compiled default. Input ordering does not change selection. All names/bodies, including losing candidates, are admitted against logical limits. Only selected candidates and compiled-default aliases are compiled. Every registered source must have valid syntax, even if this entry does not include it; a losing project/global/Hub override is not compiled. A selected syntax or render error fails; the engine does not try a lower origin.

A compiled source named `roles/scout.md` is also available as `defaults/roles/scout.md`. An override can replace the body completely or wrap the maintained default:

```jinja
Project instructions before the default.
{% include "defaults/roles/scout.md" %}
Project instructions after the default.
```

Only explicitly supplied compiled defaults exist. Templates resolve declared variables against the effective brand.

## Variables and native literals

Variable names are unique ASCII identifiers. Values are the closed set `Text`, `Bool`, `Unsigned` and `TextList`. A caller supplies actual effective values; the engine does not assume a tool-name prefix or derive grants. In particular, a template can use `{{ mcp_tool_xpto }}` with the effective catalog name given by its caller. Extension variables use the same explicit mechanism.

Rendering uses [MiniJinja 2.24.0](https://raw.githubusercontent.com/mitsuhiko/minijinja/2.24.0/minijinja/Cargo.toml) with strict undefined values and Markdown autoescaping disabled. Required missing lookups fail. Explicit Jinja optional constructs, including `is defined`, the `default` filter and `include ... ignore missing`, retain their SDK semantics. There is no parallel grammar scanner or implicit passthrough for missing variables.

Boolean text uses the SDK representation `True`/`False`; templates can explicitly apply formatting when another textual representation is needed. Values are not recursively interpreted as templates. The string `{{ another_variable }}` supplied as a value stays literal. Native tokens with conflicting syntax use a Jinja raw block:

```jinja
Use {{ mcp_tool_xpto }}.
{% raw %}{{filePath}}{% endraw %}
${var}
```

This emits the literal `{{filePath}}` and `${var}`. It preserves the native host's later interpretation; it does not implement it. Hub renders the current Kiro V3 binding as Markdown with YAML metadata. See [Native host bindings](../development/native-host-bindings.md).

Trailing LF is preserved. Whitespace trimming is disabled unless the template itself explicitly requests Jinja whitespace control. UTF-8 and Markdown characters such as `<&>` are preserved.

## Results, errors and bounds

`RenderedInstruction` exposes the completed body, the selected entry origin and all selected template name/origin pairs. The selections identify the captured bundle, not which include branches executed, a SourceRef proof or publication authority. Its Debug output reports lengths/counts without the body; it has no implicit serde or Clone projection. A caller may consume the body in memory for the existing Hub `AgentDescriptor`.

`InstructionError` classifies invalid names/variables, duplicates, missing entries, invalid limits, logical bounds, allocation, request and actual SDK errors. Display and Debug do not print authored bodies or variable values. `Error::source` retains the original request or MiniJinja error for caller-controlled diagnosis. A writer failure keeps its first cause and is not replaced by a later cancellation. Interrupted is not retried by the writer.

Every limit is positive and explicit:

| Limit | Counted boundary |
|---|---|
| Templates | All captured candidates |
| Template bytes | Entry, then all candidate names and Markdown, including losers |
| Values | Variables plus text-list items |
| Variable bytes | Variable names/text; boolean one byte; unsigned decimal digits |
| Output bytes | UTF-8 bytes sent to the completed output buffer |
| Fuel | SDK instruction fuel per render |
| Recursion | SDK range 1..=500; larger values fail instead of being silently clamped |

Equality is accepted; addition is checked. Entry admission precedes validation/lookup. The original request is checked before admission, between sources/values, during every writer operation including empty writes, and after success. No new deadline or cancellation token replaces it. An output-free loop is bounded by SDK fuel.

These are logical bounds and SDK execution controls. SDK parsing, filters and internal captures may allocate independently of the output writer; the API does not promise a hard RSS cap or preemptive deadline. There is no filesystem loader, custom callback/object, debug dump or deserialization capability exposed to templates.

The fixed default-role factory uses at most three templates, 65,536 template/output bytes, 16 values, 4,096 variable bytes, 10,000 fuel and recursion 32. Its summaries and protocol contain generic policy only. Role errors preserve existing cancellation/deadline/request/name/allocation states and classify an additional instruction failure; that default factory does not expose a full SDK diagnostic object.

## Validation and active consumers

The affected Runtime suite protects two distinct brands/artifacts/agents and effective MCP names, overrides/default wrapping, malformed/duplicate/missing inputs, explicit optional Jinja logic, literal host tokens, trailing LF, UTF-8 bounds, entry admission, fuel/recursion, original cancellation/deadline and first writer cause. Role snapshots preserve the canonical policy byte-for-byte; the existing role tests continue protecting coordinator ownership, name boundaries and actual Hub descriptors. A real Hub-renderer fixture covers the current Kiro V3 Markdown body and native schema intent.

Run the affected Cargo test/doctest and strict Clippy checks, reusing valid unchanged evidence. Record actual results and platform in Task e643; compilation/encoding does not prove native model delivery.

The independent Runtime consumer in `crates/graphit-runtime/tests/kiro_hook_plans.rs` composes `CoreInvariant` through the actual [pure Kiro hook planner](../specs/rust_kiro_hook_plans.md). Two caller-supplied brands retain literal quotes, Unicode and host tokens in current V3 tuples. The planner emits `version: "v1"` for standalone encoding and PascalCase triggers; profile rendering uses the same native tuple values in YAML. The fixture rejects a foreign binding without conversion. The same original request reaches the planner after rendering; cancellation, expiry, missing required input and exact UTF-8/native-output byte boundaries remain covered. The removed `UnitCompletionReminder` has no replacement emitter. The qualified V3 planner and policy-catalog results are retained in the be6e C6-Rust evidence; unchanged code does not require another run.

The [policy instruction catalog](rust-policy-instructions.md), [skill catalog](rust-skill-instructions.md) and [Dream consolidation prompt](rust-dream-instructions.md) use central Markdown and explicit effective values. Productive consumers capture and render those templates:

- `compiled_skills.rs` captures Project, Global and Hub selections, calls `render_instruction`, and publishes the skill and its companions. The qualified local and remote publication fixtures reopen the installed companion files; global instructions use the same publication path (0257).
- `managed_roles::compile_definition` and `render_role` compose overrides and compiled defaults for owned roles, hooks and Source runtime assembly (9bd4). Host publication is qualified for the current Kiro V3, OpenCode 2 and AGY contracts in be6e.
- `foreground_dream.rs` selects `PolicyPrompt::DreamConsolidation`, renders it through the shared engine and supplies the result to the productive Records process on stdin. Its Memory/Records, Source/CLI and drain evidence is retained in 6aa2, f2ba and db94.

These consumers preserve SourceRef/authority, owned publication and runtime lifecycle. Runtime bootstrap loads current mandatory context at its runtime boundary, without embedding that context in static templates. Template acceptance N4 is tracked in c471; final product activation is tracked separately in cab0. Native host execution is distinct from the documented binding and publication contracts.

## Maintained sources and installed files

The five built-in Graphit skills are maintained under
`templates/agent-instructions/skills/`, with Runtime render fixtures. Their installed
files under `.agents/skills/`, `.codex/skills/`, `.claude/skills/` and `.kiro/skills/`
are generated for the selected agent and tool catalog. The ecosystem repositories
ignore those five installed skill directories rather than committing another source
copy. Local installation remains available to agents; this does not remove authored
skills, project overrides, templates, or tests. A running older daemon can regenerate
its installed instructions until the current Rust path is activated; editing that
output manually does not update the maintained templates.
