---
title: Agent runtime context output
type: reference
status: partial
updated: 2026-10-16
references:
  - target: {type: task, id: tsk-be6e}
    relation: implements
  - target: {type: task, id: tsk-01eb}
    relation: derived_from
  - target: {type: task, id: tsk-a855}
    relation: implements
  - target: {type: task, id: tsk-a9ae}
    relation: implements
  - target: {type: task, id: tsk-1c5c}
    relation: used_by
  - target: {type: task, id: tsk-9bd4}
    relation: relates_to
  - target: {type: knowledge, id: Native_host_bindings_during_the_Rust_rewrite}
    relation: derived_from
  - target: {type: task, id: tsk-08ae}
    relation: derived_from
---

# Agent runtime context output

This contract is for Runtime integrators consuming the candidate pure Rust codec
in `graphit-runtime`, under `tsk-a855` / `ses-c98a`. Validation status is
**Linux library scoped checks and Root focused guard acceptance passed**.
Consumer qualifications and remaining functional boundaries are scoped below.
Encoding a payload does not establish native hook execution, model receipt,
authority or lifecycle completion.

## Responsibility and integration boundary

The codec receives an already selected semantic event and already composed text.
Its body processing is confined to memory. It returns bytes, the explicit native
binding, the semantic event and a delivery channel. The caller must consume the
channel through the actual host adapter; OpenCode plugin text belongs to a real
callback rather than command stdout.

The codec does not resolve a project, read live Memory or Task records, select an
engine, classify a tool mutation, issue or validate grants, mark bootstrap
delivery, execute hooks, invoke callbacks, sync state or release ownership.
It performs no filesystem I/O. `Context` and `Quiet` are output data, not proof of
authorization or successful delivery.

The source contract is exported by [lib.rs](../../crates/graphit-runtime/src/lib.rs)
and implemented in [output.rs](../../crates/graphit-runtime/src/output.rs).
Its consumers also need the [native host contract](../development/native-host-bindings.md),
[activation guide](../guides/agent_hook_activation.md),
[Hub consumer contract](hub_consumer_contract.md) and
[role credentials contract](role_credentials.md).

## Typed API and request lifetime

`OutputLimits` requires two caller-supplied positive values: `max_context_bytes` and `max_output_bytes`. There are no defaults. Both bounds apply independently to each encoded payload.

`OutputContext::new(limits, &original_snapshot_context)` retains only those limits and the original borrowed Core `SnapshotContext`, including its cancellation signal and deadline. It has no counters, statistics getters, message quota, reset, Clone, Debug or serde implementation. Repeated calls retain the same original request and apply the byte bounds independently; a failed call consumes no synthetic batch budget.

`OutputMessage` is `Context(&str)` or `Quiet`. Context text is borrowed UTF-8 and
must contain a non-whitespace character. Invalid byte decoding belongs to the
input producer. `HookEvent` is a closed semantic enum: `SessionStart`,
`UserPromptSubmit`, `SubagentStart`, `BeforeAgent`, `PostToolUse`, `AfterTool`,
`PreInvocation`, `PostInvocation`, `Stop`, `SessionEnd`,
`SystemTransform` and `SessionCompacting`.
`event.native_name(binding)` selects the exact supported native spelling.
`event.channel(binding)` and `event.accepts_context(binding)` expose only the
existing profile metadata for the actual consumer. A caller uses the latter to
avoid corpus acquisition at quiet boundaries; neither method grants authority
or introduces a second event matrix.

`encode(binding, event, message, &context)` returns
`Result<EncodedOutput, OutputError>`. The opaque `EncodedOutput` exposes only
`binding()`, `event()`, `channel()`, borrowed `bytes()` and consuming `into_bytes()`; it has no public bytes
constructor, `Clone`, `Debug` or serde implementation. The actual Root Source caller in 1c5c uses `String::from_utf8(encoded.into_bytes())` after validating channel, Auth, physical project and Original, without cloning the body. Its CLI and caller qualifications are recorded separately; tests require no installed vendor executable or account. Inspect the required delivery channel before transferring the payload. Body-bearing
`OutputMessage` also has no `Debug` or serde implementation.

`DeliveryChannel` is `CommandStdout`, `PluginSystem` or `PluginCompaction`. Empty plugin output means there is no text to append; it does
not mean that a callback ran.

## Exact mapping for this leaf

The matrix specifies codec shape, not native runtime qualification. Every
binding/event combination absent from the matrix returns `UnsupportedBoundary`.
An unsupported combination is rejected before body consumption or allocation.
Context supplied at a quiet-only observation or side-effect boundary returns
`ContextUnavailable`; it is never silently discarded.

| Explicit `NativeBinding` | Context-capable semantic event → native name | Context payload | Quiet-only event → native name / payload |
|---|---|---|---|
| `Codex` | `SessionStart` → `SessionStart`; `UserPromptSubmit` → `UserPromptSubmit`; `SubagentStart` → `SubagentStart`; `PostToolUse` → `PostToolUse` | Event JSON | `Stop` → `Stop`; `SessionEnd` → `SessionEnd`; empty stdout |
| `Claude` | `SessionStart` → `SessionStart`; `UserPromptSubmit` → `UserPromptSubmit`; `SubagentStart` → `SubagentStart`; `PostToolUse` → `PostToolUse` | Event JSON | `Stop` → `Stop`; `SessionEnd` → `SessionEnd`; empty stdout |
| `Qwen` | `SessionStart` → `SessionStart`; `UserPromptSubmit` → `UserPromptSubmit`; `SubagentStart` → `SubagentStart`; `PostToolUse` → `PostToolUse` | Event JSON | `Stop` → `Stop`; `SessionEnd` → `SessionEnd`; empty stdout |
| `Cursor` | `SessionStart` → `sessionStart`; `PostToolUse` → `postToolUse` | Additional-context JSON | `Stop` → `stop`; `SessionEnd` → `sessionEnd`; empty stdout |
| `KiroV3` | `SessionStart` → `SessionStart`; `UserPromptSubmit` → `UserPromptSubmit`; `PostToolUse` → `PostToolUse` | Plain UTF-8 | `Stop` → `Stop`; `SessionEnd` → `SessionEnd`; empty stdout |
| `Kimi` | `UserPromptSubmit` → `UserPromptSubmit` | Plain UTF-8 | `SessionStart`, `SubagentStart`, `PostToolUse`, `Stop`, `SessionEnd`; same native spelling, empty stdout |
| `Antigravity` | `PreInvocation` → `PreInvocation`; `PostInvocation` → `PostInvocation` | Injection JSON | `PostToolUse` → `PostToolUse` / `{}`; `Stop` → `Stop` / `{"decision":"stop"}` |
| `OpenCode` | `SystemTransform` → `context`; `SessionCompacting` → `compaction` | Plain text for the corresponding plugin channel | `Stop` → `session.idle`; `SessionEnd` → `session.deleted`; empty output consumed by plugin lifecycle callback |

The JSON shapes are:

- Event JSON: `{"hookSpecificOutput":{"hookEventName":"<native name>","additionalContext":"<body>"}}`.
- Additional-context JSON: `{"additional_context":"<body>"}`.
- Injection JSON: `{"injectSteps":[{"ephemeralMessage":"<body>"}]}`.

All command events use `CommandStdout`. At context-capable events, `Quiet` produces empty bytes. Antigravity PostToolUse retains its explicit empty-object quiet payload. Gemini CLI is removed; Google Gemini API providers remain independent.

OpenCode maps `SystemTransform` to `PluginSystem`, `SessionCompacting` to
`PluginCompaction` for the OpenCode 2 session hooks. The caller must
append the bounded text through the actual callback. This codec does not emit a
synthetic hook stdout JSON object for plugins.

Cursor `preToolUse`, subagent and
compact outputs remain unqualified. The former Kiro `agentSpawn` event is not
accepted by the current closed API. Antigravity has no `SessionEnd`, subagent or
compact mapping. OpenCode maps semantic Stop to the documented session.idle event and SessionEnd to session.deleted. These callbacks consume quiet output and request the existing Configure-authorized project rescan; they never deliver context, read Memory/Task/USER, or complete tasks. Native disposal and deletion abort and join outstanding plugin invocations.

Kiro requires explicit `NativeBinding::KiroV3`, canonical `kiro-v3`. There is no
launcher inference or personal migration in this codec. Current [V3 hook actions](https://kiro.dev/docs/hooks/actions/)
allow successful command stdout to add context; that contract alone does not
prove actual native delivery for this Rust producer.

## Payload bounds and failure behavior

The original Core request is checked at entry, during whitespace scanning, every bounded serialization write or text-copy batch, and before returning a completed output. Context byte length is checked before whitespace scanning. Quiet retains the final request check even when its payload is empty. No fresh request or deadline is created, and pure memory work consumes no filesystem I/O budget.

Typed Serde structures and `serde_json::to_writer` encode JSON once into a private bounded byte-vector writer. Each write checks Original, uses checked arithmetic against physical vector length and `max_output_bytes`, then calls `try_reserve_exact` before appending. There is no counting pass or second serialization. Text copies use batches of at most 4096 bytes. JSON Quiet payloads obey the same output bound.

The writer retains its first typed failure across Serde's transport error. `write_all` never retries persistent cancellation, including empty chunks; `flush` also checks Original. Partial private buffers are dropped on error, so no partial `EncodedOutput` escapes. Quotes, newlines, backslashes, controls and Unicode are serialized without interpreting template text.

Limits cover context and encoded bytes per payload. They are not a whole-process RSS limit, allocator hard cap or native-host memory guarantee. A failed payload can be followed by a valid payload through the same readonly context if Original remains valid; this neither resets nor replaces it. Authorized selection and protected handling remain caller responsibilities; the codec infers no authority from data.

## Closed errors

`OutputError` has static messages with no context body or personal path:

| Error | Boundary |
|---|---|
| `InvalidLimits` | A required limit is zero |
| `UnsupportedBoundary` | The explicit binding/event combination has no mapping |
| `ContextUnavailable` | Context was supplied to a quiet-only boundary |
| `EmptyContext` | Context contains no non-whitespace character |
| `LimitExceeded` | Context or encoded payload exceeds its byte bound, or output-length arithmetic overflows |
| `Cancelled` | The original request is cancelled |
| `DeadlineExceeded` | The original request deadline has elapsed |
| `Allocation` | The private output reservation fails |
| `Encoding` | Serialization or an otherwise unclassified encoding/request check fails |

An error is not a delivery acknowledgement. Encoding borrows the context readonly and changes no aggregate counters. Actual channel consumption and delivery remain caller responsibilities.

## Validation and remaining producers

The following historical focused commands passed on Linux before the current Kiro-only correction. Changed V3 consumer/codec checks require fresh receipts:

```text
cargo test --locked -p graphit-runtime --lib --test output
cargo test --locked -p graphit-runtime --doc
cargo clippy --locked -p graphit-runtime --all-targets -- -D warnings
```

| Check | Actual outcome | Recorded elapsed time |
|---|---|---|
| Scoped test command | 1 unit test and 8 integration tests passed | 7.887 s total command; 7.64 s Cargo compilation; 0.20 s integration suite |
| Documentation tests | 9 compile-fail doctests passed | 0.198 s total command |
| Strict Clippy, all targets, `-D warnings` | Passed | 3.379 s total command |

The original scoped packet also records passing formatting and diff checks.
Root `tsk-08ae` is completed at revision 26, with six active checks passed and
final Tracker NOFAULT for its focused scope. The actual `cargo xtask check`
passed with 67 tests and one ignored fixture in 4.008351 seconds total command.
The actual `cargo xtask verify-fast-guards` passed in 142.642311 seconds total
command, including isolation, formatting/Clippy negative probes, positive
fixture checks and a restored positive gate.

Actual Git pre-commit execution blocked all five injected `broken_fast_contract`
tests. The Runtime proof is the failed-test row, panic marker
`fast guard graphit-runtime sentinel`, and exact phase
`Rust guardrail: cargo test --locked -p graphit-runtime --lib --test output` in
the retained raw trace. The [Root guard contract](../development/rust-fast-guard.md)
and the linked evidence packet identify the raw log, original receipt,
proof-audit correction and final candidate hashes. The original receipt's zero
compiled-isolation count is preserved: its search string omitted `guard`; the
separate audit confirms one actual exact-line isolation proof. This counting
typo does not negate the raw proof or manufacture a native model receipt.

The historical Root snapshot matched 176 frozen hashes before that documentation
update. Its evidence did not cover the full 31-case guard campaign, the Hub
residual `tsk-7424`, Windows/macOS execution or product performance. Those are
limits of the recorded evidence, not current requirements to replay that campaign,
measure product performance. These focused passes cover the context contract. Workspace
membership, lockfiles and shared hooks remain Root-owned; the role leaf
`tsk-5a55` retains its own baseline and export acceptance.

[tests/output.rs](../../crates/graphit-runtime/tests/output.rs) contains codec
fixtures for the binding matrix, exact JSON shapes, escaped text, explicit Kiro
binding, quiet/observational boundaries, plugin channels, escaped per-payload bounds and
the original request's cancellation/deadline. Source doctests describe forbidden
cloning, debugging, deserialization and direct payload construction.
The executed checks prove codecs, limits, the original request and the opaque
API on Linux. They do not prove native-host receipt, installation, authority or
Windows/macOS behavior. No performance
gain is claimed.

The historical platform records `tsk-6b5f`, `tsk-9a3d` and `tsk-c3c2` are not a
new platform campaign for this codec correction. Current checks cover the affected
Rust consumers and journeys; unexecuted platforms remain unqualified. The current
documented Kiro V3 adapter and controlled Graphit integrations stay under
be6e/aa20. Agent tests require no vendor executable, login, account, plan or
remote catalogue; remaining platforms retain their actual functional scope. The Runtime parent `tsk-9bd4`, plugin lifecycle `tsk-aa20`,
native installation `tsk-bc28`, MCP dispatch `tsk-47fd` and startup/profile
authority `tsk-e849` retain their integration requirements.

The App [PROJECT hook assembler](rust_project_hook_context.md) now consumes actual PROJECT Memory and same-issuer Auth admissions with captured settings/version, compiled policy only and the existing Original. It loads no USER context or Task session/claim state. The Root caller in 1c5c composes actual Source/MCP/CLI transport and ordinary ReadMemory access. Controlled channel consumption and delivery are qualified under be6e/aa20 without a vendor installation or account prerequisite. Shared documents and daemon lifecycle retain their own owners. Other-vendor routing, tool mutation classification, async stop sync, ownership release and actual callbacks remain outside this codec; absent producers cannot be replaced by invented grants, boolean authority callbacks, or mocks.

Static role artifacts contain generic policy only: they must never persist
`Standing context`, mandatory-memory titles or bodies. Live context belongs only
to the appropriate protected runtime boundary. Completing this codec leaf can
qualify only the pure Rust library after its actual checks; it does not complete
the parent Runtime contract or establish native-product support.

The a855 evidence above is historical and was not rewritten to qualify the corrective single-pass codec. Current a9ae Runtime checks passed one writer test, nine functional output tests and scoped strict clippy on Linux. The real Root 1c5c Source/CLI caller compiled in the App lib+bin check; the affected App hook tests passed three writer and five Auth/Lance integration cases, and the current App strict pass includes both library and hook target. Native host execution, Root caller integration and perception remain separately qualified.
