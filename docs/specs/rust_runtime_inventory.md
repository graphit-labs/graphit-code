---
title: Runtime Knowledge publication and maintained inventory
type: reference
status: source-main-qualified
references:
  - target: {type: task, id: tsk-1c28}
    relation: implements
  - target: {type: task, id: tsk-f48f}
    relation: uses
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-7047}
    relation: uses
---

# Runtime Knowledge publication and maintained inventory

`graphit_daemon::inventory::publish_knowledge_and_retain` connects the first
native Knowledge publication to Core's maintained version inventory. Its
implementation is in `crates/graphit-daemon/src/inventory.rs`; functional
consumer tests are in `inventory/tests.rs`. This library entrypoint is available
on Linux, matching the current native Data publication boundary. Other operating
systems and product CLI, App, daemon or agent hosts require their own execution
evidence. A fixture's Kiro V2 binding does not prove Kiro launches successfully.

## Inputs and admission

Call with the original request `Arc<SnapshotContext>`, its actual
`ProjectWriteAdmission` for Knowledge, and prepared Knowledge rows bound to that
request. `KnowledgeInventoryInput` supplies existing `ProjectPaths`, the real
`GitCheckout` observation and a prepared, move-only `PrivateInstallation` from
the same original request. The checkout can come from
[project bootstrap](rust_project_bootstrap.md); this operation observes it and
does not switch Git. The admitted `VersionContext` selects the logical version.
There is no independent version, encoded branch, project ID or authority flag.

`KnowledgeInventoryLimits` supplies the existing native file limits, the Core
inventory document byte limit and the Data write limits. These are bounded
consumer policies, not permissions. The inventory is constructed from the
original installation's resolved paths, so a separate store cannot redirect it.

Before publication, the consumer checks the original cancellation/deadline,
Knowledge operation and exact snapshot Arc, rechecks Auth, validates the prepared
installation and brand, and uses Core `capture_in_scope` with the admission's
live PROJECT scope. Core binds that private identity to the scope owner and
revalidates its physical directory and persistence eligibility. Reading the
existing inventory detects invalid documents before SDK publication. No missing
identity, project root, installation or personal configuration is repaired.
See [record admission](project_record_admission.md) and
[Core inventory](rust_project_inventory.md) for these producer contracts.

Data receives the same original admission and derives its encoded branch from
that admitted selection. It owns the SDK store and original artifact writer.
Runtime checks the returned default and named generation facts against that
selection before retaining versions. Named must be materialized on the admitted
encoded branch, share the confirmed default corpus and initial version, and have
no inherited target. A default generation has no physical or inherited branch.
An inconsistent result returns `ResultBinding`, preserves the confirmed
publication and performs no inferred retention. An inconsistent PartialDefault
keeps its original Data error and records the binding failure separately.
The inventory writer is acquired only after the awaited Data operation. Its
normal kernel lock and private installation handle are dropped through RAII;
they are not retained by the result. Original deadlines and grants are never
renewed by this consumer.

## Confirmed effects and errors

| Data result | Runtime retention | Returned result |
|---|---|---|
| Confirmed default only | Retain the logical default once | Actual publication plus actual Core retention outcome |
| Confirmed default and named | Retain default, then selected named version when distinct | Actual publication plus outcomes for completed Core mutations |
| `PartialDefault` | Attempt only logical default | Original Data error, confirmed default generation within that error, completed retention or its actual failure |
| Other Data error | Do not infer a maintained version | Preserve the actual Data error |

`VersionRetention::Retained` and `Unchanged` translate Core's actual mutation
outcome. They do not grant access or establish online presence. Existing versions
are preserved. An inherited read, a role, an offline process, parking or closing
an agent supplies no publication fact and does not remove maintained inventory.

`KnowledgeInventoryError` preserves its typed `cause`, any fully confirmed Data
`publication`, and the `retained` outcomes from earlier successful Core commits.
For `PartialDefault`, `publication` is absent because the named target was not
confirmed; the original `KnowledgeIndexError::PartialDefault` retains its default
generation and original cause. A failed retention attempt is preserved separately
in `retention_failure`, and the operation still returns an error even if default
retention succeeds. The boxed error owns the real data and causes, not authority
or an invented receipt. Display gives the failing phase; `Error::source` exposes
the actual producer cause for local diagnosis. Raw diagnostics are not a remote
DTO.

Cancellation, revocation, replaced identity, read-only paths, lock acquisition,
native integrity or document size failures after publication can leave the
corpus published while inventory remains unchanged or partially updated. Auth
and the original request are rechecked before inventory effects. Each retained
version is a separate Core commit. There is no atomic transaction across Data
and inventory, no multi-version rollback and no automatic compensation.

## Recovery and delivery boundary

This is a first-publication operation; Data rejects an existing corpus. Do not
retry it as an inventory repair or adopt an existing corpus using an ID, path or
decoded result. Diagnose through actual authorized producers, preserve known
publication and retention effects, and use a separately qualified maintenance
journey when available. Do not delete a published corpus to make a retry succeed.
Version and checkout retirement remain separate deliveries with explicit intent
and actual data/use cleanup facts.

Focused verification uses `cargo test --locked --offline -j1 -p graphit-daemon
--lib inventory::tests`, owned Rust formatting, documentation/API checks and strict
daemon all-targets Clippy. The qualification packet distinguishes private Source
from Main activation and lists the executed scenarios and unavailable journeys.
The Source contract alone does not prove native host lifecycle, Kiro V2, explicit
`--v3` preview, Windows or macOS operation.

## Executed qualification

The private consumer on the current Main Auth baseline passed 14 real Core/Auth/Data
SDK tests, one inventory doctest, owned formatting and strict daemon all-targets
Clippy with warnings denied. The initial fixture failures and stale producer
snapshot compilation failures remain historical receipts in the qualification
packet. Main activation and qualification also passed: the same 14 consumer tests,
one inventory doctest, owned formatting and strict daemon all-targets Clippy
with warnings denied. The first Main strict executions exposed concurrent
ModelService fixture errors; their owner corrected the actual tests and the
required strict rerun passed. These intermediate failures remain in the packet.
These Linux library results do not establish host execution.
