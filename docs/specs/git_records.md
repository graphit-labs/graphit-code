---
title: Task and Memory snapshots for Git commits
type: specification
---

# Task and Memory snapshots for Git commits

PROJECT Task (including sessions, history, comments, checks and references) and
PROJECT Memory use their existing authoritative Lance datasets, locally or
directly on authorized S3. USER Memory remains independent of Git. Source code,
Git commits and Lance snapshots are separate resources; no Git commit contains
credentials, record payloads or host-specific storage paths.

## Installation and normal use

Project setup installs these hooks; `graphit git install` also enables them on an
already maintained project. This captures the current
HEAD baseline and installs the brand-owned `post-commit`, `post-applypatch`,
`post-checkout`, `post-merge`, `post-rewrite` and committed `reference-transaction`
notifications through the existing
Core registry (Git 2.54 or newer). Commands use the binary on PATH and Git's
checkout working directory, including linked worktrees. Other hooks are retained.
The daemon must be available with a valid native identity and the project's
Memory and Task write permissions. Missing authorization or S3 errors fail
explicitly; the hook never falls back to a local store.
Repeating installation preserves an existing commit snapshot and pending working
records. Preparing/prepared/aborted reference transactions require no daemon call.
Symbolic ref notifications are accepted as Git protocol data; only numeric
HEAD/current-branch updates can select a captured snapshot.

Each commit retains an immutable native Lance tag `git-<full-object-id>` in each
dataset. Tags reference the physical branch and snapshot version; they share
existing data and indexes and protect the snapshot from native cleanup. Capturing
the same Git commit never moves an existing tag. Empty stores also have a
snapshot, so later data cannot appear when returning to that empty commit.

A commit tag binds the admitted snapshot captured while preparing that module's
notification. A later record write or background embedding publication may advance
the working dataset without invalidating this capture. Tagging preserves the captured
rows and leaves the newer working head intact; it does not refresh an existing
corpus's textual index or wait for inference. Task and Memory do not need a shared
generation. Operations that change working records, such as checkout and merge,
retain their native revision and optimistic commit checks.

New branches are materialized from that exact snapshot using native shallow
branches. Ordinary mutable records continue to use the branch selected by
`VersionContext::lance_branch()`. Detached HEAD selects `git:<object-id>`
instead of altering the default version. Checking out only files does not move
record state. The CLI refreshes the existing daemon checkout selection after a
successful record transition; it does not introduce a second inventory.

Checkout restores the retained Task/Memory snapshot for the target commit.
Unsnapshotted record changes are a conflict, not permission to discard them.
Compatible embeddings travel with their record rows. Vectors calculated after a
commit was tagged can also be reused when record identity, content fingerprint and
embedding recipe still match. This updates only the working branch's semantic
fields; the commit tag remains immutable. Background-only semantic updates do
not turn unchanged public content into a conflicting edit. Restoring a snapshot
on its original physical branch uses native Lance restore; a new version uses a
shallow branch, and an existing different branch applies only changed families.

## Merge, rewrites and recovery

Merges compare complete record families against the unique Git merge base:
unchanged families and independent edits are combined; identical edits are
deduplicated. Divergent edits to the same family report its identity explicitly.
No last-writer-wins rule invents task completion or drops history. Both modules
are checked before publication; concurrent writes still use native optimistic
commit checks. There is no atomic transaction spanning Git, Task and Memory.
If later IO fails, confirmed module effects are reported for an explicit retry.

Git's post-operation notifications cannot undo a commit, checkout or merge that
already happened ([Git hook contract](https://www.kernel.org/pub/software/scm/git/docs/githooks.html)). Read the hook's result. Fix authorization or service failures
and retry the corresponding hook; `graphit git snapshot` explicitly binds the
current state to HEAD, and `graphit git restore [previous-commit]` retries a
checkout after resolving unsnapshotted record changes. Old references remain
immutable. A restored historical snapshot never bypasses current authorization,
claim validity, cancellation, deadlines or the normal record write rules.

Rewrites retain old references without moving immutable tags. During replay,
each post-commit uses Git's `CHERRY_PICK_HEAD` source commit and its parent to
merge that record delta onto the new parent, then tags the new commit. The final
post-rewrite verifies that every rewritten commit already has its own Task and
Memory snapshots before restoring the final HEAD; it does not alias old state
onto every replayed commit. Amend captures the current working records normally.
A missing snapshot is reported; pre-install commits and commits produced without these
hooks cannot have their historical record state reconstructed from present data.
Reference updates to a retained commit also restore its state, covering normal
resets and current-branch updates. Updates of unrelated refs are ignored. A new
commit's reference transaction reports `pending_snapshot` until its post-commit
or post-applypatch notification captures the actual state; it does
not claim a snapshot already exists. Commits produced with hooks bypassed cannot
have their historical state invented later: use an explicit `graphit git snapshot`
only to establish the chosen current baseline. Octopus and multiple-base merges require
explicit reconciliation rather than selecting an arbitrary parent.

`graphit git sync` reconciles the union of these notifications and commands from
`githooks`, without taking another snapshot. One dispatcher handles both;
native record handling precedes configured commands, and Git output diagnostics use
stderr. [Git hooks](../guides/git-hooks.md) defines command order, shell and stdin replay.
`graphit git remove` removes the brand's registrations, including configured commands. Projects
without Git continue using their normal `main` version with no Git subprocess
effects, hooks or synthetic commits. Snapshots are not copies or legacy imports.

## Implementation and validation boundary

Core owns Git facts, version selection and hook registrations. Data owns native
Lance tags and exact snapshot operations. Records owns authorized selection and
record-family merge semantics. App receives authenticated runtime notifications
on `/api/git/records` and invokes those producers; HTTP paths and hook arguments
grant no access and cannot select arbitrary datasets or commit parents.

Each module uses one admitted dataset: `memory/index.lance` or
`task/index.lance` locally, or its authoritative selected S3 dataset. Project
versions select native Lance branches within that dataset. Immutable native Git
tags associate commits with their exact branch and version, including tags
created by another installation. Checkout and merge resolve those tags directly;
no corpus routing file, alternate dataset or copied record mirror participates.
A missing native tag remains an explicit missing snapshot.

Local publication retains the existing PROJECT writer, Original, authorization
and physical directory checks. Snapshot operations preserve native history and
compatible embeddings, and retain their actual partial/uncertain effects and
recovery keys. They cannot overwrite a conflicting immutable Git tag or invent
historical state for a commit that was never captured.

Focused tests use temporary Git repositories, real Lance stores and the actual
HTTP consumer without provider accounts. Platform-specific execution is recorded
separately; Linux tests do not qualify Windows or macOS.
