---
title: Rust AI Plain with the native ProcessHost
type: architecture
status: linux-main-caller-qualified
updated: 2026-10-10
tags: [rust, ai, process, streaming, cancellation]
references:
  - target: {type: task, id: tsk-bb2a}
    relation: implements
  - target: {type: task, id: tsk-8d0c}
    relation: uses
  - target: {type: task, id: tsk-cf24}
    relation: relates_to
  - target: {type: task, id: tsk-8ca0}
    relation: derived_from
---

# Rust AI Plain with the native ProcessHost

This reference is for internal consumers of graphit_ai::cli_plain::run_plain. The consumer streams strict incremental plain text through the existing native Process supervisor. It receives the caller's original Arc<SnapshotContext>, a caller-owned ProcessHost, Request, PlainLimits and TextSink. Its result is PlainReceipt or PlainFailure, retaining the actual native process terminal.

## Service lifetime and operation context

Install ProcessHost with ProcessHost::install(&service_cancellation, host_limits). The host retains the service Cancellation; installation has no deadline or SnapshotContext. Cancelling the service seals the host and prevents new admission. Readiness and shutdown observation deadlines limit waiting and do not define host lifetime.

Each operation supplies its own original SnapshotContext with its original cancellation and absolute deadline. run_plain checks that context and policy, obtains graphit_process::operation(original, host), then calls the existing process runner with the same original and no retained output. A cancelled or expired operation can fail while the service remains available. A cancelled service can deny admission while the operation original remains valid. No replacement cancellation, renewed deadline or fallback process host is introduced.

Before using the host, observe host.ready(deadline) and act on its actual HostReadiness. Its deadline limits that observation. Requesting service cancellation is not itself proof of Sealed; consumers that require sealing must observe that state. The AI fixture observes readiness, then observes Sealed after service cancellation before checking denied Plain admission.

## Plain events, limits and terminal errors

PlainLimits captures stdout text bytes, stderr text bytes and the event count. The existing bridge incrementally decodes UTF-8 separately for each stream and forwards explicit TextChunk payloads to TextSink. These limits remain consumer policy; they do not supply execution authority or a new deadline. Native Request limits and process admission remain owned by the existing producer.

PlainReason distinguishes policy, concrete text/event/chunk limits, invalid or incomplete UTF-8, unfinished pipes, allocation, sink, original context, admission and native process failures. An admission failure such as HostSealed has no process terminal because execution was not admitted. After execution, PlainReceipt retains the actual Outcome or Failure instead of replacing native status, counts, effects or cleanup with an invented terminal.

The fixture's preadmission original cancellation/deadline cases and pending callback cancellation/deadline cases stay active. Removal of the former installation-expiry expectation does not remove an operation deadline. No timer was added to the service.

## Cleanup and observation

If a native terminal retains remaining repair work, preserve that ownership and inspect its actual repair observation. The existing AI fixture calls remaining.repair with a bounded observation deadline, observes host shutdown or repairs its pending shutdown handle, and verifies the real closure before disposing fixture resources. Those observation deadlines bound waiting; they do not reset the operation original.

The service-cancellation regression checks an observed Sealed host, typed HostSealed admission, absent process, zero sink events and no accepted TCP fixture connection while the operation original still checks valid. It uses the same real fixture and shutdown/repair path as the rest of the Plain suite.

## Source and qualification

Current sources are crates/graphit-ai/src/cli_plain.rs for the consumer, crates/graphit-ai/tests/cli_plain.rs for the service-scoped fixture and functional cases, crates/graphit-ai/tests/support/cli_plain_fixture.rs for the existing native fixture, and crates/graphit-process/src/host.rs for the producer API. This page was created because the previously proposed locator was absent in Main; no text was reconstructed from its historical line numbers.

The final affected Main suite passed all 16 cases, including service sealing and operation cancellation/deadline with real cleanup. It took 3.552s wall (2.97s compile, 0.51s harness); scoped formatting passed in 0.035s and Clippy for cli_plain with -D warnings passed in 2.006s wall (1.93s Cargo). These are functional qualification costs. The owned source delta is limited to the AI test helper, its private callsites and the service-cancellation case. Production AI, Process, Runtime, Core, manifests, lockfiles, pins and features are outside this leaf. The suite invocation is:

~~~text
cargo --config build.incremental=false test --locked --offline -p graphit-ai --test cli_plain --target-dir .build/rust-ai/local-runtime/target -j 2 -- --test-threads=2
~~~

Related producer reference: [Rust incremental Process contract](../specs/rust_process_incremental.md).
