# Native AST and Knowledge packages

The Rust CLI transfers completed native payloads through a ZIP container. Export, inspect and import share the Hub native package codec.

    graphit package export --kind ast --source ./completed-ast --output ./ast.zip
    graphit package inspect --input ./ast.zip --kind ast
    graphit package import --input ./ast.zip --output ./installed-ast

Knowledge uses --kind knowledge and an index.lance directory. Export requires an explicit source directory; import requires a missing destination with an existing parent. Relative OS paths are accepted by the CLI. Envelope kind is authoritative; inspect/import may supply an expected kind.

Knowledge has one corpus, `index.lance/`, initially containing the documentary
FULL18 schema. After a real model is admitted, the same dataset can evolve with
nullable `input_fingerprint` and `vector_id` FixedSizeBinary(32), nullable
`projection_recipe` and `role` Utf8, and nullable `vector`
FixedSizeList<Float32> of the actual model dimension. Rows without vectors and
rows with vectors are normal states of that corpus; no fake vectors, separate
semantic dataset or compatibility fallback is introduced. Model and projection
identity live in the dataset. Separate `embeddings.lance/` and
`knowledge-pair.json` payloads are rejected; the `KnowledgePair` codec is removed.

For Hub publication, `native.json` declares exactly one Knowledge component:
role `wiki`, path `index.lance`, and a Lance snapshot with the actual SDK
`dataset_version`, `manifest_path` and `manifest_sha256`. SDK generations remain
independent of the artifact version label. Native producers and readers validate
the real corpus schema, documentary rows and any vector/model projection under
one retained pin. Physical package validation alone establishes neither these
SDK semantics nor PROJECT authority.

The authorized App Knowledge package producer reads the captured corpus to
validated EOF and freezes that one SDK pin with one deep clone. It joins the
started clone worker before publishing or returning an error. Packaging streams
the frozen `index.lance/` payload and performs no new inference. ReadKnowledge
and the separate PublishedFilesystem effect remain required under the same
incoming Original. Destinations inside the live corpus are rejected. A terminal
failure after successful archive publication retains the known publication and
captured snapshot observations.

The 2026-10-27 transport/layout revision passed its Linux dev qualification:
89 focused Hub unit tests and 15 local publication integration tests, plus strict
library Clippy. These checks cover physical transport, descriptor shape, scoped
Registry placement and real Core publication; they do not qualify Data SDK
schema evolution, remote S3 authorization or installed consumers. The separate
release run was interrupted and supplies no additional pass. The earlier 38-test R2 preceded the AST/layout
extension and is retained as historical evidence, not added to the current count.
Consumer-slot changes have a separate focused qualification: the nested-worktree
journey passed for Skill and Agent, with different artifact versions, update and
removal in A preserving B and its effective installation. The 88
affected registry/publication cases also passed, with only two corrected cases
rerun after the initial 86 passes. The process-death case verifies the current
Core recovery contract: preserve the abandoned unknown stage, publish from the
real source, and never adopt foreign stage contents into the destination.

The compiled Main command uses this authorized producer through the running
Source service:

    graphit knowledge export --project-dir ./project --output ./knowledge.zip
    graphit package import --kind knowledge --input ./knowledge.zip --output ./installed-knowledge

`knowledge export` defaults to `--format native`. For a navigable Markdown, OKF or Obsidian
directory from the same authorized PROJECT/version, use:

    graphit knowledge export --format markdown --project-dir ./project --output ./knowledge-markdown
    graphit knowledge export --format okf --project-dir ./project --output ./knowledge-okf

Each text response identifies the exact requested format, output, observed Source
generation, payload fingerprint, communities and actual cleanup result. Its
directory must be a fresh destination with an existing parent; an existing output
is preserved and reports `AlreadyExists`. Its
`index.md` links to `pages/p-<SHA256 of slug>.md`; each page retains the fifteen
native YAML fields and ends with its authored body after the Content heading.
Markdown and OKF preserve that body literally. Obsidian translates observed local
wikilinks to the exported files, preserving anchors and original implicit display
or explicit labels. The Source-only reader must reach validated EOF before publication.
OKF and Obsidian declare `okf_version: "0.2"` in the index and preserve native
metadata in each page's `graphit` extension. Their generated provenance uses the
installation brand and only observed timestamps; sources map to the published
Source key verbatim. This can be a managed `knowledge-source/...` origin rather
than a checkout path; export does not decode it for filesystem access. Obsidian
normalizes percent-encoded local targets against the observed canonical slugs;
unknown targets and links in code/HTML remain literal.
See the [text export contract](../architecture/rust-knowledge-markdown-export.md).

The same command can export an exact installed Knowledge artifact without a
PROJECT selection:

    graphit knowledge export --artifact-id vendor/knowledge --artifact-version release-1 --output ./knowledge-reexport.zip
    graphit knowledge export --format markdown --artifact-id vendor/knowledge --artifact-version release-1 --context-alias wiki --output ./installed-knowledge-markdown
    graphit knowledge export --format obsidian --artifact-id vendor/knowledge --artifact-version release-1 --output ./knowledge-vault

The artifact ID and version are paired selectors; `--context-alias` is optional.
They cannot be combined with `--project-dir` or an explicit `--project-version`.
The same caller needs installation ImportedKnowledgeRead admission and the
separate PublishedFilesystem effect. All three text profiles use the retained SDK
documentary projection from the single corpus pin and do not copy a dataset or
run inference. Native export freezes that one corpus and uses physical package
transport, retaining any nullable vector columns already present.
An unknown exact registry version returns `registry_absent`, without a PROJECT
fallback or output. Selectors and package metadata grant no access.

Historical text-export evidence recorded before the 2026-10-27 layout revision:
the additional compiled test
`compiled_okf_and_obsidian_export_project_and_installed_source` qualifies both
new profiles for PROJECT and the exact installed artifact. It checks byte-identical
trees, observed Source provenance and compilation digest against MCP, metadata 0.2,
and final bodies containing open code fences and HTML. Its bodylink extension
also checks plain, anchored/labelled and percent-encoded local wikilinks against
real files discovered through the exported index. OKF remains literal; Obsidian
translates only observed local links and preserves code/HTML. The App and Main
cases passed again for this delta in 3.59 s and 19.71 s respectively. The existing
publisher authorization, EOF and occupied-output evidence is preserved without replay.

`knowledge export` captures the selected PROJECT or installed artifact;
`package export` above takes an already completed payload directory. Physical import does not register the
result or grant access. A registered LocalLink can then be selected with MCP
`graphit_knowledge_search` using `source.kind = imported_knowledge` and the exact
artifact ID/version, under the installation's own read admission.

## Container

package.json is a regular UTF-8 JSON file with exactly these fields:

    {"format":"graphit-native-package","v":1,"kind":"ast"}

AST requires `graph.icebug/` and its regular `schema.cypher` file. The asset format
permits a legitimate graph-only authored package; Source and corpus bundles are
optional there. Productive indexed AST export requires both. Its separate
Source bundle is `graph.icebug/source.lance/`; its single text/NN corpus is
`graph.icebug/corpus.lance/`, with nullable vectors under one SDK pin. Each
present bundle must be an explicit directory containing a regular descendant
file. Separate `search.lance/` and `native_nn.lance/` bundles are rejected.
All AST package files belong to `graph.icebug/`; root-level sidecars are rejected.
Knowledge requires one nonempty `index.lance/` bundle. Explicit directory entries
preserve empty descendants. Import publishes the payload directories; the
envelope is container metadata and is not installed into the payload. Remote
NativeRoot uses separate, nonoverlapping component roots rather than nesting
Source or corpus objects under its Graph component.

Member names are raw UTF-8 POSIX paths with explicit parent directories. Dot/parent components, absolute/drive/UNC paths, backslashes, control characters, reserved or invalid portable components, duplicate names, case collisions, unknown roots and file/directory conflicts are rejected. Regular files and directories with ordinary modes are accepted; links, special files, encrypted archives and other compression methods are rejected. There is no historical envelope reader or format conversion.

Export stores payloads without recompressing native Parquet/Lance files. Read accepts Stored and Deflated and verifies every body to EOF with the ZIP SDK's CRC checks. Content moves in bounded chunks. Inspect performs the same full body validation without installing a tree.

## Configuration and physical publication

The physical export/inspect/import commands use the actual Hub Distribution configuration layer. Personal/project/environment layers are not loaded by this branch. The caller supplies one Cancellation shared by capture, streams and publication; its single signal handler cancels the request. The version command does not initialize package configuration or signal handling.

Inspect/import apply the configured copy limits to untrusted archive bytes, expanded regular-file bytes, tree nodes, relative-path size and depth; direct CLI export also keeps its caller-supplied source directory bounded. Authorized AST/Knowledge producers use generated capture for the completed or frozen payloads they produce under the request. That capture retains inventory, modes, identities and content digests without retaining file bodies, and streams to captured EOF in bounded chunks. The ZIP writer itself imposes no aggregate output-size quota, including envelope overhead. Path validation, collision checks, source revalidation, cancellation and atomic destination handling remain in force.

Export streams into Core's same-parent temporary file and finishes the ZIP before atomic replacement. An output inside the source is rejected. Inspect/import spool the captured archive to an owned seekable file because the Core captured stream does not implement Seek. Metadata memory scales with SDK entry count.

Import writes an owned tree in the destination parent, verifies all data and original capture/cancellation/deadline, then renames it exclusively into the missing destination. Existing destinations are preserved. Errors attempt cleanup of the owned staging tree. Unix bundle modes and Windows readonly semantics are retained; directory modes are applied after their children. The outer import container has private temporary-directory permissions, rather than an encoded source-container mode.

### Owned staging before publication

`graphit_hub::native_package::stage_import(source, expected_kind, destination, limits, context)` returns a `StagedImport` after the same ZIP metadata, CRC, byte-bound and original-source checks used by physical import. `path()` exposes the extracted payload and `kind()` its envelope kind; the destination is still absent. An integrator can open the native SDKs on this owned tree and perform its actual schema and authority checks before publication. SDK readers or captures tied to the temporary path must be released before moving the tree.

`publish(self)` borrows the original captured source and request retained by the staging object, revalidates them, then calls the existing same-filesystem `rename_exclusive`. A changed source, cancellation or deadline before that call rejects publication with the underlying cause. A destination created meanwhile is preserved and produces `AlreadyExists`. A successful return means the physical tree was published; no new fallible operation runs after the rename. The existing physical `import` composes `stage_import(...).publish()`.

Dropping an unpublished staging object, including after a failed `publish`, attempts to make only its recorded temporary members removable before the SDK's owned `TempDir` cleanup. Cleanup can fail; it does not remove a destination or unrelated trees. Successful publication disarms temporary cleanup. These pathname operations do not promise isolation against continuous hostile replacement. This seam grants no authority and certifies neither native dataset semantics nor registry publication.

## Received native publication snapshots

The App received-package adapter stages the current container with the same
physical codec before opening its actual native SDKs. AST captures the extracted
`graph.icebug/` and retains separate Graph, Source and single EntitySearch corpus
snapshots. Knowledge opens only `index.lance/` through the SDK using a file URI
derived from the owned staging path, and validates that corpus under one pin
through EOF. Any vector rows must agree with the corpus's real model identity,
projection and document input fingerprints. Publication runs no embedding
inference and does not manufacture missing vectors or model identity.

The Knowledge descriptor uses only the actual corpus pin and SDK manifest
observation; no pair sidecar or original/clone pair API remains. The SDK handles
and captured readers close before their owned extraction is removed; the caller
retains ownership of the uploaded file. Capturing and cloning the one dataset
preserves both documentary and nullable semantic state together.

Payload streams validate their captured file bindings and the original request
on every read, including EOF. The publisher revalidates the complete captured
inventory before the descriptor, before the catalog commit, and on delivery.
S3 operation guards retain authorization, scope, leases and credential validity;
they do not re-inventory the graph. If the source changes during payload upload,
publication rejects the changed snapshot before its descriptor/catalog and can
still remove its own staging objects while the original request and authority
remain valid. It never extends request deadlines to perform cleanup.

Native publication dependencies use `[{artifact_type, id, selector}]`. Omission
preserves existing catalog dependencies (or starts empty for a new artifact);
an explicit array replaces them, including `[]` to clear them. The existing
remote metadata codec validates the proposed release and declared dependencies
using the actual Native root digest before acquiring storage credentials or
writing project Hub metadata. `HubRegistry::project` keeps `hub.json` and
`.hub.json.lock` under installation `global/projects/<ProjectID>/`, through
`ProjectPaths::project_runtime(identity.id(), &[])`, rather than the checkout's
`.graphit` directory. ProjectID is checked against `ProjectIdentityStore` and
the parent is revalidated; the location change widens no Source authority.
Checkouts with the same ProjectID can read the same pending intent. Its recorded
destination remains the original checkout path; reading does not rebase it or
authorize execution there. Recovery and publication still require that destination
in the current captured scope's allowed roots and revalidate the original request,
scope and source before physical effects.
Physical PROJECT installations select the captured publication consumer before
artifact version, host target, pagination or compare-and-swap checks. The logical
artifact key remains adapter, type and ID. Separate checkouts, including nested
worktrees, keep distinct physical installations of that key; updating, removing,
forgetting or recovering one consumer does not select another consumer's entry.
Generated artifacts retain only the canonical package/source, effective installation
and provenance/hash records; publication staging is transient.
GlobalNative, cache and context registrations remain shared by artifact identity
and version and do not introduce a payload namespace or copy per checkout.
The private Hub registry uses format 4, with required `consumer` fields on each
installed-artifact group and logical intent. `PublicationSlot::ProjectRoot` uses
the scope's captured physical root identity; `PublicationSlot::Scope` keeps
shared metadata selection. No earlier registry reader or implicit consumer
default is provided. `entries()` covers the complete catalog; read views select
their consumer explicitly.
The publisher then validates the complete updated
catalog before writing immutable payload objects. Native release contents stay
immutable; Native publication selects the published release as latest and has
no FILE replacement or set_latest input. The final catalog write retains its
conditional update marker, and known or uncertain publication
effects remain distinct from successful response delivery. Preparing a package
provides no PROJECT read or HubWrite permission; its transport consumer must keep
those admissions separate and recheck the same original request.

## Authenticated global corpus installer

The App library `graphit_app::native_install::execute_install` composes native
installation with the process-owned `DaemonAuth`, installation paths, global
`HubRegistry`, captured Hub policy, resolved `LocalGraphPolicy` and the original
request. `InstallRequest` requires an absolute normal ZIP path, producer ProjectId
(ULID), full artifact ID and concrete version, with optional AST/Knowledge kind.
It does not select a host or infer a project. The daemon's
[Native installation ingress](../specs/native_installation_ingress.md) composes this
SDK on the existing Source listener. Its scoped Linux SDK/loopback tests qualify
this HTTP boundary. The compiled CLI below consumes the same SDK. Physical `package import` above does not authenticate or register
a corpus.

The installer derives its Runtime caller in the supplied issuer and Original,
reads the effective Native Broker endpoint/revision, and verifies the actual
active signed JWT. It requires only the configured Native installation permissions
Configure, ManageCache and PublishedFilesystem. Runtime alone, Local credentials
and ordinary Source/role permissions do not supply Native installation authority.
Existing ordinary permissions are preserved. Barriers recheck the same Runtime,
profile observation, expiry, installation and Original before publication,
registry mutation and terminal delivery; they perform no token refresh or repeated
JWT/network verification.

The owner supplies global Distribution/Global/Environment/Command configuration.
Environment capture must explicitly include `LocalGraphPolicy::CONFIG_KEY`
(`data.local_graph`) before calling its real `resolve`. The installer consumes
that resolved value and never reads environment, personal configuration or PROJECT.
Invalid captured limits are errors; absent limits use the Data technical defaults.

After ZIP structure/body/CRC checks, admission runs on the owned staging tree.
`CapturedLocalGraph` verifies the canonical Icebug manifest and declared Parquet
structure. It does not scan all rows, primary keys or reverse edges;
`schema.cypher` remains opaque metadata. Optional `graph.icebug/source.lance` is opened with
the canonical AST Source schema; absence is valid, while schema/store failures
reject publication. Knowledge requires `index.lance` opened with its documentary
or model-evolved nullable schema. AST text/NN data is transported together in
`graph.icebug/corpus.lance`; a physical roundtrip alone does not certify its row
semantics. The stage content and captured directory ancestry
are revalidated; readers/captures are released before exclusive rename. These
cooperative observations do not promise hostile filesystem isolation.

A successful publication is registered as exactly one GlobalNative, Native,
Original installation without host binding, alias or entry name. Producer/full ID,
physical ID and concrete version are retained. Registry changes hold the existing
writer lock and preserve other versions, artifacts and selected agents.

`InstallExecution` retains typed SDK causes locally; only its `InstallOutcome`
should cross transport boundaries. Public diagnostics contain fixed text and
never serialize credentials, tokens or Auth documents. Effects distinguish
payload NotPublished/Published and registry NotAttempted/Registered/NotRegistered/
Unknown. A failed save can follow disk replacement: an exact valid readback under
the retained lock and same authority/Original confirms registration or absence;
an unavailable/conflicting observation remains Unknown. Known effects survive
cancellation, authority refusal or delivery failure. If Auth returns its generic
Request refusal at a Native barrier, the installer retains that cause and an
immediate typed observation of the same Original. Cancellation/deadline categories
then describe the supplementary observation, including cancellation precedence
when both apply; they do not reconstruct the first detail erased by Auth. Other
authentication faults are not reclassified from a later Original state. Published payloads are never
removed, rolled back or automatically replayed. An existing/racing destination
is preserved and returns AlreadyExists.

Historical installer evidence recorded before the 2026-10-27 layout revision:
the recorded integration command was `cargo test --locked --offline -j1 -p graphit-app --test native_install`. Its canonical empty Icebug and empty Knowledge dataset
exercise real SDK admission; SourceLance contains actual stored text. This does
not qualify nonempty Parquet rows, production Broker JWTs or installed hosts.

The current installer module is selected with
`cargo xtask cargo test --locked --offline -p graphit-app --test integration -- native_install::`.

## Install and read an exact context

Use a completed native ZIP and an already active, valid Native Broker profile in
the selected installation. Installation does not log in, refresh a token or change
profiles, credentials, trust or operator configuration.

    graphit package install --input ./ast.zip --producer 01ARZ3NDEKTSV4RRFFQ69G5FAV --id vendor/context --version release-1
    graphit ast source file.any --context vendor/context@release-1

`--producer` is the canonical producer ProjectId ULID. `--id` is the full artifact
ID and `--version` is an exact concrete version, independent of PROJECT version.
Optional `--kind ast|knowledge` constrains the envelope; omission reads its kind.
Relative inputs are made absolute and normalized lexically; the SDK captures the
selected ZIP with link rejection. Install rejects `--output`, `--host`,
`--project-version` and unknown options with exit2 before startup. It installs a
global corpus without selecting a host. Host operations select the current
Kiro V3 binding explicitly as `kiro-v3`.

This branch reads Distribution, installation Global, explicitly captured
Environment and Command configuration, including `data.local_graph`. It returns
before PROJECT discovery/configuration. Physical export/inspect/import retain the
Distribution+Command branch above. `--global-dir`, the branded GLOBAL_DIR variable
or the home directory select the actual installation paths.

The client first reads real daemon discovery. A present record uses its loopback
listener, captured Runtime headers and bounded transport for
`POST /admin/packages/install`. Invalid discovery, key, authentication, connection,
HTTP or terminal response never starts an offline owner or retries the operation.
Only genuinely absent discovery starts an installation-only `DaemonAuth` under
the actual singleton. That owner has empty ordinary, administrative and service
rules and the three Native installation permissions above; it creates no PROJECT,
Source listener or discovery record. A singleton race fails without adopting a
key. Work is awaited before SDK cleanup removes the owned Runtime. Startup failure
transfers its original typed cause and cleanup resources together; failed cleanup
retains those resources for bounded cleanup retry, never operation replay.

Stdout reports one bounded terminal JSON line. Exit0 requires Published+Registered
and no failure; a terminal failure exits1, cancellation exits130 and invalid CLI
arguments exit2. A known offline Cancelled/Deadline outcome can still be reported
when the same Original has the corresponding typed observation: this reports
effects after the operation, without renewing its deadline or admission. Known
payload/registry effects survive output errors and cleanup failures. Newline counts
toward the output limit. A failed write can leave partial stdout; it is not a
complete terminal acknowledgement.

The client rejects duplicate/unknown/missing terminal fields, identity/destination
mismatches and inconsistent effects. An incomplete, invalid or unavailable remote
terminal retains the transport's NotSent/MayHaveApplied distinction. A valid
terminal already decoded when delivery is interrupted retains its observed
effects. HTTP200 alone is insufficient. After MayHaveApplied or a published
failure, inspect the exact registry/destination rather than automatically replaying
or deleting the published payload.

Historical executable evidence recorded before the 2026-10-27 layout revision;
these results were not repeated to qualify the new corpus layout. The recorded command was
`cargo test --locked --offline -j1 -p graphit-app --test native_install_executable`.
The current module is selected with
`cargo xtask cargo test --locked --offline -p graphit-app --test integration -- native_install_executable::`.
Its Linux fixture uses compiled daemon/CLI/MCP and signed fixture Native JWTs with
canonical empty Icebug/Knowledge and stored Source text. It starts the daemon
without an active profile and publishes Native through the actual AuthStore after
discovery, because Native pre-start inventory publication has a separate authority
dependency. This setup does not qualify Native-profile daemon startup, production
JWTs, nonempty Parquet or installed Windows/macOS/agent hosts. That recorded Linux
run passed all five executable journeys: active install with exact CLI/MCP reads,
offline admission and rejection, cancellation/deadline/stdout effects, early CLI
validation, and closed-listener/real-ingress terminal delivery failures. A separate
parser unit rejects ambiguous terminal identity/effects; the existing physical
package target passes four regressions. These results qualify the stated fixture
boundaries rather than additional hosts or corpus contents.

## Native producer boundary

These commands qualify physical transport, structure, bytes, modes and filesystem publication. The producer must finish/freeze its native generation before supplying the payload. Exporting a directory does not freeze a live Lance index or validate a complete AST/Knowledge corpus.

Data generation freezing, exhaustive native row semantics and mounted Hub/Broker/remote publication remain separate integrations; the App installer above supplies local structural/schema admission and authenticated global registration. A structural fixture roundtrip, including source.lance/ bytes, does not certify a valid native dataset, bind an imported Source snapshot, register its origin or authorize Source access. The public codec accepts the original CapturedSourceReader for those producer integrations without a second generation registry.

The ZIP SDK is pinned to `=9.0.1`. Its central metadata visitor is used to reject exact duplicate member names that `ZipArchive` otherwise deduplicates. Parsing remains in the SDK; an upgrade must preserve this rejection.

A completed PROJECT GraphCopy can be exported directly with
`native_package::export_ast_graph`. It borrows the genuine retained Core reader,
uses the same Original and prefixes archive paths with `graph.icebug/` while
opening the reader's physical relative paths. It does not recapture or move the
live graph, infer authority from the prefix, or query the producer PROJECT on the
importing side. The caller retains the read admission through final export checks.
