---
title: Rust native publication
type: reference
description: Captured native snapshots, credential boundaries and conditional remote publication.
status: current-layout-qualification-pending
updated: 2026-10-30
references:
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
  - target: {type: task, id: tsk-0688}
    relation: implements
  - target: {type: task, id: tsk-ed33}
    relation: requires
  - target: {type: task, id: tsk-5651}
    relation: relates_to
  - target: {type: task, id: tsk-7706}
    relation: uses
  - target: {type: task, id: tsk-7cdb}
    relation: uses
---

# Rust native publication

The [NativeRoot contract](remote_native_root.md) defines the bytes and pins.
The publisher and explicit remote installer have historical qualification in
tsk-ed33 through the compiled CLI, Broker STSv3 and real S3 fixture. The reader
delivery in tsk-5651 retains its historical Web/STS scope. Those receipts predate
the 2026-10-27 single-corpus layout and do not qualify its final consumers.
The preceding Linux dev Hub transport/layout qualification passed 89 focused unit tests and 15 local
publication integration tests, plus strict library Clippy. The earlier 38-test R2
preceded the AST/layout extension and is historical, without adding to that count.
The separate release run was interrupted and supplies no pass. Consumer-slot
changes passed 88 affected registry/publication cases and the nested-worktree
Skill/Agent journey described in [Native packages](../development/native-packages.md).
Codec and SDK results retain their narrower scope; installed consumers remain
a separate integration qualification.

## Entry and authority

The Hub publish-native CLI uses the existing daemon and legitimate Runtime
credential. It supplies explicit artifact type, full ID, concrete artifact
version, provider and metadata plus the actual PROJECT selection. Artifact
version is independent of the retained project version. Validate this input
before autostart, then maintain the selected checkout once and send the same
prepared payload. No second DaemonAuth, local key owner or filesystem grant is
created by the client.

The CLI operation is exposed as:

```text
graphit hub publish-native --kind knowledge --provider PROVIDER --project-dir PROJECT --id namespace/artifact --version release/1 --name Artifact
```

Use the global `--project-version` option for a retained PROJECT version.
`--version` in this subcommand identifies the artifact release. The CLI validates
metadata and resolves the project path before starting or maintaining the daemon,
then calls `graphit_hub_publish_native` through its existing authenticated MCP
transport. Publication returns a structured receipt containing the original
producer, artifact identity, descriptor digest and catalog effect.

The child router in source_server/native_publication.rs uses the existing
SourceServer auth, live PROJECT selections, native verifier cache, policy and
owned jobs. Reject browser Origin and ambiguous Runtime credentials. Capture
actual PROJECT Ast/Knowledge read admission and a current HubWrite service
selection, requiring the configured provider and the same Original. HubRead,
InventoryPublish, configuration permission, a JWT or metadata alone cannot grant
remote publication. The explicit provider must match the current active profile;
it is a checked service binding, not a request to pick credentials from an inactive
profile. Select or log in to the intended profile before invoking publication.

## Broker storage credentials

native_storage_credentials.rs is one small transport shared with the reader.
Source and Graph share one startup-captured HTTP connection pool with redirects
disabled and proxy discovery disabled. Both use the same captured
`broker.allow_loopback_http` configuration; a caller cannot override it in the
publication payload.

Its broker_credentials function accepts the existing ServiceCredentialLease,
canonical producer ProjectId, a reusable reqwest client without redirects and
the captured global allow_loopback_http policy. Original, purpose, endpoint,
resource, provider revision and credential come from the admitted lease. Accept
HubRead or HubWrite transport while keeping the caller's read/write authority
separate; never infer a new purpose or try a different token type after failure.

The current Broker route is POST /v1/s3/credentials. Send only
scope=project, the exact project_id and module=hub. Validate the returned scope,
producer, module, single prefix, positive authorization revision, nonempty issued
keys, actual expiry and explicit bucket/region/endpoint before constructing
ObjectStore's maintained AWS implementation. Use only issued temporary
credentials, without the ambient AWS chain. HTTPS is required except explicitly
configured loopback development HTTP. The response and secrets remain private;
status/cause diagnostics never include a secret response or bearer value.

The returned store and topology are data, not grants. Each caller retains its
lease and checks its current validity and the issued expiry before protected
I/O and delivery. STS expiry does not invent a replacement access-token TTL.
Read failures remain read failures; a reader's HubRead cannot publish.

## Captured files and snapshots

Portable AST artifacts use the frozen graph bundle. `PreparedRead` performs
actual PROJECT admission; CapturedLocalGraph supplies the generation
and CapturedSourceReader. Enumerate captured nodes and stream each file to EOF.
Explicit export builds this bundle from the admitted current live cohort,
including separate Graph and Source captures and one text/NN `corpus.lance`
snapshot with nullable vectors. The physical package stores its Lance bundles
under `graph.icebug/{source.lance,corpus.lance}`. Full indexing does not create an
automatic portable bundle; ordinary work updates the live FILE stores. Export
does not reparse, reinfer or open unrelated HEAD snapshots.

Remote NativeRoot instead uses distinct, nonoverlapping component roots:
`NativeRole::Graph` usually maps to `graph`, `Source` to `source.lance`, and
`EntitySearch` to its required path `corpus.lance`. Graph and Source names are
publisher choices recorded in the descriptor. The asset format permits genuine
graph-only authored assets, while the indexed AST producer requires Source and
EntitySearch. `EntitySearch` is the single text/nullable-NN corpus at one pin;
there is no separate `NativeRole::Search`, `search.lance` or `native_nn.lance`
component. Graph files remain inside the Graph root, while Source and corpus
objects belong to their separate roots. Remote readers consume those mappings
explicitly and preserve actual SDK manifests, versions and digests.

### Local progressive readers

The active PROJECT graph is persisted as immutable Icebug Parquet node, CSR and
FILE-fact shards under `graph/icebug.json`, independently of artifact export.
Each FILE publication writes only its changed shards and affected cross-FILE
resolution/adjacency shards, retaining unrelated Parquet bytes and stable row
ranges. An atomic manifest replacement publishes the structural revision.
Ladybug opens only an in-memory query database over those captured Parquet
ranges; local indexing does not create a persisted `.lbug` database or export
the whole graph after an edit. CLI/MCP,
HTTP/Browser and Live readers retain their own PROJECT admission. GraphServer
captures the graph generation, separate Source pin and single text/NN corpus pin under the
project's short publication fence, then releases it before model RPC or rerank.
Structural queries hold a read fence only around the native query. Parsing and
inference do not hold that fence.

`LiveGraph` stores FILE ownership, unresolved intentions, compiler facts, official
SCIP documents and optional pending compiler payloads in Parquet beside the
public graph. Its SQLite indexes are temporary disk lookup indexes, rebuilt lazily
from retained FILE shards when mutation first needs them; they are never authoritative storage.
Changed keys/symbols/Oracle lookups select the affected FILEs. A target's removal
retains other FILEs' intentions, so its return restores the corresponding edges.
The owner holds the same publication read guard through lazy query, export and
FILE payload reads. Collection of superseded shards runs under its exclusive
writer guard after the manifest effect, and therefore cannot unlink a retained
reader's files. Post-effect cancellation or cleanup failure reports the applied
structural revision; reopen uses that committed manifest. See the
[FILE graph contract](rust_ast_live_graph.md) and
[Icebug shard format](rust_icebug_storage.md).


Raw ingestion still has an initial barrier: it captures the selected filesystem
metadata, then streams each FILE's bytes into the Source dataset. A full initial
scan or rescan currently waits for that stream's EOF and confirmed Raw publication
before syntax extraction starts. The application retains the selection metadata
and active FILE payload, rather than all source bodies. This barrier is a remaining
limitation of ingestion; once a FILE is parsed and published, its structural and
lexical results do not wait for the remaining parses, compiler enrichment or NN.

FTS is available after each FILE's structural rows and code-tokenized index
segments are committed. Text, model-independent projections and nullable vectors
share the EntitySearch corpus. Projections remain durable without retaining
parser trees or waiting for model discovery. Background inference publishes real
confirmed vector batches into that same dataset; those batches are searchable
after a coherent capture. Missing vectors are a normal state and do not block
Cypher or FTS. A corpus generation alone never grants access.

On replacement/deletion, that FILE's old vectors are withdrawn from the corpus before exposing
its new structure and FTS. Intact FILEs retain their parser Raw version, UID,
source SHA and vectors. Source and rerank consumers check out the actual FILE's
historical Raw pin on the same branch; they do not relabel every row with a new
whole-project version. A result's UID/data_key, FILE, SHA, kind, language, grammar
and extent must agree. Model identity, revision, dimensions and recipe remain
strict. Inconsistent associations remain errors; partial coverage is not an
excuse to silently drop mismatched top-k results.

Pending projections and their actual prior corpus pin support recovery without
reparsing the corpus. A completed checkpoint is acknowledged only while the
FILE's source SHA, native structural revision and pending projection set still
match the processed capture. Compiler enrichment may add projections after a
parser batch completes; acknowledging that older batch must retain the new work.
A model change
uses the normal NN reset and stored model-independent projections; it does not
force a source parse. An inference-only failure is reported separately so the
watcher retains pending embedding work without scheduling the same parse again.
Temporary Arrow IPC working files are not durable recovery evidence.

The watcher resolves FILE events directly and directory events against only the
affected subtrees and stored path index. Directory deletion and rename also
remove the previous descendants. A normal edit does not expand to a repository
scan; explicit rescans and profile/configuration changes still select the
complete configured image. Cross-file SCIP and Oracle resolution updates the
affected references using stored facts, without reparsing their intact callers.
Source retains its separately captured observation. Lexical rows, pending FILE
projections and nullable NN state belong to the single EntitySearch dataset and
are captured under its one SDK pin. Existing index segments remain valid for
intact fragments; only newly written fragments need lexical or scalar indexing.

The supervisor continues receiving and coalescing watcher events while the
current producer runs. A new batch cancels optional compiler and embedding work
and yields syntax ingress between confirmed FILE publications. The current FILE
finishes its transaction; unstarted FILEs remain pending. Cleanup is joined before
admitting the queued batch, and its changed FILEs or indexed directory descendants
run before that pending tail. Producers never overlap. Authorization, lifetime
cancellation and local deadlines remain terminal checks before a cooperative yield.
Committed projections stay pending for the next embedding pass. Compiler-only
FILEs whose compiler was interrupted remain explicit source work; already parsed
FILEs need no reparse. A priority compiler-only FILE waits for its required compiler
family, then publishes before unrelated compiler families or remaining syntax work.
Knowledge runs independently of the AST Raw future in the existing joined batch;
an AST Raw preparation does not gate Knowledge publication. Knowledge and Memory
retain unfinished work across a yield. Project shutdown cancels both scopes and
joins their work. The initial full Raw preparation boundary described above remains.
Parser facts awaiting compiler enrichment are stored with their FILE observation;
they contain extracted rows, not trees or vectors. Resume lists only pending
metadata and loads a FILE payload after its compiler document exists, validating
the same Source SHA, historical Raw and native FILE revision. Without Docker, a
normal syntax delta does not read the pending payloads or hash the old corpus.
Every genuine batch can resume durable work with an empty parse selection.
Only an actual model selection change requests a model refresh; an empty parse
selection alone does not trigger discovery, loading or inference.

The Raw writer materializes an inherited branch before its first write. Readers
retain native branch identity without demanding that vector provenance match a
separately captured whole-Raw head. Corpus inheritance is immediately queryable;
no graph membership marker excludes intact inherited records.

Syntax publication runs independently of the compiler indexer. A slow compiler
does not hide parsed files. Completed compiler facts enrich the affected file
and preserve compatible saved vectors. Source, graph, FTS and background NN
advance independently; a failed component does not revoke another component's
committed data.

An explicit artifact export captures the graph, its file observations and SCIP
documents under a short graph writer guard, releasing it before Lance cloning,
packaging or upload. Source is assembled from actual historical per-file
observations with hash checks. The text/NN corpus is captured independently.
Actual native manifests and immutable artifact integrity replace `PublishedPins`
and clone-generation comparisons; no `source_bindings.json` is emitted. Existing
immutable artifacts and snapshots remain readable by their consumed columns.

See [local graph transactions](rust_ast_live_graph.md),
[local search associations](rust_ast_live_search.md) and the
[FILE checkpoint contract](rust_tree_sitter_native_nn.md#single-ast-corpus-and-progressive-publication).
Installed activation and functional qualification are recorded separately from
source implementation and producer-only tests.
A Broker-hosted Native artifact is an immutable, explicitly published release.
Updating a local PROJECT does not continuously mutate that release: a subsequent
export/publication captures a new artifact version. This artifact lifecycle is
separate from the immediate FILE updates observed by authorized PROJECT readers.

Historical evidence recorded before the 2026-10-27 layout revision: the
[Source semantic fixtures](../../crates/graphit-app/src/source_server/semantic/tests.rs)
publish real parser graphs and SDK Source/NN clones with their bindings, then use
the actual small model-service fixture for CodeDocument and CodeQuery. They cover
advancing live heads, inconsistent pairs, missing NN with the model endpoint
closed, model schema mismatch, stored/physical Source checks, admission and
cancellation, and Rust/JSON entity identities.

Knowledge captures one `index.lance` corpus and validates it through EOF under
one SDK pin. The initial FULL18 documentary schema evolves in that same dataset
only after a real model is admitted: nullable `input_fingerprint`/`vector_id`
FixedSizeBinary(32), `projection_recipe`/`role` Utf8, and a nullable
FixedSizeList<Float32> vector of the actual dimension. Documentary-only and
vector-bearing rows are normal states; there are no fake vectors or separate
semantic corpus. Model/projection identity remains in the dataset.

The producer deep-clones that one corpus once, joins its actual worker and
constructs exactly one `NativeRole::Wiki` component at `index.lance` from the
real SDK version, manifest path and manifest SHA256. `KnowledgePair`,
`NativeRootInput.knowledge_pair`, its getter and `knowledge-pair.json` are removed;
separate `embeddings.lance` is rejected. Export/import preserves the captured
documentary and nullable vector state without new inference or a model sidecar.

## Publication and failure

Use remote_storage release_prefix/registry_key and remote_metadata's strict
catalog. Upload immutable native objects first, native.json next and catalog last.
Hash only exact physical root manifest bytes and exact final descriptor bytes;
ETags and whole-corpus hashes are not substitutes. SDK ManifestLocation supplies
the real manifest path; do not reconstruct a filename from a version.

Large files stream into an owned temporary object using one S3 multipart part
at a time (at most 5 MiB, the protocol minimum for non-final parts), followed by the
SDK conditional multipart copy to the final immutable key. Small and final parts
allocate only the captured remaining bytes; the stream is still checked through
EOF before completion. The temporary key is
at `v1/projects/{producer}/artifacts/staging/{random}` below the issued prefix,
which is inside the existing Broker artifact scope. Only this operation attempts
its cleanup. A cleanup error returns the owned temporary key as a residue,
without deleting unrelated objects or attempting work with expired authority. An
unsupported conditional copy remains an error. Empty files use conditional
creation directly. This does not retain a file or corpus in memory.

Catalog creation uses PutMode::Create. Updates preserve the observed catalog and
use PutMode::Update with both observed ETag and version. Unsupported conditionals,
conflicts, denied requests, expired credentials and cancelled work remain causal.
A failed response retains the publication stage and the catalog effect. Once
the catalog PUT succeeds, its effect remains `published` even if the final
authority or capture recheck fails. A rejected conditional write remains `rejected`. Cancellation or a transport
failure during the catalog request remains `uncertain`; no replacement write is attempted.
Report actual partial output and uncertain catalog visibility; do not claim
cross-store atomicity or delete other published output to simulate rollback.
Previously retained readers continue to reference their immutable snapshots.

## Qualification boundaries

Product tests belong to the real implementation in tsk-ed33, not a standalone
mock framework: actual SDK output and two versions; components before descriptor
and catalog; real conditional conflict and partial failure; cancellation/worker
join; an old retained reader; wrong authority rejected before S3; and compiled
Hub CLI through actual Broker STS to the configured storage fixture. Mock network
I/O may cover bounded parsing or HTTP causes but cannot qualify IAM or publication.
Run only affected suites; reuse unchanged SDK qualifications.

The external executable journey has an explicit infrastructure selector:

```sh
GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN=/absolute/path/to/hub_storage_fixture \
GRAPHIT_TEST_MODEL_SERVER=/absolute/path/to/graphit-model-server \
  cargo xtask test graphit-app/native-hub-publication
```

Historical executable qualification recorded before the 2026-10-27 layout
revision; the following results were not repeated for the new corpus layout.
Build the Broker fixture according to its `rust-hub-storage-fixture.md` recipe
first. The selector invokes only the named ignored Source executable test with
`--ignored --exact`; invoking it without the fixture is a failed precondition.
Normal App tests do not launch MinIO. An ignored result does not qualify this
journey. The fixture uses its current signed access token and actual expiry,
public Broker storage routes, issued STS credentials and owned process shutdown.
The journey reuses physical Icebug/Raw/FTS SDK output and real PROJECT Copy
publication, then exercises the normal Main watcher and one shared CPU model
server for AST/Knowledge NN. It publishes distinct retained versions, mounts
each release through the native registry, and reads through CLI/MCP consumers.
It then disables embedding in the actual profile and queries the same published
NN release: semantic remains unavailable, hybrid retains its lexical hits, and
both preserve the observed semantic generation. Final objects and IAM grants
are unchanged; the SDK focals establish the omitted NN scan.
The fixture grants the monitored sibling inventory/presence only; that does not
grant Hub/STS/S3 publication for the sibling. This is a Native product journey,
not an OP login or Web session qualification.

Affected SDK tests cover immutable versions, literal slash/Unicode identities, real Create/Update conflicts and catalog effects after a final authority guard fails. The two native SDK snapshot tests preserve captured manifests and pins. The installed-reader focal allows a display-only metadata update while rejecting a changed source. Normal readiness errors retain the actual AST/Knowledge responses and daemon error log. No broader campaign is required for that test-only removal.

## Explicit remote installation

The required explicit remote installation caller is part of ed33. It preserves the existing Runtime/Broker Native installation proof and its Configure/ManageCache/PublishedFilesystem checks, independently of HubRead credentials. It reads the validated catalog, exact release NativeRoot and referenced SDK manifest hashes before registering RemoteNative metadata last. Publishing does not install implicitly. Installation display contains only validated name/description/tags; package imports lacking that metadata retain None. LocalKey remote readers remain separate; this installer does not invent a LocalKey installation grant.

The explicit global selector is `graphit hub install-native --provider <active-provider> --producer <project-id> --kind ast|knowledge --id <qualified-id> --version <concrete-version>`. It uses `graphit_hub_install_native` on the existing Source MCP owner and does not require a checkout or activate a project version. Native installation authority remains separate from the selected producer's HubRead/STS authorization. The normal HubRead renewal runs first; a fresh Runtime Native proof is then captured against the same verifier cache and Original, so a refreshed profile cannot stale the installation proof. Source and Graph reuse the startup HTTP pool with redirects disabled and no proxy, including the configured loopback development allowance.

The installer opens each declared Lance corpus at its exact SDK dataset version
using the same explicit admitted store session as remote readers. It checks the
returned manifest path/version, streams each physical manifest through SHA-256
to EOF, and checks the exact descriptor digest and tuple before its single
registry update. It neither creates a second semantic dataset nor loads a model
for installation. Graph, Source and EntitySearch payloads, or the sole Knowledge
Wiki corpus, stay remote. The record is data for later admitted readers, not
reusable read authority. Name, description and tags come from the validated
remote manifest; absent metadata in local packages remains absent. A failed or
cancelled registry write reports its observed effect rather than deleting a
potentially installed release or claiming rollback.
