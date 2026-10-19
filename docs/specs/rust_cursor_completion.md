---
title: Rust Cursor one-shot Knowledge Ask
type: spec
updated: 2026-10-16
tags: [rust, ai, cursor, knowledge, streaming]
references:
  - target: {type: task, id: tsk-e6ff}
    relation: implemented_by
  - target: {type: task, id: tsk-9d92}
    relation: derived_from
---

# Rust Cursor one-shot Knowledge Ask

This contract covers the Cursor print adapter, its Knowledge Ask consumer and Incoming Conversation. Compatibility tests use controlled native peers without requiring a vendor binary, account or plan.

## Native boundary

The official [output contract](https://cursor.com/docs/cli/reference/output-format) defines NDJSON initialization, user/assistant messages, tool calls and a successful terminal result. [Parameters](https://cursor.com/docs/cli/reference/parameters) and [headless operation](https://cursor.com/docs/cli/headless) define print mode and literal prompt arguments.

The captured command uses `--print --output-format stream-json --model <literal> --mode ask`, followed by prepared documentary JSON as one literal argument. It preserves the captured executable, cwd and environment. It does not infer stdin support or create an agent file. Native argv/OS failures remain errors. No force, trust, automatic MCP approval, sandbox override, login or personal configuration mutation is part of this path.

Messages use complete-message output. Character delta mode is not selected because its flushes can duplicate text. Progress remains native; it does not establish completion.

## Decoder and lifecycle

[cli_cursor_wire.rs](../../crates/graphit-ai/src/cli_cursor_wire.rs) reuses the existing bounded lexical auditor. It borrows raw JSON so tool payloads, unknown fields and numeric lexemes survive without a Value round trip. Known message roles, content arrays, text blocks, tool call objects and identifiers are checked. Unknown events are audited without inventing their semantics.

[run_cursor](../../crates/graphit-ai/src/cli_cursor.rs) receives the caller's finite Original, installed ProcessHost, captured Request, concrete limits and a cooperative serial sink. It uses the real Process operation and retains its terminal and Remaining owner. It does not accept a boolean authority callback.

One initialization identifies the native session and observed model. Known events must agree with that session. A successful result requires subtype `success`, explicit `is_error:false`, a result string, native timing fields and the same session; request ID is optional. Any audited frame after the terminal, including an unknown event, invalidates the stream.

Answer becomes available only after native success, stdout and stderr EOF, complete cleanup and Original checks. Native messages, exit zero, session metadata or a result observed before exit are insufficient. A failure clears answer candidate and native result text while retaining actual counts, first cause and Process custody. Pending callbacks remain subject to the same cancellation/deadline, including an unterminated last frame decoded after Process returns.

Observed durations and request ID are exposed as native metadata. Durations are not converted into tokens or cost. Unobserved statistics do not mean zero usage or a negative capability claim. The native result JSON preserves additional fields without assigning them unsupported semantics.

## Ask consumer and authorization

The consumer uses common PreparedDocumentaryQuestion and the real borrowed ProjectReadAdmission. It checks the same Original/admission around progress and final delivery, transfers actual Remaining to the service before projection, and removes answer/native result on delivery denial. Session IDs never grant USER or PROJECT authority.

Source entry preflight, model selection, Auth, Data, filesystem scopes and UI remain their existing producers. The Incoming Conversation extension below reuses this driver with exact native resume. Compact and native clear remain outside this Cursor path; one-shot Ask does not simulate them.

## Verification

[Cursor tests](../../crates/graphit-ai/tests/cli_cursor.rs) use the existing native fixture executable and real ProcessHost. They cover progress before exit, backpressure, raw tool JSON, split CRLF, unterminated final frames, unknown events, required terminal, session mismatch, post-terminal data, native errors/exit, concrete bounds, cancellation, deadline and cleanup custody.

The recurring focal command is `cargo xtask test graphit-ai/cli_cursor`, available through the existing test selector catalog and covered by the pre-push workspace suite. Ask qualification additionally requires the real Graphit CLI/HTTP/STDIO route with Auth/Core/Data and a controlled vendor child. Such fixtures qualify the adapter and Graphit consumer, not an installed commercial provider or every supported host.

Compiled CLI tests use the [productive daemon helper](../../crates/graphit-app/tests/support/knowledge_cli_daemon.rs): `ForegroundSource::start_and_publish` receives the captured configuration, project provenance, binding and environment. `run` serves real control and Source; cancellation and joining retire discovery, runtime key and singleton. The separate read-only HTTP fixture retains its original permissions.

The CLI seed first publishes the authored corpus using its real least-privilege issuer. Before daemon startup it shuts down the seed Source, removes its Runtime key through the native outcome, checks both mutation and cleanup failures, and releases the seed issuer singleton. Cleanup failure retains the actual outcome, issuer and temporary installation and prevents a second daemon. The temporary installation remains owned until productive daemon cancellation and joining finish.

The daemon task owns the temporary installation through `run`, and its join output retains it with any real `ForegroundFailure`. `stop` borrows the task handle: dropping that wait preserves the handle and recoverable output in the guard. Explicit cleanup errors return the actual failure owner with the installation. Dropping the guard requests cancellation only; it does not attest joining or recoverability after the guard and its handle are both abandoned.

## Incoming Conversation continuation

The e6ff extension is implemented and its six focused cases and affected strict quality passed. It reuses the existing USER Conversation store and service-owned writer, with engine/provider `cursor`. Incoming MCP create/continue/list/delete retain their existing verified USER admissions, fresh PROJECT Knowledge access and trusted opt-in. Runtime/Local Ask remain stateless. Native clear remains Kiro-only.

[Parameters](https://cursor.com/docs/cli/reference/parameters) document `--resume [chatId]`; Graphit always supplies the exact private session ID previously observed in native Init and committed under that USER writer. It never uses latest, `--continue`, `create-chat`, a client-selected native ID or fallback creation. The request preserves captured program, environment, admitted absolute cwd and literal model. It retains `--print --output-format stream-json --model MODEL --mode ask`, followed by documentary JSON as one literal prompt argument; continuation inserts `--resume ID` before that final prompt. There is no inferred stdin transport.

The [store](../../crates/graphit-ai/src/knowledge_conversation.rs) configures that captured request without exposing the persisted native identity. The existing Cursor driver validates Init and the same session on each known message, tool call and result. The [Incoming consumer](../../crates/graphit-app/src/source_server/knowledge_conversation/cursor.rs) observes Init through typed custody before accepting progress. A missing or mismatched identity cannot commit. A failed resume retains the prior binding; it cannot silently replace the session. Cold reopen reloads the exact USER record using fresh admissions and selected physical/compilation source bindings. Unrelated corpus generation changes do not invalidate the selected sources.

The writer is held before the first process poll. The actual Process receipt and any Remaining owner transfer into existing custody before projection, persistence or another await. Success requires native success, exit0, both EOFs, real cleanup, current USER/PROJECT/Original and successful commit. Cancellation, deadline, service shutdown, physical denial and store failure withhold the answer and native result; cleanup retains the writer until actual process release/join. Public metadata never exposes a stored native-ID selector.

The native mode is still `ask`. Graphit adds no force/yolo/trust, automatic MCP approval or sandbox override and does not modify personal CLI settings. PROJECT authorization governs Graphit source access and delivery; it does not assert OS isolation or zero tools inside the CLI. Native argv/OS failures remain errors.

Three [SDK tests](../../crates/graphit-ai/tests/knowledge_conversation/cursor.rs) and three [Incoming tests](../../crates/graphit-app/tests/knowledge_conversation/cursor.rs) are registered with the existing Conversation targets. They use a controlled executable speaking the documented Cursor Init/result schema, not fields copied from another vendor. Compatibility tests do not require a Cursor binary, login, account, active plan or paid service; controlled peers exercise actual Process, Auth, USER store and MCP consumers. Missing selected executables retain their functional errors.
