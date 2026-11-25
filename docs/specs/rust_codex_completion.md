---
title: Rust Codex completion stream
description: Consume the pinned Codex JSONL protocol incrementally through the existing ProcessHost and caller's original context.
type: specification
updated: 2026-10-12
references:
  - target: {type: task, id: tsk-658c}
    relation: corrected_by
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: task, id: tsk-e7aa}
    relation: implements
  - target: {type: task, id: tsk-fd60}
    relation: derived_from
  - target: {type: task, id: tsk-29bb}
    relation: corrected_by
  - target: {type: task, id: tsk-81a9}
    relation: implements
  - target: {type: task, id: tsk-ba1c}
    relation: corrected_by
  - target: {type: task, id: tsk-a6e8}
    relation: implements
  - target: {type: task, id: tsk-08c0}
    relation: derived_from
  - target: {type: task, id: tsk-b978}
    relation: uses
  - target: {type: task, id: tsk-de5f}
    relation: uses
  - target: {type: task, id: tsk-8d0c}
    relation: uses
---

# Rust Codex completion stream

`graphit_ai::cli_codex::run_codex` consumes Codex stdout JSONL with the existing
[ProcessHost](../../crates/graphit-process/src/lib.rs). It sends each validated known
event to the caller's sink before transferring that same owned event to model
state. The caller supplies the complete captured Process `Request`, the host,
model identifier, execution limits and original `Arc<SnapshotContext>`.

The wire schema is pinned to Codex **0.160.1**, commit
`d27764b82f7118f674371e6d6e76271d9d606edb`. The port reuses the qualified
serde/serde_json `RawValue`, typed field audit, slots and owned arena decoder.
It uses the workspace dependencies without adding a library or changing a pin.

The completion operation requires the Original's finite deadline. Process
rejects a service context without one before execution. The serial sink adapter
also calls `required_deadline()` before awaiting delivery, including the final
fragment after actual EOF. A rejected context retains its real IO cause and
`Context(ErrorKind)` classification; no default deadline, renewed Original or
synthetic Process terminal replaces the observed receipt. Finite cancellation
and expiry continue to drop the pending callback without replay.

## Calling contract

```rust
pub async fn run_codex<'n, S: CodexSink>(
    run: &mut CodexCompletionRun<'n>,
    host: &ProcessHost,
    request: Request,
    purpose: AttemptPurpose,
    estimate: Option<KnownTextEstimate>,
    limits: CodexStreamLimits,
    sink: &mut S,
) -> Result<CodexReceipt<'n>, CodexFailure<'n>>;
```

Create the run with `CodexCompletionRun::new(&original, requested_model,
&wire_limits, completion_limits)`. Use one run for its actual Request, Retry and
Synthesis attempts. The consumer takes its original context from that run; it
has no second context argument. Process enforces receiver/request identity
before spawning. This entrypoint does not discover a CLI, launch arguments,
working directory, environment, profile, credential or provider configuration.
The requested model is captured caller evidence, not a model identity reported
by this wire schema. `requested_model: Option<&str>` preserves that distinction:
`None` leaves model selection to the native CLI default and keeps the requested
model evidence absent; `Some(model)` retains the literal selection, and
`Some("")` is invalid. Ask and Dream consumers that already selected a real model
pass `Some(model)`. The completion layer does not infer an observed model or add a
model argument to the caller-owned Process request.

The sink implements `CodexSink::push(event: CodexEventView<'a>,
original: &'a SnapshotContext)`. Calls are serial and awaited. Views borrow the
single decoded event for the duration of the callback. A sink must cooperate
with cancellation, be safe when its future is dropped, and bound any copies or
persistence it performs. The consumer does not queue DOMs, clone events, decode
an event twice or detach delivery work. The raw frame is marked consumed
before awaiting its callback, so an interrupted callback cannot replay it at EOF. It awaits the actual Process supervisor
and its cleanup using `Retention::None` and retains the operation's stop owner
through that await.

The final unterminated frame is delivered only after true stdout EOF, when
Process may already have returned. Its pending callback still observes the same
original cancellation and absolute deadline through Tokio selection. Interruption
drops that cancellation-safe callback, retains the actual successful Process
terminal and observed usage, and prevents both a finalized answer and replay.
The original context IO cause is retained without synthesizing an error from its
kind. Native usage requires an accepted `turn.started` before decoding completion.

## Wire and model observations

Eight event types preserve the native data: `thread.started`, `turn.started`,
`turn.completed`, `turn.failed`, `item.started`, `item.updated`, `item.completed`
and `error`. The nine item bodies are agent message, reasoning, command
execution, file change, MCP tool call, web search, todo list, collab tool call
and item error. Their borrowed views retain their exact fields, status and
collections, including raw arbitrary JSON and absent/null/empty distinctions.
Provider error messages are explicit payloads to the authorized sink. Debug,
Display and error sources do not print those payloads, raw JSON, model names,
paths or credentials.

Valid unknown events/items are audited and ignored; they do not establish
`Structured` classification. A known validated event does establish it, even
when its later state transition fails. Sink acceptance is an observed side
effect: a subsequent state, context or process failure neither retracts it nor
replays it. The last valid **completed** agent message wins. Started and updated
messages do not become answers. Empty text is a present message; absent text
never becomes an invented empty answer.

An answer requires a completed turn, a completed model message, actual stdout
EOF and successful Process termination with both pipes finalized at EOF.
`PipeEnd::Closed`, drain, cleanup or exit status alone never establish wire EOF.
Missing evidence on an otherwise successful execution produces
`CompletionOutcome::EvidenceUnavailable`. A nonzero exit or Process failure
returns `CodexFailure` and never publishes a successful completion, even if
native message/usage events were already observed.

## Framing, execution limits and usage

Only stdout is JSONL. LF ends a frame; only a CR immediately preceding LF is
removed. Native byte fragmentation, split UTF-8 scalars and multiple frames in
one chunk are supported. A nonempty final unterminated fragment is decoded only
after the actual stdout terminal reports EOF. A malformed JSON frame has no
plain-text fallback. Stderr is counted separately and never parsed or treated
as an answer.

All eight `CodexWireLimits` fields are positive: frame bytes (at most 8 MiB),
depth (at most 128), nodes, fields, array items, string bytes, decoded string
bytes and ID bytes. The framer bounds pending frame bytes before append; one trailing CR is held
as delimiter lookahead until LF or another byte disambiguates it. Allocation is
fallible. Invalid wire policies fail at run construction before any child. `CodexStreamLimits` separately bounds positive stdout/stderr byte
counts. `CompletionLimits` requires positive limits for actual Graphit attempts,
response bytes and model ID bytes. Frame counts use checked arithmetic and are
observations rather than an additional frame quota. Physical frame bytes and
the original deadline remain enforced. Events, errors and answers retain no
resource permit or original Arc.

Usage has explicit `Observed`, `Estimated` and `Unavailable` provenance. Native
usage requires five nonnegative metrics, with checked `u128` totals. Valid native usage is recorded at decode, so sink rejection or cancellation of
that event cannot erase it. Invalid
native usage cannot fall back to an estimate. The bytes/4 floor estimate is
used only when the caller explicitly supplies `KnownTextEstimate`; its coverage
is known text only, with an unknown native tokenizer. Failed and abandoned
attempts account exactly once, observed native usage survives execution failures,
and the first failure remains sticky. Usage values and explicit estimates are evidence; this API does not use them as token quotas, billing limits or native context-window guarantees. `NativeRpcCalls::Unobserved` prevents
Graphit attempts from being confused with the vendor CLI's internal RPCs.

## Receipts and failure handling

`CodexReceipt::usage()` exposes the actual attempt evidence independently of optional completion. It uses the existing completion evidence when present and retains a snapshot only for an incomplete attempt without completion. Nonzero exit can legitimately keep `completion == None` while the receipt still reports observed usage. No synthetic CompletionFailure, answer, EOF or IO cause is created to carry that evidence. The private incomplete finish returns its already-accounted usage alongside an optional existing failure; Drop does not account it again.

`CodexReceipt` exposes optional completion and the actual optional
`Result<graphit_process::Outcome, graphit_process::Failure>`, accepted consumer
byte counts, successfully decoded frame count, accepted known-event count and
classification. Bytes admitted to the consumer and events accepted by the sink
are separate observations. `process_mut()` borrows the same terminal so the
caller can repair its retained `remaining` owner if needed. Prelaunch policy,
context and admission failures have no Process terminal.

`CodexFailure` preserves the first typed reason and the entire receipt. Its
safe error chain retains actual IO, allocation, wire and state causes; it does
not fabricate an IO error from a diagnostic string. Context, invalid wire,
provider/state, sink and limit failures stop delivery, await supervisor cleanup
and return the real terminal. An interrupted attempt cannot claim EOF or an
answer. Accounted usage remains available through the run's totals and through the receipt's `usage()` accessor, including when no state failure or completion exists.

For a caller that must return owned resources after its attempt scope ends,
`CodexReceipt::into_parts()` moves the original completion and optional Process
`Outcome`/`Failure`, together with the same usage, accepted byte counts, frame
count, known-event count and classification. It performs no accounting,
cancellation, cleanup or replacement of the terminal. Any Process `remaining`
owner moves with that exact terminal and still needs the caller's real repair
and join.

`CodexFailure::into_parts()` moves its actual reason, entire receipt and optional
`CodexOwnedCause`. That opaque owned cause retains the existing IO/allocation
object and payload-safe diagnostics/error chain. A State cause remains the
`CompletionOutcome::Failed(CompletionFailure)` in the receipt; a Process cause
remains its actual Process `Failure`. Neither API creates an IO error from a
kind or diagnostic, or an absent completion/Process terminal from a placeholder.

The receipt parts' `completion` still has the original requested-model lifetime.
Consume an answer's authorized text inside that scope, or move an actual owned
`CompletionFailure` out of its Failed variant. The owned Process terminal,
usage, counters and opaque cause can then survive the attempt's local model,
wire limits and Original bindings. Keeping the borrowed completion in a returned
object does not make it owned. These parts do not prove tool effects, Memory
publication, native readonly enforcement or successful vendor inference.

The `tsk-e7aa` consuming-parts correction passed the affected Linux Codex suite:
20 tests, no failures or ignored cases, including three new cases that extract
owned components after the local model/run scope. They retain a real nonzero
Process terminal with completion absent and observed usage, an owned native
State failure, and the original cancellation IO cause with no fabricated
Process terminal. Harness time was 1.07 s and external wall time was 285.04 s,
including the shared build queue. Selected AI library/Codex-target Clippy passed
with warnings denied (wall 79.91 s). Owned formatting, whitespace and all three
local links passed. Commands and results are in
`.build/rust-codex-owned-parts/qualification.json`. These cases do not induce OOM
or assert a native `remaining == Some` observation; they qualify ownership
transfer through the real Process fixture, not vendor inference, readonly
policy, RSS or other platforms.

## Qualification and scope

The affected suites are the ported wire/state corpus,
[`tests/cli_codex.rs`](../../crates/graphit-ai/tests/cli_codex.rs) and the existing
[`tests/cli_plain.rs`](../../crates/graphit-ai/tests/cli_plain.rs) regression.
The native tests reuse the existing `ai-cli-plain-fixture` binary and real
ProcessHost. Fixture TCP ACKs control each native write; incomplete frames use a
separate test-control ACK and do not depend on a callback or an OS read boundary.

Initial qualification in `tsk-a6e8`: 33 wire/state tests, 14 real-Process Codex tests
and 16 unchanged Plain CLI regression tests passed on Linux; selected Clippy
passed with warnings denied. Wall times were 508.73 s for units, 372.13 s for
integration and 71.36 s for Clippy, including shared Cargo queue and builds.
Test harness times were 0.47 s, 1.04 s and 0.36 s respectively for units,
Codex and Plain. These are qualification costs, not vendor inference latency.
That initial 63-test result does not cover the later pending callback at final
stdout EOF regression.

Correction `tsk-ba1c` qualifies that EOF boundary with two additional native
tests: an accepted `turn.started` frame followed by a `turn.completed` fragment
without LF whose callback stays pending. Original cancellation and expiration
of the original deadline each drop that callback once, preserve observed usage
and the successful Process terminal with both pipes at EOF and completed
cleanup, and return no answer. The final callback remains unaccepted.
`cargo test --locked --offline -j 1 --target-dir
.build/rust-records-data/knowledge-index-source/target -p graphit-ai --test
cli_codex` passed all 16 tests on Linux (harness 1.04 s; wall 11.00 s).
Selected Clippy (`--lib --test cli_codex -- -D warnings`, with the same locked,
offline and target settings) passed in 6.95 s wall time. The first suite attempt
stopped before tests while the shared lockfile was being reconciled; the
coordinator resolved it without changing AI dependencies or relaxing
`--locked`. Receipts and qualification scope are retained in
`.build/rust-codex-eof-original/qualification.json`. No broader suite or vendor
inference measurement is implied by this correction.

Cleanup `tsk-81a9` removes token quotas and the extra frame-count quota from
`CompletionLimits`, retaining its three functional limits and all eight shared
wire limits. The selected state suite (`--lib codex_completion::tests`) passed
19 tests, including exact observed totals above `u64`, explicit known-text
estimates, invalid native usage and checked frame-count overflow. Wall time was
5.00 s; harness time was 0.40 s. The affected real-Process Codex suite passed all
16 tests (wall 54.16 s, including the shared build queue; harness 1.02 s).
The initial state-suite filter selected zero tests and is retained as an
unexercised attempt. The first integration attempt failed before tests on a
concurrent Auth compilation error; its owner repaired it before the successful
retry. Evidence is in `.build/rust-codex-usage-observation/`. These measurements
qualify this API cleanup and its retained behavior, not vendor inference.
Selected Clippy for the AI library and Codex integration target passed with
warnings denied (wall 111.70 s, including the shared build queue).
The App `knowledge_question` target compiled with the three-field initializer
(wall 184.75 s, including the shared queue and dependency work). Its first
attempt stopped on a concurrent ForegroundTasks syntax error, repaired by its
owner. This consumer check proves compilation only; it does not replay the App
integration suite.

These local fixtures qualify the Rust consumer's streaming, parser, model
state, cancellation and terminal handling. They do not authenticate or execute
a real vendor CLI/model, prove Windows/macOS behavior or qualify other CLI
protocols, conversation/resume/session-clear or a full model factory. Active
Kiro V2/V3 configuration and cloud fixtures remain separate work in the
completion parent; they are not disabled by this Codex implementation.

The `tsk-29bb` correction passed 38 affected tests on Linux, with no failures or ignored cases: state20 (harness0.31s/wall110.13s), real-Process Codex17 (1.05s/5.71s), and one actual Auth/FULL16/HTTP projection test covering nonzero and answer terminals (1.02s/35.49s). The new cases preserve observed metrics8/2/3/1/9 after exit7 and late wire failure, account exactly once, retain actual first cause and Process data, and expose usage without fabricating completion. Selected strict Clippy passed for AI lib/native Codex and App lib, with warnings denied (walls7.32s/113.29s). Six changed Rust files passed formatting; eight owned surfaces had no trailing whitespace or missing local links (15 checked). Evidence and exact commands are in `.build/rust-codex-receipt-usage/qualification.json`; these walls include shared queue/build work and do not measure inference latency. No own qualification attempts failed. The motivating STDIO failure remains in `.build/rust-knowledge-ask-stdio/stdio04.json`. These cases do not qualify vendor authentication, model identity, other platforms, RSS, or the full STDIO consumer.

Historical qualification commands used `--locked --offline -j1 --target-dir .build/rust-records-data/knowledge-index-source/target`, with `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0`: `cargo test -p graphit-ai --lib codex_completion::tests`; `cargo test -p graphit-ai --test cli_codex`; `cargo test -p graphit-app --lib source_server::knowledge_ask::tests::native_receipt_exposes_usage_independently_of_completion -- --exact`; `cargo clippy -p graphit-ai --lib --test cli_codex --no-deps -- -D warnings`; `cargo clippy -p graphit-app --lib --no-deps -- -D warnings`.

Current external Codex cases use `cargo xtask cargo test --locked --offline -p graphit-ai --test integration cli_codex::` and the shared [AI integration target](../development/ai-tests.md) for Clippy. Library and App selections retain their own qualified boundaries.
