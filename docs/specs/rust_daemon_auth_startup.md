---
title: Rust daemon authentication startup
doc_type: technical
references:
  - target: {type: task, id: tsk-9d34}
    relation: corrects
  - target: {type: task, id: tsk-bcf2}
    relation: uses_api
  - target: {type: task, id: tsk-5a7a0}
    relation: implements
  - target: {type: task, id: tsk-2436}
    relation: implements
  - target: {type: task, id: tsk-dd0f}
    relation: corrects
  - target: {type: task, id: tsk-b99b}
    relation: derived_from
---

# Daemon authentication startup

`graphit_daemon::auth_startup::DaemonAuth::start` creates one installation-owned
`RoleIssuer` for a service lifetime. Call `DaemonAuth::start(paths,
trusted_projects, policy, original)` with resolved `InstallationPaths`, an
explicit `&[ProjectPaths]` selected by the trusted service owner, the existing
typed policies in `StartupPolicy`, and the original startup `RequestContext`. The operator selects permissions and service rules before
accepting requests. Incoming request data never chooses that policy.

Startup requires the original finite deadline through Core
`SnapshotContext::required_deadline` before preparing the installation, acquiring
a lock or provisioning identity. A service context without a deadline returns
StartupError::Io(InvalidInput) with no retained resources or installation/key
creation. Cancellation and expiration preserve their original causal errors.
Identity provisioning derives its relative budget from that same absolute
deadline after checking the original again. Startup then prepares the private installation through
Core, and holds the native `FileLock` on root-level `daemon.pid` before creating
or rotating a runtime key. It provisions the persisted installation identity,
starts the actual `RuntimeKeyOwner`, captures the installation scope and Auth
store, and creates the issuer. Core alone creates and qualifies the private
`daemon/` credential directory. The persistent lock inode is neither removed nor
replaced. PID publication, listener discovery and the full daemon entrypoint are
separate consumers of this component.

The scope uses the persisted installation ID, its actual installation root,
and the roots of the explicitly maintained projects. Core validates and
canonicalizes these physical roots using the existing `ScopeLimits`. The Auth
store captures exactly the canonical `allowed_roots()` from the issuer scope;
both captures share the installation identity, scope binding and original
startup context. The installation root remains included for the installation
Auth document. An empty project slice is valid installation-only startup.
Duplicate roots or invalid physical captures preserve their SDK error and
post-key cleanup ownership.

Incoming headers, `project_dir` and tool payloads never add roots. Registration
only defines the realm: actual permissions, credential state, project identity,
version and original request checks still decide each Records admission and
managed run. Projects outside this explicit set remain denied. Maintaining or
changing the set as inventories change belongs to the service integration;
this API captures the startup set and provides no automatic refresh.

The Auth store and issuer share that installation identity. No auth profile or local key is
synthesized: runtime credentials can admit the explicit runtime permissions;
local credentials require a current active local profile. Authentication still
uses the actual Auth SDK, not the installation identity or a selector as a grant.

`issuer()`, `installation()` and `runtime()` borrow the service resources for real
consumers. Each incoming operation supplies its own original context and retains
its cancellation and deadline. The service owner stores neither the startup
request nor its snapshot, so finishing that request does not end the service or
its shared issuer. Neither this API nor its tests implement the Source MCP/CLI
route; that route must verify the actual caller before capturing or reading
project data.

## Shutdown and failed startup

Stop admissions and join requests and model consumers before calling
`remove_runtime(original)`. The method returns the unchanged Core
`RuntimeKeyMutationOutcome`; inspect effects and failure instead of interpreting
Drop as key removal. `maintain(original)` similarly forwards the actual SDK
outcome. Dropping `DaemonAuth` releases its native singleton guard but does not
claim credential removal or graceful worker completion.

`AuthStartupFailure::cause()` preserves the actual category and SDK error. Errors
before runtime creation own no runtime resources. A failed Core runtime start
retains its complete `RuntimeKeyStartOutcome` and singleton in
`StartupResources::RuntimeStart`, including opaque recovery and cleanup status.
Failure after key creation retains the actual `Arc<RuntimeKeyOwner>` and singleton
in `StartupResources::RuntimeOwner`. Use `into_parts()` to move the original typed `StartupError` and the same owned
resources together into a consumer that retains both cause and cleanup state. It
does not clone or reconstruct errors, handles or outcomes, and performs no cleanup.
`into_resources()` remains available for consumers that already handled the cause. Keep those resources while a cancelled or failed removal is retried;
release them only after inspecting the real outcome. No serialized grant or
plaintext credential is returned by this wrapper.

## Focused validation

`cargo test --locked --offline -p graphit-daemon --test auth_startup -j1` exercises
native temporary private installations and sibling projects, persisted identity, current runtime and
local credentials, independent roots, duplicate-start exclusion before key
rotation, fresh-request service use after startup cancellation and actual deadline expiry, cancelled and
expired startup, explicit removal/restart and failure cleanup ownership. A Core
runtime failure is forwarded intact; a post-key issuer failure keeps the key and
lock until actual SDK removal succeeds. Tests use real SDK stores and kernel
locks, with no mock grants or Source responses. It also exercises runtime and
local Task read/write admissions and real managed-run start/close for two
maintained sibling projects, denial of an unregistered project and insufficient
permissions, a foreign original request, cancellation of a retained admission,
cross-installation scope substitution, and explicit cleanup after a
duplicate-root capture failure.

The affected strict check is
`cargo clippy --locked --offline -p graphit-daemon --lib --test auth_startup --test source_discovery -j1 -- -D warnings`.
The ten native authentication tests and six unchanged Source discovery tests
pass on Linux. Strict Clippy for the daemon library and these two test targets
also passes; the qualification receipt preserves the commands and results.
This component's native execution qualification is Linux. It does not establish
full daemon/MCP/CLI integration, other operating systems, GPU execution or RSS
improvement.
