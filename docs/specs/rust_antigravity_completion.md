---
title: Rust Antigravity Knowledge Ask and Conversation
type: specification
status: qualified
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-117d}
    relation: implemented_by
  - target: {type: task, id: tsk-e6ff}
    relation: relates_to
  - target: {type: task, id: tsk-c847}
    relation: uses
---

# Antigravity Knowledge Ask and Conversation

The [native Process adapter](../../crates/graphit-ai/src/cli_antigravity.rs),
[wire reader](../../crates/graphit-ai/src/cli_antigravity_wire.rs) and
[Ask consumer](../../crates/graphit-app/src/source_server/knowledge_ask/antigravity.rs)
use the existing documentary preparation and borrowed PROJECT Knowledge admission.
The provider is Antigravity's `agy` CLI, selected at service startup through
`CliCompletionProvider::Antigravity`. Completion provider selection is independent
of the native Agent, Skill and MCP host configuration; it does not add or convert
a `NativeBinding`. This unit
includes SDK and compiled CLI/HTTP/STDIO consumers; source files alone do not
establish a qualified product. The seven AI cases and six App journeys are functionally qualified; affected
strict quality passed for AI, App and the portable runner.

## Native input and output

The official [headless contract](https://antigravity.google/docs/cli/headless/)
was consulted on 2026-10-16. Invoke the captured executable with literal arguments
`--input-format stream-json --output-format stream-json --model MODEL`.
Encode one native user message whose content is the prepared documentary JSON
string, append a newline and pass it through the existing zeroizing Process stdin.
The writer closes stdin after that message. Native execution finishes the turn
and exits after emitting its result. No document is passed through argv, a shell
or a temporary file. Print mode's `-p` flag is a different input surface and
must not be combined with this input stream.

The codec uses serde and the existing lexical JSON auditor. It borrows native
frames and opaque tool/subagent values, preserving numeric and string lexemes.
One init records the actual conversation ID and optional model. Step updates
retain native indices, state, category and data; repeated updates to the same
step are valid. The implementation does not manufacture tool IDs or infer
permissions from the tool list. Unknown events are audited without invented
meaning. A result and all steps must belong to the initialized conversation.
A native startup failure may instead arrive before init with an empty ID.

Only a successful terminal result's response can become the answer. Progress,
including agent text, tool output and subagent text, never builds the final
candidate. All other terminal statuses, error fields, mismatched sessions,
malformed events, missing success response or data after the terminal prevent
answer delivery. The successful response may be empty. Native usage and timing
stay in the exact result JSON; no synthetic usage/cost DTO or derived model
identity is produced. The [official mode reference](https://antigravity.google/docs/cli/modes/)
is separate from Graphit's authorization. Graphit sends no automatic approvals
and does not alter the user's permission configuration.

## Authority and process lifetime

The consumer keeps the caller's actual Original and borrowed Knowledge read
admission. It checks them before and after progress and delivery. Native model
names, CLI metadata and a valid process exit grant no PROJECT access.

The existing ProcessHost owns the child, stdin writer and pipe readers.
Backpressure remains cooperative with the same cancellation and deadline.
Concrete frame/JSON/stdout/stderr/response limits are per operation; no resource
ledger or global quota is added. The final response requires actual exit0,
stdout and stderr EOF, complete cleanup, no outstanding Remaining and valid
final guards. Cancellation, denial, native failure or malformed output preserves
the accepted prefix, first cause and actual Process custody for repair. The App
adopts Remaining before any projection or further await. Denial removes both
answer and native_result_json from the reply.

Two output formats remain because callers consume them: completion.answer is
the final text; native_result_json preserves the native terminal and its metadata.
The receipt does not retain an extra parsed result DTO or a classification layer.
Debug and errors exclude prompts, environment, native payloads and credentials.

## Qualification scope

The [AI cases](../../crates/graphit-ai/tests/cli_antigravity.rs) use the existing controlled Process fixture for native progress, raw tool values, session/terminal rules, concrete bounds, cancellation/deadline, backpressure, EOF and exit status. The [App journeys](../../crates/graphit-app/tests/knowledge_antigravity_ask.rs) use the existing real Data/Auth/Core and productive ForegroundSource composition for CLI/HTTP/STDIO, literal stdin/model/environment, denial and shutdown/join. Its STDIO oracle still expected another protocol's `input`; the actual peer emitted native `tool_info.parameters`. Only that assertion was corrected, preserving duplicate keys, large integers and the exponent lexeme. The portable registered selectors are `cargo xtask test graphit-ai/cli_antigravity` and `cargo xtask test graphit-app/knowledge_antigravity_ask`. The original portable command's exit1 proves that a failing selected target blocks the runner.

These fixtures do not authenticate a commercial account or certify Windows,
macOS, Conversation/resume, Live or Dream. Those active capabilities remain
separate migration obligations in e6ff. No earlier Cursor/Qwen/Kimi result is
claimed as Antigravity runtime proof, and their unchanged suites are not repeated.

## Incoming Conversation continuation

Antigravity Conversation is implemented, integrated and qualified by controlled native peers and actual Graphit consumers; affected AI/App quality also passed. The [official headless contract](https://antigravity.google/docs/cli/headless/#continue-a-conversation) distinguishes specific --conversation ID from --continue, which selects the most recent session. The extension uses only the exact conversation_id already observed in init and confirmed by the successful result. Create generates no ID. Continue and cold reopen reuse the private USER binding without title/latest lookup, import or fallback creation.

The request uses the mature stream-json stdin encoder and Process driver: --input-format stream-json --output-format stream-json --model MODEL. Continuation appends only --conversation ID. One prepared documentary user message is written, stdin closes, and the process must produce the current turn's result and actually exit before persistence and delivery. Program, environment, model, admitted project cwd, fresh USER/PROJECT/Original and unchanged selected physical/compiled sources stay required per turn. The same service-owned Conversation custody retains the USER writer and actual Process/Remaining until join, including native-future abandonment, cancellation, deadline and shutdown.

The Conversation sink validates Init against the persisted expected identity before exposing progress. Native steps and result must keep that ID, and only result.SUCCESS response supplies the current answer after exit0/EOFs/complete cleanup and successful USER commit. Intermediate text, tool or subagent steps do not compose the answer. Native num_turns, usage and duration_seconds are cumulative for the session; they remain exact native terminal metadata. Graphit does not reinterpret them as per-turn counts, subtract totals or invent a usage DTO.

The captured headless policy remains unchanged. Tools can have effects permitted by native settings, including workspace file access; Graphit adds no autoapproval/trust change or process-sandbox claim. Compatibility validation uses official contracts, controlled native peers and actual Graphit consumers, without requiring an installed vendor binary, account, login or paid service. Three SDK and three Incoming focals are registered for exact private resume/cold reopen, authority/source/identity rejection, native failure without commit and real progress/cancel/deadline/shutdown joins. Ask and other engine proofs are reused rather than rerun. Native clear remains Kiro-only; metadata delete leaves vendor history intact.

The current captured completion providers have a Conversation engine, so the getter returns that engine directly. The obsolete None/wildcard branch was removed; a genuinely unavailable captured Ask service still fails before locating or launching a native session. The prior test requiring rejection of Gemini Conversation was obsolete and is replaced by the controlled missing-program case, while the current AGY continuation contract remains intact. Existing workspace tests include these children without a new runner.
