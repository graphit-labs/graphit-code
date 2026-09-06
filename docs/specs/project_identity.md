---
title: Project Identity
description: The lifecycle and invariants of the stable project ULID and mutable globally unique project name.
status: draft
created: 2026-09-05
updated: 2026-09-05
tags: [project, identity, ulid, lockfile, hub]
---

# Project Identity

Every durable Graphit project has two identifiers with deliberately different jobs:

- `project.id` is a ULID. It is immutable and is the only project key used by stores, S3
  prefixes, locks, relationships, ownership, and exact access-control grants.
- `project.name` is a human-readable discovery name. It is globally unique within one Hub,
  canonically normalized for lookup, and may be renamed.

A name is never a storage address. Renaming a project changes discovery metadata without moving
or re-keying its stores, artifacts, events, memory, tasks, or installed-project claims.

## Identity lifecycle

A project receives its ULID on the first operation that must persist project state. Identity
creation is therefore independent from full adapter and baseline initialization:

```text
unidentified directory
  -> locally identified project
  -> remotely registered name
  -> published project
  -> project made visible by access grants
```

`EnsureProjectIdentity` is the conceptual boundary. It atomically creates or reads a minimal
`graphit.lock.json` containing the ULID and a derived or explicitly supplied name. A later
`graphit init` reconciles IDE adapters, baselines, configuration, and managed files without
changing that ULID. Pure read-only operations do not create identity.

Durable stores use only the lockfile ULID. A `path-*` directory is not a project store under this
contract: Graphit neither reads nor migrates it. After identity creation, missing state is rebuilt
under the ULID from the project's authoritative sources.

Ephemeral Live Search workspaces may still receive a ULID so normal resolvers work, but their
`project.ephemeral` flag prevents them from owning durable project graph, wiki, memory, or Task
state.

## Lockfile contract

The minimal identity record is:

```json
{
  "project": {
    "id": "01J...",
    "name": "payments-api"
  }
}
```

Once written, `project.id` cannot be changed by rename, sync, init, registration, publication, or
installation. A conflicting ULID is an integrity error, not an update. Installed artifact records
refer to the publishing project by ULID.

## Name contract

Names use one versioned rule before reservation or lookup: trim surrounding whitespace, convert
ASCII letters to lowercase, then require either one alphanumeric character or the regular
expression `[a-z0-9][a-z0-9._-]{0,62}[a-z0-9]`. Non-ASCII input and every other character are
rejected rather than transliterated. The resulting 1–64 byte value is safe as one S3 key segment and
needs no escaping. Two inputs that normalize to the same key are the same name.

A name-prefix selector applies the same normalization to the portion before its single trailing
`*`. Wildcards in any other position are rejected by access-control schema version 1.

Local identity can exist before the name is globally reserved. Registration creates the name
reservation with a conditional write; failure because the normalized key already exists leaves the
local ULID intact and asks for another name.

## Rename

Rename is an idempotent control-plane transaction:

1. conditionally reserve the new normalized name for the existing ULID;
2. conditionally update `v2/projects/<ULID>/project.json` at the expected revision;
3. activate the new name reservation;
4. tombstone or remove the old reservation.

Readers accept only active name records whose ULID and current project metadata agree. An
interrupted transaction may temporarily leave a pending or old tombstoned record, but never two
active owners. Repair resumes from recorded state; it does not move project data.

Exact ACL grants refer to the ULID and survive rename. Name-prefix grants are intentionally dynamic:
their matches follow the current active name.

## Related contracts

- [Hub access control](hub_access_control.md)
- [Hub S3 object layout](hub-s3-object-layout.md)
- [Hub collaboration](hub_collaboration.md)
- [Filesystem, State, and Watchers](../guides/filesystem_contract.md)
