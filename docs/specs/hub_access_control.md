---
title: Hub Access Control
description: Deny-by-default project authorization, selective discovery, and cache rules for the S3-backed Hub.
status: draft
created: 2026-09-05
updated: 2026-09-10
tags: [hub, acl, authorization, discovery, s3, cache]
---

# Hub Access Control

The project ULID is the atomic authorization unit. Hub discovery and every data operation are
deny-by-default: a subject receives no project access unless an explicit global,
anonymous-or-authenticated, user, or team document grants it.

This contract is enforced twice: Graphit filters discovery and validates project intent, while an
S3-enabled Broker evaluates the trusted principal, project, operation, and logical key when it
derives a bounded STS session policy. The client receives only temporary credentials and topology;
bucket IAM policy and the session policy restrict every direct S3 request to the granted roots and
actions. A Broker without S3 omits the storage capability and Graphit uses local paths.

## Trusted subject

Authorization begins with a trusted identity:

```text
Subject {
  user_id
  team_ids[]
}
```

The authentication layer, workload identity, or trusted deployment adapter supplies both fields.
Request parameters, project configuration, `unit.id`, CORS, and a shared daemon bearer token are not
proof of user or team identity. A deployment that cannot establish a trusted subject cannot claim
multi-user selective authorization.

Transport authentication binds the subject directly to the request context. Otherwise Graphit
uses the username and teams from the atomically selected active account profile. Provider claim
mappings are accepted only after OIDC signature/issuer/audience/expiry validation; the canonical
identity remains `iss` + `sub`. If no active profile exists, Graphit uses `anonymous` with no team
memberships.

## Authority selection

Each provider selects one authority for the entire request:

- if broker discovery advertises `graphit-hub-access-v1`, the broker derives identity from the
  bearer, resolves current normalized SQL grants, and returns project selectors; Graphit never
  reads or falls back to the documents below;
- otherwise, Graphit uses the standalone grant documents below. Anonymous discovery then reads
  only global and `v2/anonymous/projects.json`, so absent documents grant nothing.

A malformed advertised protocol, broker failure, revision mismatch, or denial fails closed rather
than changing authority. There is no dual read, dual write, synchronization, or automatic
migration.

The anonymous identity is least-privileged and cannot carry team memberships. Its user memory is
always machine-local, even when the active provider's broker exposes storage; Graphit never reads or writes
`v2/users/anonymous/memory/`.

## Standalone grant documents

The resolver reads:

```text
v2/global/projects.json
v2/anonymous/projects.json      # anonymous subject only
v2/authenticated/projects.json  # authenticated subjects only
v2/users/<user-id>/projects.json
v2/teams/<team-id>/projects.json
```

For an authenticated subject in `n` teams, this is one global read, one authenticated read, one
user read, and `n` parallel team reads: `n + 3` object reads independent of the total number of
projects. Effective access is the union of all valid selectors. The anonymous subject never reads
`v2/authenticated/projects.json` or a user document; it reads only global and
`v2/anonymous/projects.json`.

A missing or empty `projects.json` contributes no access from that level. It does not cancel grants
from another level. If every applicable document is missing or empty, the subject has no access.
Malformed documents, unknown schema versions, identity failures, and authorization-backend errors
fail closed.

The canonical schema uses unambiguous selectors:

```json
{
  "v": 1,
  "projects": [
    {"id": "01J..."},
    {"name_prefix": "payments-"},
    {"all": true}
  ]
}
```

User-facing configuration may accept exact names, `payments-*`, or `*` as shorthand, but it must
normalize them to the structured form before evaluation. Version 1 supports exact ULIDs and
normalized-name prefixes. General suffix or infix globs require a separate search index and are not
silently implemented as full registry scans.

## Selective discovery

The global name directory exists for uniqueness and friendly lookup; it is not implicitly public.
After ACL resolution:

- an exact ULID performs a direct `GET v2/projects/<ULID>/project.json`;
- a name prefix lists only `v2/registry/names/<prefix>...`;
- `all` lists `v2/registry/names/`;
- every listing is bounded by `limit` and an opaque `cursor`;
- details and artifact entries are read only for selected, authorized projects.

An exact ULID grant survives rename. A prefix selector deliberately follows current active names and
can gain or lose matches after a rename. Every returned name record is checked against current
project metadata before a consequential operation.

## Enforcement

Authorization is required for listing, search, exact project or artifact lookup, content, install,
update, artifact access, events, submit, unpublish, and cleanup. Knowing an ULID, artifact
ID, version, or logical key never bypasses the check. Broker-issued sessions also constrain remote
Memory and Task prefixes to the principal's effective grants.

Read/discovery grants do not imply publication rights. Until a capability schema is introduced,
write and publish operations remain additionally constrained by the broker ACL plus its IAM role
and bucket policy.

## Cache

`~/.<brand>/hub` is a cache root, not a registry authority:

```text
~/.<brand>/hub/cache/<hub-fingerprint>/<subject-fingerprint>/
```

The Hub fingerprint uses broker/provider identity, never private bucket topology. The subject fingerprint prevents one
identity's filtered discovery from being shown to another. The cache is lazy, bounded by size/LRU,
and invalidated with TTL plus object ETag or revision where available. It may contain name
resolutions, authorized project metadata, artifact entries, and discovery pages.

Cached data never grants access. A consequential operation revalidates authorization against the
control plane; a successful conditional `Not Modified` response counts as validation, while a
network or authentication failure fails closed. Offline discovery may show explicitly stale cached
metadata, but it cannot authorize a new content read or materialization.

The cache is distinct from `~/.<brand>/artifacts/modules/`, which contains managed copies of
file-based artifacts required by an Agent. Revocation prevents future reads, updates, and downloads; it
cannot cryptographically recall bytes already materialized on a user's machine.

## Clean cutover

Hub v2 starts from explicitly registered projects, published artifacts, and grant documents. The
runtime does not read, copy, or infer state from another Hub layout. Existing content that should
remain available is published again through the v2 contract, and every grant is created explicitly;
no previous visibility becomes an implicit global `*` grant.

The former eager `~/.<brand>/hub/registry/` mirror and authoritative
`~/.<brand>/hub.registry.json` catalog are unsupported and never consulted.

## Related contracts

- [Project identity](project_identity.md)
- [Hub S3 object layout](hub-s3-object-layout.md)
- [Hub collaboration](hub_collaboration.md)
- [S3 STS storage and UI network configuration](../guides/s3-and-ui-network.md)
