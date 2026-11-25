---
title: Rust incremental plain CLI text
description: Strict bounded UTF-8 text from the original Process invocation.
type: specification
status: main-linux-library-qualified-product-integration-pending
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-bb2a}
    relation: implements
  - target: {type: task, id: tsk-a6e8}
    relation: validated_by
  - target: {type: task, id: tsk-23ce}
    relation: corrected_by
  - target: {type: task, id: tsk-ec7f}
    relation: implements
  - target: {type: task, id: tsk-32db}
    relation: uses
  - target: {type: task, id: tsk-08c0}
    relation: derived_from
  - target: {type: task, id: tsk-12c5}
    relation: relates_to
  - target: {type: task, id: tsk-75a5}
    relation: relates_to
---

# Rust incremental plain CLI text

The CLI adapter streams UTF-8 text from a subprocess, preserves the actual process result, and returns accepted stdout text. It uses the same operation context for validation, execution, callbacks and cancellation. The Main library and caller are qualified on Linux under `tsk-bb2a`, with current regressions repeated in `tsk-a6e8`; CLI selection and full application integration remain separate.

## API and ownership

```rust,ignore
pub async fn run_plain<S: TextSink>(
    original: &Arc<SnapshotContext>,
    host: &ProcessHost,
    request: Request,
    limits: PlainLimits,
    sink: &mut S,
) -> Result<PlainReceipt, PlainFailure>
```

Install a `ProcessHost` with `ProcessHost::install(&Cancellation, HostLimits)`, then await its readiness. This cancellation belongs to the service host; each finite operation retains its own original context, cancellation and deadline. The host owns the driver runtime and enforces its configured concurrent process capacity. Each invocation uses its original context directly; no separate native resource grant or byte/handle reservation is required. `Request` provides the program, literal arguments, working directory, complete environment, optional stdin and process output/cleanup limits. Sensitive input is excluded from diagnostics.

The process supervisor uses Tokio and process-wrap for process groups on Unix and job objects on Windows. The host retains the driver while callbacks borrow their inputs. On completion, shutdown the host; if shutdown is pending, retain its returned owner and await completion. Resource lifetime follows Rust ownership and RAII.

## Text and limits

Stdout and stderr are delivered separately as borrowed `TextChunk` values. The adapter validates UTF-8 across read boundaries, preserves whitespace, CRLF and final text without a newline, and reports invalid or incomplete text. A chunk may arrive before child exit; OS read boundaries are not message boundaries.

`PlainLimits` bounds accepted stdout bytes, stderr bytes and callback events. Process `Limits` bounds stdin/stdout/stderr bytes and defines termination/cleanup durations. These are distinct input/output controls. Invalid policies and cancelled or expired contexts fail before subprocess creation.

Only a successful callback advances accepted counts and appends stdout. Text already accepted remains in the receipt if a later callback, context check, decoder or process step fails. Nonzero child exit remains an actual `Outcome`; the caller decides whether that status means an application error.

The accepted stdout count comes directly from the retained `String` length; only
stderr, whose text is not retained, needs a separate byte count. Stdout uses
fallible, amortized `String::try_reserve` before a callback. `PlainLimits` bounds
accepted content, while allocation capacity follows the standard library's
growth policy. No second output buffer or resource accounting is maintained.

## Results, failure and cleanup

`PlainReceipt::text`, `accepted` and `events` expose accepted output. `process` and `process_mut` expose the real `Result<Outcome, Failure>` from the supervisor, including exit status, pipe state, counts, cleanup and any remaining process owner. `PlainFailure::receipt` and `receipt_mut` preserve this information on failure.

A decoder or callback error keeps its causal reason when the process also fails. Admission errors report the actual host/context error and have no fabricated process result. Cancelled or expired operations request process cleanup rather than discarding an executing driver. Pending cleanup remains owned and can be awaited through the returned process owner. If the OS rejects subprocess creation, the supervisor preserves the spawn error and reports `NotSpawned`; the host can then shut down normally. Host completion uses a Tokio completion channel sent after the driver and runtime have been dropped.

## Validation

The historical private Linux integration ran `cargo test --locked -p graphit-ai --test cli_plain -- --test-threads=1`: 16 functional tests passed. The old optional fixture timing loop and its unused timing fields have been removed. These cover incremental stdout/stderr, UTF-8, whitespace, output/event limits, request validation, cwd/arguments/environment/stdin/exit status, sink errors, cancellation, deadlines, host capacity and cleanup. Five decoder/policy unit tests also passed before the interface simplification; that historical receipt does not qualify the later Main source.

No cross-platform runtime or performance result is inferred from the Linux suite. Test fixtures use temporary directories and their own process hosts. Source: `crates/graphit-ai/src/cli_plain.rs`, `crates/graphit-ai/tests/cli_plain.rs`, and `crates/graphit-process`.
