---
title: Private installation directory bootstrap
references:
  - target: {type: task, id: tsk-0070}
    relation: implements
  - target: {type: task, id: tsk-e849}
    relation: derived_from
  - target: {type: task, id: tsk-a22e}
    relation: relates_to
  - target: {type: task, id: tsk-effe}
    relation: relates_to
  - target: {type: task, id: tsk-9bd4}
    relation: relates_to
  - target: {type: task, id: tsk-e911}
    relation: derived_from
---

# Private credential directory setup

There is no Core installation-bootstrap module or captured-installation carrier.
A configured installation is a conventional `Path`/`PathBuf`. Paths can pass
through user-selected symlinks or junctions and can select another volume.
No ancestor handles, identities, depth/path budgets, creation-effect ledger or
revalidation state are retained.

A service that needs a missing credentials root creates the ordinary parent
path with `std::fs::create_dir_all`, then uses the filesystem SDK's
`create_private_directory` for the credentials directory itself. An existing
root is used without permission repair; credential operations check its current
private access policy through the SDK. No credential, provider, profile or
application authority is minted by creating a directory.

The [daemon startup](rust_daemon_auth_startup.md) performs this setup after
checking its actual original finite deadline and cancellation. Runtime and Auth
then use configured paths and real private-file operations. The
[authentication document contract](private_auth_document.md) and
[runtime key contract](runtime_key.md) describe their content/locking behavior.

An interrupted setup can leave ordinary empty directories. An explicit next
startup uses the configured path again; there is no BootstrapOutcome,
PrivateInstallation, cleanup proof, rollback or recovery state machine.
Directory links are followed using conventional filesystem semantics rather
than rejected by a universal confinement protocol. Actual authorization and
credential privacy remain separate requirements.
