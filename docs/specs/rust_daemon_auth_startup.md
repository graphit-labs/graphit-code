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

`DaemonAuth::start(paths, trusted_projects, policy, original)` creates the process
runtime owner, private Auth store and installation `RoleIssuer`. The service
owner selects `InstallationPaths`, trusted project paths and `StartupPolicy`
before accepting requests. Policy contains document/profile content limits,
authority rules and TTL policy.

`ForegroundSource` bounds serialized authority scopes and service bindings with
the captured `ContentInputBytes` policy. Permission counts and scalar fields use
the corresponding content collection and scalar limits. These Auth input bounds
are independent of filesystem snapshot limits.

Startup checks the original finite deadline and cancellation before filesystem
effects. It creates missing ordinary parents with `std::fs::create_dir_all` and
the credential root privately through the filesystem SDK. Configured paths may
be symlinks, junctions or select another volume. Credential access is checked
through the actual SDK; existing permissions remain as configured.

The root-level `daemon.pid` FileLock excludes concurrent service startups.
Installation identity provisioning uses the remaining original deadline.
`RuntimeKeyOwner::start(paths.root().to_owned(), original.snapshot())` creates
the real runtime secret and process owner lock. Auth uses
`AuthDocumentStore::new(root, document_limits, original.snapshot())` and the typed
`AuthStore`. A current active Local profile supplies any local credentials.

`PublicationScope::installation` contains the logical installation owner,
configured paths and selected trusted destinations. `RoleIssuer::new` borrows
that scope and uses the real runtime owner and AuthStore. Selected roots define
the realm; current permissions, credentials, project identity, version and the
original request decide authorization. Duplicate configured destinations are
deduplicated as data. Trusting a destination alone grants no permission.

`trusted_projects` is a slice of `ProjectPaths`: startup takes each root from that
producer, using the same platform path as `PublicationScope::project`. Direct
`RoleIssuer` consumers selecting initially trusted PROJECT roots must do the same.
Ordinary publication destinations may use other configured path spellings or name
missing output directories; those selections do not implicitly authorize a PROJECT
root with a different path. Dynamic project trust uses the authorized
`trust_project_root` operation with the actual PROJECT publication scope.

`issuer()`, `installation()` and `runtime()` borrow service resources.
`installation()` returns `&PublicationScope`; project contexts use
`ProjectSelection` and `ScopeInput`. `trust_project_root` takes an actual
`&PublicationScope` and delegates to Auth's authorized operation. Each incoming
operation supplies its own original context. The service lifetime is independent
of the completed startup request.

`maintain` and `remove_runtime` return
`Result<RuntimeKeyEffect, RuntimeKeyError>`. Stop admissions and join consumers
before explicit key removal. Dropping the service releases locks and owner
memory while leaving credential files available for explicit next startup.

A failed startup returns its typed cause. Rust drop releases its acquired locks
and local owners. An explicit next startup replaces the derived runtime key with
a fresh process secret. PID publication, listener discovery and process hosting
remain their own consumers.

## Focused verification

`cargo xtask cargo test -p graphit-daemon --test auth_startup --locked --offline`
checks runtime and current Local credentials, exclusion of a second live service,
new keys after explicit restart, original cancellation/deadline, startup failures
that release locks, trusted sibling projects and permission/realm denial. These
are real filesystem/kernel-lock and Auth journeys. Their execution evidence is
recorded in the coordinating task; native platform qualification is tracked
separately.
