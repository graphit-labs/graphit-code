---
title: "Hub S3 object layout"
description: "The versioned S3 key and document contract for project identity, discovery, access control, artifacts, and mutable project state."
status: draft
created: 2026-08-21
updated: 2026-10-11
tags: [hub, s3, registry, project, acl, artifact, icebug, lancedb]
---

# Hub S3 object layout

Artifact keys and file integrity now follow [Remote artifact storage](remote_artifact_storage.md).
The artifact sections below use that current Rust contract. Project/name/access/event document
examples remain control-plane design proposals; they do not establish implemented Rust routes.

The Hub's authoritative backend is an S3 bucket. This document fixes the keys, ownership
boundaries, publication ordering, and document shapes shared by publishers and consumers. Project
identity is defined by [Project Identity](project_identity.md); authorization and selective
discovery are defined by [Hub Access Control](hub_access_control.md).

## Location and credentials

Every key below is relative to the active provider's S3 prefix. Local providers configure the
bucket, region, endpoint and prefix directly and use login credentials, an AWS profile or the AWS
credential chain. When first-class Broker discovery advertises
`graphit-s3-credentials-v3`, the Broker selects its private route, derives an STS session policy
from the authenticated principal's current grants for the requested project, user-memory, or Hub
metadata scope and physical module, and returns the topology plus temporary credentials. Broker credentials/topology
remain only in process memory and are never written to `auth.json`. When valid discovery omits it, the provider uses local storage and this remote object
layout is unavailable. A configured or advertised credential exchange failure fails closed.

The client sends the scope, physical module, and, for project data, its immutable ULID. Project
`tasks`, `memory`, `knowledge`, and `ast` directories use their corresponding modules;
`project.json`, `registry`, `artifacts`, and `events` use `hub`. Artifact subfolders never change
that rule, so `artifacts/knowledge` and `artifacts/ast` use `hub`. User memory uses user/memory;
global registry and rules use hub/hub. It never sends a
requested bucket, prefix, policy, role or duration to the Broker. Broker
ACL and STS/IAM policy constrain the returned session, while object requests flow directly to S3.

Credential protocol v3 is the current exchange contract. An unsupported advertised protocol
fails closed. Its version is independent of the artifact object-layout version.

## Version 2 key convention

```text
v2/
  registry/
    names/<normalized-name>.json
    baselines.json

  projects/<project-ulid>/
    project.json
    events/<artifact-type>/<event-ulid>_<action>.json

  global/
    projects.json
    rules/<module>.md
    rules/<module>_skill.md

  anonymous/
    projects.json

  authenticated/
    projects.json

  users/<user-id>/
    projects.json

  teams/<team-id>/
    projects.json
```

`<project-ulid>` is the immutable `project.id`; the mutable project name never occurs in a project
data prefix. `<folder>` is the storage folder for the artifact type: `agents`, `rules`, `workflows`,
`skills`, `knowledge`, `ast`, `mcp-servers`, `commands`, `powers`, or `languages`. A numeric or named
version is encoded as one opaque collision-free segment so `branch/feature/api` cannot create nested
or overlapping prefixes.

The name directory is a small control-plane index for uniqueness and friendly lookup. It does not
contain artifact entries. Project metadata, registry entries, payloads, and project events are
colocated by ULID so one project prefix is the policy unit. Mutable Memory and Task LanceDB tables
use their own provider-scoped prefixes and are mounted directly when S3 is configured.

## Project and name documents

`v2/projects/<project-ulid>/project.json` is the canonical ULID-to-current-name record:

```json
{
  "v": 2,
  "project": {
    "id": "01J...",
    "name": "payments-api",
    "description": "Payments service",
    "revision": 7,
    "status": "active"
  }
}
```

The key ULID and `project.id` must agree. `revision` is monotonic and participates in conditional
updates. Unknown schema versions, disagreement, or a changed ULID are integrity errors.

`v2/registry/names/<normalized-name>.json` resolves an active name to its project:

```json
{
  "v": 2,
  "name": "payments-api",
  "project_id": "01J...",
  "project_revision": 7,
  "status": "active"
}
```

Name creation uses conditional put-if-absent. Rename reserves the new name, updates the project at
the expected revision, activates the new record, then tombstones or removes the old record. Readers
accept only an active name whose project document agrees. No step moves `v2/projects/<ULID>/`.

## Access documents

`v2/global/projects.json`, `v2/anonymous/projects.json`, `v2/authenticated/projects.json`,
`v2/users/<user-id>/projects.json`, and `v2/teams/<team-id>/projects.json` share this shape:

```json
{
  "v": 1,
  "projects": [
    {"id": "01J..."},
    {"name_prefix": "payments-"},
    {"all": true}
  ]
}
```

A missing or empty document contributes no grant from that level. Authenticated subjects read the
authenticated and exact-user documents; the reserved `anonymous` subject reads the anonymous
document and never uses the `users/` namespace. Unknown versions and malformed
selectors fail closed. ACL objects are control-plane inputs, never project payloads; their complete
evaluation contract is in [Hub Access Control](hub_access_control.md).

## Artifact registry entries

Use the strict Rust `RemoteArtifactManifest` version-1 codec and the exact identity
keys defined in [Remote artifact storage](remote_artifact_storage.md). Producer, type,
full artifact ID and exact concrete version must agree; current hashes have 32 bytes.
The earlier envelope, short hashes and unencoded artifact keys are not accepted or
converted. Registry metadata never supplies authorization.

## Publication ordering

Publish the content-addressed release first and conditionally update its registry
last, following [Remote artifact storage](remote_artifact_storage.md#publication-and-concurrency).
Keep partial effects and conditional-write conflicts explicit. Do not overwrite an
open native release or silently erase a concurrent publisher's versions.

## Native mounts and file artifacts

Ordinary file artifacts use one ZIP object, hashed over its final exact bytes and
streamed into an owned stage. Native Lance and Icebug remain direct S3 mounts, with
backend-specific generation/root integrity. Their SDK opener is pending in `tsk-478c`;
the file ZIP/hash contract does not qualify a native mount. Local explicit native ZIP
import remains the separate [Native packages](../development/native-packages.md) transport.

## Mutable project state

Memory and Task are authoritative LanceDB stores. They use filesystem paths when S3 is absent and
provider-scoped `s3://` prefixes when S3 is configured. LanceDB performs reads and writes directly
with the active renewable credential provider.

Project versions of Memory, Task and Dream add no prefix. Each version other than the default is a
native Lance branch inside the table dataset: its objects are under
`<module prefix>/<table>.lance/tree/<branch>/` and its reference is
`<module prefix>/<table>.lance/_refs/branches/<branch>.json`. The module's credential therefore
covers every version. Creating a version needs only read, write and list. Deleting one also needs
delete. See [Project Identity](project_identity.md#project-version).

Events are append-only objects below
`v2/projects/<ULID>/events/<artifact-type>/<event-ulid>_<action>.json`. The project key and any
explicit `project_id` field must agree. Event payloads include the trusted subject's `user_id` and
the project ULID as `project_id` by default. With `hub.events.anonymize=true`, the payload contains
neither ID: it contains `project_hash = SHA-256(project_id + client.secret)` and
`user_hash = SHA-256(subject.user + client.secret)` instead. The hashes are stable for one Graphit
installation; the S3 object key remains project-scoped and therefore still contains the project
ULID used for authorization and storage isolation.

## Local cache is not part of the object layout

`~/.<brand>/hub/cache/<hub-fingerprint>/<subject-fingerprint>/` may cache selectively read name,
project, artifact, and page metadata. It is bounded, lazy, and disposable. It never becomes an S3
replica, never grants access, and is not part of the publication protocol. The former eager registry
mirror and `~/.<brand>/hub.registry.json` authority are removed.

## Error scenarios

| Condition | Behaviour |
|---|---|
| No S3 configured on a local provider | Filesystem-only behavior |
| Valid first-class Broker discovery omits `graphit-s3-credentials-v3` | Filesystem-only behavior; remote Hub operations are unavailable |
| Broker advertises an invalid S3 capability, cannot be discovered/authenticated, fails issuance, or loses the capability while renewing an existing grant | Fail closed; do not silently change storage authority |
| ACL document absent | No grant from that level |
| Authenticated subject unavailable | Use the teamless `anonymous` subject; missing global and anonymous grants deny project access |
| ACL invalid or authorization backend unavailable | Fail closed; do not use a cached positive decision |
| Name already reserved | Registration or rename fails without changing the project ULID |
| Name record and project metadata disagree | Integrity error; do not resolve the name |
| Artifact registry entry absent | Artifact is not discoverable even if an orphan payload exists |
| Registry entry names a missing payload | Hard integrity error |
| Cache absent, stale, or deleted | Re-read selectively from the authoritative control plane |

## Current formats

Use only the current format for each implemented component. There is no old-layout
probe, converter or migration workflow. Artifact layout is defined independently in
[Remote artifact storage](remote_artifact_storage.md); control-plane examples above
remain proposals until their specific Rust producers are implemented and tested.

## Related specifications

- [Project identity](project_identity.md)
- [Hub access control](hub_access_control.md)
- [Hub collaboration](hub_collaboration.md)
- [Storage layout](../architecture/storage_layout.md)
- [Memory module](memory_module.md)
- [Task module](task_module.md)
