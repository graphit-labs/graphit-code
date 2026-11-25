---
title: Rust Claude one-shot completion
type: spec
updated: 2026-10-14
tags: [rust, ai, claude, streaming, process, knowledge]
references:
  - target: {type: task, id: tsk-0257}
    relation: relates_to
  - target: {type: task, id: tsk-be6e}
    relation: uses
  - target: {type: task, id: tsk-9953}
    relation: implements
  - target: {type: task, id: tsk-e6ff}
    relation: implemented_by
  - target: {type: task, id: tsk-c847}
    relation: uses
  - target: {type: task, id: tsk-08c0}
    relation: derived_from
---

# Rust Claude one-shot completion

The [Claude wire reader](../../crates/graphit-ai/src/cli_claude_wire.rs) and
[Process consumer](../../crates/graphit-ai/src/cli_claude.rs) implement Claude Code
print-mode JSONL. [Knowledge Ask](../architecture/rust-knowledge-question.md#ask-one-shot-com-claude)
uses the captured `claude` launcher, literal requested model, authorized documentary
prompt, project cwd and startup environment. The selected CLI owns its account and
provider. Graphit does not log in, change profiles/trust, select a fallback CLI,
use `--bare`, or add a permission bypass.

## Capture and native protocol

The App invocation is `claude -p --output-format stream-json --verbose
--include-partial-messages --model <literal model>`. The documentary prompt goes
through the existing zeroizing Process stdin. The CLI options follow the
[official headless contract](https://code.claude.com/docs/en/headless).
The field shapes are checked against the official
[SDK types](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/types.py)
and [message parser](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/message_parser.py).

The reader accepts `system/init`, other native system events, `assistant` and
`user` message content, nested `stream_event.event`, `result`, and native rate-limit
observations. Unsupported top-level event types are audited and ignored. A flat
legacy delta is not a supported stream-event schema. Tool IDs, nested input/result
payloads, duplicate tool-input keys, numeric lexemes and native parent IDs remain
in borrowed `RawValue`; the callback receives the audited native frame itself.
There is no invented tool execution or Graphit-side retry of native API attempts.

`parent_tool_use_id` distinguishes subagent progress. A result with absent origin
or `origin.kind == human` belongs to the submitted prompt; results from injected
turns remain observed progress and cannot replace that answer. A primary result
must have the captured observed session, subtype `success`, `is_error == false`,
no deferred tool use and no native aborted terminal reason. The final text comes
from that result's optional `result` field, not accumulated partial/subagent text.
An absent result field yields unavailable answer evidence; an observed empty string
remains an empty answer. A primary native error is emitted and counted when accepted,
then the actual Process operation is stopped and joined with that first cause.

## Limits, delivery and custody

The caller supplies the original `Arc<SnapshotContext>`, real `ProcessHost`,
captured `Request`, `ClaudeLimits` and a serial `ClaudeSink`. JSON frame/depth/node/
field/array/string/identifier, stdout/stderr/frame and response limits
are concrete per-operation caps. Native usage is observational metadata with u64
schema validation, including overflow rejection; it supplies no execution quota. The reader reuses the existing lexical JSON auditor;
it does not interpret Codex events. Native usage counters are optional observations,
and missing values remain unavailable. `native_result_json` preserves the final
native bytes for an authorized receipt, including observed usage/model/cost fields.
Debug and error wrappers expose categories and counts without native payload text.

Each sink callback is awaited under the same cancellation/deadline, with serial
backpressure. Success requires the primary native result, actual child success,
both stdout/stderr EOF and context rechecks. A Process failure or unfinished pipe
cannot finalize the candidate. The consumer stores the terminal before EOF projection;
it does not await a final callback while a failed terminal retains `Remaining`.
The [App adapter](../../crates/graphit-app/src/source_server/knowledge_ask/claude.rs)
transfers real `Remaining` immediately to the existing service owner before error
formatting or projection. The owner performs actual repair/shutdown/join.

App borrows the real Knowledge `ProjectReadAdmission` and calls `check_read`
before and after every native progress emission and again around delivery. No boolean
authority callback or synthetic grant is supplied. Denial after native effects retains
accepted-event counts and the process receipt. Both SourceServer and SourceProxy denial
guards remove the completion answer and `native_result_json`, which can contain answer
text. The provider's `known_events` counts only callbacks actually accepted; it is not
Kiro notifications, token counts or an inferred native RPC count.

## Validation scope

The [wire/Process tests](../../crates/graphit-ai/tests/cli_claude.rs) cover native frames before exit, backpressure/Original identity, byte-preserved tools, injected result isolation, final unterminated frame/EOF, native errors, nonzero exit or missing result, cancellation/deadline and concrete limits.

The [App test target](../../crates/graphit-app/tests/knowledge_claude_ask.rs) exercises real Auth/Knowledge/Data/SourceProxy with a controlled native child and the compiled Rust CLI/MCP STDIO. The focal selector is `cargo xtask test graphit-ai/cli_claude`; App journeys are covered by the workspace pre-push gate.

These fixtures do not authenticate an installed Claude provider, qualify native
Windows/macOS, deliver Claude Conversation/resume, or complete the other active CLIs.
e6ff remains open for that product scope. Current Kiro V3 retains its separate
protocol and qualification; older V2/preview receipts remain historical.

Claude Knowledge conversation create/continue is composed by the Incoming MCP
[conversation consumer](../specs/rust_knowledge_conversation_app.md), using the
existing native Claude producer and USER store. It binds continuation to the
observed native ID and fresh selected PROJECT sources; list/delete operate on
Graphit metadata and native clear remains Kiro-only. See tsk-9953 for its own
affected-suite evidence; this does not extend the stateless Ask qualification.
