---
title: Rust Hub resource policy
type: reference
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-c29b}
    relation: supports
  - target: {type: task, id: tsk-e493}
    relation: supports
  - target: {type: task, id: tsk-3ddb}
    relation: derived_from
  - target: {type: task, id: tsk-1833}
    relation: derived_from
  - target: {type: task, id: tsk-84f3}
    relation: derived_from
  - target: {type: task, id: tsk-427f}
    relation: supports
---

# Rust Hub resource policy

This page describes the implemented `graphit-hub::policy` library contract.
Product callers, hook composition, transport enforcement and native platform
qualification remain separate integration work. Validation evidence is recorded
by the task that implements each affected delivery.

The policy implements the finite profile in the
[Hub consumer contract](hub_consumer_contract.md).
It uses the captured [Rust configuration contract](config_module.md)
and the content producers from `tsk-1833` and `tsk-84f3`. See
[product architecture](../development/product-architecture.md) for the delivery boundary.

## Resolution and provenance

`RESOURCE_LIMITS` is the single catalog of 61 flat keys. Each `LimitSpec` declares
its `ResourceSetting`, key, explicit default and optional hard maximum.
`HubResourcePolicy::resolve(&ConfigSnapshot)` resolves each field independently
using the existing Command → Environment → Project → Global → Distribution
precedence. It neither reads process environment nor discovers configuration.

A present winning value must be a positive JSON integer representable as `u64`.
Zero, negative, fractional, wrong-type and overflowing values fail without
falling back to a lower layer or silently clamping. Environment textual JSON
parsing belongs to the upstream configuration loader; other layers are already
typed. Null is rejected before construction of a valid `ConfigSnapshot`.
`PolicyError` identifies the public setting, origin and category
(`InvalidInteger`, `NonPositive`, `HardBound`, `Overflow` or `Incoherent`), without
including the submitted value.

Each `ResolvedLimit` retains `LimitOrigin::Configuration(ConfigSource)` or
`DistributionDefault`. A winning value in a captured Distribution document is
`Configuration(Distribution)`; a missing key receives `DistributionDefault`
without pretending a document existed. `LimitOrigin::source()` projects both to
Distribution, so callers needing that distinction must retain the full origin.
`distribution_defaults(&Brand)` constructs a real document containing all 61
defaults; including it in a snapshot therefore gives configuration provenance.

## Default catalog

Every suffix below has the prefix `hub.limits.`. Byte quantities are bytes,
`request.deadline_ms` is milliseconds, and other values are counts or depth.
“—” means no additional catalog ceiling; positivity, checked representation,
constructor bounds and coherence checks still apply. It does not mean unlimited.
These are engineering defaults, not a measured optimum or an SLO.

| Key suffix | Default | Catalog maximum |
| --- | ---: | ---: |
| registry.bytes | 16777216 | 16777216 |
| registry.entries | 10000 | 100000 |
| registry.key_bytes | 65536 | 65536 |
| registry.json_nodes | 1000000 | 1000000 |
| registry.string_bytes | 16777216 | 16777216 |
| projection.identity_bytes | 65536 | 65536 |
| projection.path_bytes | 65536 | 65536 |
| projection.fallback_entries | 10000 | 100000 |
| projection.name_bytes | 16777216 | 16777216 |
| projection.batch_items | 1000 | 100000 |
| artifact.bytes | 16777216 | 16777216 |
| artifact.entries | 10000 | 100000 |
| artifact.key_bytes | 65536 | 65536 |
| copy.bytes | 536870912 | — |
| copy.nodes | 100000 | — |
| copy.path_bytes | 16777216 | — |
| copy.depth | 128 | 256 |
| transient.bytes | 67108864 | — |
| transient.nodes | 10000 | — |
| transient.path_bytes | 1048576 | — |
| transient.depth | 128 | 256 |
| snapshot.bytes | 67108864 | — |
| snapshot.nodes | 10000 | — |
| snapshot.path_bytes | 1048576 | — |
| snapshot.depth | 128 | 256 |
| retained.bytes | 8388608 | — |
| retained.files | 16 | — |
| retained.path_bytes | 1048576 | — |
| lineage.schema_bytes | 128 | 16777216 |
| lineage.selection_bytes | 256 | 16777216 |
| lineage.total_bytes | 4096 | 16777216 |
| content.input_bytes | 8388608 | — |
| content.output_bytes | 16777216 | — |
| content.depth | 32 | 80 |
| content.nodes | 100000 | — |
| content.scalar_bytes | 1048576 | — |
| content.collection_entries | 10000 | — |
| shared.input_bytes | 1048576 | — |
| shared.output_bytes | 2097152 | — |
| shared.journal_bytes | 4194304 | — |
| shared.nodes | 100000 | — |
| shared.depth | 32 | 80 |
| shared.scalar_bytes | 1048576 | — |
| shared.entries | 4096 | — |
| shared.hook_tuples | 1024 | — |
| shared.references | 10000 | — |
| shared.documents | 1000 | — |
| request.deadline_ms | 120000 | — |
| transport.bytes | 1048576 | — |
| transport.nodes | 100000 | — |
| transport.depth | 32 | 80 |
| transport.scalar_bytes | 1048576 | — |
| transport.collection_entries | 10000 | — |
| transport.inflight | 32 | — |

## Bounds and typed packets

Resolution checks conversion to `usize` where required, existing constructor
bounds and arithmetic used by downstream budgets. Copy, snapshot and transient
depth cannot exceed 256; content, shared-document and transport depth cannot
exceed 80. Byte-cap-plus-one and node-count-times-four calculations are checked.
`mib_to_bytes` performs checked multiplication by 1024² and rejects zero; the
catalog itself does not use an implicit MiB unit. Request duration is converted
to a checked `Instant` deadline by the request factory.

The following relationships are required; each left-hand quantity must fit its
right-hand quantity:

| Quantities | Required relationship |
| --- | --- |
| Registry and artifact metadata | registry.entries ≤ artifact.entries; registry.key_bytes ≤ artifact.key_bytes and registry.bytes; artifact.key_bytes ≤ artifact.bytes; registry.string_bytes ≤ registry.bytes |
| Projection | projection.identity_bytes ≤ registry.key_bytes |
| Snapshot and transient copies | each bytes/nodes/path_bytes/depth value ≤ the corresponding copy value |
| Retained assets | retained.bytes ≤ snapshot.bytes; retained.files ≤ snapshot.nodes; retained.path_bytes ≤ snapshot.path_bytes |
| Content | content.scalar_bytes ≤ both content.input_bytes and content.output_bytes; content.collection_entries ≤ content.nodes |
| Shared documents | shared.scalar_bytes ≤ both shared.input_bytes and shared.output_bytes; entries/hook_tuples/references/documents ≤ shared.nodes |
| Transport | transport.scalar_bytes ≤ transport.bytes; transport.collection_entries ≤ transport.nodes |
| Lineage | checked sum of 64 source-digest bytes plus schema_bytes and selection_bytes ≤ lineage.total_bytes |

Content input/output counters are aggregate request budgets. Retained assets and
transient staging are bounded per capture/item. The policy therefore does not
require `content.input_bytes ≤ retained.bytes` or
`content.output_bytes ≤ transient.bytes`. Several individually small captures
can feed one request. Maximum identity and
namespace capacities are not summed into a hypothetical key; each actual
encoded key remains subject to its key limit.

The policy constructs real `HubLimits`, `DocumentLimits`, `ProjectionLimits`,
`ArtifactLimits`, `CopyLimits`, `MaterializationLimits`,
`LineageLimits`, `RetainedBytesLimits` and `ContentLimits`.
`snapshot_policy(link_policy, paths)` requires an explicit `CopyPolicy` and
selected asset paths; it does not discover or retain every file in a source tree.

`SharedWriteLimits` and `TransportLimits` are independent typed packets returned
by the validated policy. The former supplies document, journal, structure,
reference and hook-tuple bounds to the managed shared-document writer. The latter supplies
decoder structure and in-flight admission bounds to future dispatchers. These
packets are neither a shared writer nor a decoder, scheduler or dispatcher;
their public fields alone do not enforce validation or admission. Implementing
bounded body/frame reads and in-flight enforcement remains consumer work.

## One captured request

`HubResourcePolicy::request_context(start, cancellation)` derives one deadline
from a captured start and creates one `RequestContext`. Standalone library
callers may instead use `RequestContext::new(content_limits, cancellation, deadline)`.
The constructor validates content limits before creating the shared snapshot context.

The context owns one private `RenderContext` and one shared
`Arc<SnapshotContext>`. `parse_skill`, `parse_agent`, `managed_agent`,
`render_skill` and `render_agent` use that same renderer; `input_bytes()` and
`output_bytes()` expose observations without a mutable renderer accessor that
could replace the counters. Capture, preparation and publication retain the
same snapshot context, cancellation and deadline. Renderer content limits apply
to actual input and output; the snapshot context carries cancellation and deadline.

`ScopedCoordinator::writer(&RequestContext)` acquires Hub before Core. It checks
`remaining()` before each lock and passes only the remaining duration, checking
again after acquiring Hub and before acquiring Core. It never restarts the
deadline or substitutes a new request context. A canceled request returns
`Interrupted`; an exhausted deadline returns `TimedOut`. Runtime/lock
acquisition can have filesystem effects even though policy resolution is pure.
Synchronous native filesystem calls are not preemptible; deadlines are
cooperative checkpoints, not hard syscall deadlines.

Core defines `SnapshotContext::new(deadline, cancellation)` in `file_io`;
ArtifactStore reexports the same type. The
[read-only document capture](../guides/filesystem_contract.md#publication-safety-and-maintenance)
uses the same Arc instance, performs actual checked reads and preserves native
observation errors. Its file, path, ancestor and access limits remain separate
from the document codec and journal size limits. No aggregate IO reservation or
write counter is required.

## Effects, cancellation and recovery

After a physical effect, mandatory Core effect recording and mode restoration
must preserve the actual publication state. That protection does
not make Hub commit and receipt publication an uncancelable
transaction. Those steps retain the captured cancellation/deadline and can
leave a durable pending operation or a reported cleanup error for explicit
recovery. Acquisition failures, post-effect pending state and post-commit
cleanup failures must remain distinguishable to callers.

There is no automatic pending TTL, guaranteed power-loss durability or promise
that all cleanup finishes synchronously. The existing Core/Hub recovery
contracts still govern authority, receipt verification and retries; these
limits do not authorize adopting foreign data or skipping recovery.

## Default rationale and readiness

The frozen calibration corpus at Git revision
`e12fe4bc77ffb1fadb914139358c855a77875667` contains 22 real blobs under
`.agents/agents`, `.agents/skills`, `.kiro/agents` and `.kiro/hooks`: 298946 total
bytes, with a largest blob of 31623 bytes. Eight managed descriptors comprise
three roles and five skills. These are snapshot inputs and selected assets;
the count does not imply all 22 blobs are rendered descriptors.

The 8 MiB retained-byte and 16-file defaults admit that selected corpus with
headroom. They establish an engineering starting profile, not evidence of an
optimal allocation or arbitrary-workload admission.
Kiro V3 is the current supported binding; V2 and preview selection are retired. Limits do not infer a binding or upgrade its native format.

| Contract | Implementation evidence |
| --- | --- |
| Catalog, provenance, coherence and typed packets | `crates/graphit-hub/src/policy.rs`: `RESOURCE_LIMITS`, `HubResourcePolicy`, `LimitOrigin`, `Values::coherence` |
| Private shared renderer and request factory | `crates/graphit-hub/src/policy.rs`: `RequestContext`, `request_context` |
| Remaining budget before ordered locks | `crates/graphit-hub/src/publication.rs`: `ScopedCoordinator::writer` |
| Shared cancellation and deadline | `crates/graphit-core/src/file_io.rs`: `SnapshotContext`; ArtifactStore reexports the same type |
| Read-only native document observation | `crates/graphit-core/src/native_document_file.rs`: `CapturedDocumentDirectory`, `NativeDocumentObservation` |
| Protected post-effect recording/restoration | `crates/graphit-core/src/artifact_store.rs`: `with_io_context` |
| Cancelable Hub commit and cleanup | `crates/graphit-hub/src/publication.rs`: `finish`, `clear_committed`, recovery |

Hook composition, CLI/MCP/HTTP caller enforcement, native Windows/macOS runtime
and distribution remain separate functional deliveries.

The [native document codec](native_document_codec.md) consumes `SharedWriteLimits` and the captured request. Its structure limits are per document; it preserves the shared cancellation and deadline. Source and output byte caps are independent, so an admitted larger source can be inspected or shrunk.
