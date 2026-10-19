---
title: Rust remote artifact metadata
type: reference
status: partial
updated: 2026-10-08
references:
  - target: {type: task, id: tsk-97a7}
    relation: implements
  - target: {type: task, id: tsk-7a13}
    relation: derived_from
  - target: {type: task, id: tsk-cc0e}
    relation: derived_from
  - target: {type: task, id: tsk-e493}
    relation: derived_from
  - target: {type: task, id: tsk-4208}
    relation: supports
  - target: {type: task, id: tsk-1dd5}
    relation: supports
---

# Rust remote artifact metadata

The `graphit_hub::remote_metadata` library validates catalog data for artifact
publishers and readers. It validates the current native wire format without a legacy reader or converter. The library neither contacts a provider nor grants
access, creates directories, hydrates payloads, or verifies physical ownership.
The authorized publisher (`tsk-4208`) and resolver (`tsk-1dd5`) are separate,
pending integrations with the real Data and Broker producers. See
[Hub access control](hub_access_control.md) and the
[caller contract](hub_consumer_contract.md) for those obligations.

## Wire contract

A document contains exactly one JSON object. Required fields are `v`, `producer`,
`artifact_type`, `id`, `name`, `versions`, and `dependencies`. Optional fields are
`description`, `tags`, `author`, and `latest`. Omission is distinct from explicit
`null`; null, unknown fields, duplicate fields (including escaped spellings), and
trailing JSON are rejected throughout the document.

```json
{
  "v": 1,
  "producer": "01KSH1CRFFG8Z74B5ZS78WW808",
  "artifact_type": "skill",
  "id": "org/tool",
  "name": "tool",
  "versions": [
    {
      "version": "branch/main",
      "hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    }
  ],
  "dependencies": []
}
```

`v` is the integer 1. `producer` is a canonical, nonzero `ProjectId`; it identifies
the publishing project, independently of the consuming project. `artifact_type`
uses the existing ten `ArtifactType` values: skill, command, workflow, agent,
rule, mcp, ast, knowledge, language, and power. `id` is the complete published
`ArtifactId`, including qualified segments. An installation alias does not
change this ID. The manifest does not carry a second physical ID, a consumer
project, a local path, a URL, credentials, or an authorization revision.

`name` follows the existing portable destination-name validation. Descriptions
and tags are bounded Unicode display strings. `author`, when present, is an
object with one required `username`: a nonempty string without leading/trailing
whitespace or control characters. Tags may be omitted or empty.

`versions` is a nonempty array of objects with required `version` and `hash`.
Versions must be unique, pass the existing bounded `ConcreteVersion` identity
validation, and cannot be the reserved selector `latest` (case insensitive).
Named values with branch slashes and Unicode are preserved exactly. Each hash is
exactly 64 lowercase hexadecimal characters and is exposed as a 32-byte SHA256
digest. The codec validates the digest's representation; it does not hash a
payload or establish that it exists. Payload-integrity verification belongs to
the publisher and resolver.

`latest` is optional. If present, it names an exact member of `versions`. A
publication containing only named branches may omit it; the codec does not
fabricate a release or silently choose a version. Dependencies are objects with
required `artifact_type`, complete `id`, and string `selector`. Their selectors
remain literal data, including empty/latest, numeric prefixes, exact releases,
and named channels. This library does not resolve, normalize, or authenticate
selectors. The future version producer preserves those capabilities.

## Library boundary and resource policy

`RemoteArtifactManifest::encode` receives a borrowed `ManifestInput`, explicit
`MetadataLimits`, and the original `policy::RequestContext`.
`RemoteArtifactManifest::decode` receives bytes, the same explicit policy shape,
and the original context. Decoded manifest/version/dependency fields are private
and available through read-only getters. There is no public deserialization or
constructor that turns arbitrary stored data into an authenticated result.

Every capacity is positive: document bytes, JSON nodes, scalar bytes, collection
entries, versions, and dependencies. `ProjectionLimits` separately validates
artifact identities and concrete versions. Capacities apply to one document;
no product defaults are hidden in this library. JSON keys count as scalar nodes,
as do scalar values and object/array containers. Collection capacity applies to
every object and array. Version/dependency arrays also have their own independent
caps. A policy with zero capacity returns `InvalidPolicy`.

Decoding uses typed visitors for the fixed wire shape, without an intermediate
JSON value tree or a second parse. It checks document bytes before parsing,
nodes/scalars before retaining values, and array limits before decoding another
item. Retained vector growth uses bounded geometric reservations. Unknown fields
fail before their values are decoded, so arbitrary nested extension trees are
not part of this schema. Escaped-string parser scratch space and allocator
overhead are not a hard RSS guarantee.

Encoding preflights borrowed fields, cross-field consistency, nodes and
collections. A counting serializer then checks the exact encoded byte length
before reserving output capacity; the second serialization checks that length as
it writes. No output is returned on failure. Both directions check the original
permanent cancellation and absolute deadline throughout processing and before
returning. Writer cancellation and deadline errors are terminal IO errors; they
do not use `Interrupted`, which `Write::write_all` would retry. The original
`MetadataError` stays in the private processing state. The codec does not
construct a new request context or extend its deadline.
The codec performs no filesystem/network IO and does not charge or reset the
original snapshot's IO counters; real IO accounting belongs to those producers.

Errors are closed `MetadataError` values: `InvalidPolicy`, `InvalidDocument`,
`Limit`, `Cancelled`, and `Deadline`. Debug/Display expose only the error class,
with no source error, input bytes, credentials, URLs, or field values. Invalid
identity syntax or its `ProjectionLimits` rejection is `InvalidDocument`;
document/node/scalar/collection/version/dependency capacity failures are `Limit`.

## Integration and qualification

The Rust resolver must authorize before metadata, content or hydration, bind the
requested producer/type/ID to the manifest, resolve a concrete version, and
revalidate its live grant and source observation. A path, digest, `SourceRef`,
cached manifest, or membership record provides no authority. Global metadata
needs the separately qualified multi-version schema. Ephemeral reads create no
durable registry, pins, stores or supervisor registration. Registry bytes and
payload integrity do not substitute for a private source lease.

Focused fixtures in `crates/graphit-hub/src/remote_metadata/tests.rs` cover all ten
artifact types, qualified IDs, producer distinct from consumer, branch-only
metadata, selector preservation, encode/decode, composition with real `SourceRef`
without payload writes, all six capacity boundaries, invalid policies, wire
faults, hash/version consistency, and original cancellation/deadline. A private
writer regression fixture runs the real counting/output writers and JSON
serializer in four bounded subprocesses: cancellation and expiry of the
original deadline after successful preflight, for each phase. The parent kills
a child exceeding five seconds, so an `Interrupted` retry regression fails
without hanging the suite. The child test is ignored as a standalone entry and
is executed by the parent; it is not omitted from this qualification. Commands:

```text
cargo test --locked -p graphit-hub remote_metadata
cargo clippy --locked -p graphit-hub --all-targets -- -D warnings
```

Qualification of this pure library does not establish a remote endpoint,
authorization/hydration integration, Windows/macOS execution, native host
behavior, performance equivalence, or completion of the ecosystem migration.
