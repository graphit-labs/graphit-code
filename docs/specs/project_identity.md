---
title: Project Identity
description: The lifecycle and invariants of the stable project ULID and mutable globally unique project name.
status: current
created: 2026-09-05
updated: 2026-11-06
tags: [project, identity, ulid, lockfile, hub]
references:
  - target: {type: task, id: 01M666RGGWZ1EFRFR1Z4N4Y8PN}
    relation: implements
  - target: {type: task, id: 6017MN1RVS69PVF71AC4DMPGFX}
    relation: derived_from
  - target:
      type: task
      id: tsk-28d2
    relation: supports
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-1fee
    relation: supports
  - target:
      type: task
      id: tsk-e079
    relation: supports
  - target:
      type: task
      id: tsk-8d62
    relation: supports
  - target:
      type: task
      id: tsk-c29b
    relation: supports
---

# Project Identity

The native identity is `.<brand>/project.json`. Setup, Source, configuration
and native stores consume the same typed identity and project context.

Every durable Graphit project has two identifiers with deliberately different jobs:

- `project.id` is a ULID. It is immutable and is the only project key used by stores, S3
  prefixes, locks, relationships, ownership, and exact access-control grants.
- `project.name` is a human-readable discovery name. It is globally unique within one Hub,
  canonically normalized for lookup, and may be renamed.

A name is never a storage address. Renaming a project changes discovery metadata without moving
or re-keying its stores, artifacts, events, memory, tasks, or installed-project claims.

## Identity lifecycle

Setup or explicit identity initialization gives a project its ULID. Configuration
writes require that initialized identity and never generate or replace it. Identity
creation remains independent from adapter publication:

```text
unidentified directory
  -> locally identified project
  -> remotely registered name
  -> published project
  -> project made visible by access grants
```

`ProjectIdentityStore` creates or reads the immutable project ID under its
writer lock. Pure read-only operations do not create identity. Setup and
subsequent instruction publication preserve that ID.

Ephemeral Live Search workspaces may still receive a ULID so normal resolvers work, but their
`project.ephemeral` flag prevents them from owning durable project graph, wiki, memory, or Task
state.

## Project version

Branches and worktrees of one repository share the committed identity and therefore the ULID,
but not the code. The **project version** selects which copy of the project's state a call uses:
the checked-out Git branch, or `main` for a directory without Git. A detached HEAD uses
`git:<full-object-id>`, isolated from the default branch. The **default version** is the
branch `origin/HEAD` names, else an existing `main`, else `master`, else the checked-out branch of
a fresh repository that has no branch yet, else `main`.

`GitCheckout::inspect` uses the system Git executable for reference formats,
worktrees, submodules and separate Git directories. Its `as_checkout` result
feeds the immutable version selector; it does not parse an old implementation
format or mutate the repository.

A version is never part of the identity. It does not change the ULID, the name, S3 prefixes, access
grants or relationships. It selects native Lance branches within the same corpus datasets
for AST Source/entities, Knowledge, Memory and Task. AST Icebug manifests/shards have their
own `ast/live/<version-key>/graph` publication; there is no copied wiki per version. The Git record hooks seed new branches from the exact
commit snapshot and merge independent record changes when Git merges. See
[Git record integration](git_records.md) for conflicts, commits without retained
snapshots and projects without Git. An ordinary readonly branch selection does
not create a snapshot or perform a merge.

Callers override the checkout's version with the MCP argument `project_version`, the CLI flag
`--project-version` or the `GRAPHIT_PROJECT_VERSION` environment variable. The variable applies only
to the project of the process's starting directory. See [Storage Layout](../architecture/storage_layout.md#versions-and-worktrees).

## Identity and global inventory

The local native identity schema is specified below. Project identity and
configuration share the project document; installed artifacts have separate state. The global lock
records maintained projects and their selected versions for discovery, while
the project identity remains the authority for its immutable ULID. Inventory
metadata does not grant access to a project or an imported artifact.

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

## Rust local-identity contract

`crates/graphit-core/src/project_identity.rs` keeps stable project identity separate
from inherited configuration while sharing one local document with its Project
settings. The canonical file is `.<brand>/project.json` (`.graphit/project.json`):

```json
{
  "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "name": "payments-api",
  "cluster": {"domain": ["payments"]},
  "ephemeral": false,
  "githooks": {"pre-commit": ["git diff --cached --check"]},
  "settings": {}
}
```

The complete UTF-8 JSON document has the configuration reader's 256 KiB limit;
the identity/metadata portion retains its existing 64 KiB validation. The file
codec accepts only root metadata. Public typed `ProjectIdentity` snapshots used
by inventory/API consumers retain their own `id`/`metadata` wire contract; that
snapshot serialization is not a second local document reader.

`description` and `cluster` may be omitted when empty; `ephemeral` is a boolean
that defaults to false. `settings` may be omitted; `githooks` is optional, at the
root. Identity/metadata are never inherited from configuration layers. Unknown or duplicate fields and duplicate cluster JSON keys are
rejected. `ProjectId` uses `ulid` 3.0.0, accepts only a canonical uppercase
26-character nonzero ULID, and has no public setter/generation path. Validated
`ProjectIdentity` snapshots expose `id`, immutable metadata and
`allows_persistent_stores()`; eligibility is false for ephemeral projects.
This does not yet enforce eligibility in native-store consumers.

`ProjectMetadata::new(name)` trims and ASCII-lowercases explicit names, requiring
1–64 ASCII bytes with alphanumeric edges and only alphanumeric, `.`, `_` and `-`
inside. Invalid/non-ASCII explicit names fail instead of being transliterated.
`for_directory(root)` sanitizes/truncates the canonical directory basename and
falls back to `project`; it does not derive a name from a Git remote. Description
text is preserved. Cluster keys/values are trimmed, empties removed and values
deduplicated/sorted. Nonempty trimmed-key collisions are rejected before
discarding entries with empty values. Input strings are bounded before making
normalized copies, and serialization uses a bounded writer. Stored metadata
must already be canonical rather than being silently repaired during reads.

`ProjectIdentityStore::at(&ProjectPaths)` consumes an already resolved immutable
brand/root context without selecting an ancestor or consulting global state.
`ProjectPaths::at` guards the canonical installation/global directory and aliases;
`discover` checks that guard both at the canonical starting directory and at
the discovered root, before returning a project. An absent installation path is
not created. `load()` returns `None` only for
actual absence and creates no directories, lock or identity. Invalid JSON,
noncanonical IDs/metadata, observed symlinks/dangling links/nonregular sources
and other I/O failures are errors, not absence.

`writer(timeout, &Cancellation)` opens the brand directory without following
links and retains its File for the non-Clone RAII writer's whole lifetime.
The existing brand parent may remain public. The stable adjacent
`.<brand>/.project.json.lock` is opened relative to that held parent without
following links, then acquired by the existing FileLock owned-file route.
Acquisition cannot recreate a disappeared parent. Both Project ConfigStore and
ProjectIdentityStore use this same relative private-lock acquisition, retaining
the parent for the writer lifetime regardless of which producer creates the
lock first. A new lock is empty and uses mode 0600 on Unix or the protected
explicit-owner private profile on Windows. An existing incompatible lock is
rejected without permission repair, unlinking or replacement. Identity
initialization creates no runtime or module store.
The parent handle and lock do not establish artifact/Stage ownership. Mutable
`ensure(metadata)` validates input and rereads under that lock: it creates a new
ULID only when absent and otherwise preserves the existing ID and metadata.
`ensure_with_id(metadata, id)` initializes an absent identity with a typed canonical,
nonzero `ProjectId`. An existing equal ID preserves its metadata and file; a
different ID returns `InvalidInput` without replacement. Both operations use the
same writer lock and native document validation. No legacy format is read.

`graphit setup --agent codex --project-dir . --project-id <ULID>` exposes explicit
initialization. The ID option requires `--project-dir`; invalid IDs are rejected
by the argument parser. A conflict is checked before changing the installation
agent configuration. Omitting the option retains normal ID generation. This
selects identity only: it imports no Task or other store data and grants no
PROJECT permission. Setup with a project then asks the authenticated Source to
maintain the checkout; it may start/register the native daemon manager. Use
installation-only setup when no maintained project is intended.
`update(expected_id, metadata)` requires an existing matching ID and changes
only root metadata while preserving settings/githooks; ConfigWriter uses the same
lock and preserves identity/metadata in the other direction. Absence is `NotFound`,
conflict is `InvalidInput`, and an equal
update does not rewrite. Never unlink/replace the lock with active holders or
contenders. The owned-drop correction in `tsk-e079` explicitly unlocks before
closing the handle, including when another descriptor retains the file description.

Identity and ConfigStore share the `local_files.rs` I/O boundaries:
regular observed sources, bounded reads, same-target-directory temporary files,
permissions applied before payload writing, file sync and cancellation before
atomic replacement. Existing permissions are restored, new files use Unix
0666 with umask or Windows defaults, read-only changes fail, and errors before
replacement clean the temporary without truncating the destination. Cancellation
before writer acquisition creates no directories; timeout bounds lock waiting,
not filesystem I/O. Atomic old/new visibility does not guarantee ACL/owner/xattr
preservation, directory fsync, power-loss durability, abrupt-kill cleanup,
external-editor coordination or hostile filesystem race protection.

On Windows the same-directory scratch is created with normal file attributes,
synced, then published by `std::fs::rename`. This uses the standard library's
native POSIX rename semantics when replacing a file held by a share-delete
reader; the reader keeps its previous bytes while new opens see the replacement.
Temporary pathname cleanup is disabled only after successful publication. Failure
or cancellation before publication retains temporary cleanup and the previous
destination. This does not change caller locking, access checks or cancellation,
and does not add retries. See the [Rust concurrency fix](https://github.com/rust-lang/rust/issues/123985).

`resolve_project_root(directory, &Brand)` discovers boundaries using
only a valid `.<brand>/project.json` or an observed regular-file/directory `.git`
marker, falling back to the starting canonical directory when neither exists.
Observed brand/project/Git symlinks, dangling links and special files are rejected;
bad identity JSON fails closed without selecting an ancestor. A `.git` scope
marker alone does not validate Git references; GitCheckout does that separately.
There is no `graphit.lock.json` fallback, import or format converter. Persistent
consumers bind the guarded `ProjectPaths` context. `ScopedPin::new(&ProjectPaths, version)`
and `VersionContext::select(&ProjectPaths, ...)` then perform pure selection with
no repeated discovery/I/O. Pins compare canonical root and brand key: another
brand in the same physical directory does not inherit the pin or identity.

## Related contracts

- [Hub access control](hub_access_control.md)
- [Hub S3 object layout](hub-s3-object-layout.md)
- [Hub collaboration](hub_collaboration.md)
- [Filesystem, State, and Watchers](../guides/filesystem_contract.md)
