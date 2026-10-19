---
title: Storage Layout
type: architecture
updated: 2026-10-30
tags: [architecture, storage, ast, knowledge, memory, hub]
references:
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
---

# Storage Layout

This reference describes local storage ownership, version isolation and maintenance.
Use Graphit consumers to read and modify records; a filesystem path, project ID or
cached artifact never grants authorization.

## Where the root is

`InstallationPaths::resolve` captures the global directory once at bootstrap.
The default is `~/.graphit`; a private brand uses its own brand directory. An
explicit `--global-dir` or brand-specific `GLOBAL_DIR` environment override is
resolved against the captured process start directory. The installation cannot
also be a project checkout. Missing home or invalid Unicode paths fail explicitly.

Hooks, MCP declarations and shared configuration use the executable from PATH and
project-relative inputs. Private installation records can retain runtime-resolved
absolute paths. Moving a checkout does not change its storage identity.

## Authoritative local data

```text
<global>/
├── config.json
├── auth.json
├── project-inventory.json
├── projects/<project-id>/
│   ├── ast/
│   │   ├── source.lance
│   │   ├── corpus.lance
│   │   └── live/<version-key>/
│   │       ├── graph/icebug.json         current FILE/shard publication
│   │       ├── graph/shards/             immutable Icebug Parquet node/fact/CSR files
│   │       └── ...                       text/NN publication state
│   ├── knowledge/index.lance
│   ├── memory/index.lance
│   ├── task/index.lance
│   ├── hub.json
│   ├── artifact-store.json
│   ├── cache/
│   └── logs/
├── users/<principal-key>/runtime/memory/index.lance
├── cache/scip/<family>/                shared compiler package caches
├── models/
├── tmp/                               private, active verified model snapshots
├── daemon/
├── logs/
└── broker/                             when running a local Broker
```

The immutable project ID comes from `.graphit/project.json`. `ProjectPaths`
retains the installation root; `project_runtime(project_id, parts)` resolves
only validated components below that project's global directory. It does not
hash a checkout path to create an identity. Different projects have different
namespaces even when their names or source files match.

The inventory retains identities, versions and checkout locations. These locations
select Source and Git context; they are not alternative authoritative stores.
The USER namespace derives from the verified issuer/subject pair and retains its
installation binding. PROJECT permission does not confer USER access.

### Versions and worktrees

Memory, Task, Knowledge and AST corpus datasets use the existing native Lance
branch selection. The configured default version uses the default Lance branch;
other versions use Core's collision-free `lance_branch_name` encoding. New
branches share the existing fragments until writes diverge. A readonly inherited
view does not materialize a branch or copy its rows.

For Task and PROJECT Memory, [Git record hooks](../specs/git_records.md) retain
immutable native tags for commits, restore commit state and merge record changes.
These references share fragments and protect snapshots used by Git history;
they must not be treated as disposable unreferenced internal versions.

AST graph publication remains under `ast/live/<version-key>`.
The graph directory persists Icebug Parquet and its manifest, with stable sparse
node ranges and FILE-sized CSR members. Ladybug reads those captured local or
remote ranges through the same in-memory query engine. It does not persist a
local `.lbug` file. Rebuildable lookup indexes use a temporary directory outside
the authoritative graph; closing the owner removes them. A FILE edit retains
unrelated node/CSR files and replaces the manifest atomically. Superseded shards
are collected only after retained graph readers release the publication guard.


Each PROJECT/corpus has exactly one authoritative dataset at its canonical
module path. Core `local_corpus::resolve` validates that fixed relative path;
it does not read a per-version directory map. Project versions select native
Lance branches in that same dataset. Source and the entity corpus are distinct
AST corpora because a complete source file is not duplicated into each entity.

Remote storage is selected before local path resolution. S3 operations open the
authorized remote dataset directly and do not create an authoritative local
copy. USER Memory keeps its separate verified-principal namespace.

Fragment compaction, index maintenance and obsolete-version cleanup use Lance's
native APIs, never filesystem globs over active dataset contents. Native branch
heads, retained Git tags and still-used snapshots constrain reclamation. Do not
remove fragments or index files merely because another branch no longer refers
to them. Maintenance does not re-embed unchanged records or create a parallel
search corpus.

Two checkouts with the same project identity and selected version read the same
local corpus. Read selectors may pair a retained checkout path with an explicit
version retained in another checkout of the same project, brand and global root.
The selected checkout still requires fresh authorization; a path or version is
not an access grant. Publication keeps its exact checkout/version selection.
Different versions remain isolated. Graph and file Source have
their own granularities and captured version/generation contracts; storing the
whole file in every search entity is unnecessary. The Ladybug/Icebug format is
independent of this directory organization.

Task's single dataset includes sessions, checks, comments, claims, history and
associated records. Memory's current rows, revision history, references and
nullable embeddings remain together. Moving these datasets must preserve their
records and revisions, not replace them with rebuilt databases.

## Text and vectors share a corpus

AST entities and Knowledge documents publish text before inference completes.
Their FTS and vector indexes belong to the same Lance dataset as the searchable
rows. There is no second corpus for pending work or an authoritative local mirror
of an inference result. Pending work is derived from current persisted rows.

A corpus can exist before a model has been admitted. The first actual model adds
nullable semantic fields and a `FixedSizeList<Float32>` vector of its real width
using native Lance schema evolution. This metadata operation does not rewrite
all text. An incompatible model change invalidates only incompatible semantic
state and follows normal background reindexing.

Each embedding carries its model identity, dimensions, recipe and input fingerprint.
Publication verifies the current entity/document identity and fingerprint before
attaching the result. Updating or deleting content cannot resurrect an old vector.
Unchanged compatible inputs retain their embeddings during incremental processing.
Semantic queries select only compatible, valid vectors; records without embeddings
remain covered by lexical search. Source per file and graph relationships can stay
separate because they do not duplicate the searchable corpus's purpose.

Index preparation uses the SDK's native FTS/vector APIs. Queries retain coverage of
new fragments even before an index maintenance pass has folded them in. Merely
opening a query does not rebuild an index or create a publication. A small corpus
may use the SDK's exact vector scan until a usable native index is available;
this does not suppress valid vectors or postpone textual availability.

## Remote storage

When authorized Broker discovery enables S3, Memory and Task obtain scoped
short-lived credentials and open Lance tables directly at the authorized remote
URI. The module, project or USER identity and selected version remain part of
resolution. The Broker authorizes access; it does not proxy record data.

Valid discovery explicitly disabling remote storage can select the local backend.
Authentication, STS or S3 failures are errors, not permission to write a local
replacement. Credential lifetime and SDK snapshot freshness are independent:
new operations observe committed heads while an already captured reader may
retain its snapshot. See [S3 and UI networking](../guides/s3-and-ui-network.md).

## What stays in the project

```text
<checkout>/
├── .graphit/
│   ├── project.json                   ID, root metadata/githooks and ordinary settings
│   ├── rules/                         maintained instruction overrides
│   └── ast/queries/                   maintained extraction overrides
├── .astignore
├── .wikiignore
└── agent-specific files               at their host's required destinations
```

Identity and Project configuration writers share `.graphit/.project.json.lock` and
preserve each other's sections during atomic updates. The lock is generated state,
not repository content. Identity creation needs no preexisting project ID
and creates no project runtime directory. Locks remain stable while holders or
contenders exist; removing a lock is not a contention recovery mechanism.

Shared `.gitignore` rules exclude generated state, temporary output and execution
evidence. The policy cannot rely on one engineer's global ignore or `.git/info/exclude`.
Files already tracked require explicit removal; ignoring them alone is insufficient.
Installed agent files may remain at their required project destinations and keep
user-authored modifications under the existing publication ownership rules.

## Growth and cleanup

Lance commits preserve reader snapshots and native branches. Compaction, index
optimization and removal of unreferenced files must use SDK maintenance APIs and
account for retained snapshots and branches. A request does not trigger corpus
reconstruction, forced allocator purges or a new auxiliary commit merely to read.
Physical SDK versions are distinct from the record revision/history stored in rows.

The installed Graphit package is the canonical source of bundled skills, agents
and templates. Their rendered files at the host's effective destinations (for
example `.agents/skills/`) are the installation, not a cache projection backed by
another full tree. `artifact-store.json` retains package origin, package version,
content fingerprint and the physical receipts needed for safe publication and
recovery. Updates and removals compare the current content with that fingerprint;
user modifications cause a conflict and are preserved. Byte-identical atomic
editor saves remain acceptable. Rendering uses transient staging only; there is
no `owned-payloads/`, retained render cache, pin inventory or payload GC.

The catalogue stores the logical owner only once in `scope`. Its own file inode
is observed while reading/writing, not embedded in the document. Persisted scope
directory bindings contain only path, physical identity and permissions. File
receipts keep identity, relative name, permissions, content length/hash and the
temporal fence needed by native readers; `verified` does not repeat identity,
kind, length or permissions. JSON identities use their native fields without a
per-node OS tag.

These physical identities have concrete consumers: checkout-root identity
selects one worktree's installation, and receipts detect replaced objects during
update, cleanup and recovery. They are independent of the content hash that
detects user edits. Hashes do not confer permission to an arbitrary path.

`hub.json` records artifact origin/version, target, alias, display data and the
backend necessary to find or update the installation. A source directory is
derived from its origin/root and artifact identity rather than stored twice.
A distinct physical artifact ID is recorded only for a real alias. Rendered
metadata retains the original selection, source policy and source fingerprint;
the output hash belongs to the Core receipt. Transient renderer context, output
staging selections and duplicated output fingerprints are not persisted.

External Hub packages retain their genuine source cache and upstream identity;
this is distinct from duplicating a generated installation. Generated artifacts
are copied to their effective destination. Explicit Link mode is rejected for
rendered artifacts; genuine local source links remain supported. Forget removes
ownership metadata and leaves effective files in place.

Physical agent and skill installations in different checkouts have explicit
publication slots based on the captured filesystem root identity. The logical
artifact key and the PROJECT catalog stay shared; updating or removing one
checkout's installation does not select the other checkout's installation, even
when their roots are nested. Hub selects the consumer before artifact version,
target or pagination. This selection does not grant filesystem access: the
original request, source and destination guards still apply. Permissions are
not part of the slot identity. An update, remove or recovery affects only the
selected physical publication. Native context imports, their cache
and remote S3 releases retain their existing shared scope and artifact versions.

Model weights are cached once per selected artifact and shared by projects.
An active native worker retains its verified GGUF snapshot under `<global>/tmp`,
on the installation filesystem rather than the operating system's temporary
directory, which may be RAM-backed. The snapshot is removed after the native
model is dropped, including idle unload; it is not a second permanent model
cache. CPU and accelerator selection do not change this lifetime.
Temporary test copies, completed release staging, old validation output, historical
exports and already-used recovery scripts have no role as permanent product data.
The maintained documentation sources belong at their normal source paths; recovery
metadata and duplicate recovered copies are not another Knowledge source.

This layout introduces no old-layout discovery, symlink redirection, dual readers
or permanent importer. An installation relocation is a coordinated, one-time
operation over current stores. Its validation must distinguish source changes,
installed behavior and actually measured allocated bytes; directory counts and
logical file sizes alone do not establish disk savings.

## Related contracts

- [Filesystem, state and watchers](../guides/filesystem_contract.md)
- [Project identity](../specs/project_identity.md)
- [Memory embeddings](../specs/native_memory_embeddings.md)
- [Native packages](../development/native-packages.md)
- [Rust Knowledge semantic data](rust-knowledge-semantic-data.md)
- [Native runtime packaging](../development/native-runtime-packaging.md)
