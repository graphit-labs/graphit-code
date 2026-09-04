---
title: "Hub S3 object layout"
description: "The key convention and document schemas of the Hub's S3 bucket — the contract that replaces the git repository as the Hub's persistence and retrieval backend."
status: draft
created: 2026-08-21
updated: 2026-09-04
tags: [hub, s3, integration, spec, registry, artifact, icebug, lancedb]
---

# Hub S3 object layout

The Hub's backend is an S3 bucket. This document is the **contract** between the publisher
and the consumer: which keys exist, what each one holds, and which of them a query engine
mounts directly instead of downloading.

It replaces the git repository described in
[Hub Collaboration Specification](hub_collaboration.md), whose five responsibilities —
registry, one orphan branch per artifact version, `refs/events/*` telemetry, rule
distribution on `main`, and memory worktrees — all map onto key prefixes here. Implemented by
`internal/hub` over `internal/s3store`; tracked in
the migrated Graphit Task `tsk-c049ad9ad5b7`.

## Location and credentials

| Setting | Config key | Notes |
|---|---|---|
| Bucket | `hub.bucket` | Absent means local-only mode, which is not an error |
| Region | `hub.region` | Falls back to the AWS default chain when unset |
| Endpoint | `hub.endpoint` | For MinIO and other S3-compatible servers; implies path-style addressing |
| Key prefix | `hub.prefix` | Optional, lets several Hubs share one bucket |
| Access key | `hub.access_key_id` | Optional; active only with the secret key |
| Secret key | `hub.secret_access_key` | Optional; active only with the access key |

Authentication has two modes. A complete explicit pair can be stored in the global
Graphit config by `graphit setup`; otherwise the AWS SDK default provider chain resolves
environment variables, shared profiles, container credentials, or workload/instance roles.
If either explicit key is blank, both are removed and the provider chain remains active.
`config.S3Config` carries the pair to the AWS SDK, LanceDB/object_store, and LadybugDB only
when both values exist. The secret is redacted from config output but is plain text at rest
in the owner-only global config file. See
[S3 Credentials and UI Network Configuration](../guides/s3-and-ui-network.md).

Every key below is written relative to `hub.prefix`. `internal/s3store` applies and strips the
prefix, so nothing above that layer ever sees it.

## Key convention

```
registry/
  projects/<h2>/<hash>/project.json                     project metadata
  projects/<h2>/<hash>/<type>_<name>_<version>.json     one artifact entry
  baselines.json                                        default baseline artifacts

artifacts/
  <folder>/<project>/<id>/<version>/…                   file-based artifact types
  ast/<project>/<version>/…                             graph + search stores
  knowledge/<project>/<version>/index.lance/            complete published wiki

events/
  <project>/<artifactType>/<ULID>_<action>.json         telemetry, append-only

rules/
  <name>.md                                             team-wide rule overrides

memory/
  <scope>/<id>/…                                        authoritative LanceDB memory tables

<task.prefix>/
  project/<id>/…                                        authoritative LanceDB task/control/check/comment tables
```

Where:

- `<h2>` is the first two hex characters of `<hash>`, which is `sha256(projectRemoteID)` in
  lowercase hex. The fan-out keeps any single listing small. A globally-scoped artifact uses
  the literal project id `_global`.
- `<folder>` comes from `hub.TypeFolderMap`: `agents`, `rules`, `workflows`, `skills`,
  `knowledge`, `ast`, `mcp-servers`, `commands`, `powers`, `languages`.
- `<project>` is the publishing project's remote id, or `_global`.
- `ast` and `knowledge` omit the `<id>` segment because their compiled context is scoped by
  publishing project and version.
- `<task.prefix>` is the normalized `task.prefix` configuration value and defaults to `tasks`.

Numeric versions keep their literal key segment. Named versions may use Git-style branch paths such
as `branch/feature/api`; the logical name remains unchanged in the registry, while the storage layer
encodes it as one opaque, collision-free segment. A named version therefore cannot create nested or
overlapping S3 prefixes.

### An artifact version is a prefix, not a file

`artifacts/<folder>/<project>/[<id>/]<version>/` is the unit of publication and of
retraction. It is written by `s3store.UploadDir` and removed by `s3store.DeletePrefix`.
An upload is an exact mirror: after every local object has uploaded successfully, remote objects
missing from the local directory are deleted from that version prefix. No unrelated version or
artifact prefix is touched.

`branch/...` is the deliberate exception to exact directory mirroring. Its `*.lance` directories
are authoritative mutable lineages, and `graphit-history.json` maps Git commits to protected native
Lance tags and versions for each table. Republishing mirrors non-Lance files but never deletes these
datasets or their history manifest. A fresh sync shallow-clones a protected table tag, so removing a
referenced branch manifest or data fragment can orphan that local clone.

A `branch/...` name first published outside Git uses ordinary exact mirroring and has no
`graphit-history.json`. This supports mutable named channels for non-Git projects without pretending
that commit ancestry exists. Such a publication is rejected if that prefix already contains a
Git-backed history manifest.

**The registry entry is the commit for a new version.** The prefix is uploaded first, and only then
does the entry file under `registry/` name it. A publication interrupted midway leaves an orphan
prefix that no entry points at — wasted bytes, never a half-visible version. Replacing an already
published version is supported and updates the entry's content hash, but readers already using that
known prefix do not gain a new atomic indirection; production publishers should use a new version
when live readers require snapshot isolation.

For a named version beginning with `tag/`, the staged `search.lance` and `index.lance` stores are
release snapshots. Every table is compacted and pruned until only its current Lance version remains;
publication aborts if that invariant cannot be verified. This removes edit/time-travel history and,
combined with exact mirroring, prevents stale manifests or data files from accumulating when the
same tag is republished. Other version prefixes are not part of that cleanup.

## What the query engines mount directly

This is the part that makes installation stop downloading.

| Artifact half | Engine | What it receives |
|---|---|---|
| graph | LadybugDB | `s3://<bucket>/<prefix>/artifacts/ast/<project>/<version>/graph` as a table's `storage`, with `format = 'icebug-disk'` |
| search | LanceDB | `s3://<bucket>/<prefix>/artifacts/ast/<project>/<version>/search` as the connection target |
| knowledge wiki | LanceDB | `s3://<bucket>/<prefix>/artifacts/knowledge/<project>/<version>/index.lance` as the connection target |

Both URIs come from `s3store.Store.URI`, which is why its exact shape (`s3://bucket/key`,
never an HTTPS endpoint URL) is load-bearing rather than cosmetic.

Installing a mountable artifact therefore records its versioned claim and derives the URI at read
time. **No knowledge-index bytes are transferred at install time.** File-based artifact types — rules, skills,
commands, agents, MCP configs, languages — are still downloaded, because the IDE reads them
from disk.

### The graph half: icebug-disk

`artifacts/ast/<project>/<version>/graph/` holds, per table:

| Object | Content |
|---|---|
| `nodes_<table>.parquet` | the node's own attributes |
| `indices_<table>.parquet` | target nodes sorted by source — the CSR `indices` array |
| `indptr_<table>.parquet` | the CSR row-pointer array |
| `schema.cypher` | the DDL that mounts every table, with `storage` rewritten to this prefix's S3 URI |

Each Parquet carries `icebug_disk_version` in its file metadata. Reading requires the
LadybugDB `httpfs` extension, which ships inside the launcher payload and is loaded with
`LOAD EXTENSION '<runtime>/httpfs.lbug_extension'` — never `INSTALL`, so no query ever waits
on the network for it.

### The search half: LanceDB

`artifacts/ast/<project>/<version>/search/` is a LanceDB database directory. Indexes are
**data here, not an engine structure to rebuild**: unlike the SQLite era, the inverted index
and the vector index travel with the table, because the consumer never opens a local copy to
build them in.

## Document schemas

### `registry/projects/<h2>/<hash>/<type>_<name>_<version>.json`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Hub registry entry file",
  "type": "object",
  "required": ["v", "entry"],
  "additionalProperties": false,
  "properties": {
    "v": {
      "type": "integer",
      "const": 1,
      "description": "Manifest version. A consumer that does not know this value must refuse the file rather than guess its shape."
    },
    "entry": {
      "type": "object",
      "required": ["id", "name", "type"],
      "additionalProperties": false,
      "properties": {
        "id": {
          "type": "string",
          "minLength": 1,
          "description": "Stable artifact identifier, unique within its type."
        },
        "name": {
          "type": "string",
          "minLength": 1,
          "description": "Human-readable name shown in listings."
        },
        "type": {
          "type": "string",
          "enum": ["agent", "rule", "workflow", "skill", "knowledge", "ast", "mcp", "command", "power", "language"],
          "description": "Artifact type. Determines the folder segment of the artifact prefix."
        },
        "description": {
          "type": "string",
          "description": "One-line summary; this is the text Hub search matches as a substring."
        },
        "tags": {
          "type": "array",
          "items": {"type": "string"},
          "description": "Free-form labels for discovery."
        },
        "author": {
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "username": {"type": "string", "description": "Publisher's username."}
          },
          "description": "Who published the artifact."
        },
        "latest": {
          "type": "string",
          "description": "Numeric or named version resolved when an install omits @version."
        },
        "versions": {
          "type": "array",
          "items": {"type": "string"},
          "description": "Every published version, each with a prefix under artifacts/."
        },
        "hashes": {
          "type": "object",
          "additionalProperties": {"type": "string"},
          "description": "Version to content hash, for detecting a prefix that changed under a published version."
        },
        "dependencies": {
          "type": "array",
          "description": "Artifacts installed alongside this one.",
          "items": {
            "type": "object",
            "required": ["id"],
            "additionalProperties": false,
            "properties": {
              "id": {"type": "string", "description": "Dependency artifact id."},
              "type": {"type": "string", "description": "Dependency artifact type; inferred when omitted."},
              "version": {"type": "string", "description": "Version constraint; latest when omitted."}
            }
          }
        },
        "project_id": {
          "type": "string",
          "description": "Publishing project's remote id. Absent means the artifact is global."
        }
      }
    }
  }
}
```

Example:

```json
{
  "v": 1,
  "entry": {
    "id": "payments-core",
    "name": "Payments Core",
    "type": "ast",
    "description": "Indexed code graph of the payments service",
    "tags": ["billing", "go"],
    "author": {"username": "laino.santos"},
    "latest": "2.1.0",
    "versions": ["2.0.0", "2.1.0"],
    "hashes": {"2.1.0": "9f2c4e1b7a03d5c8"},
    "project_id": "payments-service"
  }
}
```

### `registry/projects/<h2>/<hash>/project.json`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Hub project file",
  "type": "object",
  "required": ["v"],
  "additionalProperties": false,
  "properties": {
    "v": {"type": "integer", "const": 1, "description": "Manifest version."},
    "project": {
      "type": "object",
      "required": ["remote_id", "name"],
      "additionalProperties": false,
      "properties": {
        "remote_id": {"type": "string", "minLength": 1, "description": "Stable project identity; the input to the sha256 that names this directory."},
        "name": {"type": "string", "minLength": 1, "description": "Human-readable project name."},
        "description": {"type": "string", "description": "What the project is for."}
      }
    }
  }
}
```

### `events/<project>/<artifactType>/<ULID>_<action>.json`

Append-only telemetry. The ULID prefix keeps a listing chronological, and every object is
written exactly once — there is no update path, so concurrent publishers never contend.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Hub telemetry event",
  "type": "object",
  "required": ["action", "at"],
  "additionalProperties": false,
  "properties": {
    "action": {
      "type": "string",
      "enum": ["global.setup", "global.uninstall", "project.init", "project.update", "project.remove", "artifact.install", "artifact.uninstall"],
      "description": "What happened. The set is closed: an unknown action is a newer publisher and the consumer must skip it, not fail."
    },
    "at": {"type": "string", "format": "date-time", "description": "When it happened, RFC 3339 in UTC."},
    "artifact_id": {"type": "string", "description": "Artifact acted on, when the action names one."},
    "artifact_type": {"type": "string", "description": "Its type, mirroring the key segment."},
    "version": {"type": "string", "description": "Version acted on."},
    "project_id": {"type": "string", "description": "Project the action happened in."},
    "attributes": {
      "type": "object",
      "additionalProperties": {"type": "string"},
      "description": "Action-specific detail, such as the ide and cli chosen at setup."
    }
  }
}
```

## Error scenarios

| Condition | Behaviour |
|---|---|
| `hub.bucket` unset | `s3store.ErrNotConfigured` — local-only mode, never surfaced as a failure |
| Object absent | `s3store.ErrNotFound` — a missing registry is a first run, not an error |
| Bucket unreachable or credentials missing | Fatal at `setup`, which names the bucket and endpoint and explains that the configured pair or AWS provider chain must supply authentication |
| Entry file with an unknown `v` | Refused, not parsed — a newer publisher wrote it |
| Artifact prefix present with no registry entry | Ignored as an interrupted publication; safe to delete |
| Context prefix absent | Not an error — the project published no wiki; the consumer indexes zero chunks and says so |
| Registry entry naming a version whose prefix is absent | Hard error on install: the pointer moved without its data, which is the one ordering this layout is designed to prevent |

## Deliberate non-goals

- **No locking.** Two publishers writing different artifacts touch disjoint prefixes. Two
  publishers writing the *same* artifact version is a collision the registry entry resolves
  last-writer-wins, exactly as the branch-per-artifact layout did. A CI publisher that can update
  multiple refs must serialize registry writes.
- **No automatic artifact-version retention.** Lance maintenance applies only to local database
  history and skips remote mounts. Published versions remain registry-addressable until explicitly
  retracted, so an independent bucket lifecycle must not delete referenced prefixes.
- **No backward compatibility with the git layout.** Nothing reads a Git branch, Git ref, or
  worktree; a named Hub version is registry data, not a repository reference. The project is in
  development and the old format is not migrated.
