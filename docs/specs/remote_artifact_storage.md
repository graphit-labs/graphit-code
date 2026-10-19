---
title: "Remote artifact storage"
description: "Current keys, file payload integrity and publication boundary shared by the Rust Hub and Broker."
status: qualified
updated: 2026-10-17
tags: [hub, s3, artifact, integrity]
references:
  - target: {type: task, id: tsk-81ba}
    relation: implements
  - target: {type: task, id: tsk-4208}
    relation: implements
  - target: {type: task, id: tsk-6725}
    relation: derived_from
  - target: {type: task, id: tsk-97a7}
    relation: uses
  - target: {type: task, id: tsk-826e}
    relation: implements
  - target: {type: task, id: tsk-478c}
    relation: requires
  - target: {type: task, id: tsk-8cc0}
    relation: requires
  - target: {type: task, id: tsk-0257}
    relation: relates_to
  - target: {type: task, id: tsk-d75f}
    relation: relates_to
  - target: {type: task, id: tsk-b788}
    relation: relates_to
---

# Remote artifact storage

This is the Rust publication and retrieval contract. The strict metadata codec is implemented in `graphit-hub::remote_metadata`, and shared keys plus incremental SHA-256 are implemented in `graphit-hub::remote_storage`.

## Identity and keys

Keys are relative to the explicitly admitted provider prefix. Project IDs use the
canonical ULID representation. Types use `ArtifactType::as_str()`. Encode each full
UTF-8 artifact ID and exact `ConcreteVersion` with base64 URL-safe, without padding.
Do not normalize Unicode, case, slashes or versions, and do not use an alias or basename.

```text
v1/projects/<producer>/registry/<type>/<encoded-id>.json
v1/projects/<producer>/artifacts/<type>/<encoded-id>/<encoded-version>/<digest>/
```

The digest segment is 64 lowercase hexadecimal characters. An artifact's registry
document covers all its versions; each payload prefix identifies one immutable release.
Different producers, types, full IDs, versions and digests have different keys.
The publisher and reader use the same key functions. The Broker uses the same
`v1/projects/<producer>/` project boundary and `hub` module for registry and artifact
objects, including AST and Knowledge artifacts. The complete configured key must fit
the backend's actual key-length restriction; do not invent a resource quota.

The manifest retains the implemented strict version-1 shape: `producer`,
`artifact_type`, `id`, metadata, and `versions` with exact `version` and 64-character
`hash`. Compare document identities with the requested key before accepting metadata.
An unsupported format is rejected. There is no old envelope decoder, alias lookup,
legacy key probe or data migration.

## Implemented primitives

`remote_storage::registry_key` and `release_prefix` receive the existing typed producer,
artifact type, full ID, exact version and digest. They return `io::Result<String>` and
reject a relative result exceeding S3's 1024-byte key limit. The consumer must also
check its configured provider prefix and final object name when composing a complete key.
`hash_reader` receives a reader and the existing Core `SnapshotContext`. It hashes from
that reader's current position through EOF, retaining one 64-KiB buffer, checking the
original cancellation/deadline between reads and preserving causal I/O errors. Cooperative
cancellation cannot interrupt an already blocked synchronous `Read`.

The publisher must finish and rewind a ZIP before passing its file to this helper.
The remote consumer must hash all received bytes before admitting its stage. These
primitives do not open S3, decode ZIP files, grant access or publish a release by themselves.

## File payload and integrity

Artifacts needed as ordinary files use one `payload.zip` object under the release
prefix. This is an ordinary ZIP of the artifact's relative files and directories,
not an AST/Knowledge `native_package` envelope. Use the maintained ZIP library; retain
relative-path containment, duplicate/path collision, special-file and actual decoded
input checks. A malicious archive must not escape its own temporary stage.

Write the archive to an owned temporary file and finish the ZIP before hashing it.
The registry hash is SHA-256 over **all exact final ZIP bytes**, from byte zero through
EOF, including headers and the central directory. It is not a directory hash, ETag,
CRC, original file hash or hash of bytes observed before the ZIP writer finishes.
ZIP implementations can seek and rewrite headers, so hashing writes before `finish`
would compute the wrong digest. Byte-identical reconstruction is unnecessary: publish
and verify the same complete object. Use mature `zip`, `sha2` and `tempfile` libraries.

The reader streams the object into its own temporary file while hashing chunks. It
checks the exact digest and normal ZIP/path integrity before materializing files and
publishing them through the existing Hub filesystem coordinator. It retains the
current chunk and SDK state rather than all payload bytes in memory. Failure,
cancellation or deadline releases only that attempt's stage. If actual blocking work
still owns the stage, join it before cleanup; do not claim that dropping a future
aborted the worker.

## Publication and concurrency

Upload the complete release before writing the artifact registry document. Use the
SDK's conditional create/update with the observed object version or ETag where that
backend supports it. A conflicting registry update returns a conflict; never silently
overwrite another publisher's versions or invent a retry that replays authorization.
If conditional writes are unavailable, report that limitation instead of claiming
concurrent publication is safe.

Content-addressed release prefixes prevent a changed payload from modifying an open
reader's previous release. Reusing an exact version and digest is idempotent. Updating
a version to a different digest changes its registry selection only after the new
payload exists. An interrupted attempt may leave an unreferenced complete/partial
upload; report actual partial effects. Do not run a new background garbage collector
or delete existing mounted releases as part of this operation.

## Broker STSv3 storage capabilities

The strict project Hub credentials response requires `conditional_delete` as a
boolean and `copy_if_not_exists` as exactly `multipart` or `if_none_match`.
Missing, null, duplicate or incorrectly typed fields fail decoding; unknown
copy strategies are rejected. These values are declared by the selected Broker
storage route, independently of endpoint spelling, provider name and each other.
They do not add a permission or change the issued credential expiration.

The [Native credentials consumer](../../crates/graphit-app/src/native_storage_credentials.rs)
passes the copy strategy directly to the maintained object_store SDK:
`multipart` selects `S3CopyIfNotExists::Multipart`; `if_none_match` selects
`S3CopyIfNotExists::Header("if-none-match", "*")`. The strategy is consumed when
building the client, without retaining a second strategy in BrokerStorage.
Filesystem conditional CopyObject uses the latter; the S3 route emits multipart.
An immutable create conflict must preserve the existing destination bytes and
ETag, rather than silently overwrite it. The two routes need not advertise the
same conditional-delete capability.

## Native remote catalog retraction

On Linux, [`hub unpublish`](../guides/cli_reference.md#remote-hub-catalog-retraction)
and `graphit_hub_unpublish` select an explicit configured Broker provider,
producer, artifact type and full ID. Select either one literal version with an
optional expected current SHA-256 or the whole artifact explicitly. There is no
local PROJECT, binding or source-path selector. Current Runtime/Local HubWrite,
the Broker producer namespace grant, Original cancellation/deadline and issued
credential expiration remain enforced. Discovery, a valid JWT and installed
metadata do not substitute for that authority.

Removing one version while others remain re-encodes the observed manifest with
its other versions, metadata and dependencies and performs conditional PUT.
Latest is retained only if its version remains; removing latest clears the
pointer without promoting another version. Last-version and whole-artifact
withdrawal require DELETE with the catalog's observed ETag in If-Match. No empty
manifest, tombstone or unconditioned GET-then-DELETE fallback is written.

The [retraction kernel](../../crates/graphit-app/src/hub_unpublish.rs) checks
`conditional_delete` only in this DELETE path. False returns Unsupported with
`stage: catalog_delete` and `catalog: not_attempted` before looking up signing
credentials or sending DELETE. The prior authorized catalog read and the
partial-version CAS PUT path remain separate. A true capability uses the SDK's
`AwsAuthorizer::try_authorize` for SigV4 headers, including If-Match, on a URL
without query credentials. The existing client keeps redirects disabled and
uses the configured path-style endpoint. The request deadline is the earlier
of Original and STS expiry; no new credential TTL is introduced.

Success returns `status: unpublished` and a retraction receipt containing the
observed targeted versions, desired latest and catalog `updated` or `removed`.
Failures preserve `not_attempted`, `rejected`, `uncertain` or a known committed
effect. A late delivery failure can retain the operation under `outcome`.
Payloads, project metadata, source directories, caches and installed copies are
preserved. Receipts are not shortened to fit output limits: a bounded inspection
response, or an empty MCP error when even that cannot fit, requires inspecting
the original selection before retrying. There is no automatic mutation replay.

### Native qualification scope

The Root120 HTTP filesystem test failed in its setup client, which still forced
multipart copy. That test-only consumer now uses the required issued strategy.
Root121 failed from ENOSPC during App library compilation after 180.594s, without
executing tests. Root124 then compiled that library successfully in 173.991s.
Root128's real HTTP filesystem journey passes in 54.11s (runner wall 54.131s), and
the affected App library/tests strict check with `-D warnings` passes in 82.352s.
The earlier failures remain recorded rather than being counted as successful runs.

These outcomes close the stated Linux Native, filesystem HTTP and observed UI
paths. The earlier Root60x MinIO browser observation proves its recorded UI effects,
not conditional-delete atomicity. Root105/114/115 failure history remains retained.
Other platforms, full Server SQLite and the remaining migration scope are not
qualified by these results.

## Native mounts and authority

Lance and Icebug remain native S3 mounts. They do not become whole-corpus ZIP downloads
to satisfy the file hash contract. Their root manifests, native generation references
and release digest must be closed using the actual SDKs in task `tsk-478c`; the file
digest above does not define a native corpus checksum. No native release may be called
verified merely because its registry field contains 32 bytes.

Keys, hashes, metadata, source references and local cache membership are data. They
never grant access. Use explicit provider configuration and the caller's actual Hub
credential/policy admission, with the same original cancellation and deadline. JWT
validity is local until token expiry; PROJECT and current remote grants remain separate.
Anonymous access requires an explicitly admitted public provider/bucket. It neither
chooses a default provider nor manufactures an STS or user credential.

## Functional verification

The key producer must exercise two publishers, two types, equal basenames in different
full IDs, slash-containing versions and distinct Unicode values. The file consumer and
publisher must exercise a real ZIP round trip, changed/truncated bytes, path escape,
cancelled streaming, conditional registry conflict and registry visibility only after
payload upload. Their production entrypoints must consume the key/hash producer; a
test-only helper is insufficient. Native mount checks belong to the actual native
SDK consumer. No general benchmark or cross-language comparison is required.

Related contracts: [Hub S3 object layout](hub-s3-object-layout.md),
[Hub access control](hub_access_control.md),
[Remote artifact version selection](remote_artifact_version_selection.md).

## Ordinary FILE installation and content

The productive FILE consumer streams and verifies payload.zip before publishing through the existing coordinator. Core-owned receipts validate cache hits before another payload GET. The physical FILE cache and receipt key bind origin/profile plus archive digest, preserving separate immutable copies when a concrete version is republished. Installed content reads its recorded original digest, reauthorizes the bound origin and never hydrates missing content or substitutes a newly published digest before update. Explicit remote content owns a temporary extraction stage and creates no installed membership. GlobalCache records independent versions without native projection; PROJECT installs render maintained instruction templates. Current origin authorization is required for reads, updates and list. See [the FILE consumer contract](remote_file_install.md).

## Ordinary FILE publication through CLI and MCP

On Linux, `graphit hub publish` and `graphit_hub_publish_file` use the same
authenticated Source worker. The active provider must support
[HubWrite](hub_write_service_credentials.md); the selected producer PROJECT also
requires current ReadHub authority. Neither permission substitutes for the other.
Broker STSv3 grants the actual producer prefix; incoming paths, metadata or hashes
never grant storage access. Native and LocalBrokerKey follow their existing
credential paths, with no anonymous write or credential downgrade.

Select one maintained resource artifact by provider, FILE type, full ID and
concrete version. Types are skill, command, workflow, agent, rule, mcp, language and
power. `SourceRef::resources` derives the physical artifact directory inside the
selected PROJECT; clients cannot supply an arbitrary source path or payload URI.
The PROJECT version selects that project's context and remains separate from the
artifact release. Symlinks and source changes fail through the captured Core
reader. Capture, ZIP writing and transfer retain bounded blocks, not file bodies.

```text
graphit --project-version main hub publish --provider team --kind skill --id team/review --version release/one --name Review --set-latest --project-dir .
```

The publisher finishes an owned temporary ZIP, validates it with the ordinary
FILE consumer, rewinds and hashes every final byte, then streams that exact object
to its immutable digest prefix. It uploads `payload.zip` before conditional catalog
Create/Update. Existing versions remain intact. A preexisting payload can be reused
only when the current authorized manifest validates producer/type/full-ID and
the same version/hash; a digest match alone cannot adopt an orphan or foreign
object.

A changed hash at the same concrete version requires `--replace-sha256` with the
current lower-case SHA256, or `publication.expected_sha256` in MCP. An absent or
incorrect precondition conflicts. The catalog still uses its captured ETag/version
for CAS; a concurrent winner is preserved and no authorization replay is hidden
in a retry. An already-current version/hash remains idempotent after full identity
validation. Old digest objects and installed readers remain unchanged until an
explicit consumer update.

`--set-latest` / `publication.set_latest: true` explicitly makes the published
concrete version the latest pointer. Omission preserves an existing pointer; a
first publication may omit latest. CLI `--dependency KIND ID SELECTOR` declares
one typed dependency and can be repeated; `--clear-dependencies` explicitly clears
the list and conflicts with declarations. MCP retains `publication.dependencies`
objects with `artifact_type`, full `id` and literal `selector`. Omission preserves
existing declarations, an explicit empty array in MCP clears them, and a new
artifact without declarations has none.
Dependencies are never inferred from archive contents.

```json
{
  "publication": {
    "provider": "team",
    "artifact_type": "skill",
    "id": "team/review",
    "version": "release/one",
    "name": "Review",
    "set_latest": true,
    "dependencies": [{"artifact_type":"rule","id":"team/style","selector":"release/one"}]
  },
  "project_dir": "/project",
  "project_version": "main"
}
```

The NativePath is the already-monitored PROJECT selector, not a payload path;
use the exact Unicode path string supplied by the CLI. Native non-Unicode
paths and former OS/units objects are rejected, without a compatibility reader.

Success returns `status: published` and a receipt containing producer, type, ID,
version, `archive_sha256`, `payload` and `catalog`. Failures include finite
codes, the stage and known effects: `not_attempted`, `uncertain`,
`rejected` or `published` where applicable. A final permission/cancellation
failure after a successful catalog write reports that published effect. An upload
or lost response can leave partial residue; this operation does not claim rollback
or remove another attempt's objects. Original cancellation/deadline, current
admission, lease expiry and owner shutdown remain enforced by the shared jobs.

Install, update and inspect the release through the maintained
[FILE consumer](remote_file_install.md). Native AST/Knowledge keep their separate
[immutable publication contract](rust_native_publication.md).

## FILE publisher qualification (Linux)

The real CLI/MCP consumer passed in 42.73s using the actual Broker STSv3/S3 fixture:
CLI publish and install/content, denied foreign producer, MCP advertisement and
same-version conflict, explicit expected-hash replacement, update and global
remote content. The old original cache remained intact and the catalog retained
one version with its new digest. Six FILE unit tests, four affected Native
publication tests and the existing Native MCP stdio guard passed separately.
The receipt records coordinated App emission34f, production-source Clippy33ce,
scoped formatting/whitespace and their distinct runtime/build costs.

This qualifies publication to a known producer's artifact catalog. The fixture's
configured producer identity and grants do not prove a `project.json` metadata
writer or project discovery catalog. Those remain separate deliveries, as do
other platforms and migration-wide integration.

Subsequent shared project-metadata publication hooks have their own delivery and
qualification. They are not covered by this earlier FILE snapshot receipt; their
new effects and failures must be tested separately.

## PROJECT metadata production and discovery

The current Rust metadata key is
`v1/projects/{project_id}/hub/metadata/project.json`, below the existing
PROJECT-Hub Metadata root. `graphit-hub::project_metadata` owns its strict v1
codec and typed PROJECT identity. FILE and Native Source publication use the
captured local identity/name and the same HubWrite/PROJECT/STS Original to
conditionally create or update this document before publishing artifact metadata.
No older key, import or alias is read. Name changes retain ID. A different ID at
the key is invalid data, even when the caller can write that key. A failed later
artifact stage can leave the PROJECT document available; there is no rollback
claim or artifact-delete cascade to the PROJECT record.

Broker discovery uses current literal Hub principal/capability rules and their
server-selected `s3_route` (configured default when absent). Conflicting matching
routes for a PROJECT are rejected. The reader only enumerates PROJECT directories
and reads the canonical bounded JSON; it grants no S3 or PROJECT access. AWS uses
the existing route/provider cache and mature paginated S3 common-prefix listing;
filesystem uses confined nofollow directory/file operations. A page retains only
its metadata/results, not corpora. Exact IDs avoid a universe scan. Names and
payloads of unauthorized projects never enter a response/cursor.

`GET /v1/hub/catalog/projects` accepts optional `id` or `name_prefix`, `limit`
(1..1000, default 100), and `cursor`. Current authority is resolved per page.
Cursor fields bind the query, canonical subject, authorization revision and
server routes, with only the last returned ID as its position. They are not a
credential or snapshot and contain no backend continuation token. Restart after
`409 catalog_restart_required`; invalid query/cursor is 400, ambiguous routing is
403, expired/invalid identity is 401, malformed PROJECT metadata is 502 and storage
unavailability is 503. An absent document yields no available PROJECT, while an
invalid document never becomes a successful empty page. Functional qualification
of this producer is tracked separately from the prior FILE artifact receipt in
tsk-81ba; App/CLI/Web discovery consumers remain separate integrations.

Publication success and errors expose `project_metadata` independently of the
artifact receipt: `unchanged`, `published`, `uncertain`, `rejected`, or
`not_written` for that attempt. A confirmed metadata PUT remains `published`
even when a final guard or later artifact stage fails. Cancellation before an
unobserved PUT response remains `uncertain`; it is not reported as a rollback.

The server keeps one global fallback when composing credentials and catalog
routers. STS policy generation omits permissions already granted by a literal
ancestor path, preserving additional staging DELETE and sibling boundaries; it
keeps the concrete 2048-byte ceiling. Three affected policy tests pass (36e),
including the exact byte boundary. The receipt preserves the earlier
`policy_too_large` failure and distinguishes compilation from execution.

Standalone cursors are positions, not authority. Under a finite ID ACL, an
adulterated `after` outside that ACL is rejected before reading its metadata.
Name-prefix and all-project grants retain metadata discovery. The real CLI
regression uses a malformed foreign document to distinguish an unauthorized
read from the required `catalog_restart_required` and passes in 36g.

App and Source strict checks pass (36j, 34.41s); affected Broker strict passes
(36k, 6.49s). Broader application/CLI
journeys (tsk-0257), shared MCP/HTTP adapters (tsk-d75f), and React/browser journeys
(tsk-b788) remain separate open deliveries. The three functional passes do not
close those scopes or the whole migration; no CPU/NN replay or vendor account
was required.

### Available manifest search

The [current caller contract](hub_consumer_contract.md#available-artifact-manifests) defines `hub search` / `graphit_hub_search` over an explicit remote PROJECT. It reads only canonical current manifest documents through FileTransport and the existing codec. Each page uses current authorization; Broker STSv3 authorization_revision is retained for cursor binding without inventing a subject key. Projects discovery keeps its existing subject/revision binding.
