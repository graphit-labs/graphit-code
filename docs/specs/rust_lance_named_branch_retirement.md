---
references:
  - target: {type: task, id: tsk-1e53}
    relation: specified_by
  - target: {type: task, id: tsk-8a17}
    relation: uses
---

# Named Lance branch retirement

Status: five real SDK cases qualified on current Main in sdk-fourth.log (5 PASS, no ignored cases); retire_named is present in Main and all six affected API cases passed (api-current.log, no ignored cases). Strict Data lib/affected-target lint, formatting/diff checks and the changed doctest passed. Qualification is Linux/current Main; no Windows/macOS runtime claim is made. No Knowledge/App destructive consumer is delivered by this document.

The existing Data Foundation and Branch modules use Lance 14.0.2 and Arrow 58.3. A named branch is a Lance reference within one captured corpus URI. Retirement is separate from local checkout unregister, Core inventory removal, whole-corpus deletion, garbage collection, and other modules' data.

## SDK operation and effects

The public SDK `Dataset::delete_branch(name)` calls `Branches::delete(name, false)`. It checks the current branch references and tags, removes the target's `BranchContents`, then cleans up its branch directories. Descendant references and any tag pointing to the branch protect the branch's version chain. Default is not a named retirement target; an inherited, absent target must not be materialized to remove it.

A successful call reports completion of this SDK operation. A failure preserves its actual SDK cause. It can happen before effects, after reference deletion, or during cleanup. A later `RefNotFound` from exact reference observation says only that the reference is absent at that observation. It does not establish complete cleanup, no previous effects, a stable absence, a compare-and-swap fence, or authority to reuse the name. Forced deletion is not ordinary recovery.

The SDK does not accept an expected branch identity/revision for deletion and does not register users of captured `Arc<Dataset>` snapshots. A captured scalar branch/version is not a name-ownership fence. Removing and recreating the same name produces another branch identity even when the scalar version is unchanged. Captured readers and uncommitted operations must be tested without promising global quiescence or transactional cancellation.

## Current affected fixtures

`crates/graphit-data/tests/lance_branch_store.rs` reuses the existing real Foundation/Branch fixture. Five `named_branch_retirement` cases exercise local A/B/default/inherited pins; descendant and tag refusal before effects; real ObjectStore before-effect failure, lost acknowledgement and cleanup failure; reused name identity; prepared commit and simultaneous SDK create/delete. The test ObjectStore wrapper delegates to the actual SDK store and changes only a selected failing I/O boundary. It is not a replacement storage engine or an authorization fixture.

The first build failed before tests because the disk was full. After recovery, a missing ObjectStoreExt import was corrected. The first actual execution passed three cases and rejected two expectations: a materialized named reader lost its deletion file after cleanup, and a concurrent creation error was not necessarily RefConflict. The prepared commit published orphan version 2 after branch-reference removal. These failures and causes remain recorded; the corrected SDK execution passed all five cases in 0.04 seconds (normal shared build queue included separately). Simultaneous creation returned DatasetAlreadyExists to one caller; it is preserved instead of normalized to RefConflict. The current API execution adds default/inherited refusal before SDK I/O and reuses the actual fault cases; all six cases passed in 0.05 seconds (the normal shared build queue and compilation took 1 minute 49 seconds). No supplier test was replayed. Supplier suites from `tsk-8a17` are reused as prior evidence and are not new executions.

## Consumer boundary to close before production

Data exposes the SDK operation and its causes without promising that retained pins remain readable. The caller stops and joins the producers it owns and uses its existing corpus writer to order destructive effects against its writes. This leaf does not introduce control of every HTTP request or a global reader registry. Concurrent SDK read failure is an operation error, not memory unsafety. The actual destructive App/Knowledge compositor remains a separate delivery, responsible for the integrity of the retired project's commits and its own users. Existing checkout retirement (tsk-2515) joins owners of the exact ProjectId/checkout pair; it does not assert that independent jobs have stopped. The current Knowledge retirement contract (tsk-d581) is planning. None of these limits blocks the small SDK API or authorizes a fake pin CAS, usage ledger or reservation.

`BranchSnapshot::retire_named(self) -> Result<(), BranchError>` consumes the selected pin. It accepts no second selector or URI. `DefaultSnapshot` and `InheritedSnapshot` are returned before SDK I/O; a physical named target invokes `Dataset::delete_branch` once. SDK failures remain `BranchError::Sdk`, whose `Error::source` exposes the original Lance cause. The method does not automatically retry, force cleanup, recreate the name, or declare absence on failure. The caller can reobserve the reference through existing SDK/store operations, without converting absence into successful cleanup.

A Knowledge consumer requires a real `ProjectWriteAdmission` for Knowledge mutation. It derives the branch from the admission's selected `VersionContext`, retains the same original request Arc, and rechecks authentication and physical scope. It captures an existing corpus through B1 and acquires the existing project `ArtifactWriter` before capture/delete, then retains that writer across SDK effects. This is cooperative serialization among real producers, not atomicity between stores or a registry of readers. It must not create a corpus or inherited branch, adopt a foreign target, retry an uncertain deletion, or infer permissions from DTOs, Configure, inventory mutation, or another module's authority.

Only `index.lance` is covered by the existing index wrapper. `embeddings.lance` is a separate Knowledge corpus, with a separate consumer and independent effects. Neither an index deletion nor an SDK reference observation completes the removal of a project's maintained version. Core inventory removal is performed separately after the affected domain consumers acknowledge their own effects; no rollback across SDK and inventory is promised.

## Validation

The affected command is `cargo xtask cargo test --locked --offline -p graphit-data --test integration -- lance_branch_store::named_branch_retirement --nocapture`, using the current checkout configuration. Formatting and strict lint apply to changed Data targets. No cold harness, CPU window, resource reservation, broad supplier replay, or cross-platform runtime qualification is required or claimed.

Primary SDK documentation: [Lance 13 Dataset::delete_branch](https://docs.rs/lance/14.0.2/lance/dataset/struct.Dataset.html#method.delete_branch). The effect ordering is verified against the installed pinned SDK's `dataset/refs.rs`, not inferred from an acknowledgement alone.
