---
title: Storage Layout
type: architecture
updated: 2026-08-14
tags: [architecture, storage, ast, knowledge, memory, hub]
---

# Storage Layout

> Where every compiled artifact of this framework lives, and why there is exactly one
> of each.

Everything the framework compiles — code graphs, documentation wikis, memory wikis,
their search indexes and their caches — lives in the **global brand directory**,
keyed by an identifier. A project directory holds no compiled data at all: only its
own source, its lockfile, and a handful of small per-project records.

The single resolver for all of it is `internal/store`. Nothing else composes these
paths.

---

## Where the root is

The root is `~/.graphit` by default — `os.UserHomeDir()` joined with the brand's dot
directory — and `brand.GlobalDir()` (`internal/brand/brand.go`) is the one function that
resolves it. Every path in this document hangs off its return value.

### Moving it: `GRAPHIT_GLOBAL_DIR`

Set `GRAPHIT_GLOBAL_DIR` and it **prevails** over the home directory. On a white-label
build the name follows the brand, so a binary built as `acme` reads `ACME_GLOBAL_DIR` —
the variable is `strings.ToUpper(Brand) + "_GLOBAL_DIR"`, the same rule every other
`<BRAND>_*` variable follows.

```bash
export GRAPHIT_GLOBAL_DIR=/mnt/fast/graphit
graphit sync
```

The override is total, not partial. Everything named below moves with it:

| What | Resolver |
|---|---|
| every AST graph, wiki and memory store | `store.Root()` |
| the global config file, `config.json` | `config.AppDir()` |
| the embedding and reranker models | `ai.ModelsDir()` |
| the extracted core runtime, `runtime/<version>/` | the launcher, and `brand.RuntimeDir()` |
| global and Hub rule overrides | `brand.GlobalRulesDir()`, `brand.HubRulesDir()` |
| `frameworks/`, `artifacts/`, the global `AGENTS.md` | `paths.GetPaths()` |
| user grammar libraries, `grammars/{treesitter,antlr}` | the AST grammar loaders |

Things worth knowing before you set it:

- **It is not a config key, and `graphit config` cannot set it.** The global config file
  lives *inside* the global directory, so a key naming that directory could only be read
  after the directory had already been found. The environment is the only layer that
  resolves before the filesystem does. See
  [Config Module](../specs/config_module.md#the-one-setting-that-is-not-a-config-key).
- **It must be inherited, not just exported once.** The daemon, the MCP server and the
  core binary the launcher execs all resolve the root independently; they agree because
  they share an environment. Set it where every one of them will see it — a shell
  profile, the MCP server definition, the service unit — not in a single terminal.
- **Nothing migrates.** Pointing the variable at an empty directory gives you an empty
  framework: the next `graphit sync` reindexes from source, and the old root is left
  untouched. Move the contents yourself if you want the history.
- **A blank value means unset.** Whitespace-only is treated as absent, and the home
  default applies.
- **A relative value is resolved once**, against the working directory the process
  started in — not against the live one, because the daemon chdirs into the global
  directory and would otherwise resolve one level deeper on every later call. Prefer an
  absolute path.
- **`GRAPHIT_MODEL_CACHE` still wins for the models.** It is the narrower override and
  is checked first, which is what lets the ~1.1 GB of model weights sit on a different
  volume from the rest.
- **The test suite ignores it.** `internal/brand/testhome.go` unsets the variable inside
  a test binary, so an operator with it exported in their shell does not have a `go test`
  run write into their real store. Test isolation is `HOME`, and stays `HOME`.

---

## The layout

```
~/.graphit/
├── ast/
│   ├── project/<project-id>/            a project's own code graph — icebug filesystem, :memory: catalog
│   │   ├── graph.icebug/                    the graph — Parquet CSR bundle (nodes_*.parquet, indices_*.parquet, indptr_*.parquet)
│   │   │   ├── schema.cypher                    storage = '<abs>/graph.icebug', format='icebug-disk'
│   │   │   └── icebug.json                      canonical manifest v2
│   │   ├── search.lance/                    the search index — text, terms, vectors
│   │   ├── manifest.json                    parse-shard manifest
│   │   └── shards/                          independently compressed per-file caches
│   │       ├── <relPath>.nodes.json.zst     parsed entities
│   │       ├── <relPath>.edges.json.zst     parsed relationships
│   │       └── <relPath>.emb.zst            exact float32 embedding cache in a Zstandard frame
│   ├── context/<name>/                  a locally imported code graph — same icebug filesystem layout
│   │   └── graph.icebug/ + search.lance/ …
│   ├── hub/<context-id>/<version>/      a Hub code graph, MOUNTED per version
│   │   ├── schema.cypher                    storage = 's3://…', format='icebug-disk' — the graph's Parquet stays on S3, never downloaded
│   │   ├── icebug.json                      canonical manifest v2
│   │   └── search.uri                       where the search index lives (an s3:// URI)
│   └── queries/                         USER grammar query overrides (not a store)
├── wiki/
│   ├── knowledge/
│   │   ├── project/<project-id>/        a project's documentation wiki
│   │   ├── context/<name>/              a locally imported documentation wiki
│   │   └── hub/<context-id>/<version>/  a Hub documentation wiki, shared per version
│   └── memory/<scope>/<scope-id>/       a memory wiki (scope: project | user | <context>)
├── memory-raw/memory-<scope>-<id>/      the raw memory store — the source of truth
│   └── .wiki/shards/…                       the scope's shards, uploaded with it
│   └── history/<id>/NNNN.md                 previous revisions of a page
├── models/coderankembed/                the embedding model — downloaded at setup
│   ├── model.onnx                           ~132 MB, NOT carried in the binary
│   └── tokenizer.json
└── …                                    hub clones, registry cache, global lock, runtime
```

Every wiki directory — knowledge or memory, project or context — has the same shape:

```
<wiki dir>/
├── <Slug>.md                    one page per source document
├── index.md, log.md             the catalogue and the sync timeline
├── index.lance/                 the search index — DERIVED, rebuildable
├── .manifest.json               knowledge staleness hashes (knowledge only)
├── .cluster_cache.json          community assignments (knowledge only)
└── shards/<relPath>.wiki.json   the processed chunks
    <relPath>.emb.json           the embedding vectors
    <relPath>.meta.json          hash, stat, slug and outgoing cross-refs
```

There is **no shared index file** in that directory, and that is deliberate: a memory scope is
multi-writer — every developer on a team pushes to it — so a single file holding an entry per
document is a shared write target that the last upload wins. Per-file sidecars remove it, so two
people adding different memories add different files and both survive.

The reasoning came from git, where the shared file conflicted on every concurrent push and git
could not merge JSON. **It outlived git**, because object storage has no merge at all: a whole-file
upload simply overwrites. See
[Wiki Module](../specs/wiki_module.md#-process-cache-one-file-per-source-file).

### Two engines, and which one owns what

**LadybugDB holds the graph. LanceDB holds everything that answers a text query.** The split
is by question, not by convenience:

| | engine | holds |
|---|---|---|
| the graph | LadybugDB | nodes, edges, and the properties a Cypher `MATCH` reads |
| the search index | LanceDB | file text, the matchable fields, the gram bags, the vectors |

So a code store is a **bundle directory** and a Lance index — `graph.icebug/` and `search.lance/` — and a wiki is one
directory, `index.lance/`. The Ladybug catalog is `:memory:`, rebuilt per connection from `graph.icebug/schema.cypher` (`storage='<abs>'`, `format='icebug-disk'`); no `ladybugdb` file, no `.wal`/`.shadow`, no `CHECKPOINT`. Nothing indexed for retrieval lives in the graph, and no
structure lives in the index.

This was one engine for three days, with the search tables inside the graph database. What
brought it back apart is a single measured property: **liblbug does not maintain a full-text
index on insert** — 22 of 25 rows stay invisible — so every write had to DROP and CREATE all
seven indexes afterwards, which is `O(corpus)` work for an `O(1)` change. On the real corpus
at 39,429 files a full rebuild took 988 s and an incremental of ONE file took **1,178 s**,
slower than rebuilding everything. A sidecar index does the same incremental in ~300 ms, because
it does not have to reindex the corpus to see one new row — SQLite got there with triggers, and
LanceDB gets there by scanning the fragments it has not folded in yet.

Three consequences follow, and each was chosen rather than inherited:

- **The search index is updated in place**, not copied into the graph's working directory
  and not published by its `rename(2)`. That is what buys the ~300 ms. The cost is real and
  bounded: for the width of one update a reader can see the new index against the old graph.
  The next incremental corrects it, and paying `O(corpus)` per edit to close that window is
  exactly the trade being refused.
- **An appended row is searchable before it is indexed.** The engine scans the fragments it has
  not folded into the inverted index yet, alongside the index — MEASURED, and the opposite of
  what the intuition says. So an incremental deletes by path, appends, and folds afterwards for
  latency; the fold is never load-bearing for correctness. The SQLite era got the same property
  from triggers, and paid for it per row on every write.
- **The embedding is a column of the entity.** There is no separate vector table and no row-id
  bridge between the two, so the whole class of bug where a stale vector answers for an entity
  that no longer exists cannot be expressed: deleting the entity deletes its vector. The uid is
  the key, so nothing has to be numbered either.

Two things the engine imposes, worth knowing before reading a surprise as a defect:

- **A vector index needs 256 rows to train.** IVF-PQ trains on the data, so below that floor the
  index is skipped and semantic search answers by scanning — which at that size is what an index
  would degenerate into anyway. A project with fewer than 256 indexed entities therefore has no
  vector index, and that is correct rather than broken.
- **One text column, not seven weighted fields.** The engine's full-text query takes one column,
  so the fields are concatenated into one document and BM25 ranks it — it already weights by term
  rarity, which is what the manual weights approximated. The fusion that used to do this in Go is
  gone, and reranking that could not be expressed as one query is the engine's own RRF.

What the split costs is a **build tag**, and the tag changed character when the engine did. It
used to be `fts5`, which was one compiler flag on a vendored C file — `#cgo CFLAGS:
-DSQLITE_ENABLE_FTS5` — so a build without it linked and then failed at run time with `no such
module: fts5`, and two `!fts5` guard files existed to turn that into one actionable line.

It is now `lancedb`, and it is heavier: the native is built from Rust source for the host and
**cannot be cross-compiled**, which is why the release runs one job per platform rather than
cross-compiling from one. There is no guard file this time, deliberately — a build without the tag
links stubs whose error already names the tag and the fix, which is precisely what `no such
module: fts5` did not do. See [the LanceDB link contract](../../internal/lancestore/cgo_lancedb.go)
for why there are two rpaths.

The link library itself resolves like every other native this tree consumes — through what a
machine-global location already holds, not through per-checkout state. `make lancedb-native` walks
a cascade: an existing `.native/liblancedb_go.so` wins; otherwise it symlinks (copies on Windows)
the library the launcher already extracted into `~/.<brand>/runtime/dev/`, the same machine-global
location `embedding_local.go` reads `libonnxruntime` from; only when neither exists does it build
from source with cargo. The extracted runtime copy does not record which pinned `LANCEDB_SHA`
produced it — builds by cargo stamp the SHA beside the project copy (`lancedb_go_build.sha`) so
provenance can be checked later, and `make fetch-lancedb` remains the explicit rebuild when the
pin moves.

### What identifies a store

| Store | Key |
|---|---|
| a project's graph and wiki | the project's **lockfile ULID** |
| a project that has never been `init`ed | `path-<16 hex>`, a hash of its absolute path |
| a locally imported context | its **sanitised name** |
| a Hub context, AST or documentation | its **publishing project's id** plus the **version** |
| a memory scope | `project` → the project id; `user` → a hash of `unit.id` from the global config; a context → its own name |
| the embedding model | nothing — there is one copy per machine, shared by every project and every version |

`store.ProjectStoreID` implements the first two. The path-hash fallback exists so
that `ast index` keeps working in a directory that has not been initialised — it
never has required `init`. The consequence is worth stating: running `init` in a
directory that was already indexed re-keys its store, and the next query reindexes
once. That is a one-time cost at the moment a project gains an identity.

### Having an identity is not the same as being entitled to a store

A live search session builds a workspace that looks exactly like a project: it has a
lockfile, and therefore an identity, because the agent CLI discovers everything from its
working directory and would otherwise find an empty folder. Every resolver above keys on
that identity, so for a while a search that existed for a few minutes acquired a code
graph, a documentation wiki and a memory scope — and the memory scope meant an orphan
branch and a worktree in the **shared** memory repository. Nothing reclaimed any of it.

So the lockfile carries `project.ephemeral`, and `store.IsEphemeralProject` is what the
resolvers ask:

| | a project | an ephemeral session |
|---|---|---|
| its own code graph | yes | **no** — refused, and there is no source to index |
| its own documentation wiki | yes | **no** — the sets it can read are the contexts it selected, by name |
| its own memory scope | yes | **no** — a project-scope request is served from the **user** scope, and says so |
| imported contexts, graph and wiki | yes | **yes** — this is what a session is for |

The flag is recorded rather than inferred. "Has no source of its own" would also describe
a real project on its first day. See
[An ephemeral session owns no store](../tasks/an-ephemeral-session-owns-no-store.md).

### Two platform rules the identity has to obey

**The path hash folds case on Windows and macOS, and not on Linux.** Those two treat
paths differing only in letter case as the same directory, so a project reached as
`C:\Proj` and as `C:\proj` would otherwise get two store ids — two graphs and two
wikis, each looking healthy while holding half the answers. `store.pathStoreID` takes
the fold as a parameter so both behaviours are testable on any host; the platform
decides the value once, in `caseInsensitivePaths`.

**A Windows device name is defused on every platform.** `nul`, `con`, `com1` and the
rest are refused as directory names by Windows, and an identity can genuinely be one:
`HubContextID` falls back to a Hub artifact ID its author chose freely. `nul` becomes
`nul_` everywhere — not only on Windows — so that one artifact resolves to the same
path on every machine, including a global directory carried to another one or a shared
CI image.

Both rules go through `storeIDSegment`, which changes nothing else: case and characters
are left alone, because an id is an identity and the two resolvers keyed by it
(`ProjectStoreID` and the `*ByID` variants) must agree on the path or a project's own
store and the same store seen from the ecosystem become two directories.

`internal/store` is CGO-free and is exercised on Linux, macOS and Windows by the
`platform-semantics` CI job, which is where these rules are actually verified rather
than asserted.

---

## What stays in the project

```
<project>/
├── graphit.lock.json          project identity + installed Hub artifacts
└── .graphit/
    ├── ast/queries/                       project query YAMLs — versionable
    ├── rules/                             rule and skill overrides — versionable
    ├── grammars/                          local parser libraries — gitignored
    │   ├── treesitter/
    │   └── antlr/
    └── runtime/                           generated output and state — gitignored
        ├── ast/export/                    default `graphit ast export` output
        ├── dream/                         reports, sentinels, last-seen marker
        ├── daemon/                        daemon.log and dream.state
        ├── cache/skills/<ide>/<skill>/    sync cache
        ├── cache/artifacts/<ide>/<type>/  artifact sync cache
        ├── mandate.hash
        ├── sync.stamp
        ├── sync.lock
        └── sync-heavy.lock
```

The project-local tree mixes repository source, platform-specific parser binaries,
generated output, and machine state. The ownership boundary — not the file extension —
decides what belongs in Git:

- **Data is global.** One graph, one wiki, one memory store per identity.
- **Membership is per-project.** Which contexts a project may query is a fact about
  the project and cannot be derived from a global directory listing without telling
  project A about project B's imports.

### Inside a project's brand directory

The brand directory is split by **ownership**. The generated `.gitignore` names the two
machine-local trees (`**/.graphit/runtime/` and `**/.graphit/grammars/` — see
[Git Module](../specs/git_module.md#the-generated-gitignore-block)).

| Path | What | Committed? |
|---|---|---|
| `ast/queries/` | grammar query overrides, `ast.queries_dir` by default | yes — a grammar override is about the repository |
| `rules/` | module rule and skill overrides, `session.md` | yes — written by a human for the team |
| `grammars/{treesitter,antlr}` | local parser libraries added or customized for this checkout | **no** — platform-specific binaries; distribute shared grammars through a Hub language artifact |
| `runtime/ast/export/` | default output of `graphit ast export` | **no** — an explicit `--output` remains user-owned |
| `runtime/dream/` | default Dream reports, deep-sleep sentinels, and last-seen marker | **no** — set `dream.reports_dir` to publish them elsewhere |
| `runtime/daemon/` | this project's daemon log, dream state | **no** |
| `runtime/cache/{skills,artifacts}/` | content hashes that let `sync` skip unchanged artifacts | **no** |
| `runtime/mandate.hash` | per-trigger hashes for the mandate fast path | **no** |
| `runtime/sync.stamp`, `runtime/sync.lock`, `runtime/sync-heavy.lock` | the sync's debounce stamp and its two locks | **no** |

Every writer under `runtime/` goes through `brand.ProjectRuntimePath`. That is the point
of the helper: a path built by hand somewhere else lands outside the generated-output
boundary and can appear in `git status`. Grammar loaders resolve the separate ignored
`grammars/` tree because it stores platform-specific parser libraries, not runtime state.

Nothing under `runtime/` is repository source. It contains generated exports and reports,
caches, locks, stamps, and logs. Delete it only when those local outputs are no longer
needed; the framework recreates the directories as required.

> `runtime/` is also where the **degenerate** store path goes: `store.globalOr` falls
> back to it on a machine with no home directory, since a store is machine state too.
> On a normal machine no store is ever written inside a project.

The whole brand directory stays out of the **code graph** regardless, through a default
`.astignore` pattern — being versioned and being indexed are different questions. See
[Ignore Files](../guides/ignore_files.md).

Legacy paths `.graphit/ast/export/` and `.graphit/dream/` are no longer defaults. Graphit
does not move or delete them automatically; use `--output` or `dream.reports_dir` to point
at them temporarily while migrating data explicitly.

### Membership has one home: `graphit.lock.json`

Every context a project can reach is an artifact entry in its lockfile, whatever the
origin:

| Origin | `version` | `origin` | `source_path` |
|---|---|---|---|
| `hub install` | the resolved version | `hub` | — |
| `ast install <path>` / local knowledge import | `local` | `local` | the directory it was indexed from |
| `hub link` at a sibling project | `local` | `link` | the sibling's directory |

It used to be two homes. A Hub **AST** artifact resolved from the lockfile while a local
import, a link, and a Hub **knowledge** artifact resolved from a separate
`.graphit/contexts.json`. That split followed no principle — it followed the store path:
an AST Hub store is version-keyed, so resolving one required reading the version, and the
version was in the lockfile; a knowledge store was not version-keyed, so its resolution
never needed the lockfile and never learned to read it. The consequence nobody chose was
that **a knowledge context was not versioned at all**: two projects pinned to different
versions shared one directory and the last install won.

`source_path` is stored **relative to the project**, slash-separated. The lockfile is
committed and shared, so an absolute path would only be true on the machine that wrote
it. See `projectlock.RelSourcePath` / `SourceDir`.

**No store path is ever recorded.** A link keeps the sibling's *directory* and the store
is derived from it on every read (`store.ASTContextDBPathIn`,
`store.KnowledgeContextDirIn`). Recording the store instead froze it: the moment the
sibling ran `init` and re-keyed, the stored path pointed at the old location.

See [One registry for context membership](../tasks/one-registry-for-context-membership.md).

---

## Why one copy

The previous layout compiled into the global directory **and** replicated into every
project that read it. Three costs, and the third is the one that hurt:

1. **Disk.** A graph plus its search index, and a wiki plus its chunk and embedding
   shards, once per consuming project.
2. **Work.** A compile per copy, or a copy per compile, plus a fan-out pass on every
   change and a Windows-specific failure mode where a replica held open by a reader
   cannot be overwritten.
3. **Correctness.** Two copies of one wiki can disagree, and a project reading a
   replica nobody refreshed answers with yesterday's data *and looks perfectly
   healthy while doing it*. The memory module had exactly this: `RunProjectCycle`
   compiled into the project replica while the daemon compiled into the global wiki,
   so whichever ran last decided what a project could recall — and a memory arriving
   from the remote reached nobody until someone ran a sync inside each project.

One copy removes the class of bug rather than its instances. There is nothing to keep
in step, so nothing can fall out of step.

The second reason is about the agent. An agent is confined to its own workspace and
must not read another project's files. Because a graph and a wiki are never read as
files — every path into them goes through this binary — moving them out of the
project costs the agent nothing and gains it the ability to query a sibling project
by passing a different `project_dir`.

---

## How a store is reached

| What you want | Tool |
|---|---|
| source text of any indexed file, in any project or context | `graphit_ast_source` |
| the graph | `graphit_ast_query`, `graphit_ast_search`, `graphit_ast_schema` |
| a wiki page's content | `graphit_wiki_source` |
| the wiki index | `graphit_wiki_search`, `graphit_wiki_browse`, `graphit_wiki_xrefs`, `graphit_wiki_log`, `graphit_knowledge_search`, `graphit_memory_search` |

Every one of them takes `project_dir`, which is what makes a sibling project's store
reachable: it is a parameter, not an ambient working directory. Correspondingly,
**no store is readable with a file tool** — there is no copy inside the project, and
the global directory is outside the agent's workspace. The three skills state this
as a hard rule.

## Deleting a store

`os.RemoveAll` on a file another process holds open **fails on Windows**, where on Unix
the directory entry simply goes and the open inode lives on. Two commands now aim at
the authoritative store rather than at a copy — `graphit_memory_remove` for a memory
context, and `graphit_knowledge_remove` — so on Windows a removal contending with a
reader returns an error and the store survives. That is the intended outcome: a visible
failure on the real store beats a silent success against a replica, which is what the
same commands used to do.

---

## Cross-references

- [AST Module](../specs/ast_module.md) — the graph, its shards and its search index
- [Wiki Module](../specs/wiki_module.md) — how a wiki is compiled and indexed
- [Memory Module](../specs/memory_module.md) — worktree, wiki, and the scopes
- [Hub Collaboration](../specs/hub_collaboration.md) — artifacts, versions, shared stores
- [Retrieval Architecture](../guides/retrieval_architecture.md) — which tool reads what
- [Task log](../tasks/centralize-stores-in-global-dir.md) — the change that produced this layout
