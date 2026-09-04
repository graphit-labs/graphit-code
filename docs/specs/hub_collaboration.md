# Hub Collaboration Specification

The Hub is the S3-backed registry used to share rules, skills, commands, agents,
MCP definitions, language queries, AST graphs, and knowledge contexts. It also owns
the bucket namespace used by project and user memories. No Git checkout or Hub
repository is involved in the current persistence model.

## Backend and configuration

The location is resolved from `hub.bucket`, `hub.region`, `hub.endpoint`, and
`hub.prefix`. An empty bucket enables local-only behavior where supported. S3
authentication uses a complete `hub.access_key_id` / `hub.secret_access_key` pair
when one is configured; otherwise every consumer keeps using the AWS SDK provider
chain.

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
[S3 Credentials and UI Network Configuration](../guides/s3-and-ui-network.md).

## Registry and artifact operations

The registry is a set of JSON objects below `registry/`; artifact payloads are
stored below `artifacts/`. Operations use object-store semantics:

- `Sync` refreshes the registry catalog and reconciles the project's installed
  artifacts against `graphit.lock.json`.
- `Submit` publishes a versioned payload first and writes its registry pointer
  last, so a visible entry does not name incomplete data.
- `Install` downloads file-based artifacts or mounts the read-only remote stores
  used by AST and knowledge artifacts.
- `Update` resolves newer registry versions and reapplies installation.
- `Uninstall` removes the current project's claim and deletes shared local data
  only when no project still references it.

S3 object writes replace the old commit/push/fetch workflow. Independent artifact
versions use disjoint prefixes. Concurrent writes to the same registry entry are
last-writer-wins; publication ordering prevents consumers from observing a pointer
before its payload exists.

## Where installed artifacts live

| Type | Local placement | Claim |
|---|---|---|
| `rule`, `skill`, `command`, `agent`, `mcp` | IDE/project files | `graphit.lock.json` |
| `knowledge` | global knowledge context store, once per machine | project lockfile and context registry |
| `ast` | global Hub AST store, once per version | project lockfile |
| `language` | global grammar query directory | project lockfile |

AST and knowledge artifacts publish remote graph/search data that is read-only to consumers. A
publisher may add a version or replace the payload and content hash of an existing version. The
consumer creates only the local catalog needed to mount that data; it does not copy one full store
per project. File-based artifacts are written into the target IDE/project and remain version-locked.

Local project graphs use the **same canonical icebug format** as Hub artifacts,
but with `storage='<abs>/graph.icebug'` (filesystem) and a `:memory:` catalog
rebuilt per connection from `graph.icebug/schema.cypher` – no `ladybugdb` file,
no WAL, no swap. Publish to Hub is a filesystem-to-S3 copy plus `storage` URI
rewrite (`/abs` → `s3://bucket/prefix/graph.icebug`); Parquets are identical.

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
folded `Entity` file.

AST artifacts are published in the CANONICAL icebug layout: one node table
per label over its own columns and primary key, and one rel table per
(type, from, to) pair declared over the real endpoints —
`calls__function_function(FROM Function TO Function)` — plus optional
`<member>_reverse` mirrors for inbound and undirected reachability.
Self-loops live once, in the forward member CSR. The v2 `icebug.json`
manifest maps each logical TYPE to its member tables, records the
invariants (indptr single row group, self-loop policy), and travels
beside schema.cypher; the installer stages it next to the mounted
catalog so the backend adopts it at connect.

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

The folded layout (one Entity table plus a label column) remains readable
by the same backend when no v2 manifest is present, for bundles published
before this change.
`graphit hub link --type ast|knowledge <path>` records a sibling-project pointer
instead of copying its compiled store. Reads resolve that sibling's global store
from the source project identity.

## Project lockfile

Every initialized project carries `graphit.lock.json`. Its `project` section holds
identity, `ides` lists adapters, `config` stores project-scoped layered values, and
`artifacts` records installed versions and origins.

Configuration values mirror dotted CLI names as one nested level and are strings:

```json
{
  "project": {"id": "01JM6B7T3B...", "name": "billing"},
  "ides": ["codex"],
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

1. resolves the current S3 registry and installed versions;
2. verifies payloads and re-installs missing or changed files;
3. reinjects managed rule blocks into configured IDE targets;
4. maintains project claims in the global lock; and
5. refreshes team rule overrides from the bucket's `rules/` prefix.

The team-wide `rules/<module>.md` and `rules/<module>_skill.md` objects form the Hub
layer of the rule hierarchy. Project rules win over global CLI rules, which win
over Hub rules, which win over compiled defaults.

## Collaboration channels

There is one knowledge distribution channel: `hub submit` publishes a named, versioned artifact and
`hub install` records the selected version. Knowledge readers derive its read-only `s3://` LanceDB
URI and query it in place; there is no unversioned project-identity export/install channel.

## Publication mutation and cleanup

The registry is mutable. A version may be a numeric release or an exact named channel such as
`branch/main`, `branch/feature/api`, or `tag/v2.0.0`. Publishing a new version appends it to the
artifact and advances resolution for unqualified installs. Republishing an existing numeric or
named version is also valid: its payload prefix is mirrored, stale objects within that exact prefix
are deleted after successful upload, and the version's content hash changes. Concurrent writes to
the same entry or version remain last-writer-wins and should be serialized by the publisher.

A `tag/...` publication is a compact release snapshot. The publisher operates on its temporary
staging copy, compacts every LanceDB table, prunes every superseded MVCC version, and verifies that
one current table version remains before upload. The source database is unchanged. Exact S3
mirroring then removes stale files from an earlier publication of that same tag without touching
any other branch or tag prefix.

A new numeric version is the safe production cutover because its registry pointer is written only
after the new prefix exists. A same-version replacement is useful for mutable branch/tag channels,
but a reader that already mounted the known prefix must close and reopen it after publication and
may not treat the replacement as an atomic snapshot switch.

Lance maintenance does not garbage-collect Hub artifacts. It compacts local indexes, prunes only
superseded local Lance versions after retention, and skips remote mounts. Published Hub versions are
retained until explicitly retracted; deleting their S3 prefixes independently would leave registry
pointers that correctly fail integrity checks. See
[Publishing Graphit artifacts from GitHub Actions](../guides/github-actions-artifacts.md) for the
unattended named-channel workflow.

Memory is mutable and multi-writer, so it is not a versioned Hub artifact. Its authoritative LanceDB
table uses the bucket's `memory/<scope>/<id>/` namespace and the direct-write semantics in
[Memory Module](memory_module.md).

Task is also mutable and multi-writer. Its authoritative LanceDB database uses
`<task.prefix>/project/<project-id>/`, with scheduler leases and fenced task revisions described in
[Task Module](task_module.md). It is never a published versioned artifact or repository replica.

## Security and failure behavior

- Bucket policy and endpoint/network controls are the authorization boundary.
- Explicit Graphit credentials are optional and stored globally as plain text in
  an owner-only file; provider-chain roles are preferred.
- A missing object is normally first-run state. A registry entry whose payload is
  missing is a hard integrity error.
- A missing bucket leaves memory local-only and disables remote Hub operations;
  it is not interpreted as a Git fallback.
