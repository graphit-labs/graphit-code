---
title: "Hub S3 object layout"
description: "The versioned S3 key and document contract for project identity, discovery, access control, artifacts, and mutable project state."
status: draft
created: 2026-08-21
updated: 2026-09-05
tags: [hub, s3, registry, project, acl, artifact, icebug, lancedb]
---

# Hub S3 object layout

The Hub's authoritative backend is an S3 bucket. This document fixes the keys, ownership
boundaries, publication ordering, and document shapes shared by publishers and consumers. Project
identity is defined by [Project Identity](project_identity.md); authorization and selective
discovery are defined by [Hub Access Control](hub_access_control.md).

## Location and credentials

| Setting | Config key | Notes |
|---|---|---|
| Bucket | `hub.bucket` | Absent enables local-only behavior where supported |
| Region | `hub.region` | Falls back to the AWS provider chain |
| Endpoint | `hub.endpoint` | Supports MinIO and other S3-compatible services |
| Key prefix | `hub.prefix` | Isolates Hubs sharing one bucket |
| Access key | `hub.access_key_id` | Used only with the secret key |
| Secret key | `hub.secret_access_key` | Used only with the access key |

Every key below is relative to `hub.prefix`. S3 credentials authenticate a workload to the bucket;
they do not identify an end user or replace project authorization. A production deployment either
routes reads through an authorizing Hub service or issues temporary credentials scoped to the
authorized `v2/projects/<ULID>/` prefixes.

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
    memory/...
    <task.prefix>/...

  global/
    projects.json
    rules/<module>.md
    rules/<module>_skill.md

  users/<user-id>/
    projects.json
    memory/...

  teams/<team-id>/
    projects.json
```

`<project-ulid>` is the immutable `project.id`; the mutable project name never occurs in a project
data prefix. `<folder>` is the storage folder for the artifact type: `agents`, `rules`, `workflows`,
`skills`, `knowledge`, `ast`, `mcp-servers`, `commands`, `powers`, or `languages`. A numeric or named
version is encoded as one opaque collision-free segment so `branch/feature/api` cannot create nested
or overlapping prefixes.

The name directory is a small control-plane index for uniqueness and friendly lookup. It does not
contain artifact entries. Project metadata, registry entries, payloads, project events, project
memory, and project Tasks are colocated by ULID so one project prefix is the physical policy unit.

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

`v2/global/projects.json`, `v2/users/<user-id>/projects.json`, and
`v2/teams/<team-id>/projects.json` share this shape:

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

A missing or empty document contributes no grant from that level. Unknown versions and malformed
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

## Directly mounted artifacts

AST and knowledge data stay on S3:

| Artifact half | Engine | Authoritative URI |
|---|---|---|
| AST graph | LadybugDB | `s3://<bucket>/<prefix>/v2/projects/<ULID>/artifacts/ast/<id>/<version>/graph` |
| AST search | LanceDB | `s3://<bucket>/<prefix>/v2/projects/<ULID>/artifacts/ast/<id>/<version>/search` |
| Knowledge wiki | LanceDB | `s3://<bucket>/<prefix>/v2/projects/<ULID>/artifacts/knowledge/<id>/<version>/index.lance` |

Install records a versioned claim and creates only the local mount metadata required by the engine.
It does not download graph, search, or wiki data. File-based artifact types are materialized under
the managed `~/.<brand>/artifacts/modules/` tree because IDEs consume ordinary files.

Authorization is revalidated before creating or reopening a remote mount. A lockfile claim proves
membership and version selection, not current permission.

## Mutable project state

Project memory and Task tables use the same project policy boundary:

```text
v2/projects/<ULID>/memory/...
v2/projects/<ULID>/<task.prefix>/...
```

`task.prefix` defaults to `tasks` and remains the configurable final segment. User memory remains
under `v2/users/<user-id>/memory/...`. These are mutable multi-writer LanceDB
stores, not versioned artifacts. Their table schemas, revisions, leases, and concurrency rules stay
owned by the Memory and Task module specifications.

Events are append-only objects below
`v2/projects/<ULID>/events/<artifact-type>/<event-ulid>_<action>.json`. The project key and any
`project_id` field must agree.

## Local cache is not part of the object layout

`~/.<brand>/hub/cache/<hub-fingerprint>/<subject-fingerprint>/` may cache selectively read name,
project, artifact, and page metadata. It is bounded, lazy, and disposable. It never becomes an S3
replica, never grants access, and is not part of the publication protocol. The former eager registry
mirror and `~/.<brand>/hub.registry.json` authority are removed.

## Error scenarios

| Condition | Behaviour |
|---|---|
| `hub.bucket` unset | Local-only behavior where supported; remote Hub operations report not configured |
| ACL document absent | No grant from that level |
| Trusted subject unavailable | Deny remote discovery and access |
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
