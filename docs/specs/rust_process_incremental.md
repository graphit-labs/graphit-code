---
title: Original-context Rust subprocess execution
status: main-process-linux-qualified
updated: 2026-10-12
type: specification
id: Rust_Process_incremental_supervision
tags: [rust, process, core, streaming]
references:
  - target: {type: task, id: tsk-b9b9}
    relation: implements
  - target: {type: task, id: tsk-c6d6}
    relation: implements
  - target: {type: task, id: tsk-12c5}
    relation: implements
  - target: {type: task, id: tsk-430a}
    relation: relates_to
  - target: {type: task, id: tsk-425f}
    relation: relates_to
  - target: {type: task, id: tsk-32db}
    relation: implements
  - target: {type: task, id: tsk-c847}
    relation: derived_from
  - target: {type: task, id: tsk-9f12}
    relation: validated_by
  - target: {type: task, id: tsk-8d0c}
    relation: implements
  - target: {type: task, id: tsk-cf24}
    relation: validated_by
  - target: {type: task, id: tsk-0993}
    relation: validated_by
  - target: {type: task, id: tsk-8ca0}
    relation: relates_to
  - target: {type: task, id: tsk-bb2a}
    relation: relates_to
---

# Original-context Rust subprocess execution

`graphit-process` runs subprocesses with finite binary capture or incremental borrowed delivery. An installed `ProcessHost` owns the Tokio driver runtime; process-wrap supplies Unix process groups and Windows job objects. The service lifetime API is promoted to Main and qualified on Linux against the effective Core and SDK producers. The Plain caller and service supervision consumers have their own completed Main deliveries; their receipts do not qualify every daemon or CLI journey.

## Invocation and ownership

Install the host with `ProcessHost::install(&service, limits)`, where `service` is a Core `Cancellation` owned by the service, then await `ready`. The Host clones that signal and has no installation request or deadline. Each invocation retains its own original `Arc<SnapshotContext>`, deadline and cancellation. Expiring or cancelling request A, or stopping/dropping its operation handle, preserves request B and the service. The configured slots bound concurrent operations and unresolved cleanup. After the drive has dropped its physical owner, `Cleanup::Complete` or `Cleanup::NotSpawned` releases that invocation's reservation idempotently before its `Closed` observation becomes visible. A retained terminal observer can still inspect that record; its later Drop does not release a slot reserved by a newer invocation. Running, unknown and remaining cleanup retain their capacity protection.

`HostLimits::new(slots, blocking_threads)` requires both values to be positive. Slots bound admitted operations and reservations retained for unresolved cleanup; blocking threads bound the Tokio blocking pool. The host thread uses the standard library default stack and Tokio threads use the SDK default stack. The API does not request stack byte sizes or impose an RSS quota. Each application service selects its own logical capacity.

The revised release ordering awaits affected Runtime/`host_runtime` qualification; historical receipts below retain their recorded scope.

An already cancelled service returns `io::ErrorKind::Interrupted` from installation. Cancellation observed at the pre-Ready boundary reports `HostFailure::Io(Interrupted)`. Public readiness and reservation observe service cancellation and seal admission, including before the driver processes its async signal. These checks are not atomic with cancellation: an earlier accepted reservation retains its owners and enters the existing stop/drain path. Pending, failed and capacity-full outcomes remain distinct.

```rust,ignore
let (stop, receiver) = operation(&original, &host)?;
let result = run(request, original.as_ref(), Retention::Full, sink, receiver).await;
drop(stop);
```

`operation` admits one invocation and returns its stop handle and receiver. Pass that receiver to `run` with the same original context. Dropping either handle stops that invocation; the host continues to own its driver during caller cancellation or abort. Context cancellation and its absolute deadline govern validation, execution and callbacks. No separate native grant, memory ledger or handle reservation is required.

`operation` requires a finite deadline before Host reservation. A service context without one returns `OperationAdmissionFailure::Core(InvalidInput)` even when the Host has no free slot. The admitted control lease keeps that actual deadline alongside the same Original Arc; supervision uses it without inventing a clock endpoint. Cancellation and expiration remain Core admission errors. A daemon service context must create its real finite operation before calling Process.

`Request` owns the absolute executable and working directory, literal arguments, a complete environment and optional zeroizing stdin. The runner clears ambient environment and invokes the executable directly. It does not add a shell, discover executables or mutate the parent's cwd/environment. Windows executables use the native Rust command argument encoding.

`Limits` bounds stdin/stdout/stderr bytes and specifies termination grace and cleanup duration. Grace may be zero and must be shorter than the positive cleanup duration. Zero output caps are valid. Invalid requests and already cancelled or expired contexts fail before subprocess creation.

## Output and callbacks

`Retention::None` keeps no transcript. `Retention::Full` retains accepted stdout/stderr separately. Both modes enforce the same output limits. `IncrementalSink::push` receives a borrowed chunk and the original context; callbacks run serially. While a callback awaits, output readers pause and OS pipes provide backpressure. Input, process observation, cancellation and cleanup continue independently.

Each pipe preserves its byte order. Cross-pipe order follows observation. Two scratch buffers serve the readers, without a copied output queue. Read sizes are bounded by the remaining cap plus one byte; a violating chunk is rejected before retention or delivery. Earlier accepted chunks remain visible.

`observed` counts bytes actually read, including bounded cleanup discard. `delivered` counts only callbacks that return success. Full retention accepts a valid chunk before invoking its callback, so a rejected callback can leave retained bytes that were not delivered. The sink must poll cooperatively and tolerate cancellation of its borrowed future; external effects belong to the sink.

## Results and cleanup

A zero or nonzero exit returns the actual status. `Outcome` and `Failure` expose cleanup, pipe states, observed/delivered counts, transcript and any remaining process owner. Pipe closure and EOF are distinct. The primary request, sink, decoder or process error survives a later cleanup failure.

An ordinary OS error returned by spawn reports the original `Reason::Io(Stage::Spawn, ...)`, `Cleanup::NotSpawned` and `PipeEnd::NotSpawned`; it does not leave an executing driver. Setup errors after a child was created recover that owned child and request cleanup. A caught panic with no observable child returns an unconfirmed result instead of fabricating an exit status. Terminal driver or host errors preserve their failure cause and release owned SDK resources via RAII; they do not park threads or retain completed futures indefinitely.

Cancellation, deadline, output, I/O or sink errors stop new callbacks, close stdin and request termination while draining only within the output caps. One cleanup endpoint covers grace, escalation, waiting and pipe completion. Cleanup can finish after execution expiry, but delivery cannot continue after expiry. Pending cleanup remains owned by the installed host. `RemainingProcess` retains the same record observation through `Arc<HostSlot>` directly, without an extra boxed state; `repair` observes that record and does not create a new driver or renew the original request. If the caller has not consumed the terminal driver observation, it retains `RemainingProcess` even when that driver finishes just before the result is returned. The next repair then returns that same terminal cleanup and status; a later snapshot cannot discard the receipt.

The operation runner checks Original cancellation/deadline before observing ready native completion, pipe EOF or a callback already started. Those ready observations precede expiry of the cleanup wait; the endpoint is never extended. New bytes cannot start a callback after that endpoint, and a continuously ready discard cannot postpone expiry. A pending callback or pipe can still fail the operation with `Reason::Cleanup`. Its `cleanup` field preserves the actual terminal result consumed from the same driver, including `Complete` or Windows uncertainty, rather than replacing completed cleanup with an observation timeout. If native cleanup has not been consumed, its existing pending/failed receipt and `RemainingProcess` remain available. Original cancellation or deadline remains the primary failure even when native cleanup has already finished.

Unix cleanup targets the owned process group, including escalation after leader exit. It does not contain processes that deliberately leave that group. Windows uses the SDK's job object and reports `TreeUnknownWindows` when job emptiness is not established; the actual leader status is preserved. Native Windows/macOS runtime validation remains pending.

Cancelling the service seals admission and stops/drains its actual operations through the existing control lifecycle, without cancelling their original request signals. Explicit `shutdown` and Host Drop seal only that Host and do not mutate the externally shared service signal or request signals. Shut down the host after its callers finish. `shutdown` seals admission and waits on a Tokio completion channel sent after the host driver and runtime are dropped, then joins the thread. Its observation deadline does not define a service lifetime. Reaching it returns `PendingHost`, which retains the owner for a subsequent `repair`. RAII closes handles and requests best-effort termination on drop.

Request, chunk, transcript and error Debug representations contain categories and counts, excluding private arguments, environment, input/output and raw OS error text. Callers explicitly access and present private bytes.

## Validation

The finite-admission update (`tsk-b9b9`) passed the missing-deadline reservation test, three lifecycle regressions, two incremental deadline regressions and one borrowed-cancellation regression. Child fixture entries are excluded from these functional counts. Strict all-target Clippy and scoped formatting passed on Linux. Windows/macOS execution remains untested.

Select affected Linux tests for each change and run scoped rustfmt/Clippy. Recovery regressions are `cargo test --locked --offline -j1 -p graphit-process --lib native_setup_errors_and_panics_recover_the_actual_child` and `cargo test --locked --offline -j1 -p graphit-process --lib sdk_wait_panic_preserves_terminal_cause_and_releases_host_slot`; strict library validation is `cargo clippy --locked --offline -j1 -p graphit-process --lib -- -D warnings`. Broader Process test/doc commands remain useful when their actual impact warrants them, without requiring a cold rebuild or replay of unchanged consumers. The tests cover literal arguments/cwd/environment, binary output, exit status, stdin, caps, borrowed callbacks, cancel/deadline, setup errors, group cleanup and caller abort. The prior Linux qualification had 43 tests and 6 documentation tests, strict Clippy and scoped formatting; it is historical evidence for the earlier API. The private Linux service-Cancellation qualification passed 51 tests and 6 documentation tests, strict all-target Clippy and scoped formatting. The same promoted Process code passed those 51 tests and 6 documentation tests in Main, strict all-target Clippy and scoped formatting, using the existing Main target. Added regressions cover independent native A/B requests and C admission, local stop/Drop and original cancel/deadline, service cancellation, retained terminal slots, and pre-Ready cancellation. A physical gate proves Pending and SAME thread join; the active native drain/Remaining tests provide complementary resource evidence. A real-child regression covers a panic in SDK wait: it preserves the observed exit status and cleanup cause, releases the terminal observer and allows the host slot to be reused. This does not qualify a separately injected HostPhase::Pump panic. These Process tests do not certify every application/AI/daemon journey, model context delivery, native Kiro or Windows/macOS runtime. Consumer receipts under cf24/0993/8ca0/bb2a retain their own scope. No universal guard-process custody framework or CPU/RAM quota is required for Child recovery. Agent compatibility and required host versions follow the maintained consumer documentation. These Process fixtures use project-owned subprocesses and require no installed vendor agent, account or subscription.

The two-argument HostLimits change (`tsk-c6d6`) passed the affected Linux library and five integration suites: 59 top-level tests, zero failures, and one ignored native child entry exercised by its parent tests. Test wall time was 25.122 s; per-suite harness times were 2.07/3.42/2.02/2.00/1.01/0.71 s. Selected strict library/test Clippy passed in 178.148 s, including the shared build-lock wait. The three AI test targets, Daemon library/model-child target and App Knowledge-question target compiled successfully in 18.160/22.846/65.538 s. Those consumer checks establish compilation of the mechanical calls, not a replay of their runtime suites. Scoped formatting and Source/documentation delta review passed. This qualification does not measure stack bytes or RSS and does not qualify the App ask ingress or native Windows/macOS execution.

Source: `crates/graphit-process/src/{host,operation,supervisor,lib}.rs` and the capture/incremental tests. The CLI text consumer is documented in [Rust incremental plain CLI text](rust_cli_plain.md).
