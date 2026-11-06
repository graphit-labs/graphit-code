---
title: Rust Grok Knowledge Ask and Conversation
type: specification
status: qualified
updated: 2026-10-16
references:
  - target: {type: task, id: tsk-e6ff}
    relation: implemented_by
  - target: {type: task, id: tsk-c847}
    relation: uses
---

# Grok Knowledge Ask and Conversation

This existing e6ff delivery includes the AI adapter and productive App CLI/HTTP/STDIO
consumers. Qualification uses the documented protocol, controlled native peers
and actual Graphit consumers; no vendor installation or account is required.

## Official native contract

The [headless guide](https://docs.x.ai/build/cli/headless-scripting),
[upstream scripting documentation](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/14-headless-mode.md)
and [native reducer](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/src/headless/reducer/acp.rs)
were inspected on 2026-10-16. Their current headless interface does not consume
piped stdin. Use literal arguments --prompt-file PATH --output-format streaming-json
--model MODEL --verbatim --deny '*'. The final two arguments are literal argv
values, without shell expansion. The App writes the prepared documentary JSON to a private
NamedTempFile using the existing tempfile dependency, closes its file handle and
retains TempPath until actual Process observation. A pending Remaining record
adopts the file in the same existing service owner until reap/shutdown observes
that process. The document is absent from argv and
stdin. File cleanup also runs on error, cancellation and abandoned execution;
ProcessHost retains custody of the actual child independently. The prompt String
is dropped before awaiting native execution, avoiding redundant retained input. No shell, IDE shim,
permission bypass, trust/profile modification or compatibility path is added.

The [native permission contract](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/22-permissions-and-safety.md#tool-names)
defines a bare `*` rule as matching every tool and gives deny rules precedence
over allow rules and permission modes. Ask supplies `--deny '*'` to prevent
model-requested local, shell and MCP tools from expanding the already admitted
documentary input. `--verbatim` alone does not restrict tools; headless cancellation
of permission prompts also leaves auto-approved tools available. PROJECT and
ReadKnowledge authorize Graphit's preparation and delivery, not those external
capabilities. This native rule is not an OS sandbox: Grok still owns its configured
startup hooks, credentials and session persistence. Graphit does not rewrite those
personal settings or claim process filesystem isolation.

The native stream has type-tagged text/data, thought, tool_call/tool_call_update,
usage, plan, available_commands and lifecycle messages. Tools carry native
camelCase toolCallId/toolName/rawInput/rawOutput; their opaque values remain
borrowed RawValue. Only text.data contributes to the native answer candidate.
Thoughts, tools, plans, usage and lifecycle results never contribute. Native end
has stopReason, sessionId and requestId, not a response field or an init preamble.
requestId may be the native empty string. Usage/cost fields are optional and
preserved in the terminal JSON, without derived totals, fake zeros or identity.

End/end_turn is necessary but not sufficient: success also requires actual exit0,
stdout/stderr EOF, complete cleanup/join and the same Original and PROJECT Knowledge
read admission before/after effects, progress and delivery. Native errors,
max_turns_reached, other stop reasons, missing/duplicate terminal, any postterminal
frame, malformed data, concrete limits, denial, cancellation or deadline prevent
answer/native terminal delivery. Unknown events are audited without invented
meaning. One-shot Ask stays stateless; the separately qualified Incoming
Conversation extension below uses actual native continuity.

## Composition and ownership

Use CliCompletionProvider::Grok via explicit ai.cli over an existing host binding;
there is no new NativeBinding or Host configuration. Reuse ProcessHost, the JSON
lexical auditor, serde and SourceProxy/Data/Auth preparation. New sources are
crates/graphit-ai/src/cli_grok.rs, cli_grok_wire.rs, tests/cli_grok.rs, App
source_server/knowledge_ask/grok.rs and tests/knowledge_grok_ask.rs. Shared exports,
AskService, Source dispatch and MCP progress now compose this adapter. The portable
runner registers graphit-ai/cli_grok and graphit-app/knowledge_grok_ask, preserving
the existing Cargo environment boundary and other provider selections. The AI
guard is also in pre-commit after its observed 0.52-second run; App consumers remain
selected explicitly.

## Test environment

Acceptance uses the documented native protocol, controlled child peers and the
actual Graphit consumers. Installing or authenticating the vendor CLI, an active
account or paid plan, a paid service and a remote catalogue are not prerequisites.
A missing selected executable remains a real launch error; its tests use a
controlled missing program rather than requiring a personal vendor installation.
Existing native-peer and Main/CLI/MCP/HTTP/daemon proofs are reused when unchanged.

## Incoming Conversation continuation

Grok Conversation reuses the same private USER Conversation record and Process supervisor rather than creating a second store or owner. Incoming create/continue/cold reopen require current USER read/write and fresh PROJECT Knowledge admission with unchanged selected physical/compiled source bindings.

The current [headless reference](https://docs.x.ai/build/cli/headless-scripting), [session contract](https://docs.x.ai/build/features/sessions) and [upstream headless guide](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/14-headless-mode.md) distinguish `--resume ID` from `--session-id`, which creates a session. Grok can resolve titles as well as IDs; Graphit uses only a full UUID observed in native `end.sessionId`, preserving the ID path rather than title matching. Latest, continue, fork, supplied creation IDs and fallback creation are excluded. No ID is generated locally or exposed as a stored native selector.

Native `streaming-json` has no Init. The Conversation sink observes the End identity before commit, while earlier text progress remains native progress under current Graphit authority. A differing, missing or invalid End identity cannot replace the persisted binding or release a final answer. Messages do not need an invented session echo. The mature driver/wire, raw terminal projection and native `text.data` answer semantics are unchanged; an End/end_turn still requires actual exit0, both EOFs, successful cleanup and current authority.

The captured request retains `--prompt-file PATH --output-format streaming-json --model MODEL --verbatim --deny *`; continuation adds the exact private `--resume ID`. A private NamedTempFile carries prepared documentary JSON without argv or stdin copies. Conversation custody adopts its TempPath before the first process poll and holds it with the existing USER turn/Remaining owner until actual terminal observation or Host join, including an internally dropped native future. There is no parallel manager. Pre-submit rejection removes the unused file; post-effect denial/store failure suppresses answer and native terminal data while cleanup finishes.

The native deny rule remains two literal arguments and is not an OS sandbox claim or personal-settings change. Provider selection uses actual `ai.cli=grok` independently of Host9. Tests reuse the productive Foreground helper with optional explicit completion provider; existing host-selected callers keep None without behavioral changes. Controlled peers test actual Auth/USER/PROJECT/Data/Core/Process/MCP consumers without a vendor binary, login, account or paid plan.

Three [SDK](../../crates/graphit-ai/tests/knowledge_conversation/grok.rs) and three [Incoming](../../crates/graphit-app/tests/knowledge_conversation/grok.rs) focals plus one [custody test](../../crates/graphit-app/src/knowledge_ask_service/grok_tests.rs) with internal-drop and actual-Remaining phases are registered. Commands use the relevant module filters: `cargo xtask cargo test -p graphit-ai --test integration knowledge_conversation::grok::`, `cargo xtask cargo test -p graphit-app --test integration knowledge_conversation::grok::` and the exact App library custody case. The workspace guard already includes them.
