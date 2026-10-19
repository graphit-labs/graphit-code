---
title: Original request context across crate boundaries
content-type: reference
audience: developers
references:
  - target: {type: task, id: tsk-136a}
    relation: implements
  - target: {type: task, id: tsk-051d}
    relation: supports
  - target: {type: task, id: tsk-2706}
    relation: implements
  - target: {type: task, id: tsk-bcf2}
    relation: implements
  - target: {type: task, id: tsk-c728}
    relation: uses
---

# Original request context across crate boundaries

`graphit_core::file_io::SnapshotContext::new(deadline, cancellation)` creates a finite operation. `SnapshotContext::service(cancellation)` creates a continuous service context with cancellation and no deadline. Both keep shared cancellation and private request identity. Hold the context in `Arc<SnapshotContext>` when readers and consumers belong to the same operation. `deadline()` returns `Option<Instant>`; no replacement timeout is assigned to a service.

`deadline()`, borrowed `cancellation()` and `check()` preserve that original operation. The guard checks cancellation first (`Interrupted`), then expiry (`TimedOut`). Passing a borrowed context or cloning its Arc does not extend its deadline. Content and file-size bounds remain on the corresponding reader/document APIs.

Finite consumers use `required_deadline()` before their first effect. It checks cancellation and expiry first, then returns `InvalidInput` for a service context. App model provisioning, installation, role execution, browser session requests and finite MCP clients keep that admitted Instant through the operation. Knowledge Ask also retains the same Original in its progress gate; SDK timeouts use the remaining duration of that Instant.

`graphit_hub::policy::RequestContext::service(content, cancellation)` carries a service Original with the existing content bounds. Filesystem rechecks use `ScopeContext::from_snapshot(original)`, preserving the actual optional deadline and cancellation. Service lifetime does not extend signed token expiry or confer PROJECT permissions; authentication and project authorization keep their independent checks.

`same_request(&other)` compares the actual private request identity. Borrows and Arc clones of the original context match. Constructing another context with equal deadline and cancellation produces a different identity. Serialized records cannot restore that identity. Equality alone does not establish liveness, filesystem access or role permission: consumers also check the original context and their current physical/policy observations.

Broker purpose verification borrows the original context for network cancellation/deadline and local binding. PROJECT directory revalidation compares the actual caller with its captured request before checking original cancellation/deadline and the held native chain. A cancelled operation cannot be renewed by supplying a newly constructed context.

`cargo test --locked -p graphit-core --test original_request_context` covers shared identity versus equal inputs, cancellation shared across borrows, and deadline/cancellation precedence in three external API tests. `cargo test --locked -p graphit-core --doc file_io` checks private identity and restoration boundaries. Linux execution evidence does not imply native Windows or macOS execution.
