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
- `Install` downloads file-based artifacts or mounts the immutable remote stores
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

AST and knowledge artifacts publish immutable remote graph/search data. The
consumer creates only the local catalog needed to mount that data; it does not
copy one full store per project. File-based artifacts are written into the target
IDE/project and remain version-locked.

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

There are two distribution channels:

- The artifact channel (`hub submit` / `hub install`) publishes a named,
  versioned, discoverable artifact.
- The project-identity channel (`knowledge export` / `knowledge install`) publishes
  the project's compiled documentation context and synchronizes the matching
  memory scope by project ID.

Memory is mutable and multi-writer, so it is not a versioned Hub artifact. It uses
the bucket's `memory/<scope>/<id>/` namespace and the merge/publish semantics in
[Memory Module](memory_module.md).

## Security and failure behavior

- Bucket policy and endpoint/network controls are the authorization boundary.
- Explicit Graphit credentials are optional and stored globally as plain text in
  an owner-only file; provider-chain roles are preferred.
- A missing object is normally first-run state. A registry entry whose payload is
  missing is a hard integrity error.
- A missing bucket leaves memory local-only and disables remote Hub operations;
  it is not interpreted as a Git fallback.
