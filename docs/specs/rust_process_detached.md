---
title: Rust Linux detached startup custody
type: spec
updated: 2026-10-14
tags: [rust, process, daemon, lifecycle, linux]
references:
  - target: {type: task, id: tsk-e6ff}
    relation: implemented_by
  - target: {type: task, id: tsk-0529}
    relation: supports
  - target: {type: task, id: tsk-9d92}
    relation: derived_from
---

# Rust Linux detached startup custody

The [Process SDK](../../crates/graphit-process/src/detached.rs) provides Linux startup custody for the per-user daemon fallback. The [service contract](rust_daemon_services.md) chooses that fallback only when the composition observes a genuinely unavailable manager. A present manager's failure remains a failure.

## Capture and native birth

`spawn_detached(DetachedRequest, &Arc<SnapshotContext>)` requires the caller's finite Original. It rejects a service context without a deadline, relative executable/cwd paths and unrepresentable cleanup/termination-grace clock endpoints before native birth. The request preserves literal executable, argv, cwd and the complete supplied environment; it clears inherited environment and connects all three stdio descriptors to Null. Debug output excludes these captured inputs.

The existing process-wrap 10.0.1 [ProcessSession](https://docs.rs/process-wrap/10.0.1/process_wrap/std/struct.ProcessSession.html) creates the session and process group through native setsid. The SDK does not combine a second ProcessGroup wrapper, launch a shell/setsid subprocess, discover a PID to signal, or retain a ProcessHost/ServiceProcess to fake detached lifetime.

A dedicated standard-library reaper thread is created before spawning the native child. Failure to create that thread therefore has no child to lose. The concrete Child is installed in its owner before returning the startup handle; a post-spawn Original failure returns the same owner as `remaining`. The thread waits for the startup decision and does not occupy Tokio's blocking pool, whose shutdown could otherwise wait indefinitely for the daemon.

## Caller admission and explicit transfer

`DetachedStartup::pid()` reports the PID from the actual Child. `try_wait(&mut self)` reports its native exit status, including early successful exit. Spawn or exit zero alone does not establish application readiness.

The launcher must use the same finite Original to read existing discovery, call authenticated Runtime Configure daemon_control Status, and compare `Reply.pid` with this Child's PID before `commit`. The SDK does not authenticate that reply or create an authority grant. Those consumers and singleton/listener/discovery publication remain owned by the launcher and daemon integration.

`commit(self)` is synchronous. It checks Original, checks the actual child for observed exit, checks Original again and transfers the lifecycle to the independent reaper. It returns `DetachedCommit { pid }`, a transfer receipt. After transfer, the SDK does not cancel the daemon when the startup context, launcher or its Host ends. Application health and later stop/restart remain the authenticated daemon-control contract.

## Failure, cancellation and cleanup

The launcher bounds readiness waiting with Original. Cancellation/expiry prevents commit. Before transfer, dropping the startup guard requests cancellation; `cancel_join(self, deadline)` observes actual cleanup even when the startup Original has already expired. An observed leader exit retains its native status but does not establish group cleanup. Before transfer, cleanup always requests TERM through the group owned by ProcessSession and, after the configured grace, KILL even if the leader has already exited. ESRCH from that concrete group signal means no such group exists; other native signal errors remain failures. The SDK waits for the same Child and retains it if group cancellation was not delivered or native wait failed. A receipt observes the owned Child wait and group termination request; it does not attest that non-child descendants have been reaped by another parent.

`DetachedFailure` retains the primary `cause`, an actual observed `status` when available, and `remaining` when cleanup is not yet confirmed. `DetachedRemaining::repair(&mut self, deadline)` observes the same reaper: `None` means its observation deadline elapsed, `Some(status)` is an actual native wait result, and errors preserve their native cause. Cancelling an observation retains its receiver and child owner. Failed native wait keeps the concrete Child for repair; a disconnected reaper can be recreated around that same Child without repeating native birth. A transferred daemon is never killed as a response to that wait failure.

Drop requests cleanup and is not a join receipt. No stdout/stderr transcript, fabricated EOF, model readiness or manager availability is inferred from this SDK.

## Qualification boundary

The [native Linux SDK tests](../../crates/graphit-process/tests/detached.rs) exercise session/group identity, Null descriptors, literal captures, invalid/expired/cancelled startup, early exit, a leader exiting with a live TERM-ignoring member in its group, guard drop, actual cleanup and survival after the launcher process exits. These are SDK child tests; their results do not qualify the Graphit daemon product or commercial hosts.

The daemon integration must independently exercise a private installed Rust CLI with manager unavailable, authenticated PID-matched Status, singleton/double-start/stalePID, stop/restart and present-manager failure. Personal services/profiles/configuration are never test fixtures. The recurring focal gate is `cargo xtask test graphit-process/detached`, available through the existing test selector catalog and covered by the pre-push workspace suite.
