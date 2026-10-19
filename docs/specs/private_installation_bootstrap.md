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

# Private installation directory bootstrap

`graphit_core::installation_bootstrap::prepare` qualifies an existing private
installation root or creates its missing directories with the filesystem SDK.
It accepts immutable `InstallationPaths`, explicit limits and the original
`SnapshotContext`. It creates no configuration, authentication document, lock,
MCP key, provider or profile and grants no caller permissions.

The startup consumer supplies captured bootstrap intent. An MCP request body
cannot substitute that intent. AuthStore and runtime startup integration remain
separate deliveries; this module prepares their directory prerequisite.

## Preparation and effects

The nearest existing ancestor is captured by actual native handles. Missing
children are admitted before creation, including their canonical capture paths
and retained ancestor paths. Each create uses private SDK birth, with original
context and physical parent checks before and after the operation. The returned
creation handle must match the captured directory and pass SDK privacy checks.

Existing ancestors above the created section may be ordinary directories. They
are never changed. An existing final root must already be private. Files,
symlinks at the final root, nonprivate directories and physical replacements
fail closed. The operation never repairs permissions or deletes directories.
An entry created concurrently is qualified as current private state without
claiming that this operation created it.

`BootstrapOutcome` exposes a known count of successful creations and whether a
failed native creation may have left an entry. A subsequent error or cancellation
preserves those effects. Callers can inspect effects and failure before consuming
the optional installation observation. No implicit retry or rollback occurs.
Empty directories left by an interrupted preparation can be qualified by a new
invocation; this does not adopt authentication data or recover authority.

## Limits and observations

`BootstrapLimits::new(path_bytes, ancestors)` bounds each native pathname and
its ancestor depth. The independent start directory, requested root and planned
canonical paths are checked before any directory creation. Windows preflight
checks the encoded child name before growing its path. Directory privacy uses
the filesystem SDK's platform policy. There is no quota on created directories
or accumulated pathname accounting; `created_directories` reports confirmed
physical effects, including when later qualification fails.

Preparation and revalidation reuse the supplied cancellation, deadline when present
and request identity. Directory metadata does not consume content-read bytes.
`PrivateInstallation` retains the actual directory and private handle, has no
public reconstruction, cloning, debugging or serialization interface, and can
be revalidated only by the original request.
A new request prepares and qualifies the existing root again. Resolved paths
remain data and cannot reconstruct the observation or a permission grant.

The checks detect cooperative physical changes. They do not provide atomic
protection against a hostile filesystem racing native path operations.

## Validation and consumers

`cargo test --locked -p graphit-core --lib installation_bootstrap` exercises eleven
focused tests: existing and nested roots, SDK private birth, no auth effects,
request identity, exact and insufficient limits, cancellation/deadline,
uncertain native effects, concurrent creation, privacy and physical changes.
`cargo test --locked -p graphit-core --doc installation_bootstrap` checks four
compile-fail boundaries. Executed native evidence is Linux; Windows and macOS
runtime qualification remains required.

The authentication reader is documented in
[Private installation authentication document capture](private_auth_document.md).
Its publisher manages its separate fixed stage and lock. Future AuthStore and
runtime consumers must enforce their current permissions and exclude private
authentication names from agent filesystem views. Bootstrap supplies neither
login, OAuth verification, RoleParent nor those permission checks.

The separate [runtime key owner](runtime_key.md) consumes the installation
observation through a private original-request-checked bridge. It then retains
physical namespace handles and its own process lease for fresh legitimate
runtime operations. This does not alter `PrivateInstallation::revalidate` or
turn resolved paths into an owner proof.
