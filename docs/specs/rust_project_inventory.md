---
title: Rust maintained project inventory
description: Local project and version facts consumed independently of online presence.
status: implemented-main-qualified
references:
  - target: {type: task, id: tsk-f48f}
    relation: supports
  - target: {type: task, id: tsk-a437}
    relation: supports
  - target: {type: task, id: tsk-b91e}
    relation: relates_to
  - target: {type: task, id: tsk-0137}
    relation: supports
  - target: {type: session, id: ses-c98a}
    relation: relates_to
---

# Maintained project inventory

`graphit_core::project_inventory` stores local project and version facts in
`project-inventory.json`. The stable `.project-inventory.lock` serializes actual
writers. The JSON contains historical data; it supplies no authorization, online
status, heartbeat, lease or proof that a checkout is currently present.

## Identity, checkouts and versions

Projects use the persistent nonzero `ProjectId` from `project.json`. Renaming
metadata preserves that ID. Ephemeral identities cannot enter persistent
inventory. Checkouts record their configured native path, real `GitCheckout`
observation and selected `VersionContext`. Paths use `NativePath` JSON strings;
non-Unicode paths fail explicitly. Paths and selection data grant no access.

Registration retains the selected version. Subsequent registration updates the
checkout and preserves older versions. `retain_version` records a version after
its domain producer confirms maintenance; `remove_version` follows confirmed
removal. A version still selected by a registered checkout cannot be removed.
Removing a checkout preserves its versions. The project entry disappears only
when both checkouts and versions are empty. These operations never delete datasets.
Closing or parking a daemon does not remove maintained inventory.

## Observation and mutation API

`ProjectInventoryObservation::observe(project, expected, checkout, selection,
request)` receives `&ProjectPaths`, the expected `ProjectId`, `&GitCheckout`,
`&VersionContext` and `&SnapshotContext`. It loads the existing identity and
checks the expected ID, persistence eligibility, brand and selection association.
It neither creates an identity nor obtains authorization. An Auth consumer uses
the ProjectId from its admitted logical `PublicationScope`.

An observation owns ordinary project/identity/checkout data and can be cloned or
inspected. Before atomic publication, a mutation checks the current ProjectId and
persistence eligibility. If checkout maintenance would replace existing metadata,
it first checks that the observed metadata still matches the checkout document.
A stale observation can maintain checkout or version facts while preserving
newer inventory metadata for the same ID. An identical registration performs no
publication or extra project reread. Use a fresh observation to publish fresh
metadata through checkout maintenance. An observation is not bound to a previous request: each writer uses
its own actual Original cancellation and deadline.

`GlobalProjectInventory::at(&InstallationPaths, max_document_bytes)` selects the
brand, configured root and positive JSON byte limit. Construction and reads create
no directory, lock or identity. Exact absence returns an empty snapshot. Invalid
JSON/schema, unknown or duplicate fields, duplicate project IDs/checkouts/versions,
invalid domain values, wrong file types and oversized documents remain errors.
The bounded reader reports an oversized existing document as `InvalidData`.
Configured filesystem links and ordinary permissions on this non-secret JSON are
accepted through normal platform path semantics.

`store.writer(&original)` receives `&Arc<SnapshotContext>` and requires an
existing parent. It acquires the kernel `FileLock` with the same cancellation
token and any deadline already present; it invents no finite timeout. The writer owns the lock until drop. There is no retained
directory chain, physical identity, directory-depth budget or namespace receipt.

`register_checkout`, `retain_version`, `remove_checkout` and `remove_version`
reread the document under the writer lock. Equivalent updates and missing removals
return `false`; a checkout registered for a different ProjectId cannot be rekeyed.
Brand/selection mismatch fails with `InvalidInput`; wrong ProjectId, invalid JSON
or ineligible persistent identity fails explicitly; missing identity stays
`NotFound`. Filesystem, deadline and cancellation errors keep their actual causes.

The existing same-directory temporary-file helper atomically replaces complete
content after the final pre-publication checks. Existing normal permissions are
preserved; inventory JSON has no credential-privacy mode gate. JSON output and
input stay bounded by the actual document byte limit. This is not a transaction
with Lance, project identity or any remote store. A confirmed Knowledge or AST
publication remains confirmed if its subsequent inventory mutation fails.
Derived state is recovered by explicitly rerunning its producer.

## Reading and consumers

Snapshots expose borrowed `projects()`, `get(ProjectId)`, `identity()`,
`versions()` and `checkouts()`. Reading does not reopen every recorded checkout or
remove records because a checkout is offline. Deserializing historical data
restores no grant, request or filesystem capability.

[Project bootstrap](rust_project_bootstrap.md) observes checkout selection.
Knowledge publication retains only targets confirmed by the real SDK and returns
partial publication and retention outcomes without cross-store rollback.
Maintained-inventory reads combine the running service's logical InstallationId
with this JSON; realtime Now and Broker admission remain separate contracts.

## Verification

`crates/graphit-core/src/project_inventory/tests.rs` covers actual Git/worktrees,
independent process writers, brand and ProjectId conflicts, renamed metadata,
ephemeral/missing/corrupt identities, literal Unicode paths, configured links,
ordinary JSON permissions, bounded decoding, Original cancellation/deadline and
atomic failure preservation. Current Linux qualification is recorded by the
session coordinator. Earlier directory-capture tests do not qualify this API.
Native Windows/macOS execution requires separate evidence. Synchronous filesystem
checks are cooperative and do not promise hard SDK preemption or power-loss durability.


## Local project discovery

`graphit projects list` and `graphit projects show --project-id ID` (or
`--name NAME`) use the live authenticated service's `graphit_projects_list/get`.
They read the maintained local inventory, independently of the remote Hub.
`list` filters by `--name` (substring), `--project-id`, `--version` and
`--online[=true|false]`. Use `--label-key KEY` to require a label and add
`--label-value VALUE` to require an exact value. `--page-size` defaults to 25. Continue using the exact
`--cursor` returned, with unchanged filters. If the authorized observation
changes, the cursor fails as expired instead of silently changing pages.
`show` matches an exact ID or unique exact name. Unknown and unauthorized IDs
share a single not-found-or-denied result.

Each entry includes `description` and the `cluster` map of label keys to values.
It distinguishes historical `registered`, retained `versions`, and
checkouts with `registered_version`, actual observed `exists` (null when
filesystem observation fails), daemon-local `online` membership and `current`
selection. Online does not describe Broker presence; use `graphit_instances`
for that independent journey. `--no-paths` omits local paths. Returned
`checkouts[].dir` is selection data and never grants module/content access.
Only native callers admitted for that PROJECT/version see it; installation
Configure administrators can inspect historical entries including retired
projects. Discovery never starts a project or adds access.

```sh
graphit projects list --name graphit --page-size 10
graphit projects show --name graphit-code
graphit projects list --online=false --no-paths --json
graphit projects list --label-key service --label-value api
```

Use `graphit projects label-set --project-id ID --key service --value api` to
add a value. `graphit projects label-unset --project-id ID --key service --value api`
removes that value. Omit `--value` from `label-unset` to remove the entire key.
These commands target the active checkout in the current project by default.
Use `--project-dir DIR` to select a different checkout and `--version VERSION`
when that checkout has multiple active versions. The supplied ID must match.

Root MCP tools `graphit_projects_labels_set` and `graphit_projects_labels_unset`
take `project_id`, exact `project_dir`, `key`, optional `project_version` and
`value` (required for set). For MCP discovery use `label_key` and `label_value`.
Keys and values are case-sensitive after trimming surrounding whitespace.
CLI text output retains the exact spelling of label keys. On version-aware
`projects` commands, `--project-version` supplies the same exact version as
`--version`. Differing values fail before any daemon request. Retirement targets
the whole checkout and rejects `--project-version`.
Mutations use the current Native project administrative authority and write only
the selected checkout's identity document and its projection in local inventory.
They leave checkout registration, current selection and retained versions in
place. Other checkouts can have their own copies of the project document. The
inventory reflects the metadata last written or maintained for its project ID.
An older observation cannot replace the current metadata after a label edit.
Background version retention preserves the metadata of existing entries.
Writes serialize under the existing inventory and project identity locks, in
that order. Each document is replaced atomically. If the project document was
written but its inventory projection could not be saved, the operation reports
`inventory_refresh_failed` and whether the project document changed. The CLI
exits with code 1 and writes the partial receipt to stdout in the selected
format. MCP retains the structured tool error. Repeating the same request can
repair the projection after the underlying error clears.
