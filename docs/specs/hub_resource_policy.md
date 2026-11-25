---
title: Rust Hub resource policy
type: reference
updated: 2026-11-07
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

`RESOURCE_LIMITS` is the single catalog of flat keys. Each `LimitSpec` declares
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
`distribution_defaults(&Brand)` constructs a real document containing all catalog
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
| projection.fallback_entries | 10000 | 100000 |
| projection.name_bytes | 16777216 | 16777216 |
| projection.batch_items | 1000 | 100000 |
| artifact.bytes | 16777216 | 16777216 |
| artifact.key_bytes | 65536 | 65536 |
| copy.bytes | 536870912 | — |
| copy.nodes | 100000 | — |
| snapshot.bytes | 67108864 | — |
| snapshot.nodes | 10000 | — |
| retained.bytes | 8388608 | — |
| retained.files | 16 | — |
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
bounds and arithmetic used by downstream budgets. Content, shared-document and transport JSON depth cannot exceed 80. Byte-cap-plus-one and node-count-times-four calculations are checked.
`mib_to_bytes` performs checked multiplication by 1024² and rejects zero; the
catalog itself does not use an implicit MiB unit. Request duration is converted
to a checked `Instant` deadline by the request factory.

The following relationships are required; each left-hand quantity must fit its
right-hand quantity:

| Quantities | Required relationship |
| --- | --- |
| Registry and artifact metadata | registry.key_bytes ≤ artifact.key_bytes and registry.bytes; artifact.key_bytes ≤ artifact.bytes; registry.string_bytes ≤ registry.bytes |
| Projection | projection.identity_bytes ≤ registry.key_bytes |
| Source snapshots | each bytes/nodes value ≤ the corresponding copy value |
| Retained assets | retained.bytes ≤ snapshot.bytes; retained.files ≤ snapshot.nodes |
| Content | content.scalar_bytes ≤ both content.input_bytes and content.output_bytes; content.collection_entries ≤ content.nodes |
| Shared documents | shared.scalar_bytes ≤ both shared.input_bytes and shared.output_bytes; entries/hook_tuples/references/documents ≤ shared.nodes |
| Transport | transport.scalar_bytes ≤ transport.bytes; transport.collection_entries ≤ transport.nodes |
| Lineage | checked sum of 64 source-digest bytes plus schema_bytes and selection_bytes ≤ lineage.total_bytes |

Content input/output counters are aggregate request budgets. Retained assets are
bounded per capture/item, so several individually small captures can feed one
request. The policy does not require `content.input_bytes ≤ retained.bytes`.
Maximum identity and
namespace capacities are not summed into a hypothetical key; each actual
encoded key remains subject to its key limit.

The policy constructs real `HubLimits`, `DocumentLimits`, `ProjectionLimits`,
`ArtifactLimits`, `CopyLimits`, `MaterializationLimits`,
`LineageLimits`, `RetainedBytesLimits` and `ContentLimits`.
`snapshot_policy(link_policy, paths)` requires an explicit `CopyPolicy` and
selected asset paths; it does not discover or retain every file in a source tree.
`CopyLimits::new(bytes, nodes)` bounds payload bytes and node count.
`RetainedBytesLimits::new(bytes, files)` bounds the content retained for explicitly
selected descriptors and their count. Neither sums relative-path lengths.
Package paths still must be relative, valid and non-colliding.

`SharedWriteLimits` and `TransportLimits` are independent typed packets returned
by the validated policy. The former supplies document, structure,
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

`ScopedCoordinator::writer(&RequestContext)` requires a finite publication
request. It observes cancellation and the original remaining deadline before
opening metadata snapshots. Core and Hub acquire short locks for their actual
catalogue commits; source preparation does not hold both writers locked.
Hub keeps the Original absolute deadline and computes the remaining duration
immediately before its metadata lock, so a later commit cannot restart the budget.
Cancelled work returns `Interrupted`; expiry returns `TimedOut`. Native blocking
I/O still has cooperative checkpoints rather than hard syscall deadlines.

## Effects and explicit reruns

Physical publication and Hub metadata are independent atomic writes. Errors
retain `ArtifactEffect::NotApplied`, `MayHaveApplied` or `Applied`. There is no
pending-operation journal or replay lifecycle. After a confirmed physical change,
a metadata failure reports `Applied`. An explicit rerun reconciles owned desired
content and logical metadata; conflicting user content remains intact. See
[native installation publication](hub_native_installations.md) and
[shared document partial effects](native_shared_documents.md).

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
| Finite request and publication effects | `crates/graphit-hub/src/publication.rs`: `ScopedCoordinator::writer`, `PublicationError` |
| Shared cancellation and deadline | `crates/graphit-core/src/file_io.rs`: `SnapshotContext` |
| Document byte comparison and private registration | `crates/graphit-hub/src/native/shared_document.rs`: `publish_bytes`, `Persistence` |

Hook composition, CLI/MCP/HTTP caller enforcement, native Windows/macOS runtime
and distribution remain separate functional deliveries.

The [native document codec](native_document_codec.md) consumes `SharedWriteLimits` and the captured request. Its structure limits are per document; it preserves the shared cancellation and deadline. Source and output byte caps are independent, so an admitted larger source can be inspected or shrunk.
