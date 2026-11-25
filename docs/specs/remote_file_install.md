---
title: "Remote file artifact installation"
description: "Ordinary ZIP staging, streaming retrieval and productive Hub FILE consumers."
status: implemented
updated: 2026-10-16
references:
  - target: {type: task, id: tsk-1dd5}
    relation: implements
  - target: {type: task, id: tsk-826e}
    relation: uses
  - target: {type: task, id: tsk-6873}
    relation: uses
  - target: {type: task, id: tsk-5651}
    relation: relates_to
---

# Remote file artifact installation

Ordinary file artifacts use the `payload.zip` contract in
[remote artifact storage](remote_artifact_storage.md). Ast and Knowledge retain
their native readers and installation path. The decoder and productive CLI/MCP consumers are connected in task `tsk-1dd5`.
The affected Main journeys and SDK checks passed; the evidence below separates
functional runs from static analysis.

## Owned extraction stage

`graphit-hub::file_payload::stage_file_payload` receives an owned-spool file,
an explicit temporary parent, existing `CopyLimits` and the original
`SnapshotContext`. The caller verifies the digest of all downloaded ZIP bytes
before extraction and retains its actual authorization. The returned `TempDir`
owns the extracted tree; neither a path nor a digest grants access.

The decoder uses the maintained ZIP SDK and shares the native-package path,
ordinary-mode, duplicate-header and checked-copy helpers. It accepts ordinary
relative files and directories without a native `package.json` envelope.
Traversal, reserved names, case/path collisions, links and special members,
encryption, unsupported methods, CRC errors and truncated input fail causally.
It checks entry/node/path/depth and decoded-byte limits for this archive, reads
file bodies through EOF and checks cancellation and the original deadline.
Payload bodies are copied through one buffer rather than retained as a corpus.

An exclusively owned temporary tree is created after metadata validation.
Directories remain writable within that private tree so an archive's read-only
directory modes cannot prevent cleanup. File modes are preserved on Unix.
Dropping the owner releases the stage; extraction publishes no registry or
managed files. A blocking extraction caller must join the worker before releasing
its stage or claiming cancellation has completed.

## Productive consumer boundary

The file consumer must resolve a typed publisher/type/full ID/version using
the existing metadata and version SDKs, authorize before protected data reads,
stream into its owned spool while calculating SHA-256, verify that digest and
publish through the existing filesystem coordinator. Installed-content reads
do not silently hydrate a missing artifact. Global/project versions, explicit
ephemeral use and actual partial publication outcomes remain required.

Broker reads reuse the qualified Hub access and STS endpoints. Standalone reads
retain their explicit configuration and global/anonymous/authenticated/user/team
grant behavior. Authenticated identity and groups must come from verified
OIDC/JWT proofs; stored profile strings are not authority. Advertised Broker
failure or denial never changes authority to standalone or public access.
The standalone storage/profile seam and Main consumers are qualified separately
from decoder fixtures in the evidence below.

## Integrated consumers

The Auth standalone lease and sealed access-token attributes have affected-suite
qualification in the `tsk-1dd5` receipts. The App consumer resolves authorization,
strict catalog identity and version before requesting `payload.zip`. Its current
implementation separates resolution from download so the Core-owned copy receipt
can verify a cache hit first. The selected release digest and configured RemoteFileOrigin participate in the
existing ArtifactKey and canonical physical cache path. The fixed-size key
fingerprints the same typed origin/producer/type/ID/version/digest identity; no
archive, grant cache or second receipt format is retained.
The network transfer holds no publication lock. A second receipt check precedes
publication so concurrent downloads cannot overwrite an already owned copy.

`SourceRef`/`SourceRecord` now retain `RemoteFileOrigin` for ordinary remote files:
Broker provider, or standalone provider and explicit optional credential profile,
paired with the archive SHA-256. FILE records require both origin and digest.
Their immutable physical directories include an origin fingerprint and archive
digest; logical ID, remote ID and concrete version retain their real values.
These references contain no credentials, user identity or project permission.
Local resources and native roots have no FILE origin by their own meaning.
Installed-content reads and updates keep that origin and revalidate its ACL;
a caller cannot replace a protected Broker source with a public provider.

Broker STSv3 always requires Bearer authentication. Anonymous Broker catalog
resolution is metadata-only and does not produce S3 credentials. Unsigned S3 is
available only through explicitly selected standalone transport with no credential
profile; that choice is not an error fallback. LocalS3/AwsChain retain the exact
configured provider/profile pair. A standalone request may select a separate
Native identity provider explicitly for verified user/team ACLs; this does not
change its S3 credential selection. Invalid selected identity fails the operation.

The Source handlers `graphit_hub_install`, `graphit_hub_update`,
`graphit_hub_content` and `graphit_hub_list` share the original request,
live authorization, current project capture and bound source provenance. The
Main CLI exposes them through `hub install`, `hub update`,
`hub content installed|remote` and `hub list`. Installation and update
require management authorization; content and list retain ordinary ReadHub
admission and the origin's current remote policy.

Agent/Skill instructions render with the maintained Markdown template engine,
then the native content renderer and derived-publication SDK. Physical Command
and Workflow instructions use the SDK's actual selected Markdown, including its
canonical-file/fallback rules, and the same template engine. PublicationWriter's
small read-only preview observes that selection without provisioning destinations.
Rule keeps its dynamic context backend and original template for its instruction
consumer; structured native artifacts retain their existing SDK path. Templates
receive brand, artifact identity/version, binding and dynamic MCP/skill names.
Names never add tool permissions; undefined values fail causally. Agent native
tool grants come from the selected SDK binding catalog and are filtered by its
readonly/shell capabilities; Codex/Cursor retain their native sandbox fields
without a fabricated tool allowlist. The rendered effective name comes from
the same SDK projection as the destination (alias, entry name or artifact ID
basename), independently of the authored descriptor name.

Update preserves an omitted installed alias/entry name and checks the previous
record again under the publication lock. It cannot replace the bound provider or
producer. A concurrent record change fails as a conflict. The affected Main
journeys cover real Broker/STS storage, a verified cache hit despite an overwritten
remote payload, project update, raw content, pagination, global versions and
explicit Standalone identity/storage. Standalone passed on Main29m and the full
consumer journey passed on Main29w.

Global FILE uses the existing versioned cache and Hub metadata with the explicit
GlobalCache target/backend. It rejects project selection, host binding, alias
and entry name, and performs no native projection or renderer call. Project
installation continues through ScopedCoordinator. GlobalCache does not imply
Live availability; content/update must reauthorize its bound origin.

## Content, list and explicit transport

Installed content inputs use the stored FILE origin, exact installed version and
archive digest. They read that immutable snapshot without resolving the current
catalog into a substitute source. A later publication of another digest under
the same version does not change an already installed snapshot. Update writes a
new immutable cache copy and CAS-publishes its reference at the same installed
destination/alias; old copies and opened readers are preserved.
An omitted global artifact version is accepted only when the ID has one version;
ambiguous IDs require an explicit version. Installed-content reads verify the
owned cache and do not download or hydrate missing local content. Explicit remote
content uses a request-owned extraction stage instead of installing implicitly.
The selection/read/template modules are registered in the productive handlers.
The Main29w journey qualifies these behaviors through the productive CLI/MCP boundary.
List validates an explicitly selected Native identity independently of returned
items, including empty pages and offsets past EOF. It captures each Standalone
origin/profile once for the original request, then revalidates every producer
on that page using the same transport. Broker STS remains project-specific. It
uses the current captured scope and returns metadata without downloading payloads.

The current Web Live catalog selects the global registry and filters text entries
through `InstallationTarget::includes(binding)`. PROJECT FILE installation writes
the project registry, and GlobalCache returns false for that predicate. The new
FILE producers therefore do not introduce remote FILE into Live. No new Auth
admission or network check is added to Live for that unreachable path. This does
not waive origin checks in the actual FILE content/update consumers.

AWS_CHAIN uses the mature AWS configuration/provider selection. The transport
resolves credentials once per selected origin for the original request,
passes them to the mature S3 builder and retains their actual expiry. It does not
create a global credential cache, invent a TTL, or build AWS runtime components
only to reach an internal cache. SDK profile region may supply an omitted S3
region; Local/Anonymous modes keep explicit configuration. Expired temporary
credentials fail instead of changing profile or refreshing authority implicitly.
The dedicated Standalone executable fixture uses real temporary, prefix-limited
Get/Put credentials; it does not borrow PROJECT STS permissions or root keys.

## Same-version content replacement

Publishing a new archive hash for an existing concrete version is supported.
Installation/update resolves the new digest into another immutable owned copy;
it never deletes the old copy to bypass ownership. Physical cache identity also
separates providers/transports/credential profiles, even for equal bytes. Source
restoration validates the origin/digest pair and its canonical path; mismatched
or obsolete FILE records fail rather than invoking a migration or fallback.
Current origin authorization remains mandatory for every installed read.
Standalone29m qualified distinct origin/profile copies and one AWS process
resolution per page origin. Main29w qualified same-version replacement, including
the old immutable cache, retained open projection and unchanged alias/identity.

## CLI examples

Select the provider and producer explicitly; install and update use the same
selection arguments. A project invocation captures its project/version before
dispatch, while `--global` selects cache-only metadata.

```sh
graphit --project-version main hub install --provider company --producer PROJECT_ID --kind skill --id team/reviewer --version release/one --binding codex --alias reviewer
graphit --project-version main hub update --provider company --producer PROJECT_ID --kind skill --id team/reviewer --version release/two --binding codex
graphit --project-version main hub content installed --kind skill --id team/reviewer --binding codex
graphit hub list --global --kind skill --offset 0 --limit 20
graphit hub content installed --global --kind skill --id team/reviewer --version release/two --path SKILL.md
graphit hub content remote --provider company --producer PROJECT_ID --kind skill --id team/reviewer --version release/two --path SKILL.md
```

Standalone selections additionally use `--transport standalone`, the explicit
storage provider and `--profile` when using LocalS3/AwsChain. An optional
`--identity-provider` selects the verified Native identity independently for
user/team grants. Omitting that proof cannot manufacture an authenticated user.

## App FILE platform composition

The App composes FILE install/update/content/list and local lifecycle consumers
on Linux, Windows and macOS through the same existing producers. Native requests
retain their captured service leases and administrative authority; Web requests
retain their own verified caller and source read admission. PROJECT mutations
require private administration together with current PROJECT/read; Global
mutations use separate private administration. Both preserve the physical
selection. A shared HTTP client and verifier owner reach Source before serving.
No Native or PROJECT grant is inferred from an operating system.

Browser FILE routes use the supplied Source service. Missing Source returns HTTP 503;
a real Unsupported backend error retains HTTP 501. Native AST/Knowledge and remote
catalog route gates are independent of this FILE composition.

Platform guards establish composition, not runtime qualification. Linux
functional evidence can be reused for unchanged kernels; Windows/macOS require
their own build or execution evidence before being reported as qualified.
