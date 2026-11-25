---
title: Remote native root and GlobalNative locator
type: reference
description: Exact native descriptor integrity and explicit remote registry dispatch for direct SDK mounts.
status: consumer-integration-pending
updated: 2026-10-27
references:
  - target: {type: task, id: tsk-b86e}
    relation: derived_from
  - target: {type: task, id: tsk-478c}
    relation: uses
  - target: {type: task, id: tsk-826e}
    relation: uses
  - target: {type: task, id: tsk-6873}
    relation: uses
  - target: {type: task, id: tsk-8cc0}
    relation: requires
  - target: {type: task, id: tsk-a5e7}
    relation: implements
  - target: {type: task, id: tsk-19cd}
    relation: implements
  - target: {type: task, id: tsk-70cf}
    relation: requires
  - target: {type: task, id: tsk-5651}
    relation: requires
  - target: {type: task, id: tsk-0688}
    relation: requires
  - target: {type: task, id: tsk-ed33}
    relation: implements
  - target: {type: task, id: tsk-265f}
    relation: requires
  - target: {type: task, id: tsk-f6ba}
    relation: requires
---

# Remote native root and GlobalNative locator

Native PROJECT/USER corpus and Hub registry/rules/metadata/event object roots
have a separate [storage-location contract](rust_native_storage_uri.md).
Its URI producer grants no access and does not replace the artifact release
descriptor, registry dispatch or remote SDK mount described here.

The NativeRoot codec, shared object keys and hashing, HubRead/HubWrite credential
selection and Broker STSv3 are implemented. The Code remote backend and callers are being integrated. Codec or SDK checks
alone do not qualify remote native publication or installed queries. Local
[imported Source](rust_imported_source.md) keeps its separate physical-directory backend.

The normative layout below is revised on 2026-10-27. Its preceding Linux dev qualification
passed 89 focused Hub unit tests and 15 local publication integration tests,
plus strict library Clippy. This covers the codec, physical package, Registry
isolation and Core publication interfaces. Data SDK schema evolution, remote S3
and installed-consumer qualification remain separate; the release run was interrupted
and supplies no pass. The later physical consumer-slot change passed 88 affected
registry/publication cases and the nested-worktree Skill/Agent journey, with
global Native registrations remaining shared. See [Native packages](../development/native-packages.md).
Earlier receipts below retain their historical scope and are not replay evidence
for the new layout.

## Root document

Publish one `native.json` object under the [immutable release prefix](remote_artifact_storage.md).
Its strict current-only JSON object has required `format: "graphit-native-root"`,
integer `v: 1`, canonical `producer`, `artifact_type`, full `id`, exact concrete
`version`, and `components`. This contract supports AST and Knowledge; Language
has no defined native root here. Reject unknown, duplicate, null and trailing fields.
Identity must equal the selected catalog producer/type/full ID/version.

Each component has a closed `role`, safe release-relative `path`, and one
`snapshot` object. The AST asset format requires `graph` and permits optional
`source` and `entity_search`; graph-only authored assets are legitimate current
content, not compatibility payloads. The indexed AST producer/export API requires
both Source and EntitySearch. `entity_search` is the single text/nullable-NN
corpus and has the fixed path `corpus.lance`; there is no separate `search` role.
Knowledge requires exactly one `wiki` component at the fixed path `index.lance`.
Roles cannot repeat or cross artifact types. Graph and Source paths are explicit
publisher choices, usually `graph` and `source.lance`; readers never guess them
from role names. A path contains nonempty slash-separated normal
components: no absolute/root path, URI, colon, backslash, NUL, dot/parent component,
empty segment or alternate decoding. Paths are exact object-key data, not NativePath.
Component roots cannot overlap.

Knowledge is one `index.lance` corpus: initial FULL18 documentary data and,
after real model admission, nullable `input_fingerprint`/`vector_id`
FixedSizeBinary(32), `projection_recipe`/`role` Utf8 and `vector`
FixedSizeList<Float32> of the actual model dimension in the same dataset.
Without-vector and with-vector rows are normal states. Identity and projection
metadata live in that dataset, not a sidecar. The single Wiki component records
its actual SDK pin and physical manifest. `KnowledgePair`, `knowledge_pair`,
its input/getter APIs and `knowledge-pair.json` are removed; separate
`embeddings.lance` is rejected. No fake vector, legacy decoder or fallback
converts split payloads. The root codec validates the descriptor; native readers
still validate the actual corpus contents and authority.

A Lance snapshot has `kind: "lance"`, positive integer `dataset_version`,
safe dataset-relative `manifest_path`, and `manifest_sha256` as 64 lowercase
hexadecimal characters. An Icebug snapshot has `kind: "icebug"`,
`manifest_path: "icebug.json"` and the same digest field. It has no duplicate
`generation` field; the strict codec rejects that retired field. Source/entity_search/wiki require Lance; graph requires Icebug. Optional components
are absent only when not declared. A declared missing object, denied read or invalid
schema is an error, never optional absence.

The catalog version hash is SHA-256 of **all exact final native.json bytes**.
A component digest covers **all exact root manifest bytes**. Do not hash a
reserialized JSON object, ETag, directory or entire corpus. The root contains no
own digest or absolute content-addressed release prefix, avoiding circular hashing.
SDK fragments, Parquet and CSR files remain direct native reads.

The implemented Hub codec API is `NativeRoot::decode(bytes, MetadataLimits,
&RequestContext) -> Result<NativeRoot, MetadataError>` and
`NativeRoot::encode(&NativeRootInput, MetadataLimits, &RequestContext)
-> Result<Vec<u8>, MetadataError>`, with borrowed typed identity accessors and
finite typed component/snapshot records. Reuse serde and existing policy/projection
types, without a second JSON parser, authority DTO or legacy conversion.

The public `NativeRootInput` borrows its component slice, with no knowledge-pair
field or getter. NativeComponent carries a closed
NativeRole, exact relative path and NativeSnapshot; the root exposes producer,
artifact_type, id, version and components through typed getters. Decode caps the
complete input bytes before Serde and validates the finite accepted shape's nodes,
scalar bytes and collection entries. Encode validates that shape first and checks
output bytes and the same cancellation/deadline between serializer writes. Parsing
is synchronous and checked before/after; this is cooperative cancellation, not
hard preemption of an allocation or an active parse.

## Pinning native data

The Data producer task `tsk-265f` supplies
`LanceModuleStore::open_native_snapshot(uri, Arc<Schema>, Arc<Session>, u64)`
returning `(Self, lance_table::io::commit::ManifestLocation)` in StoreError.
Use the actual SDK ManifestLocation path in the admitted ObjectStore. Its producers
preserve ObjectMeta.location or compute base_path plus the SDK naming scheme;
therefore the physical key can already include the dataset base. The descriptor
manifest_path is dataset-relative: strip the exact observed dataset prefix when
publishing and compare against dataset_base joined with manifest_path when reading.
Do not prepend the dataset base twice. Compare the actual version and hash the
physical manifest object through the admitted SDK store. Preserve SDK naming; do not reconstruct a filename from version or parse
Lance's physical file with a custom protobuf reader. The SDK
`with_serialized_manifest` input is not the physical manifest-file envelope.

Lance 14.0.2 uses ObjectStore 0.14.2. Construct AmazonS3Builder with explicit admitted
bucket/region/endpoint and actual credentials. Anonymous uses skip_signature only
for an explicitly admitted public bucket. A small private ObjectStoreProvider
validates the requested scheme/bucket and supplies the existing SDK ObjectStore;
install it into a private Session store registry. Do not use Lance's ambient AWS
provider or add a transport framework. Sessions retain a fixed topology and
credential generation; do not replace providers while older Dataset owners live.

A fresh operation admits current credentials and constructs its reader. Retained
old readers keep their Arc snapshot of immutable output; they do not acquire a
fresh grant from a hash or cached Dataset. ETags may help observe a physical object
generation; neither an ETag nor None proves dataset incarnation. Actual shallow
manifest base paths must lie within admitted topology and scope. Foreign roots
fail specifically, without default providers or widened grants.

The existing Data Icebug v3 parser validates format, finished/relation_uids,
nonempty generation and safe table/CSR paths. Remote range reading is a separate
producer from `tsk-f6ba`; Parquet 60's maintained AsyncFileReader can use ObjectStore 0.14.1 ranges
and SDK metadata, while Arrow 60 stays internal to that reader. The deprecated
ParquetObjectReader adapter is not required. The schema.cypher file is metadata.
This descriptor does not require an exhaustive corpus scan or semantic row digest.
Graph and Source remain separate components. EntitySearch is one `corpus.lance`
dataset containing text and nullable NN data under one observed SDK pin. It
records that corpus's actual manifest path, version and SHA256; no independent
NN Search component or second NN pin is declared. Remote component roots are
separate and nonoverlapping, unlike the physical ZIP layout beneath
`graph.icebug/{source.lance,corpus.lance}`. Readers open the declared exact corpus
version using maintained lexical/vector APIs, without resolving remote latest,
substituting string matching or inferring missing vectors.

## Installed remote readers

The App reader retains the actual imported AST or Knowledge admission, selected
GlobalNative registry tuple, HubRead service selection and credential lease through
delivery. The shared STSv3 transport supplies an explicit ObjectStore. Its bucket,
region, endpoint and prefix must match the selected record before reading data;
expiry and the same Original remain checked independently of immutable SDK pins.
No PROJECT scope is manufactured for a global import. A physical Source fallback
requires an explicitly selected, independently admitted PROJECT of the producer;
its project version is not inferred from the artifact's catalog version.

Source uses the same indexed-source decoder and line selectors for remote pins.
Entity Source resolves extents with the same official query engine, checks the
published Raw clone pin and reads that exact remote Raw snapshot. It never applies
artifact entity coordinates to unrelated workspace bytes. Missing optional Source
can permit the separate physical fallback; declared missing or corrupt remote data
is an error.

Remote Icebug exposes the same parsed schema metadata and captured table handles.
The CXX engine requests ranges from those handles, retaining captured object ETag
and version on SDK requests; it does not download a complete graph or construct a
second query evaluator. Exact manifest bytes are checked against `manifest_sha256`.
There is no duplicated descriptor generation, derived GraphGeneration or revision
comparison. The native Icebug parser still validates its own required format fields.

Remote Knowledge readers capture the single Wiki corpus at its declared SDK
version, validate its physical manifest and documentary schema through EOF,
and use the maintained decoder/preview renderer. The same pin contains any real
nullable vector columns, model identity, roles and input fingerprints. Semantic
results require matching model/projection data in that corpus; no second NN
dataset or pair sidecar supplies identity. Native export/import clones this
one corpus once and performs no new inference. The query consumer separately
requires a ready matching model for its query embedding. SDK pins do not replace
external authority checks, turn an S3 URI into a local capture or authorize
ambient providers. Current-layout installed query behavior requires its own
qualification; historical Source/NN-pair runs below do not establish it.

The existing query policy remains: an unavailable query model yields
`model_unavailable` for semantic search, while hybrid retains the lexical
contribution from the same validated corpus pin. It does not manufacture
vectors, switch corpora or run document inference to repair missing coverage.
Manifest/schema and documentary EOF checks remain required; actual vector
results require validation of their stored identities and fingerprints.

The App regression `remote_engine_queries_pinned_ranges_and_rejects_replacement_and_cancel` exercises the CXX/range adapter, Unicode relationship results, entity extent, readonly denial, captured-object replacement and cancellation. An InMemory storage fixture does not grant Broker or STS authority.

## Registry record and actual dispatch

The Hub now exposes `InstalledBackend::RemoteNative(RemoteNativeRecord)`.
The record stores only explicit `provider_name`, `bucket`, optional `endpoint`,
`region`, `prefix` and `root_digest`. Validate finite current-only serialization,
topology and key lengths. It carries no credential, token, subject, grant, expiry
or authorization revision. Its identity remains in SourceRef, not a duplicate URL.

`remote_native::RemoteNativeRecord::new(provider_name, bucket, region, endpoint,
prefix, root_digest, ProjectionLimits)` validates borrowed fields before copying.
Its getters return the admitted data only. Provider, bucket and region are required;
the optional endpoint uses the existing URL library and accepts explicit canonical
HTTP(S) origins without user information, query, fragment or object path. Prefixes
are empty or normal relative slash segments; literal Unicode and percent bytes
remain literal. The complete provider-prefix/release-prefix/native.json key must
fit S3's 1024-byte limit. Registry restoration uses private raw records under the
current HubLimits, with the existing strict JSON codec rejecting duplicate keys,
unknown fields, nulls and wrong types. Backend serialization has no independent
unchecked public Deserialize path.

Remote records require GlobalNative, global scope, AST or Knowledge,
Original payload with no selection binding, no alias/entry name and HubCache
SourceRef with physical_id equal to full ID. SourceRef::hub_cache identifies an
owned physical directory containing only the small native.json cache; it does not
identify a locally copied native corpus. Keep local Native records distinct.
GlobalNative has no invented Codex/Kiro binding.

The existing HubDocument::get_version(type, full_id, &ConcreteVersion) selects
the exact version. HubWriter::put_metadata remains the metadata writer under the
existing installation owner, lock and atomic replace. Restore/decode is data only.
A reader explicitly matches Native versus RemoteNative after current admission.
Local NativeOrigin cannot consume RemoteNative, and no s3 URI enters NativePath.
Do not silently fall back to a local corpus on remote errors.

App RemoteNative selection retains the authorized installation directory captured
under the same Original and uses `HubRegistry::load_captured` for initial reads
and rechecks. The registry keeps its configured logical parent and existing
SourceRef/GlobalCache metadata; it is not rebuilt from the canonical IO path.
Directory bounds come from the existing consumer policy, while the registry's
own document bound remains authoritative. A foreign physical root, changed
selection or interrupted Original fails before delivery with its real cause.

The registry qualification uses the real NativeRoot codec and byte hash to cache
native.json, then the actual HubWriter to register AST and Knowledge versions,
retain a distinct local Native version, restore exact versions and remove only
selected metadata. Invalid scope, type, host target, binding, derived payload, alias, source origin,
physical ID, topology and final key length leave the published registry intact.
Historical qualification recorded before the 2026-10-27 layout revision:
`remote_native::tests` passed 2 tests, `registry::tests`
passed 34 tests, and strict Hub library/tests Clippy passed with warnings denied.
Scoped rustfmt, diff checks and three local documentation links also passed.
These tests qualify the metadata backend; they do not mount SDK corpora or grant
remote access. Productive Native publication and explicit installation are qualified
in `tsk-ed33` by the SourceOnly24q and NN25s executable phases; the separate
Web/STS reader entrypoint is functionally qualified in `tsk-5651` below.

Current Auth `RoleIssuer::capture_hub_service(caller, provider_name)` and
ServiceSelection::provider_config require current checks; the caller retains that
selection, Original and lease around protected I/O and before delivery. Confirm
stored topology against the newly admitted explicit provider configuration rather
than letting a stale record redirect credentials. Installation capture/ReadAst or
ReadKnowledge precedes global registry load. Installation writes additionally
need the actual native installation authority; readonly permission is insufficient.
[HubRead credentials](hub_service_credentials.md) separate native JWT identity,
PROJECT where required, and outbound policy. Broker resolve and STS remain distinct: STSv3 is delivered by `tsk-80f3`/`tsk-7cdb`,
with real authorization and storage qualification in `tsk-b770`/`tsk-6488`.
Anonymous Resolve does not create STS credentials. Code callers must still use
the current lease and actual `/v1/s3/credentials` route.

## Publisher ordering and readers

Finish native output and choose immutable component snapshots through maintained
SDKs. Hash their exact root manifests, encode and hash native.json, then upload
the immutable release objects followed by native.json. Referenced shallow roots
must already be immutable and admitted; do not rewrite published manifests to
force all data beneath a new prefix. Publish catalog last using ObjectStore's
PutMode::Create or Update(UpdateVersion with both observed ETag/version).
Conditional capability and SDK errors remain observable; do not emulate concurrency
safety if the backend lacks it. Report conflict, partial upload and uncertain
catalog visibility without fake rollback or two-store atomicity.

Cache only the descriptor through the existing owned local staging/atomic file
mechanism. Registry publication follows actual native installation admission and
exact readback. Cache presence never grants read access. Failed attempts clean up
only their own temporary files; older registry entries and readers remain intact.

Source's current read_indexed_source accepts &ModuleSnapshot, a relative file
and &Arc<SnapshotContext>, enabling the strict raw Source reader after remote
pinning. It does not authenticate or admit the remote store. Knowledge's local
reader opens a branch through ProjectReadAdmission. The separate Data producer
tsk-70cf adds a selected snapshot read seam using the complete FULL18 decoder;
its current signature and three result states are described below. Do not
fabricate ProjectReadAdmission for a global import.
Installed Source/Knowledge caller integration has historical Linux entrypoint
qualification below. The SDK seam alone was not that proof, and those runs do
not qualify the 2026-10-27 layout.

## Required focal qualification

The codec/registry producer must exercise wrong identity/type/role, duplicate and
unknown/null fields, unsafe/overlapping paths, zero dataset version, invalid
manifest digest, retired graph-generation fields, optional absence, both local and remote GlobalNative
records, exact multiversion selection and rejecting host/project/derived mixtures.

The native caller must use real SDK manifests and two versions, compare exact
root bytes/path/version/digest, retain an old reader while a fresh admitted reader
selects changed output, and distinguish denied/missing/schema/digest errors.
Exercise cancellation and deadline with the same Original before/after protected
I/O and before delivery. Join owned blocking work before releasing its files or
credentials; future drop alone proves no hard preemption. Compatible sibling
readers must survive one owner's cancellation/drop; last-owner cleanup follows
actual SDK ownership, not a claimed RSS decrease.

Actual installed context must reach Source text and a Knowledge document through
production entrypoints. Mock object I/O can test protocol and cleanup, but cannot
qualify Broker authorization or a functioning installed query. No broad benchmark,
legacy parity or three-platform campaign is required for these focused producers.

## Returned implementation units

Historical qualification recorded before the 2026-10-27 layout revision:

| Outcome | Actual Task | Historical boundary |
|---|---|---|
| NativeRoot codec | tsk-a5e7 | Library qualified; productive Native installation qualified by ed33 |
| RemoteNative registry metadata | tsk-19cd | Library and exact-version consumer selection qualified |
| Knowledge selected snapshot reader | tsk-70cf | SDK reader qualified; installed Web caller qualified by CPU32bq |
| Installed native query caller | tsk-5651 | Linux CLI/MCP/HTTP composite proof over actual snapshots, Web and Broker STSv3 |
| Immutable native publisher and installer | tsk-ed33 | SourceOnly24q retained releases and NN25s remote/degraded consumers qualified |

The Data seam in tsk-70cf is
`read_knowledge_snapshot(&Arc<SnapshotContext>, &ModuleSnapshot, slug: &str,
KnowledgeIndexReadLimits) -> Result<KnowledgeSnapshotPage, KnowledgeIndexError>`.
The small data result distinguishes PublishedEmpty, PageAbsent and Found(Box<KnowledgeDocument>).
The caller retains the actual SDK snapshot/version; NoCorpus belongs to the opener.
Validate the complete FULL18 stream through EOF with the maintained decoder, retaining
only the selected document and bounded duplicate-slug tracking. Keep input bounds
per document and batch; do not charge an aggregate corpus body ledger for unretained
pages or return a full corpus Vec merely to render one page. Authorization stays
with the real caller. Data owns this API and its separate qualification.

HubRead is read-only. The publisher refinement must identify an actual outbound
write-purpose producer and caller; neither HubRead nor installation management
authority implies a Broker S3 write grant. Task tsk-8cc0 is a Broker refinement; its returned STSv3 producers are delivered.
Code callers consume their current route and actual HubWrite credential selection
from tsk-7706. Neither the refinement nor the codec alone qualifies the caller. Read each implementation unit's current checks in Task;
library qualification does not establish an installed remote query or publication.

## Codec qualification

Historical evidence recorded before the 2026-10-27 layout revision:

Task tsk-a5e7 implements only `graphit_hub::native_root`. Six focused tests passed:
typed AST/Knowledge and optional components, strict wire rejection, role/snapshot
version/digest validation, relative path containment and overlap, exact policy and
Original boundaries, and the shared SHA-256 of exact manifest/final descriptor
bytes. `cargo clippy -p graphit-hub --lib --tests --no-deps -- -D warnings` also
passed. The first deadline fixture incorrectly constructed an already expired
RequestContext; the preserved retry creates a valid context and lets it expire
before invoking the codec. No S3 mount, Broker route or installed caller is
qualified by these synchronous codec tests.

The removed Knowledge-pair extension historically added coverage for SourceOnly and captured NN,
original versus clone versions, optional Search consistency, strict pair fields
and concrete JSON limits. That reader validated actual Source/NN snapshots
and observed model identity; these historical codec tests do not qualify the
current single-corpus descriptor or remote queries. The 38-pass Linux Hub R2
metadata/package run preceded the AST/layout extension. The preceding transport/layout
results are the 89 focused unit and 15 local publication tests described above; they do
not qualify remote mounts or installed queries.

## Linux installed consumer qualification

Historical evidence recorded before the 2026-10-27 layout revision; these
consumer runs were not repeated to qualify the new single-corpus format:

The App `native_hub_install` source enables the same remote AST/Knowledge
installation and update composition on Linux, Windows and macOS. It retains the
existing Native installation authority, HubRead lease, Original checks, exact
descriptor and snapshot integrity, and registry writer/CAS. The module uses the
shared Auth, storage SDK, Data and Hub APIs without a platform-specific substitute.
Windows/macOS compilation and native execution remain separate platform
qualifications; the Linux evidence below retains its recorded scope.

The actual Broker Web journey is qualified by three unchanged Web29w cases and
one CPU32bq case, using DCR/LOCAL/PKCE/callback/session and real HubRead/STSv3/S3.
Web29w covers two distinct physical SDK releases, schema/sample/Cypher/typed
neighborhood, basic Source head reads, all-byte root and physical-manifest SHA
failures, current selection/provider/origin denial, cancellation, a healthy
sibling and retained owned shutdown. CPU32bq adds independent producer/consumer
installations and bundles, entity reads and all Source selectors, all four AST
search modes, Knowledge contexts/heads/
FULL18 pages for two artifact releases, real Source/NN Pair search in FTS,
semantic and hybrid modes, then model-unavailable semantic and hybrid results
that retain the same observed generations. Both CPU releases reuse captured
physical SDK pins; Web29w supplies the distinct-physical-version proof.
