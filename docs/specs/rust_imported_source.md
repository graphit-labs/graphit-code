---
title: Rust imported Source
doc_type: technical
references:
  - target: {type: task, id: tsk-5651}
    relation: implements
  - target: {type: task, id: tsk-24b6}
    relation: implements
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: task, id: tsk-99c1}
    relation: implements
  - target: {type: task, id: tsk-9a88}
    relation: implements
  - target: {type: task, id: tsk-16f7}
    relation: implements
  - target: {type: task, id: tsk-e676}
    relation: implements
  - target: {type: task, id: tsk-77cb}
    relation: derived_from
---

# Rust imported Source

## Install through the compiled CLI

`graphit package install --input ./ast.zip --producer 01ARZ3NDEKTSV4RRFFQ69G5FAV --id vendor/context --version release-1`
uses the real Native installer to publish a global Original corpus and its exact
registry record. It consumes the existing daemon first; genuinely absent discovery
alone permits installation-only offline startup. Authentication/transport failures
do not fall through to another owner. The command does not select PROJECT or a host.
See [Native packages](../development/native-packages.md#install-and-read-an-exact-context)
for prerequisites, configuration, terminal effects, exits and partial delivery.

After a complete Published+Registered terminal, the same corpus is selected by
`graphit ast source file.any --context vendor/context@release-1` and the MCP tool
`graphit_ast_source` with `{ "path": "file.any", "context": "vendor/context@release-1" }`.
These reads retain ordinary global ReadAst authorization; Native installation
permissions do not grant Source access. A second exact version is independently
registered and read without overwriting the first. Installation in an active daemon
preserves its Runtime owner/key/discovery and other Source operations.

`tests/native_install_executable.rs` exercises this compiled Linux journey with
actual signed fixture Native JWTs, stored SourceLance text and canonical empty
Icebug/Knowledge. It activates Native only after daemon discovery through the
real AuthStore. Native-profile inventory startup, nonempty Parquet, production JWTs
and installed hosts/Windows/macOS remain separate qualifications. All five current
Linux executable journeys pass, including exact active CLI/MCP reads and preserved
independent versions. The terminal parser unit and four physical package regressions
also pass; these checks retain the fixture and platform limits above.

## Runtime selection and corpus reading

`graphit_ast::imported_source` reads raw Source from an installed native AST
artifact. `inspect_imported_source_runtime` and `inspect_imported_source_local`
authenticate with the actual `RoleIssuer`, then authorize global `ReadAst` before
loading the Hub registry or acquiring the corpus and again before delivery.
The caller composes the issuer and installation capture from the same DaemonAuth.

`ImportedSourceInput` supplies the global `HubRegistry`, full `ArtifactId`, exact
`ConcreteVersion` and installation capture, with no host binding. The registry
must belong to the captured installation owner. Selection requires
`InstallationTarget::GlobalNative`, Native backend and Original payload. The
registry rejects a host target or selection binding for a global corpus; Kiro
V2, V3 and other host selection cannot change which corpus is read. Local
Source still uses its captured native binding. A restored
`SourceRef` retains producer, logical and physical IDs, version and origin as
metadata; its pathname grants no permission.

The App separately dispatches an exact RemoteNative record to the
[remote NativeRoot reader](remote_native_root.md). Its current imported admission
precedes registry/storage access; an explicit HubRead lease supplies STSv3
credentials to a private SDK session. ReadRoot identity, exact dataset version and
physical manifest digests are checked independently of authorization. A remote
error never retries the local backend. Browser reads retain their verified Web
caller and dedicated imported HubRead permission, with no PROJECT grant or native
profile inferred from login. The finite transport Original also bounds credential
acquisition and remote reads.

Remote Source uses the same indexed-text decoder and selectors. Entity lookup uses the official CXX query engine directly on captured remote table ranges, then the Raw snapshot identified by the publication clone pin. Its query job remains owned through cancellation and shutdown.

The global registry stores independent concrete versions under each type and full
artifact ID. `HubDocument::get_version` selects the exact requested version;
`get` returns a record only when that ID has a single version. An ambiguous ID
never selects the latest version. Global `put_metadata` replaces only the same
concrete version. `remove_metadata_version` removes only that metadata record,
leaving its payload and other versions intact. Registry format 3 is strict;
earlier formats are rejected, with no import or conversion path. Project
registries retain one record per type/ID. Physical intents with no version in
their key are rejected for an ambiguous global ID before any publication starts.
GlobalNative never enters the physical publication journal, including Remove and
Forget; exact metadata removal uses `remove_metadata_version`.

The implementation captures the actual origin directory, its `graph.icebug` directory
and the optional `graph.icebug/source.lance` directory, opens the strict Source schema through `LanceModuleStore`,
and reads one retained `ModuleSnapshot`. Resources, LocalLink and HubCache origins
can supply indexed Source under global `ReadAst` without a project capture.
An available empty string is indexed Source and takes precedence over disk.
Only an absent corpus, absent file row or NULL text permits physical fallback.
Malformed datasets, incompatible schemas, duplicates, invalid paths, IO failures,
authentication failures and cancellation remain errors.

Fallback requires an explicit `ProjectCapture` independently resolved by Runtime.
Its producer ID, brand, record and version scope must match the captured project;
the caller establishes that the selected Runtime version matches the desired
origin. Artifact `ConcreteVersion` is never converted into a project version.
There is no cwd, workspace or Git inference. Without fallback, an explicit miss
returns `ImportedSourceError::SourceUnavailable`. The existing FILE reader captures
the requested file and `inspect_source` applies the existing text selectors,
including their UTF-8 validation, for every extension.

Runtime service requests may retain an original with no deadline. Origin and
fallback scope checks derive `ScopeContext` from that same original, preserving
its cancellation and any finite deadline. No missing deadline creates a grant
or changes signed credential expiry, registry checks or final authorization.

All operations retain the original request snapshot, origin capture, indexed
snapshot and optional physical reader until native revalidation and final
authorization. Reads create no directories, datasets, registry entries or caches.
Native revalidation brackets pathname SDK IO; it does not promise continuous
confinement against hostile filesystem mutation or pin external dataset GC.
Transporting a native package does not register it: a producer must register
the actual installation with `HubWriter::put_metadata` before this consumer can
select it. The App Source server supplies this input without a binding; HTTP,
MCP and CLI imported reads consume that same global registry. Package semantic
installation and its native authority are separate producers. SDK, loopback and
compiled Linux CLI tests do not qualify installed hosts or Native JWT login.

See [Source inspection](rust_ast_source_handler.md) and
[native packages](../development/native-packages.md).

All ten affected Linux imported-Source tests passed with the optional-deadline
extension, including actual runtime admission, retained indexed text and
subsequent service cancellation. Fixture identity provisioning retains its
already configured finite setup wait; it does not assign a deadline to the
service request. Strict Clippy passed. Windows/macOS are UNRUN for this
extension; attempts remain in `.build/ast-option-deadline/`.
