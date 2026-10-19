---
title: Captured local Lance branch staging
content-type: reference
audience: developers
status: library-qualified-consumer-pending
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-b913}
    relation: implemented_by
  - target: {type: task, id: tsk-eb91}
    relation: uses
  - target: {type: task, id: tsk-c453}
    relation: derived_from
  - target: {type: task, id: tsk-ed33}
    relation: supports
  - target: {type: task, id: tsk-0688}
    relation: supports
  - target: {type: task, id: tsk-8cc0}
    relation: relates_to
---

# Captured local Lance branch staging

`BranchSnapshot::spawn_native_clone_local(stage: TempDir, original: Arc<SnapshotContext>)`
returns `io::Result<JoinHandle<io::Result<NativeModuleClone>>>`. It delegates to the
existing [local staging worker](lance_native_clone.md) through a private module
snapshot. It does not expose a writable `ModuleSnapshot`, materialize an inherited
target, adopt a newer head or infer an SDK version from an artifact or Git version.

Current Knowledge writers materialize missing named branches from captured Main.
Those SDK snapshots retain Main data through base references and can add their own
fragments. The bridge preserves the captured physical branch and version, including
old pins after Main or that named branch advances. Default and inherited captures
clone their retained physical Main version without creating the requested branch.
The destination's generated physical version and manifest location are returned by
the SDK and can differ from the source version.

The shared worker accepts a direct local primary store and non-Blob schema.
Bases referenced by data/overlay files, deletion files and raw index metadata
must be SDK dataset roots at the Main location of the same table, with explicit
absolute paths or `file`/`file+uring` URIs. Entries not referenced by any of these
SDK objects are ignored; overwriting a shallow branch can leave such metadata even
when the branch is empty. No old base is relocated, imported or aliased. Existing SDK URI/path
resolution checks them before initializing the base client; the actual client must
also use direct local paths. Relative, tilde, remote, foreign-root, data-only and
other-branch bases are `Unsupported`. No environment or working-directory fallback
is added. The SDK can normalize a relative caller input before capture; staging
uses the resulting absolute captured base rather than resolving that input again.

These checks describe the maintained storage layout. They do not grant access.
The caller retains its real whole-corpus/source and PROJECT captures, authenticated
admission, selected SDK session and the same Original through the joined operation,
then performs its required revalidation before delivery or publication. A source
URI, client, schema, pin or base metadata cannot manufacture authority. The SDK
and cooperative rechecks do not provide continuous pathname confinement.

The worker owns `TempDir` until the SDK future finishes. The caller retains and
joins the standard handle before shutdown, cleanup or publication. Original is
checked before spawning, before copying and after completion. A started local
filesystem copy cannot be hard-aborted; cancellation and deadline checks are
cooperative. Existing I/O kinds, SDK causes and directory-owner cleanup remain
unchanged. `NativeModuleClone` provides the existing readonly directory and
[pinned metadata view](lance_native_metadata.md).

[Official Lance](../development/lance-upstream.md) consumes native local paths
for new data and clone destinations. Persisted referenced file-URI bases require
an explicit one-time SDK conversion; opening a table by another locator alone
does not change them. Lance owns fragments, deletions, row IDs, indices, files
and generated manifests. Product code neither patches the SDK nor reconstructs
a manifest. Blob remains unsupported by this local export contract; mixed
remote credentials are not implemented here.

Implementation and real SDK fixtures:
[lance_module_store.rs](../../crates/graphit-data/src/lance_module_store.rs) and
[lance_native_branch_clone.rs](../../crates/graphit-data/tests/lance_native_branch_clone.rs).
Actual publisher `tsk-ed33`/`tsk-0688` and fresh HubWrite/Broker STS `tsk-8cc0`
remain separate required outcomes; local fixture success does not qualify them.

Historical Lance 13 qualification: six focused SDK/Data cases and 30 affected
regressions pass, along with strict Data library/branch-clone/clone/branch-store
checks. The first fixture assumptions (branch version and SDK normalization) and
one pre-test linker failure remain recorded in Task history; corrected fixtures
and the retry after operational disk cleanup passed. No new caller authority or
remote publication is certified by these checks.
