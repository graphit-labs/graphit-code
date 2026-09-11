# Hub Collaboration Specification

The Hub is the broker-mediated object control and data plane used to share rules, skills, commands, agents,
MCP definitions, language queries, AST graphs, and knowledge contexts. Every project's durable
remote state is rooted at its immutable ULID. A lightweight global name directory provides friendly
discovery without making the mutable project name a storage address. No Git checkout or Hub
repository is involved in the persistence model.

## Backend and configuration

The active profile's named provider supplies the storage mode. Local providers may configure S3
directly; direct OIDC providers obtain renewable web-identity STS credentials when S3/STS is
configured; first-class Broker providers obtain a restricted STS session plus topology when
discovery advertises `graphit-s3-credentials-v2`; credentials and topology are resolved in memory
for each project or Hub metadata scope. Local, OIDC, and Broker providers without S3
remain filesystem-only. A configured STS exchange, advertised Broker capability, or renewal of an
existing Broker S3 grant fails closed on error.

AST publication also resolves `hub.icebug.reverse_edges` through the standard
inline → environment → project → global → default chain. Its default is `true`, and
only an explicit `false` disables it. The environment spelling is
`GRAPHIT_HUB_ICEBUG_REVERSE_EDGES`. Because `ConfigMap` nests only the first dotted
section, the project lockfile representation is:

```json
{"config":{"hub":{"icebug.reverse_edges":"false"}}}
```

The exact object prefixes, registry documents, publication ordering, and error
contract are defined in [Hub S3 Object Layout](hub-s3-object-layout.md). Operator
configuration and security guidance are in
[Authentication providers and account profiles](../guides/authentication.md).
The separation between immutable ULID and mutable globally unique name is defined in
[Project Identity](project_identity.md). Deny-by-default grants, trusted subjects, selective
discovery, and cache rules are defined in [Hub Access Control](hub_access_control.md).

## Registry and artifact operations

The global registry contains only `name -> project ULID` identity records. Project metadata,
artifact entries, payloads, events, memory, and Tasks live below `v2/projects/<ULID>/`.
Operations use object-store semantics:

- `Sync` resolves the caller's grants, reads only authorized project and artifact metadata, and
  reconciles installed artifacts against `graphit.lock.json`.
- `Submit` publishes a versioned payload first and writes its registry pointer
  last, so a visible entry does not name incomplete data.
- `Install` records the selected version, downloads file artifacts needed by Agents, and mounts
  native AST and Knowledge stores directly on S3.
- `Update` resolves newer registry versions and reapplies installation.
- `Uninstall` removes the current project's claim and deletes shared local data
  only when no project still references it.

S3 object writes replace the old commit/push/fetch workflow. Independent artifact versions use
disjoint prefixes. Publication ordering prevents consumers from observing a pointer before its
payload exists. Name creation and rename use conditional writes; two clients cannot successfully
reserve the same normalized name. Renaming changes only the name index and project metadata and
never moves the `v2/projects/<ULID>/` prefix.

## Where installed artifacts live

| Type | Local placement | Claim |
|---|---|---|
| `rule`, `skill`, `command`, `agent`, `mcp` | managed materialization below `~/.<brand>/artifacts/modules/`, then adapter/project files | `graphit.lock.json` |
| `knowledge` | direct S3 LanceDB mount for Hub; shallow-clone base plus local overlay for local projects | project lockfile and context registry |
| `ast` | direct S3 LanceDB FTS and Icebug Parquet mounts for Hub; local FTS overlay and local Icebug for local projects | project lockfile |
| `language` | managed global grammar/query materialization | project lockfile |

AST and knowledge artifacts publish versioned graph/search data that is read-only to consumers. A
publisher may add a version or replace the payload and content hash of an existing version. Native
engines read the selected S3 version on demand. File-based artifacts are written into the target
Agent/project and remain version-locked.

Local project graphs use the **same canonical icebug format** as Hub artifacts,
but with `storage='<abs>/graph.icebug'` (filesystem) and a `:memory:` catalog
rebuilt per connection from `graph.icebug/schema.cypher` – no `ladybugdb` file,
no WAL, no swap. Publish uploads the completed local bundle through the provider's S3 store. Hub
consumers open the versioned Icebug prefix through LadybugDB with temporary credentials; Parquets
are identical and their bodies do not traverse the Broker.

Every AST relationship is exported as two independent Icebug CSR tables by default:
`TYPE` contains exactly the directed graph, while `TYPE_REVERSE` contains the mirror
of every non-self-loop edge with the same properties. Keeping them separate lets
agent queries use the reverse adjacency for inbound or direction-agnostic traversal
without making a directed `-[:TYPE]->` pattern invent edges. Reverse rows are derived
and therefore do not increase the manifest's logical edge count.

Every Icebug Parquet file contains exactly one row group. This is a correctness
constraint, not a tuning default: the current reader can silently return incorrect
bound-endpoint results on large graphs when a file has multiple row groups. The
writer emits one Arrow record with one `Write` per file, and
`TestIcebugWritesOneRowGroupPerFile` protects that container contract. Consequently,
row-group pruning is intentionally unavailable; node-label filtering may scan the
corresponding canonical label table.

AST artifacts are published in the CANONICAL icebug layout: one node table
per label over its own columns and primary key, and one rel table per
(type, from, to) pair declared over the real endpoints —
`calls__function_function(FROM Function TO Function)` — plus optional
`<member>_reverse` mirrors for inbound and undirected reachability.
Self-loops live once, in the forward member CSR. The v3 `icebug.json`
manifest maps each logical TYPE to its member tables, records the
invariants (indptr single row group, self-loop policy), and travels
beside schema.cypher. It also records node-table row totals and per-language
histograms so interactive context/schema statistics do not scan the graph. The
installer stages it next to the mounted catalog so the backend adopts it at connect.

On a canonical catalog every multi-hop query belongs to this project's
planner, which resolves the logical TYPE against the manifest and runs
UNBOUNDED breadth-first frontiers — termination comes from visited
saturation and the caller's deadline, never a hop ceiling — expanding
only members whose both endpoint tables carry `uid`. It answers
`RETURN DISTINCT reached.prop [AS alias]` projections (materialized per
uid when more than the uid itself is projected, because batched
bound-node lookup is not reliable on large Parquet files) and
`count([DISTINCT] reached.uid)` over the reached set. Anything richer
fails CLOSED naming the plannable types. Bare single-hop patterns are
exactly-one-hop traversals through the same mechanism. `X.uid = 'lit'`
is rewritten to an IN list before anything else runs, because MEASURED
equality against an icebug-disk primary key answers zero rows.

The v3 `icebug.json` manifest and canonical per-label tables are required. A bundle without that
manifest, or with a different table shape, fails as an unsupported artifact format.
`graphit hub link --type ast|knowledge <path>` records a sibling-project pointer
instead of copying its compiled store. Reads resolve that sibling's global store
from the source project identity.

## Project lockfile

Every durable project receives `graphit.lock.json` when its first stateful operation needs an
identity; this may happen before full `graphit init`. Its `project.id` is an immutable ULID and its
`project.name` is mutable discovery metadata. `agents` lists adapters, `config` stores project-scoped
layered values, and `artifacts` records installed versions and origins. See
[Project Identity](project_identity.md).

Configuration values mirror dotted CLI names as one nested level and are strings:

```json
{
  "project": {"id": "01JM6B7T3B...", "name": "billing"},
  "agents": ["codex"],
  "config": {
    "ui": {
      "host": "127.0.0.1",
      "allowed_origins": "http://localhost:8080"
    }
  },
  "artifacts": {
    "language": {
      "elixir": {"version": "1.0.0", "origin": "hub"}
    }
  }
}
```

See [Configuration Module](config_module.md) for precedence and the full key list.

## Reconciliation

On `graphit sync`, the Hub:

1. authenticates the subject, resolves global/anonymous-or-authenticated/user/team grants, and obtains
   or refreshes the provider's S3 credential for the project or Hub-metadata scope being accessed;
2. verifies payloads and re-installs missing or changed files;
3. reinjects managed rule blocks into configured Agent targets;
4. maintains project claims in the global lock; and
5. refreshes team rule overrides from the logical `rules/` prefix through the active S3 store.

The team-wide `rules/<module>.md` and `rules/<module>_skill.md` objects form the Hub
layer of the rule hierarchy. Project rules win over global CLI rules, which win
over Hub rules, which win over compiled defaults.

## Collaboration channels

There is one knowledge distribution channel: `hub submit` publishes a named, versioned artifact and
`hub install` records the selected version. Hub Knowledge readers open its remote LanceDB URI
directly. Local project Knowledge can shallow-clone a compatible branch base from S3 and add a
filesystem overlay until the next commit/publication.

## Publication mutation and cleanup

The registry is mutable. A version may be a numeric release or an exact named channel such as
`branch/main`, `branch/feature/api`, or `tag/v2.0.0`. Publishing a new version appends it to the
artifact and advances resolution for unqualified installs. Republishing an existing numeric or
named version is also valid: its payload prefix is mirrored, stale objects within that exact prefix
are deleted after successful upload for immutable and tag snapshots, and the version's content hash
changes. Branch publication preserves Lance history while mirroring only non-Lance files. Concurrent
writes to the same entry or version remain last-writer-wins and should be serialized by the publisher.

A `branch/...` publication is a mutable, Git-addressed Lance lineage. The branch prefix remains
stable; every clean Git commit advances its tables, receives a native `git-<sha>` tag per table, and
is appended to the branch history manifest only after all tables and non-Lance files are durable.
Sync may shallow-clone the exact commit or nearest compatible ancestor into an empty local
filesystem store. Compatibility is semantic (artifact format plus embedding provider, model, and
dimensions); the Graphit producer version is retained only for audit. Project writes remain local
until the next explicit publication.

Outside Git, sync remains local and does not attempt branch hydration. A non-Git publisher may use a
new `branch/...` name as a mutable exact snapshot, but it has no commit manifest or ancestor reuse.
It cannot replace a channel that already contains Git-backed Lance history.

A `tag/...` publication is a compact release snapshot. The publisher operates on its temporary
staging copy, compacts every LanceDB table, prunes every superseded MVCC version, and verifies that
one current table version remains before upload. The source database is unchanged. Exact S3
mirroring then removes stale files from an earlier publication of that same tag without touching
any other branch or tag prefix.

A new numeric version is the safe production cutover because its registry pointer is written only
after the new prefix exists. A same-version replacement is useful for mutable branch/tag channels,
but a reader with a cached materialization must refresh it after publication and may not treat the
replacement as an atomic snapshot switch.

Lance maintenance does not garbage-collect Hub artifacts. It compacts local indexes only. Branch
commit tags protect historical versions used as shallow-clone bases; deleting or
pruning their manifests and fragments independently can orphan a local clone. Published Hub versions
are retained until explicitly retracted. See
[Publishing Graphit artifacts from GitHub Actions](../guides/github-actions-artifacts.md) for the
unattended named-channel workflow.

Memory and Task are mutable native LanceDB stores. With S3 configured they use authoritative remote
tables and LanceDB performs its internal reads and writes on demand with renewable credentials;
without S3 they use the global filesystem directory. See [Memory Module](memory_module.md) and
[Task Module](task_module.md).

## Security, discovery, and cache behavior

- A trusted subject supplies a user ID and team IDs. Without one, Graphit uses the reserved,
  teamless `anonymous` subject. Request parameters, CORS, `unit.id`, and a shared daemon bearer key
  are not user identity.
- Without broker Hub-access capability, effective project visibility for a non-anonymous subject is the union of
  `v2/global/projects.json`, `v2/authenticated/projects.json`, the user's projects file, and one
  `v2/teams/<team-id>/projects.json` file per team. Anonymous instead reads only global and
  `v2/anonymous/projects.json`.
  Missing files contribute no grant; invalid or unavailable access state fails closed.
- With `graphit-hub-access-v1`, broker SQL grants are authoritative instead. Graphit resolves
  selectors with the request bearer and never reads or falls back to those S3 grant documents.
- List and search are paginated and ACL-filtered. Exact ULIDs use direct reads and name-prefix
  selectors list only the matching portion of the global name directory.
- Authorization protects exact lookup, content, install, update, events, submit, and unpublish;
  knowing a logical key never bypasses it.
- `~/.<brand>/hub/cache/<hub>/<subject>/` is a bounded, lazy, disposable metadata cache. Cached data
  never grants access and consequential operations revalidate permission. It is not the former
  eager registry mirror and is separate from installed file-artifact materializations.
- S3-enabled Broker providers keep the temporary credentials and topology returned by the Broker
  only in process memory and isolate them by authenticated identity, provider revision, and project,
  user-memory, or Hub-metadata scope. The Broker retains its permanent identity and converts current grants into the STS session policy.
  A Broker provider whose valid discovery omits storage keeps its authenticated identity but uses
  filesystem paths.
  Local and OIDC providers use their explicitly configured S3 topology. Bucket/IAM policy remains
  the data-plane boundary.
- A registry entry whose payload is missing is a hard integrity error. A valid Broker discovery
  document without the optional storage capability selects local storage, so remote Hub operations
  are unavailable. A malformed advertised capability, discovery/authentication failure, or failed
  renewal of an existing S3 grant fails closed and does not trigger local fallback. None of these
  states is interpreted as a Git or old-layout fallback.
