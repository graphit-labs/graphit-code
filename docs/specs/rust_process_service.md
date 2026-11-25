---
title: Rust service child supervision
status: main-linux-qualified-application-integration-pending
updated: 2026-10-29
type: specification
id: Rust_Process_service_supervision
tags: [rust, process, runtime, streaming]
references:
  - target: {type: task, id: tsk-b9b9}
    relation: implements
  - target: {type: task, id: tsk-0993}
    relation: implements
  - target: {type: task, id: tsk-96ad}
    relation: implements
  - target: {type: task, id: tsk-9bd4}
    relation: derived_from
  - target: {type: task, id: tsk-cf24}
    relation: derived_from
  - target: {type: task, id: tsk-5c46}
    relation: relates_to
---

# Rust service child supervision

This contract is for applications that start a native child whose lifetime belongs to the installed `ProcessHost` service. The implementation is integrated and qualified on Linux in Main under `tsk-0993`, following private qualification under `tsk-96ad`. Application and native model integration remain separate steps. The existing [original-context runner](rust_process_incremental.md) continues to govern individual operations and borrowed callbacks with their original request.

## Startup and handover

Install one Host with the actual service `Cancellation` and explicit `HostLimits`, then await its readiness. Call `spawn_service(request, &startup, &host).await`. The startup argument is the real `Arc<SnapshotContext>` of the initiating operation. Its cancellation and absolute deadline govern admission, native spawn and finite stdin delivery. The Host's actual queue and slots govern capacity; service children and ordinary operations compete for the same slots.

Startup must have a finite deadline. `spawn_service` rejects a service context with no deadline before reserving a Host slot or starting a child. The rejection has `ServiceFailure.admission = Some(OperationAdmissionFailure::Core(InvalidInput))`, `Reason::Invalid(InvalidRequestContext)` and `NotSpawned` pipes/cleanup. Cancelled and expired startup contexts retain their cancellation and timeout causes. No synthetic deadline is supplied; after successful handover, the existing service cancellation governs the child's lifetime.

`Request` carries an absolute executable/cwd, literal arguments, complete environment, optional zeroizing finite stdin and actual input/output/cleanup limits. The existing Tokio/process-wrap path clears the ambient environment and installs Unix process-group or Windows job control. The runner does not discover an executable or encode a model bootstrap protocol. The application supplies its real bootstrap bytes and cap.

The startup driver writes stdin, shuts it down and closes it under startup and service fences. It drops its startup Arc before publishing successful handover. The returned `ServiceProcess`, native owner, control and running driver retain no startup context, request identity or execution deadline. Expiry, cancellation or destruction of the first request after handover therefore does not stop the service child. Service cancellation, Host sealing, local stop, child exit and real I/O/output limits can still stop it.

Successful handover means native spawn plus finite bootstrap completion. Model readiness must be established from the actual child's protocol/output by the application. A child that fills stdout before accepting its finite input can stall bootstrap; the real startup deadline/cancellation stops it and preserves cleanup evidence. There is no persistent stdin session.

## Interactive transport acquisition

Applications that own interactive stdin/stdout can use `spawn_owned` with an
already prepared `process_wrap::tokio::CommandWrap`. This existing acquisition
boundary is public for the ACP and Kiro adapters. The caller retains executable
and Original validation, process-group or Job setup, admission, cancellation and
teardown; the helper only preserves ownership across wrapper errors and panics.

`SpawnFailure` preserves `reason`, `phase` and any recovered native `child`.
`SpawnPhase::BeforeNative` confirms no child was acquired;
`NativeAttempted` without a handle leaves birth unobserved; `ChildObserved`
retains the actual child. Move a recovered child into its teardown owner before
the first await and preserve the initial cause. The recovered Tokio child no
longer carries its wrappers: waiting for that leader does not establish Windows
Job emptiness. The adapters retain that distinction instead of reporting a
successful tree join from the leader alone.

## Output and local control

Pass the owned child to `run_service(process, retention, &mut sink).await`. Implement `ServiceSink::push(chunk, &service)`, returning one cancellation-safe borrowed future. The signal passed to the callback is the Cancellation captured in the installed Host. Output callbacks are serial, preserve binary bytes and stream identity, and stop at the service/local/cleanup fences. No pipe read starts while a callback is pending. Cancelling that callback drops the borrowed future and leaves the Host-owned native supervisor running through cleanup.

`ServiceProcess::stop()` requests local stop. `stop_handle()` returns a `ServiceStop`; stopping or dropping it requests local stop as well. Dropping `ServiceProcess`, including by aborting its borrowed runner or startup caller, requests stop of that record. These actions do not cancel the externally shared service signal or another operation. Hold a stop handle while it is needed; dropping it is an active control action.

`Retention::None` uses incremental scratch buffers and does not retain a transcript. `Retention::Full` retains accepted bytes within the request's actual output caps. Observed counts and delivered counts remain distinct; a rejected sink callback does not count as delivered output. Requests, owners, transcripts and failures expose categories/counts through Debug and exclude arguments, environment, bootstrap/output bytes and raw OS error strings.

Application authorization, per-operation `LocalServiceLease`, private connection control and model protocol belong to their real application/Auth producers. This Process API grants no role or authority and does not replace per-operation checks.

## Failure, cleanup and recovery

`ServiceFailure.admission` distinguishes `HostNotReady`, `HostSealed` and `CapacityFull` before native submission. `reason` preserves the primary startup, service, sink, output or SDK error. `ServiceOutcome`/`ServiceFailure` expose the actual status, cleanup result, pipe finalization, observed/delivered counts, optional transcript and optional `ServiceRemaining`.

Ordinary prebirth SDK errors report `NotSpawned`. Setup failure after observing a child conserves that child and requests cleanup. A native attempt whose birth cannot be observed reports `BirthUnobserved` and unobserved pipes; it cannot manufacture a child owner or exit status. Driver and Host panics report their actual unknown cleanup category and release their SDK guards through RAII.

The SAME driver remains in the Host's owner table. Cleanup starts a finite endpoint from the captured `Limits.cleanup`, sends termination, escalates and waits using the existing SDK. It has no inherited RPC execution deadline. If observation expires before the terminal native result is consumed, the result carries `ServiceRemaining` for the actual Host record. `repair(deadline)` observes that SAME record and does not recreate a process or a second native wait. The cleanup result and remaining handle use the same consumed observation. When the driver finishes after the caller stops observing, the handle remains available to receive its actual terminal cleanup and status. Once the drive has dropped its physical owner, `Cleanup::Complete` or `Cleanup::NotSpawned` releases that invocation's reservation idempotently before `Closed` is observable. Terminal observers may retain the result without blocking that released capacity; their later Drop cannot free a newer invocation's reservation. Running, unknown or remaining cleanup keeps its capacity protection. The revised release ordering awaits affected Runtime/`host_runtime` qualification; earlier receipts below keep their recorded scope.

Service cancellation and local stop retain priority over delivery. Ready native completion, EOF and the result of an already started callback are observed before expiring the cleanup wait, without extending its endpoint. New output cannot start a callback after that endpoint; continuously ready discard bytes cannot postpone expiry. A callback still pending at expiry fails with `Reason::Cleanup`, while the `cleanup` field preserves the actual consumed native result, including `Complete` or Windows uncertainty. `ServiceRemaining` is retained whenever the terminal native observation has not been consumed. This ordering does not attach the startup Original's deadline to the handed-over service.

Unix cleanup targets the owned process group, including escalation after leader exit; processes deliberately leaving the group remain outside its containment. Windows uses the same retained parent Job and fixed-size `ActiveProcesses` accounting query as [operation cleanup](rust_process_incremental.md#results-and-cleanup). The parent is assigned while the child remains suspended, before the pinned SDK assigns its nested Job and resumes execution. A zero observation is required for `Complete`; an absent capability or query failure remains `TreeUnknownWindows`. Partial SDK-wrapper failure preserves the parent Job in the owned command. EOF, connection closure, leader status and successful control calls do not independently prove all descendants are quiescent.

Host service cancellation seals admission and closes actual records. `shutdown` and Host Drop seal that Host without cancelling the external service or request signals. Use `HostShutdown`/`PendingHost::repair` to observe the actual Host thread join; its observation deadline is not a child lifetime. Drop requests best-effort termination and is not an awaited shutdown receipt.

## Qualification

Run the affected commands identically on supported hosts:

```text
cargo xtask cargo test --locked --offline -p graphit-process --all-targets
cargo xtask cargo test --locked --offline -p graphit-process --doc
cargo xtask cargo clippy --locked --offline -p graphit-process --all-targets -- -D warnings
```

The finite-admission update (`tsk-b9b9`) passed the eight existing service regressions, including the new missing-deadline case in the admission test. An exact rerun after preserving cancellation/timeout mapping passed. Strict all-target Clippy and scoped formatting passed; this update adds no model or foreground qualification.

Main qualification passed eight real service regressions, seven documentation tests, strict all-target Clippy and scoped rustfmt. The native-child entry is exercised by the parent tests. Private Linux qualification passed 59 tests (51 existing and eight service regressions), seven documentation tests, strict all-target Clippy and scoped rustfmt. One ignored native-child entry is exercised by its parent tests. The new tests execute native child processes and actual Host/SDK owners for finite bootstrap, first-request independence, local/service/Host stop, caps, failure, A/B isolation, capacity and remaining cleanup. Windows/macOS, application/model context delivery and Kiro are unexecuted here. Current agent compatibility targets Kiro V3; former V2/preview requirements are superseded and impose no vendor installation/account gate. A fixture child is evidence for Process behavior and cannot qualify a native model or host journey.

Source: `crates/graphit-process/src/service.rs`, shared Host/supervisor integration and `crates/graphit-process/tests/service.rs`.

The Linux inherited-SIGCHLD regression starts a Cargo-prepared C executable that sets `SIGCHLD` to `SIG_IGN` and immediately `execv`s the Rust test process. Its path comes from the build output. The test inspects the actual inherited disposition before installing Tokio or the Process host and then exercises the real wait. No interpreter or compiler runs during the test.

The default bootstrap-failure regression separates its causes: the blocked child exercises the finite deadline, while the broken-input child announces readiness over its private loopback connection and waits for the parent ACK before exiting. The latter uses the normal startup watchdog and still requires an actual input I/O failure.
