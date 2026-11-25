---
title: Rust Qwen one-shot completion
type: spec
updated: 2026-10-16
tags: [rust, ai, qwen, streaming, process, knowledge]
references:
  - target: {type: task, id: tsk-0257}
    relation: relates_to
  - target: {type: task, id: tsk-e6ff}
    relation: implemented_by
  - target: {type: task, id: tsk-c847}
    relation: uses
  - target: {type: task, id: tsk-08c0}
    relation: derived_from
---

# Rust Qwen one-shot completion

The [wire reader](../../crates/graphit-ai/src/cli_qwen_wire.rs), [Process consumer](../../crates/graphit-ai/src/cli_qwen.rs) and [Ask adapter](../../crates/graphit-app/src/source_server/knowledge_ask/qwen.rs) consume Qwen headless JSONL with the captured launcher/model/cwd/environment and documentary stdin. This is the bounded Qwen unit in e6ff; the eleven CLI products and Conversation obligations remain.

## Capture and schema

The literal invocation is `qwen --input-format text --output-format stream-json --include-partial-messages --model <literal model>`. The existing Process Request carries zeroizing stdin through a non-TTY pipe. No documentary argv, login, profile/trust mutation, permission bypass, sidecar or bidirectional control session is introduced.

The [official headless guide](https://github.com/QwenLM/qwen-code/blob/9dacfdbe1a2047c85b29ebe504c40657daeab3bd/docs/users/features/headless.md), [stdin entry](https://github.com/QwenLM/qwen-code/blob/9dacfdbe1a2047c85b29ebe504c40657daeab3bd/packages/cli/src/llm.tsx) and [mode selection](https://github.com/QwenLM/qwen-code/blob/9dacfdbe1a2047c85b29ebe504c40657daeab3bd/packages/cli/src/config/config.ts) establish text piped without query/prompt as one-shot execution. Sources were checked on 2026-10-14 at that upstream commit; this is provenance, not a dependency pin.

Qwen has its [own types](https://github.com/QwenLM/qwen-code/blob/9dacfdbe1a2047c85b29ebe504c40657daeab3bd/packages/cli/src/nonInteractive/types.ts). The [one-shot emitter](https://github.com/QwenLM/qwen-code/blob/9dacfdbe1a2047c85b29ebe504c40657daeab3bd/packages/cli/src/nonInteractive/nonInteractiveHelpers.ts) emits system/init. Assistant/user/stream preserve native session and explicit nullable parent IDs. Assistant has message identity/model/usage; thinking signature is optional and tool input is arbitrary JSON, including null or arrays. Nested deltas, goal state and tool progress remain observations. Control envelopes grant no Graphit authority and trigger no Graphit-generated approval response.

Success requires result text, usage and permission-denial observations. Missing text or required usage is malformed; an empty string is an empty answer. RawValue preserves frames, duplicate tool-input keys and numeric/string lexemes. Optional cache/total/model usage fields remain native observations. Unknown top-level kinds are audited and ignored.

## Error origin, delivery and custody

The [upstream builder](https://github.com/QwenLM/qwen-code/blob/9dacfdbe1a2047c85b29ebe504c40657daeab3bd/packages/cli/src/nonInteractive/io/BaseJsonOutputAdapter.ts) can emit a subagent error with the same session and no parent marker. Graphit fails conservatively for any native result/error, without attributing it to the main agent or inferring origin from timing, turns, order or IDs. The [stream adapter](https://github.com/QwenLM/qwen-code/blob/9dacfdbe1a2047c85b29ebe504c40657daeab3bd/packages/cli/src/nonInteractive/io/StreamJsonOutputAdapter.ts) uses assistant messages to build its result. Graphit preserves the supplied result and does not claim to repair its internal provenance.

Answer comes only from native success after actual exit 0, both EOFs and Original/admission rechecks. Partial/subagent messages never compose Graphit answer. Serial callbacks retain cancellation/deadline and backpressure. Concrete JSON/IO/frame/response caps, first cause, Process terminal and actual Remaining custody survive failures. Native usage and model metadata remain u64 observations with numeric/overflow validation, without an execution quota. App transfers custody before projection and directly checks the borrowed Knowledge admission around progress and delivery. Denial suppresses answer and native_result_json while retaining accepted counts/process evidence. known_events counts accepted callbacks, not RPCs or Kiro notifications.

## Qualification

The Qwen runner uses controlled local children and real Graphit consumers in tests. Vendor installation, account and payment are not prerequisites.

See [Knowledge Ask](../architecture/rust-knowledge-question.md#ask-one-shot-com-qwen) and the [AI guide](../guides/ai_models.md).

## Incoming Conversation continuation

The Qwen extension in e6ff reuses the [private USER store](rust_knowledge_conversation.md)
and [Incoming consumer](rust_knowledge_conversation_app.md). The current
[headless resume contract](https://github.com/QwenLM/qwen-code/blob/main/docs/users/features/headless.md#resume-previous-sessions-headless)
was checked on 2026-10-16: `--resume <sessionId>` restores project-scoped recorded
history. The [configuration loader](https://github.com/QwenLM/qwen-code/blob/main/packages/cli/src/config/config.ts)
rejects a missing selected session; Graphit never selects latest, forks or retries
as a new session. Native chat recording must remain enabled for future resume.

`ConversationEngine::Qwen` uses the same strict private record.
`configure_qwen_request` validates the actual Original, engine/provider, captured
program, absolute cwd, stdin and the seven literal Ask arguments before appending
only the persisted `--resume` selector for a continuation. Create has no invented
session ID. `observe_qwen_session` accepts the decoded native Init session ID;
missing, invalid or mismatching identity prevents persistence, and mismatch stays
failed. Public conversation metadata contains the Graphit ID, not the private
native selector. The existing Qwen decoder and Process consumer are unchanged.

The [Qwen Incoming adapter](../../crates/graphit-app/src/source_server/knowledge_conversation/qwen.rs)
registers the USER turn before the first native poll, observes Init before progress,
and transfers actual Remaining before persistence or another await. Each turn
uses fresh PROJECT ReadKnowledge and actual Incoming USER read/write admissions.
Binding changes fail before a child; failed continuity, persistence or delivery
suppresses answer and native terminal JSON. List/delete remain USER metadata
operations; native clear remains Kiro-only.

The native headless tool policy remains captured as in Ask. Graphit sends no
autoapproval/yolo/trust override and changes no personal settings. This is not a
zero-tool or corpus-only sandbox: the CLI can apply its own configured tools,
hooks and session storage. PROJECT admission governs Graphit source access and
delivery, independently of those CLI capabilities. Upstream
[permission configuration](https://github.com/QwenLM/qwen-code/blob/main/docs/users/configuration/settings.md)
and [rule matching](https://github.com/QwenLM/qwen-code/blob/main/packages/core/src/permissions/rule-parser.ts)
do not provide a global tool wildcard equivalent to Grok `--deny *`; an empty
core-tools list disables nothing. Graphit does not invent a flag, Node adapter
or exhaustive denylist to conceal that distinction.

## Test environment

Acceptance uses the documented native protocol, controlled child peers and the
actual Graphit consumers. Installing or authenticating the vendor CLI, an active
account or paid plan, a paid service and a remote catalogue are not prerequisites.
A missing selected executable remains a real launch error; its tests use a
controlled missing program rather than requiring a personal vendor installation.
Existing native-peer and Main/CLI/MCP/HTTP/daemon proofs are reused when unchanged.
