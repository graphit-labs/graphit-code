---
title: Installed self-update
references:
  - target: {type: task, id: tsk-19ca}
    relation: implements
  - target: {type: task, id: tsk-20d9}
    relation: uses
---

# Installed self-update

`graphit self-update` verifies and installs the maintained public release without
requiring a project. Development builds and an already current version return
without changing the installation or starting the daemon.

## Preparation and destination

The caller supplies the actual executable, installation, request cancellation
and deadline. The updater resolves the executable's parent using ordinary
filesystem paths. Configured links and other volumes are allowed. No ancestor
handles, directory identities or metadata-equality contract are retained.

The [release preparer](rust_self_update_preparation.md) downloads and validates
the release metadata, checksum, archive entries and platform runtime. Its limits
bound this external input, including decoded archive bytes; they are not corpus
quotas. Archive entry traversal checks protect the extraction destination.
Preparation completes before the updater takes publication locks or stops the
daemon. Temporary package contents belong only to that invocation.

## Publication and coordination

The updater uses the existing file-lock API to exclude another updater in the
same executable prefix. Its `.graphit-update.lock` is a cooperative writer lock,
not an authorization credential or a directory-identity record. Different global
directories sharing that prefix therefore cannot install concurrently.

The launcher obtains authenticated daemon status. A running daemon is stopped
before replacement. The existing `daemon.pid` singleton lock excludes startup
while package files are replaced. Missing discovery does not bypass an occupied
singleton. Only a daemon that was previously running is restarted, and success
requires observed running readiness; self-update does not install a service.

Incoming runtime files are published first, followed by root runtime libraries,
the model-server executable and finally Graphit. Each file is streamed to a
same-directory temporary file, synchronized and atomically replaced through the
existing filesystem helper. This also works when a configured runtime directory
is on another volume: the temporary file is created at its actual destination.
There is no requirement for atomic replacement of the entire package.

Only incoming package paths are replaced. Configuration, credentials, project
stores and unrelated files remain untouched. Broker owns
`graphit-runtime/broker` and `graphit-runtime/licenses/broker`; a Code package
claiming those namespaces is rejected. Updating Code does not copy or replace
the Broker runtime. The updater does not remove arbitrary extra runtime files.

## Failures and explicit continuation

There is no installation journal, backup set, rollback protocol or automatic
replay. A failure reports its first cause, the affected package path when known,
whether package files remain unchanged, were partially installed or were fully
installed, and the daemon effect actually observed.

If replacement stops part way through, already published files remain published
and the daemon is not restarted into a known partial package. Resolve the
reported conflict or access error and explicitly run installation again using
the verified release. A restart failure reports an installed package and a
failed restart; it does not pretend the package was restored. Cancellation or a
local deadline does not grant new authority to perform cleanup or restart.
Temporary-file replacement is not a guarantee of power-loss durability across
multiple files. Permission errors are returned without changing existing access
policy to make replacement succeed.

An operating system may reject replacement of an executable or library still in
use. In particular, Windows in-process self-replacement is not qualified by Linux
fixtures. If it is denied, stop the affected process and use the external package
installer before restarting it. Native platform qualification must exercise the
real installed executables; a cross-compilation is not that evidence.

## Tests

The self-contained tests consume small releases prepared through the real
loopback HTTP, checksum and archive path. They cover independent file
publication, preservation of unrelated files and Broker namespaces, explicit
rerun after a partial filesystem error, cancellation, competing updater locks,
linked destinations and the real daemon singleton. They do not require a
published release, vendor account, Docker or a model download.

The separate installed-binary qualification exercises authenticated daemon stop
and restart, including a restart denied by an occupied port. Its executable is
an explicit qualification input rather than a dependency of the default suite.
