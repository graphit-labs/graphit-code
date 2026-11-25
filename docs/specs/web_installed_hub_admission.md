---
title: Web installed Hub read admission
description: Explicit Web authority for installed Agent, Skill and Rule readers.
content-type: reference
audience: developers
status: implemented
updated: 2026-11-07
references:
  - target: {type: task, id: tsk-0074}
    relation: supports
  - target: {type: task, id: tsk-d75f}
    relation: used_by
  - target: {type: task, id: tsk-e302}
    relation: implements
  - target: {type: task, id: tsk-0af2}
    relation: implements
  - target: {type: task, id: tsk-58db}
    relation: uses_api
  - target: {type: task, id: tsk-69f2}
    relation: supports
---

# Web installed Hub read admission

`IssuerAuthorityPolicy::with_web_installed_hub_read(provider)` explicitly
permits installed Hub reads for one server-configured Web provider. The default
is deny. Provider names must be nonempty, unique and within the existing issuer
scope-string limit. USER, PROJECT and imported AST/Knowledge permissions do not
imply this authority. Foreground enables it explicitly for configured Browser providers, independently
of whether Knowledge conversations are enabled.

`RoleIssuer::admit_web_installed_hub_read(caller, verifier, original, provider,
installation)` accepts the actual registered `AuthenticatedWebCaller`, its
verifier and Original request, and the Installation's logical `PublicationScope`.
The current Auth provider must match that caller's Broker binding and revision.
A profile or Native identity is not required. A valid JWT alone does not select
or authorize storage.

The opaque `InstalledHubReadAdmission` borrows those proofs. It checks the current
configured provider and Installation owner through the existing Auth producers.
`request()` and `installation()` expose borrowed data; consumers call
`recheck()` before I/O and delivery. Rechecks preserve the original deadline,
cancellation, signed expiry, installation identity and current provider state.
The factory grants no mutation, generic service, USER or PROJECT conversion.
Explicit HubRead transport capture below requires an additional service rule.
PROJECT access keeps its independent [authorization](auth_broker_project_records.md).
Web FILE mutations and remote publication require the separate
[installation mutation policy and proofs](web_hub_mutation_authority.md);
this read admission and its provider-level enablement grant neither.

`InstalledHubReadAdmission::check_project_source(&project)` joins a separately
admitted PROJECT Hub/read source with the installed-artifact reader. Both proofs
must retain the same issuer instance, Original snapshot, actual registered Web
caller instance, verifier instance and configured provider name. Knowledge,
Incoming, Runtime/Local, Role, another Web caller or another Original cannot
substitute for this PROJECT Hub proof. The method rechecks both existing
admissions; it performs no network request, creates no permission and converts
neither proof. Resolving a PROJECT descriptor requires this join along with
the actual source selection and content checks.

## Installed Native inventory

The Foreground Web consumer `POST /api/hub/installed/native` uses this admission
and the Source server's logical Installation scope to read its local global
registry. It checks the owner, InstallationId and registry root before reading
and before delivery. Its explicit `global_installation` scope lists only installed
AST, Knowledge and Language versions with `GlobalNative` targets. It exposes
public metadata, without opening payloads or storage transport. This operation
needs no PROJECT admission, active Native profile, embedding model or GLOBAL STS.
Reading an installed corpus still requires the separate AST/Knowledge authority.

The React route `/hub/installed` uses the current Web provider/subject and
installation endpoint for its request identity. A project, version or agent
selection does not change this global inventory. Logout, account changes and
request cancellation discard obsolete results. Details and filtering retain
actual artifact versions; storage errors and denied reads are not empty lists.
See the [typed HTTP contract](hub_consumer_contract.md#installed-native-web-inventory).
The consumer's HTTP and Browser qualification is tracked in tsk-0257; Auth
factory qualification below does not establish that consumer's completion.

## Explicit HubRead transport

`RoleIssuer::capture_installed_hub_service(&read, provider)` returns the
existing service selection with its purpose fixed to HubRead. It borrows this
installed admission and requires the same issuer instance. The Broker provider
must match the admitted Web provider, its current revision, endpoint and exact
optional resource. An explicit Web Broker HubRead rule must select
IncomingBearer without an incoming principal selector. Its credential lease
uses the verified Web caller and reports a Web user subject; no active Native
profile supplies identity or credentials. Generic Web embedding authority
remains restricted to its separate PROJECT embedding contract.

`capture_installed_hub_standalone_s3_service(&read, provider, profile)` selects
an explicit Web StandaloneS3 rule instead. Its target provider can differ from
the Broker provider that authenticated the installed reader: the target must be
configured Local without Broker, and its explicit optional profile must belong
to that target at the current provider revision. No active profile is inherited.
The existing `user_from_web` proof retains the actual caller identity separately
from transport/profile data. Even an explicitly unsigned transport has a Web
subject; that identity is not itself an S3 or ACL grant.

Both selections and their finite leases retain the installed read, Original,
verifier, Installation identity and current provider state. Checks before and
after waits, storage I/O and delivery reject changed provider/profile state,
an Installation owner mismatch, cancellation, deadline and signed expiry. They do not
add UserInfo, introspection, refresh, TTL or ambient credential fallback.

The GLOBAL registry is local to the admitted installation. A remote entry keeps
its actual producer ProjectId for existing PROJECT STSv3 authorization; this
bridge creates no GLOBAL STS request or synthetic project. A PROJECT source
still needs the separate `check_project_source` join before registry access,
I/O and delivery. See [Hub transports](hub_service_credentials.md#installed-web-reads).

The d75f Web remote catalog adapters use these captures for `POST /api/hub/remote/projects` and `POST /api/hub/remote/search`. The Web authenticating provider admits the reader; the query selects the catalog target. They call ordinary `ServiceSelection::credential`, retain the Web subject and recheck the read and lease through actual catalog/FileTransport I/O and delivery. No `identity_provider` input or active Native profile can replace that subject. The [HTTP contract](hub_consumer_contract.md#web-remote-catalog) records typed fields, finite errors and lifecycle.

The [FILE readonly consumers](hub_consumer_contract.md#web-file-readonly) retain
this installed read authority. PROJECT list/content requests first resolve the
explicit PROJECT ID and retained version against the live checkout selection.
An ambiguous selection returns HTTP409 `state_conflict` before registry or
payload reads. A selected PROJECT also requires its own Web Hub/read admission
and `check_project_source` join with the same caller, verifier and Original.
`PublicationScope` supplies logical ownership and configured paths; it grants no
access by itself. Current membership and authorization are checked before reads
and delivery. Configured paths use ordinary OS resolution; content fingerprints
and SDK snapshot checks retain their own integrity guarantees.

Remote content uses an owned temporary SDK stage. Installed content verifies the
existing cache without hydration. Reads do not install or mutate artifacts.
[Local Web FILE mutations](hub_consumer_contract.md#web-file-local-mutations)
require their separate mutation admission and report actual publication effects.
A known physical change survives a later metadata or authorization failure;
explicit reruns inspect current content and preserve foreign-file conflicts.
Request cancellation, deadline, logout and shutdown stop admissions and retain
ownership of started jobs until those jobs finish cleanup.

Producer tests use the real signed OAuth/JWKS and AuthStore fixtures in
`crates/graphit-auth/src/issuer/tests/installed_hub/services.rs`. Product
qualification and execution evidence belong to the consuming Tasks.

Authentication uses existing local JWT validation and discovery/JWKS caches,
including key rotation. UserInfo is optional profile enrichment; this admission
performs no UserInfo or introspection request. Revocation, account blocking and
remote claim changes may take until the access token expires to be observed.
The Broker default remains 10 minutes, configurable from 1 minute to 1 hour;
this API creates no TTL. PKCE, state, nonce, ID-token validation and token
renewal are unchanged.

The consumer must still resolve the full installed artifact tuple in the actual
Hub registry, retain the matching content reader, and validate its logical
selection and source fingerprint. Provenance fields are data, not a PROJECT grant.
A LocalLink outside the admitted installation needs its own real authority;
this API does not allow a fallback across that boundary. Knowledge and AST
read contexts use their own admissions and mounted readers, without copying
the corpus into the agent workspace.

This factory does not by itself qualify Live preparation, native Skill/Rule
discovery, MCP tools or Browser conversation execution. Those consumers belong
to the [Live delivery](../architecture/rust-knowledge-ai-live.md).

Source: `crates/graphit-auth/src/issuer/installed_hub.rs`, issuer policy in
`src/policy.rs`, and provider composition in
`crates/graphit-app/src/foreground_source.rs`. Focused tests reuse the existing
signed Web OAuth fixture and physical Auth store; implementation validation is
recorded in tsk-0af2.
