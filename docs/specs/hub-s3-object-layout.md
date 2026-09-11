---
title: "Hub S3 object layout"
description: "The versioned S3 key and document contract for project identity, discovery, access control, artifacts, and mutable project state."
status: draft
created: 2026-08-21
updated: 2026-09-10
tags: [hub, s3, registry, project, acl, artifact, icebug, lancedb]
---

# Hub S3 object layout

The Hub's authoritative backend is an S3 bucket. This document fixes the keys, ownership
boundaries, publication ordering, and document shapes shared by publishers and consumers. Project
identity is defined by [Project Identity](project_identity.md); authorization and selective
discovery are defined by [Hub Access Control](hub_access_control.md).

## Location and credentials

Every key below is relative to the active provider's S3 prefix. Local providers configure the
bucket, region, endpoint and prefix directly and use login credentials, an AWS profile or the AWS
credential chain. Direct OIDC providers configure that topology and obtain renewable credentials
with `AssumeRoleWithWebIdentity`. When first-class Broker discovery advertises
`graphit-s3-credentials-v2`, the Broker selects its private route, derives an STS session policy
from the authenticated principal's current grants for the requested project, user-memory, or Hub
metadata scope, and returns the topology plus temporary credentials. Broker credentials/topology
remain only in process memory and are never written to `auth.json`. When valid discovery omits it, the provider uses local storage and this remote object
layout is unavailable. A configured or advertised credential exchange failure fails closed.

The client sends only the scope and, for project data, its immutable ULID. It never sends a
requested bucket, prefix, policy, role or duration to the Broker. Broker
ACL and STS/IAM policy constrain the returned session, while object requests flow directly to S3.

## Version 2 key convention

```text
v2/
  registry/
    names/<normalized-name>.json
    baselines.json

  projects/<project-ulid>/
    project.json
    registry/<artifact-type>/<artifact-id>.json
    artifacts/<folder>/<artifact-id>/<version>/...
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

`v2/projects/<project-ulid>/registry/<artifact-type>/<artifact-id>.json` contains discovery metadata
and all published versions for one artifact. The tuple `(project_id, type, artifact_id)` is the
stable artifact identity; an artifact ID need not be globally unique.

```json
{
  "v": 2,
  "entry": {
    "project_id": "01J...",
    "id": "payments-core",
    "name": "Payments Core",
    "type": "ast",
    "description": "Indexed code graph of the payments service",
    "tags": ["billing", "go"],
    "author": {"username": "publisher"},
    "latest": "2.1.0",
    "versions": ["2.0.0", "2.1.0"],
    "hashes": {"2.1.0": "9f2c4e1b7a03d5c8"},
    "dependencies": []
  }
}
```

The key type and ID, enclosing project ULID, and document fields must agree. Dependencies may carry
`project_id`, `type`, `id`, and `version`; omission of `project_id` means the same publishing project,
not a global artifact.

## Publication ordering

An artifact version is the prefix
`v2/projects/<ULID>/artifacts/<folder>/<artifact-id>/<version>/`. It is the unit of upload,
replacement, and retraction.

The publisher uploads or mirrors the payload first and writes the per-project registry entry last.
An interrupted upload therefore leaves unreferenced bytes, never a visible half-published version.
Replacing an already published version updates its content hash, but readers that already mounted
the prefix must reopen it; a new numeric version is the safe snapshot cutover.

`branch/...` may retain native Lance history and `graphit-history.json`; `tag/...` is a compact
self-contained snapshot. Retention must not delete objects still referenced by a registry entry or
protected branch history.

## Native mounts and file artifacts

Hub Knowledge and AST FTS are LanceDB prefixes opened directly on S3. Hub AST graphs are canonical
Icebug Parquet bundles queried directly from S3 by LadybugDB. A local project may shallow-clone a
published Lance branch: inherited fragments stay in S3 and new fragments form a local overlay until
publication. File artifacts required by an Agent as ordinary files are downloaded into the managed
local installation. A lockfile claim proves membership and version selection, while current grants
and storage policy still determine access.

## Mutable project state

Memory and Task are authoritative LanceDB stores. They use filesystem paths when S3 is absent and
provider-scoped `s3://` prefixes when S3 is configured. LanceDB performs reads and writes directly
with the active renewable credential provider.

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
| No S3 configured on a local or direct OIDC provider | Filesystem-only behavior |
| Valid first-class Broker discovery omits `graphit-s3-credentials-v2` | Filesystem-only behavior; remote Hub operations are unavailable |
| Broker advertises an invalid S3 capability, cannot be discovered/authenticated, fails issuance, or loses the capability while renewing an existing grant | Fail closed; do not silently change storage authority |
| ACL document absent | No grant from that level |
| Authenticated subject unavailable | Use the teamless `anonymous` subject; missing global and anonymous grants deny project access |
| ACL invalid or authorization backend unavailable | Fail closed; do not use a cached positive decision |
| Name already reserved | Registration or rename fails without changing the project ULID |
| Name record and project metadata disagree | Integrity error; do not resolve the name |
| Artifact registry entry absent | Artifact is not discoverable even if an orphan payload exists |
| Registry entry names a missing payload | Hard integrity error |
| Cache absent, stale, or deleted | Re-read selectively from the authoritative control plane |

## Clean cutover

Only this v2 layout is valid. Hub does not discover, import, copy, or fall back to objects stored
under another layout. Projects and artifacts that must remain available are registered and
published again into v2, and grant documents are created explicitly. Objects outside `v2/` are
invisible to the runtime and never imply access.

## Related specifications

- [Project identity](project_identity.md)
- [Hub access control](hub_access_control.md)
- [Hub collaboration](hub_collaboration.md)
- [Storage layout](../architecture/storage_layout.md)
- [Memory module](memory_module.md)
- [Task module](task_module.md)
