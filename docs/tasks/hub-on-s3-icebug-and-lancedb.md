---
title: "The Hub leaves git and goes to S3; graph in icebug and search in LanceDB, both queried on-the-fly"
status: in-progress
created: 2026-08-21
updated: 2026-08-24
tags: [hub, s3, icebug, ladybug, lancedb, parquet, sqlite, search, migration, architecture]
---

# Hub on S3, graph in icebug, search in LanceDB

## HOW TO CONTINUE — read this first (2026-08-22)

### Where it stands

| phase | state |
|---|---|
| **T1–T3** S3 config, `internal/s3store`, `setup` asking for the bucket | **done, tested** |
| **T4–T5** `internal/hub/s3_store.go`, rewire, `git_store.go` **deleted** | **done, tested** |
| **T7** `httpfs` in the launcher payload + `LoadExtensions` verified | **done, tested** |
| **T8** native icebug writer in Go (`internal/ladybugstore/icebug.go`) | **data 100% correct; `[:A\|B]` counting FIXED by ordering; 1 upstream defect left with no workaround** |
| **T6** memory leaves git | **done, tested** |
| **T9** install stops downloading | **DONE for both**: knowledge mounts the index, ast mounts the icebug graph + the index |
| **T10** LanceDB native compiled per platform (`make fetch-lancedb`) | **done, tested** |
| **T11** `internal/lancestore` (local + on-the-fly, engine hybrid) | **done, tested** |
| **T12** AST index on LanceDB (`internal/ast/search_lance.go`) | **done, tested** |
| **T13** wiki and memory index on LanceDB (`internal/wiki/store.go`) | **done, tested** |
| **T14** SQLite DELETED — 5,737 lines, the `fts5` tag and the deps | **done, tested** |
| **RELEASE** native per platform, on each OS's runner | **done, verified on linux** |
| **T15** end to end through the CLI in a clean project | **DONE** — found THREE defects no test was catching |
| **T16** documentation | **DONE**: specs, architecture and guides up to date; ADR written; changelogs deliberately untouched |
| **T17** timeout in on-the-fly 3-hop traversal | **DONE** — planner bounded by frontiers, 3 hops in ~0.3 s local / ~0.4 s S3 (previously >30 s); reverse edges committed in `42cc1af`; fix in a separate commit |

`go build -tags lancedb ./...` and the suite are green. No Python in the production path.

### T17 — fix the 3-hop timeout on the remote icebug graph (2026-08-24)

**Objective.** Make three-hop queries against the AST graph mounted on-the-fly stop hitting the
timeout, without reintroducing download or local reconstruction of the graph. The export must
remain native in Go and **always** materialize the reverse adjacency: it is a functional
requirement so the agent can query `-[:TIPO]-` without direction, and it may also help the
anchored plan. The direct and reverse relations must remain separate so that `->` preserves the
semantics of the original graph. The query path must avoid the
`TABLE_FUNCTION_CALL(a._ID) -> RECURSIVE_EXTEND` plan that has already been measured enumerating
the entire universe.

**Reasoning and justification.** The reverse-edges hypothesis, on its own, has already been
discarded by control: on the 60,000-node/200,000-edge corpus, doubling the adjacency did not change
the timeout. It remains relevant to the artifact's contract, but the fix also needs to attack the
query plan and the number/locality of Parquet objects read from S3. The diagnosis will be guided by
`EXPLAIN`, a local benchmark equivalent to the remote mount, and upstream documentation/code; any
change will only be accepted with a test that fails on the previous behavior.

#### Plan and task specification

- [x] **T17.1 — Reproduce and measure the bottleneck.** Identify the real three-hop query, capture
  the physical plan and establish a reference time/result. Done when there is a deterministic test
  or benchmark that distinguishes the slow path from the fixed one without depending on a real bucket.
  **DONE** — `TestIcebugRealGraphThreeHopPlans` captures the five `EXPLAIN` plans; controls:
  native 8.6–13.6 ms vs recursive/reverse/fixed chain >30 s; manual BFS over one-hop frontiers
  returned the same set in ~292 ms.
- [x] **T17.2 — Research the upstream behavior and alternatives.** Consult Ladybug/Kuzu,
  icebug-format, Parquet and httpfs/S3 in primary sources, including the official notebook
  `LadybugDB/ladybug-icebug-notebooks/index.ipynb` pointed out by the Engineer. Done when each
  hypothesis has evidence and is classified as applicable, discarded or dependent on an upstream
  fix. **DONE** — recursive join/global init/explosion/filter placement are upstream pending items
  (kuzu#4285/#4941/#4459/#5040); the official notebook only proves the semantics of reverse edges;
  row-group split discarded by correction; httpfs cache researched and not enabled without evidence;
  secondary indexes nonexistent on this path (`CREATE INDEX` is a no-op).
- [x] **T17.3 — Fix the export of the reverse adjacency.** Every Icebug artifact published by the
  Hub must materialize `TIPO_REVERSE` by default, without contaminating `-[:TIPO]->`. Done when
  round-trip, self-loop, properties, manifest pairs and orientation remain exact and the layered
  opt-out is covered by regression.
- [x] **T17.4 — Fix the three-hop query path.** Rewrite/plan the traversal to start from the
  already-filtered selective set, avoiding global enumeration and preserving public Cypher;
  `-[:TIPO]-` patterns must use `TIPO|TIPO_REVERSE` without altering directed queries. Done when the
  three-hop query completes below the timeout and returns the same result as the native storage.
  **DONE** — bounded-frontier planner in `internal/ast/ladybug_icebug_traversal.go` wired in before
  `runQuery`; `-[:TIPO]-` expands `TIPO` and `TIPO_REVERSE` in separate queries (no alternation in
  the engine); 3 hops identical to native in 291 ms local / 429 ms S3.
- [x] **T17.5 — Verify and document.** Run focused tests, benchmark and a proportional suite;
  update spec/architecture and record trade-offs, files, use cases and BDD scenarios. Done when
  code, documentation and indexes are in sync and without known regressions. **DONE** —
  spec `docs/specs/hub_collaboration.md` covers one row group, bounded planner, safe subset,
  fallback, UID fast path and the conclusion on indexes; focused tests green; the expanded suite
  passed twice; `go vet` clean on the changed files; `git diff --check` clean; memories and Graphit
  sync executed; fix in a separate commit.

### RELEASE FIXED: the native is built on each OS's runner

The defect was real: `BUILD_TAGS := lancedb` applied to all three targets and the native does not
cross-compile, so `.native/` had only the linux `.so`.

**The CI was already per platform** — `build-linux` on `ubuntu-22.04`, `build-darwin` on `macos-14`,
`build-windows-native` on `windows-2022`. So `build-darwin` does **not** cross-compile: it runs on
the mac. What was missing was the native, and the fix ended up symmetric to what already existed
for `liblbug` and ORT:

- the three targets gained `lancedb-native` as a dependency and `cp -L $(LANCEDB_LIB)` into the
  launcher payload;
- the three CI jobs gained `dtolnay/rust-toolchain@stable` and caching of
  `~/.cargo` + `$(LANCEDB_CACHE)`, keyed by `hashFiles('Makefile')` — which is where
  `LANCEDB_SHA` lives, so changing the SHA invalidates the cache;
- `LANCEDB_CACHE` became `?=` so the CI can point it where it knows how to cache (on Windows,
  `/c/cache/lancedb`);
- `ci.yml` too: `make vet`/`make test` now compile with `-tags lancedb`, and without Rust there the
  whole CI would break.

**The `build-windows` that cross-compiled from linux was REMOVED.** It was the only target that
could not carry the native, and the Engineer's decision settles this: `build-windows-native`
already exists, runs on the Windows runner and carries it. Keeping the cross alive meant keeping a
path capable of producing a binary that compiles, links and then answers `ErrNotBuilt` to every
query — the same failure this project spent a day removing from `fts5`.

`build-all` went with it: **no machine builds everything**, because the native does not
cross-compile and a binary without it has no search. The target now fails with the explanation and
points to `.github/workflows/release.yml` (three runners) or `make build-local` (this machine).

#### Verified here, on linux

What matters in the payload is **`$ORIGIN`**, because the absolute `rpath` does not exist on the
user's machine. Tested by provoking exactly that: binary and library copied to a temporary
directory, the absolute `rpath` target **hidden**, and then

```
ldd → liblancedb_go.so => /tmp/tmp.fP3770Pf3n/liblancedb_go.so
graphit version dev
```

resolves through the binary's own directory and runs. `make vet` green. `make build-local` produces
a binary that resolves the library.

**Not verified here:** the darwin and windows builds, because there is neither `clang` nor mingw on
this machine — the first run of the workflow is their test.

### ICEBUG IS THE ONLY LADYBUG MECHANISM IN THE HUB (2026-08-23)

The Engineer's decision: despite the upstream gaps, what exists is enough — and it has to be the
**only** path. Publish exports icebug; install **always** reads on-the-fly.

**Two fallbacks were REMOVED, and that is what settles the decision:**

| deleted path | what it was doing wrong |
|---|---|
| Parquet bundle of the graph | the consumer **loaded** the graph: the bytes traveled and every project pinned to a version kept its copy of the same immutable data |
| publishing the shards | the consumer **rebuilt** the graph, paying per installation for a result the publisher had already frozen |

And the two together made an artifact's behavior depend on **which path it happened to take** — so
a consumer had no way to know whether its context was mounted or copied.
`internal/ast/parquet_transfer.go` and its test were deleted.

**Publishing now FAILS if it cannot export**, instead of falling back. An artifact nobody can mount
is one nobody can install the intended way, and the moment to find that out is the publisher's, not
every consumer's.

**`storage` is decided by the publisher and never rewritten.** It is the only one that knows where
it put the objects, so the consumer's URI is computed before the export and written into every
`CREATE … WITH (storage = 's3://…', format = 'icebug-disk')`. Pinned by a test that refuses an
empty URI (a `storage = ''` mounts against the process's working directory) and that verifies the
published schema **does not leak a local path** — leaking would make the artifact work on exactly
one machine.

**Install runs the DDL against an empty local catalog.** What comes down is `schema.cypher` — a few
KB of metadata, and without it there is nothing pointing at the objects. Not one byte of graph.

#### Verified: the whole cycle, and the data does NOT come down

`TestIcebugArtifactMountsAndAnswers` indexes real source, publishes, mounts the DDL and queries:

```
published bundle holds 7 data files; the local catalog holds 0
mounted graph answers: 5 nodes
mounted graph answers: 1 CALLS edges
```

**The assertion is by absence, not by size** — and that was fixed during the work. Comparing sizes
was the first attempt and proves nothing: a catalog has a floor of one page (16 KiB here), so
against a two-function graph the catalog is legitimately the larger of the two. On a real graph the
ratio inverts by orders of magnitude, but a test that only holds for large input is a test that will
be wrong on somebody's fixture. What is verifiable at any size: the Parquet files are in the
published bundle and **nowhere** under the mount.

#### The gaps, accepted and declared where you trip over them

They are at the top of `internal/ast/icebug_transfer.go`, not only here, because that is where a
caller finds them:

- **multi-hop traversal** over a mounted graph is weaker than over a native one;
- **one edge table holds one CSR**, so it declares exactly one FROM/TO pair. With ~97 pairs in this
  graph, every label is folded into an `Entity` table with the label as a column — so
  `MATCH (f:Function)` becomes `MATCH (e:Entity {label:'Function'})` against a mounted context.

The trade was made deliberately: an installed context that answers one-hop questions over objects
nobody downloaded is worth more than one that answers everything after copying a gigabyte, and the
alternative on offer was neither of the two.

### T9 DONE FOR KNOWLEDGE: publish uploads, and the read is straight from S3

The recorded blocker was about **multi-hop traversal**, and it only hits the **graph**. A
`knowledge` context has no graph — it is wiki, only LanceDB — so nothing was blocking it.

**Installing a knowledge context stopped transferring bytes.** The decision is taken BEFORE the
clone, because the clone is the transfer; the rest of the installation continues (lockfile,
dependencies, telemetry), and it is the lockfile entry that makes the mount resolvable later.

**The URI is DERIVED, never written.** The context entry already carries everything the location is
made of — artifact, version, publishing project — so the URI is computed from that plus the
configured bucket. Writing it into the lockfile was the obvious move and would have been wrong
twice: it changes a format every project on disk already has, and it freezes an endpoint, so
pointing the framework at another bucket would leave every installed context resolving to the old
one.

**`MountsKnowledge` requires BOTH conditions** — bucket configured AND engine linked. Without the
engine there is nothing that opens an `s3://` index, so a build without the `lancedb` tag has to
keep downloading; answering "yes" there would install a context with no bytes and no way to read
them.

**Page reading now comes from the index.** `wiki_source` read the `.md` file on disk, and a mounted
artifact has no file at all. The body is in the `body` column, and it is the **same** text because
the wiki compiles one chunk per document — so it is a faithful read, not an approximation.

#### Two real defects that the end-to-end test found

1. **THE READ WENT THROUGH A WRITE PATH.** `ensureTables` calls `EnsureTable`, which **creates** —
   and creating is refused on a published store. Result: every published wiki answered
   `this store is read-only` to any query, which reads as a permission problem and was a
   code-path problem. **No published wiki was readable.** In a mounted artifact the tables exist by
   definition: the publisher wrote them. Now they are **opened**, and a missing table is tolerated
   (an artifact published by an old build may have no sync log, and refusing the whole wiki over
   that would make one missing table cost the pages).
2. **The mount has to address the directory the publisher wrote.** Pinned by a test that compares
   the mount URI against the list of objects actually uploaded — the divergence there is exactly the
   one that shows up as `no such table`.

#### What was verified, and over which transport

**Always:** `TestPublishedWikiCarriesItsIndexes` asserts that the published artifact contains
`_versions/` (the manifest), `data/` and **`_indices/`** — this last one is what proves install
stopped rebuilding, because the inverted index travels.

**The whole wiring**, with local transport: publish → resolve the mount → open → search → read
page → walk xrefs. The two defects above are transport-independent, and they were real.

**OVER THE NETWORK, VERIFIED.** After the Engineer freed up disk space, the same test ran against
the real MinIO:

```
running over a REAL object store
reading on-the-fly from s3://graphit-hub/artifacts/knowledge/acme/1.0.0/index.lance
--- PASS
```

Publish, resolve the mount, search, read page and walk xrefs — all over objects that nothing
downloaded.

### THE OTHER HALF OF THE MOUNT: the AST search also reads from S3 (2026-08-23)

A gap found while checking the architecture against what the code does, and it is the kind that
slips through: **the graph mounted and the search did not.** `NewQueryService` opened the index at
`LanceIndexPath(dbPath)` — a local path — and in a mounted context nothing had been downloaded.
Result: the context **traverses perfectly and answers every search with nothing**, which reads as a
corpus with no match, not as a missing index.

The fix gives search the **same treatment the graph already had**: local metadata pointing at remote
data. `WriteSearchMount` writes a `search.uri` next to the catalog, and `OpenSearchIndex` resolves
through it — so a `QueryService` built the same way serves a local project and a Hub context,
without the caller knowing which is which.

**Only the URI is written.** Region and endpoint are resolved from the environment configuration at
open time, for the same reason as not writing them into the lockfile: writing them freezes the
endpoint, and pointing the framework at another one would leave every installed context searching
the old one. The bucket is part of the location and goes into the URI; the rest is *how* you connect,
not *where* it is.

Verified in the same full-cycle test:

```
mounted search answers: 4 hits for "helper"
mounted index serves file text
```

with one guard: the test fails if a local index exists next to the catalog, otherwise it could pass
by reading a local copy.

**One defect of the test, not of the code, found along the way:** `IndexSource` is opt-in and OFF by
default, so the first version of the test asserted file text on a store that had never been
instructed to keep any.

### MEASURED: the documented label form in a mounted context FAILS LOUD, not in silence

I had raised this as a risk — that `MATCH (n:Function)` would silently match nothing against a
mounted context, because the label became a column of `Entity`. **Measured, and it is wrong at the
point that mattered:**

```
MATCH (n:Function)                            -> ERROR: Table Function does not exist.
                                                 — "Function" is not a label or relationship type
                                                 in this project's graph… Present: CALLS,
                                                 CONTAINS, Entity, REFERENCES
MATCH (n:Entity) WHERE n.label = 'Function'   -> 2
MATCH (n:Entity {label:'Function'})           -> 2
```

The error message that already exists **lists the tables present**, including `Entity`. So the user
gets a named failure and the path to the right form, which is the behavior you want — there is no
defect to fix here and no transpiler to build.

**A note on the "label transpiler":** it does NOT exist as code, even though the log and some tests
name it as necessary. What exists in `internal/ast/ladybug.go` are two adjacent rewriters:
`rePatternLabel`, which backticks labels so that one colliding with a keyword parses, and
`reLabelPredicate`, which converts `WHERE n:Function` (Neo4j syntax) into `label(n) = 'Function'`.
Neither of them translates `(n:Function)` into the folded form.

### The `[:A|B]` bug: HALF RESOLVED, and the attribution was wrong

**The counting was fixed** — by ordering, and it is one line. **The filter on a bound endpoint is
still wrong, and it is UPSTREAM.** Full detail in the section "RESOLVED: `[:A|B]` is TWO defects".

Summary of what matters for deciding:

| | before | now |
|---|---|---|
| `[:A\|B]` counting, the 28 pairs | 9 wrong | **28/28 exact** |
| the 8 alternatives at once | — | **204,353 = exact** |
| edge identity in the alternation form | not tested | **exact** (source and target) |
| `[:A\|B]` with `WHERE b.name = …` | wrong | **still wrong — UPSTREAM, no workaround** |

**The fix:** `sortRelsLargestFirst` — `schema.cypher` creates the edge tables **from largest to
smallest**. The engine caps every alternation by the row count of the **first table created**; with
descending order, the lowest-id one in any subset is also its largest, so nothing is ever truncated.

**Consequence for the design: partitioning by pair does NOT become viable again and the label
transpiler is still necessary.** The remaining defect is exactly the form partitioning would use:
`MATCH (f:Function)-[:CALLS]->(g) WHERE g.name = …` becomes alternatives with a filtered endpoint.

### How to recreate the fixtures (the previous session's scratchpad no longer exists)

```bash
# 1. FROZEN copy of the real graph. Mandatory: reading the store the daemon is rewriting
#    causes a segfault in cgo and a duplicate-key error that does not exist.
mkdir -p /tmp/icebug-fix && cp ~/.graphit/ast/project/01KSH1CRFFG8Z74B5ZS78WW808/ladybugdb \
  /tmp/icebug-fix/ladybugdb

# 2. A `demo` graph (60k nodes, 200k edges) for GRAPHIT_TOOL_ICEBUG, and a
#    Big/Small pair at the sizes of the pair that was failing, for GRAPHIT_TOOL_ICEBUG_MULTI.
#    pyarrow is NOT in the system python — use `uvx --with pyarrow --from pyarrow python`.
mkdir -p /tmp/icebug-fix/src /tmp/icebug-fix/demo-src
uvx --with pyarrow --from pyarrow python - <<'EOF'
import pyarrow as pa, pyarrow.parquet as pq, random
def graph(d, name, n, N=60000):
    pq.write_table(pa.table({"id": pa.array(range(N), pa.int64()),
        "name": pa.array([f"{name}_{i}" for i in range(N)])}), f"{d}/{name}-v.parquet")
    pq.write_table(pa.table({"src": pa.array([random.randrange(N) for _ in range(n)], pa.int64()),
        "dst": pa.array([random.randrange(N) for _ in range(n)], pa.int64())}), f"{d}/{name}-e.parquet")
random.seed(11); graph("/tmp/icebug-fix/src", "Big", 92396); graph("/tmp/icebug-fix/src", "Small", 54823)
random.seed(7);  graph("/tmp/icebug-fix/demo-src", "demo", 200000)
EOF

# Dois grafos numa source-dir => a ferramenta escreve UM SUBDIRETÓRIO POR GRAFO.
uvx icebug-format --source-dir /tmp/icebug-fix/src --output-dir /tmp/icebug-fix/out --backend pyarrow
# Um grafo só => escreve direto na output-dir, e `--storage` tem que apontar para ela.
uvx icebug-format --source-dir /tmp/icebug-fix/demo-src --output-dir /tmp/icebug-fix/tool \
  --backend pyarrow --storage /tmp/icebug-fix/tool
```

`--backend pyarrow` is required: the default demands duckdb and fails with `ImportError`.

### Tests and the variables they ask for

```bash
GRAPHIT_REAL_STORE=/tmp/icebug-fix/ladybugdb \
GRAPHIT_TOOL_ICEBUG=/tmp/icebug-fix/tool \
GRAPHIT_TOOL_ICEBUG_MULTI=/tmp/icebug-fix/out \
  go test -tags fts5 -run "TestIcebug" ./internal/ladybugstore/ -v
```

| test | what it guarantees |
|---|---|
| `TestIcebugAgainstARealGraph` | round trip label by label and type by type on the real graph |
| `TestIcebugWritesOneRowGroupPerFile` | **the invariant that cost the most** — one row group per file |
| `TestIcebugFiltersOnBothSidesOfAPattern` | regression guard for the row group bug |
| `TestIcebugCountOfANodeVariableAgrees` | same |
| `TestIcebugEveryPairOfTypesSumsExactly` | **the guard of the ordering fix** — 28 pairs + the 8 at once, and the descending order in `schema.cypher` |
| `TestIcebugAlternativesBoundIsTheFirstTable` | pins the RULE (bound = first table created, not the minimum) with 2 and 3 tables |
| `TestIcebugAlternativesKeepEdgeIdentity` | counting **and identity** of the edge, with disjoint source ranges |
| `TestIcebugAlternativesWithAFilteredEndpointIsWRONG` | asserts the remaining defect, and **fails when it gets fixed upstream** |
| `TestIcebugPairsSumWithPerTableStorage` | keeps the shared `storage` directory hypothesis buried |
| `TestIcebugAlternativesDefectOnToolOutput` | reproduces the truncation in the tool, **in both orders** |
| `TestIcebugFilteredAlternativesDefectOnToolOutput` | reproduces the filtered-endpoint defect in the tool |
| `TestIcebugDefectsReproduceOnTheReferenceToolOutput` | separates our defect from the upstream one |
| `TestIcebugRealGraphQueryCost` | native cost against icebug |

### Traps that have already cost time

1. **A matching count does not prove the edge links the right nodes.** Test identity: **disjoint**
   source id ranges per table, and ask which sources the pattern reports.
2. **`count(<node variable>)` and `count(r)` are not the same question.** Comparing one against the
   other made me report a nonexistent defect.
3. **A small fixture hides and misleads:** 2+2=4 is indistinguishable from 2×2=4.
4. **`parquet.WithMaxRowGroupLength` does not merge row groups** — each `FileWriter.Write` opens a
   new one. The whole table goes in a single record.
5. **Measure against a frozen copy of the store.** A count changing between runs is the signal that
   the daemon is writing.
6. **One store handle per test process** (`openRealStore` with `sync.Once`) — opening the same store
   repeatedly causes a failure that disappears when the test runs alone. In the synthetic harness,
   use **a subtest per case** so the store closes between them: opening dozens in one process gives
   `failed to open database with status 1`.
7. **`count(<property>)` does NOT skip null in this engine** — `count(r.line_number)` equals
   `count(r)` even on a table that does not have the column. It is no use for distinguishing an
   alternative. An earlier inference ("all the rows came from CALLS") came from there and the premise
   never held.
8. **Testing 6 pairs out of 28 found 1 failure where there were 9.** Enumerate the whole matrix: it
   was the matrix that revealed the discriminant was the creation order, not the pair.
9. **The reference tool was only compared in one order.** A test against a reference implementation
   has to sweep the order too, otherwise it absolves the engine by luck.

### Upstream: one defect that blocks, one with a workaround

- **Multi-hop traversal does not complete.** Native does 2 hops in 2,133 ms (867,766 paths); icebug
  does not complete in 100 s. **Reproduced on the official tool's output.** `EXPLAIN` shows
  `TABLE_FUNCTION_CALL` over `a._ID` enumerating all the nodes before the `RECURSIVE_EXTEND`. It
  matches [kuzu#4941](https://github.com/kuzudb/kuzu/issues/4941), [#4459](https://github.com/kuzudb/kuzu/issues/4459),
  [#5040](https://github.com/kuzudb/kuzu/issues/5040), [#4540](https://github.com/kuzudb/kuzu/issues/4540),
  [#4285](https://github.com/kuzudb/kuzu/issues/4285). **Reverse edge does NOT fix it** (measured).
- **`=` on the primary key returns empty.** Exact workaround: `IN [value]`. `STARTS WITH`,
  range and `ORDER BY` also work.

Also worth reporting upstream, with a ready reproduction: [kuzu#2866](https://github.com/kuzudb/kuzu/issues/2866)
and [#5049](https://github.com/kuzudb/kuzu/issues/5049) are the architectural origin of the
multi-pair problem, and [#4189](https://github.com/kuzudb/kuzu/issues/4189) is the `[:A|B]` family.

### Memories to read before touching this

`graphit_memory_search` for "icebug", and read in particular:
- *ROOT FOUND: pqarrow opens a NEW row group on every Write…*
- *The `[:A|B]` defect is also OURS: reduced to ONE pair…*
- *PROVEN: multi-hop traversal over icebug is an optimizer bug…*
- *LadybugDB extension: the directory is ~/.lbdb/extension, INSTALL/LOAD are a silent no-op…*
- *The Hub leaves git and goes to S3…* (the Engineer's four decisions)

## Objective

Swap out, in one pass, the **persistence and retrieval** layer of everything the Hub distributes.
Four changes the Engineer asked for in the same pass, and which are interdependent:

1. **The Hub's backend stops being a git repository and becomes an S3 bucket.** Today the
   Hub is a git clone in `~/.graphit/hub` with five distinct responsibilities (registry,
   one orphan branch per artifact/version, `refs/events/*` for telemetry, distribution of rules
   through `main`, and memory worktrees). **All five go to S3** — git leaves the Hub
   entirely.
2. **Text search leaves SQLite and moves to LanceDB**, using what it actually has:
   inverted index (BM25), vector index, hybrid search with RRF and reranking.
3. **The graph stays in LadybugDB, but is no longer persisted as flat Parquet per
   table** — it becomes persisted in **icebug format** (`icebug-disk`), Ladybug's
   graph-lake format.
4. **Querying becomes on-the-fly on both engines.** Installing a Hub context no longer
   downloads any file: Ladybug mounts the icebug tables straight from `s3://` (via the
   `httpfs` extension) and LanceDB opens the table straight from `s3://`. The download at
   install time ceases to exist.

Consequences the request implies and that are in scope:

- **`setup` stops asking for the git repository and starts asking for the bucket** (and the
  region/endpoint).
- **The moment of export to the Hub has to convert the data to icebug** — it is not the
  consumer that converts.
- **No backward compatibility.** We are in dev: no fallback for an artifact published in the
  old format, no migration of an existing store, no read path from git.

### Entry reasoning

What was already known before starting (memory + wiki + the feasibility investigation in
[icebug-remote-graph-on-s3-feasibility.md](icebug-remote-graph-on-s3-feasibility.md), whose
T1/T2 were closed and whose T3–T7 this task closes):

- A context store is **TWO stores**: the graph (LadybugDB, `graph/` bundle) and the search
  index (**SQLite** FTS5+vec0, `search/` bundle). The file text and the embeddings
  live on the SQLite side — without the `search/` bundle the context "can be traversed but not
  searched nor read" (`internal/ast/parquet_transfer.go`).
- Today's Parquet is **per table, flat**, generated by `COPY (MATCH (n:T) RETURN n.*) TO`
  in `internal/ladybugstore/transfer.go`. **It is not CSR and it is not icebug** — it needs conversion.
- Moving text search back to SQLite (2026-08-19) was a deliberate and measured decision:
  liblbug's FTS is not maintained on insert, forcing an O(corpus) DROP+CREATE per write
  (988s for a full rebuild and 1178s for ONE incremental file, against ~300ms on SQLite).
  **This task does not reverse that measurement — it changes the destination.** The measured problem
  was with *liblbug's* FTS, not with SQLite; the reason for leaving SQLite now is a different one
  (remote on-the-fly querying, which SQLite does not do) and the substitute is a different one
  (LanceDB, which does).
- `ATTACH ... (dbtype lbug)` works, but **FTS does not cross the attach** (measured 2026-08-16).
  Irrelevant from here on: the remote graph is not an attach, it is `storage = 's3://...'`, and
  FTS is no longer in Ladybug.

### Design justification, and what was discarded

Four decisions by the Engineer taken in this session, with the alternatives that fell:

| Decision | Discarded alternatives |
|---|---|
| **S3 replaces the whole of git in the Hub** | "artifacts + registry only" and "only the data bundles" were discarded: `setup` asking for a bucket *in place of* the git repo implies nothing is left in git to ask for. |
| **icebug generated by calling `uvx icebug-format`, for now** | A writer in Go discarded *for now* — it becomes a backlog item. Reason: the format is documented but not formally specified, and the official implementation is Python with three backends. The Python/uv dependency is accepted on the machine that **publishes** (not on the one that consumes) so as not to block the rest of the migration on a reverse-engineered writer. |
| **`httpfs` pre-embedded in the launcher, loaded with `LOAD EXTENSION '<path>'`** | `INSTALL httpfs` (network at runtime) discarded: it turns the first remote query into a network call that can fail. The launcher already extracts `liblbug`, ONNX Runtime, ICU and the grammar YAMLs to `~/.graphit/runtime/<version>/` — the extension goes into the same payload, by the same mechanism, and stays 100% offline. |
| **Credentials via the standard AWS chain; the config keeps only bucket/region/endpoint** | Writing `key_id`/`secret` into `~/.graphit/config.json` discarded: it puts a secret in cleartext in a file the rest of the framework reads and logs. The OS keychain discarded for being a new per-platform dependency with a new failure path on headless Linux. |

### What this session's research established (closes T3/T4 of the investigation)

- **`icebug-format` accepts `--source-dir` with a directory of Parquet** per vertex/edge
  table — besides `--source-db <duckdb>` and `--graphar`. This matters because it is
  exactly the shape of what `ladybugstore.ExportTables` already produces. The source of truth
  remains the **local Ladybug store, already populated and consistent**; the Parquet directory is
  an intermediate of the publishing pipeline, not an artifact that travels.
- **icebug-disk output layout**: per table, `nodes_<t>.parquet`,
  `indices_<t>.parquet` (targets ordered by source — the CSR `indices` array),
  `indptr_<t>.parquet` (the row-pointer array), plus a `schema.cypher` in the directory. Each
  Parquet carries `icebug_disk_version` in its metadata.
- **Mount DDL**: `CREATE NODE TABLE t(...) WITH (storage = '<uri>', format = 'icebug-disk')`
  and `CREATE REL TABLE r(FROM a TO b, ...) WITH (storage = '<uri>', format = 'icebug-disk')`.
  `storage` accepts a URI — `s3://`, `gcs://`, `https://` require `httpfs`; `az://` requires `azure`.
- **S3 credentials in Ladybug**: the docs say `CALL s3_credential(key_id=..., secret=..., region=...)`
  and **that is wrong for this engine** — MEASURED: "Catalog exception: function s3_credential
  does not exist". The real form is OPTIONS, one statement each:
  `CALL s3_access_key_id='…'`, `s3_secret_access_key`, `s3_session_token`, `s3_region`,
  `s3_endpoint`, `s3_url_style='path'`. Same form as the documented `CALL http_cache_file=true`.
- **LanceDB has an official Go SDK**: `github.com/lancedb/lancedb-go`, CGO over the Rust core,
  with pre-compiled native binaries for linux/darwin/windows on amd64 and arm64. It covers
  FTS (inverted index + BM25), vector (IVF-PQ, IVF-Flat, HNSW-PQ, HNSW-SQ), scalar
  (BTree, Bitmap, LabelList), **hybrid search with RRF**, reranking, and connecting to `s3://`,
  `az://`, `gs://` and MinIO with `storage_options`.
- **The Hub has no artifact for any of the three** (`graphit_hub_search` for lancedb,
  ladybug and s3 came back empty; `graphit_hub_list` of `knowledge` and `ast` is empty). The
  fallback to the official documentation was declared to the Engineer before being used.

## Plan & Task Breakdown

### Phase A — Foundation (no behavior change yet)

- [x] **T1 — S3 config and credential resolution** — Spec: `internal/config/config.go`.
  New keys `hub.bucket`, `hub.region`, `hub.endpoint`, `hub.prefix`; `hub.repo` goes away.
  A credential is NEVER read nor written by us — it resolves through the standard AWS chain.
  Acceptance: `config.HubBucket()` and friends resolve with the same precedence as the other keys
  (inline > env > project > global > default), and `ResolveHubRepo`/`HubRepoURL` stop
  existing without breaking compilation.
- [x] **T2 — `internal/s3store`: the object layer** — Spec: new package over
  `aws-sdk-go-v2`. Operations: `Get`, `Put`, `Delete`, `List` (with prefix and pagination),
  `Head`, `Exists`, `URI(key)`. Acceptance: tested against a fake in-process S3 server
  (no test touches the real network), and `URI` returns exactly the form that Ladybug and
  LanceDB accept as `storage`.
- [x] **T3 — `setup` asks for a bucket, not a repository** — Spec: `cmd/graphit/commands/setup.go`.
  The `hub.repo` and `memory.repo` prompts go away; bucket, region and endpoint come in. It validates
  with a `HeadBucket` and fails with the same discipline as the model download (error in your face,
  naming what was missing and the credential route). Acceptance: `setup` in an environment without a
  credential explains which variable to set, and does not report success.

### Phase B — The Hub on S3

- [x] **T4 — `internal/hub/s3_store.go` written and tested** (replacing the callers is T5) — Spec: the same surface
  contract the rest of the package already uses (`ReadFile`, `WriteFile`, `RemoveAll`, `ListDir`,
  `EnsureArtifactClone` → URI resolution, `WriteArtifactBranch` → prefix upload,
  `DeleteArtifactBranch` → prefix delete, `WriteEventFile`/`SyncEvents` → ndjson
  objects, `MemoryWorktree` → memory prefix). Key layout defined in
  `docs/specs/hub-s3-object-layout.md` and versioned with a JSON Schema.
  Acceptance: no `exec.Command("git")` is left in the `hub` package.
- [x] **T5 — Rewire whoever uses the GitStore, and `git_store.go` deleted** — Spec: `registry.go`, `service.go`,
  `lifecycle.go`, `event_tracker.go`, `reconcile.go`, `ast_store.go`, `rule.go`,
  `grammar_install.go`, `language_global.go`. Acceptance: `make test` of the package passes without
  any remaining git store test.
- [x] **T6 — Memory leaves git** — Spec: `internal/memory` + `MemoryWorktree`. The memory
  store becomes a prefix in the bucket. Acceptance: `graphit_memory_export` publishes to S3;
  `memory.repo` stops existing. **Design and result in "T6 — the design" below.**

### Phase C — Graph in icebug, queried on-the-fly

- [x] **T7 — `httpfs` in the launcher payload** (the wiring of the icebug mount was done in T9: `ast.MountIcebugGraph`) — Spec: `Makefile` (per-platform
  fetch target, in the mold of `setup-lbug`), `cmd/launcher/` (extraction), and the connection-opening
  point in `internal/ladybugstore/store.go` (`LOAD EXTENSION '<runtime>/httpfs.lbug_extension'`
  + `CALL s3_credential(...)`). Acceptance: a query against `s3://` works with the machine
  offline except for S3 itself, and with nothing in `~/.lbug/extensions`.
- [~] **T8 — native writer in Go: data and 1 hop CORRECT; `[:A|B]` counting FIXED; multi-hop traversal and alternatives with a filtered endpoint blocked UPSTREAM** — Spec:
  `internal/ladybugstore/icebug.go` (native, **no Python in the production path**). Pipeline:
  populated Ladybug store → `ExportIcebug` → upload of the directory to
  `s3://<bucket>/<prefix>/…`. Acceptance REACHED: the published `schema.cypher` mounts on a clean
  Ladybug and answers with the same numbers as the origin; `[:A|B]` exact on the 28 pairs after
  `sortRelsLargestFirst`. **Left open, and it is upstream:** multi-hop traversal, and alternatives
  with one filtered endpoint. See "RESOLVED: `[:A|B]` is TWO defects".
- [x] **T9 — install does NOT download: both types mount** — Spec: `internal/ast/icebug_transfer.go`,
  `internal/hub/ast_store.go`, `internal/hub/mount.go`. Install records the location and runs the
  mount DDL; what comes down is `schema.cypher`, a few KB of metadata. Search mounts by the
  same idea — `search.uri` next to the catalog. Acceptance REACHED: published, mounted and
  queried, with `published bundle holds 7 data files; the local catalog holds 0`, and the search of
  a mounted context answering. The format gaps were **accepted** by the Engineer's decision
  and are declared at the top of `icebug_transfer.go`.

### Phase D — Search in LanceDB, queried on-the-fly

- [x] **T10 — the LanceDB native is COMPILED per platform and goes into the payload** — Spec:
  `Makefile` (`fetch-lancedb`, `lancedb-cgo-env`), `.gitignore`. Acceptance: `make fetch-lancedb`
  produces `liblancedb_go.so`/`.dylib`/`.dll` in `cmd/launcher/runtime/`, and the launcher already
  finds it at runtime without a single new line of code. **See "T10 REDEFINED" and "PROVEN: hybrid".**
  `lancedb-go` is in `go.mod`, and the link contract is declared in the repository in
  `internal/lancestore/cgo_lancedb.go` — not in the environment. The library lives in `.native/`, and
  the release builds it on each OS's runner.
- [x] **T11 — `internal/lancestore`: the search layer** — Spec: new package. Open a local or
  `s3://` connection, create a table, upsert, delete by key, create an FTS/vector/scalar index,
  and the three queries. **Hybrid is the ENGINE's**, not ours. Acceptance: ten tests green, including
  on-the-fly against MinIO. See "T11 DONE" below.
- [x] **T12 — Port of the AST index** — Spec: `internal/ast/search_lance.go` (new);
  `search_sqlite.go` and `search_fusion.go` **deleted**. `search_common.go` survived intact —
  it was written storage-independent on purpose, and that is what made this port a single layer.
  Acceptance REACHED with one premise correction: the 13/16 floor **was measuring tie-breaking**, so it was
  re-derived to a strict 11/11 + 5/5 of recall. See "STEP 2 (T12) DONE".
- [x] **T13 — Port of the wiki index** — Spec: `internal/wiki/store.go` rewritten over
  LanceDB, `store_query.go` deleted, types moved to `types.go`. Acceptance REACHED: knowledge and
  memory search through LanceDB with the chunk model preserved, and **four** tables instead of
  five — `chunk_emb` became a column, so an orphan vector stopped being expressible.
- [x] **T14 — SQLite goes away** — 5,737 lines removed: `mattn/go-sqlite3`,
  `sqlite-vec-go-bindings`, the `fts5` tag, the two guard files and the whole of
  `internal/sqlitestore/`. Acceptance REACHED: `go build ./...` compiles with no tag at all, and no
  sqlite import is left — only historical comments remain, kept because they record the why.

### Phase E — Closing

- [x] **T15 — End to end THROUGH THE CLI** — Run with real MinIO: `init`, `ast index`, `hub submit`,
  `hub install` in a clean project, and querying.

  **IT FOUND THREE DEFECTS NO TEST WAS CATCHING**, all in the layer the tests were bypassing:

  1. **publishing and installing disagreed on the prefix.** Publishing wrote to
     `artifacts/ast/_global/3.0.0/` and install read from `artifacts/ast/t15-demo/3.0.0/`, because the
     mount prefix came from the *context id* instead of the artifact's identity. The tests called the
     two sides with hand-picked arguments, so they **agreed by construction**.
  2. **nobody turned on remote access in the query path.** `LoadExtensions` and `ConfigureS3`
     had existed since T7 and no caller invoked them, so a mounted context resolved the URI and
     reported `No such file or directory` for an object that was there — the engine had no
     filesystem that reached it.
  3. **the HTTP endpoint was unreachable.** The engine prefixes `https://` always (passing the scheme in
     the endpoint is accepted and produces `https://http://localhost:9000/…`), and the option is not
     called anything you would expect: `s3_use_ssl`, `s3_ssl`, `http_use_ssl`, `s3_scheme`, `s3_protocol`,
     `s3_insecure`, `s3_verify_ssl` and `s3_use_tls` **all** return `Invalid option name`. The one that
     exists is **`s3_disable_ssl`**, found by probing and not by documentation.

  Result against real MinIO: `MATCH (n:Entity) RETURN count(n)` → **6**;
  `(a)-[:CALLS]->(b)` → `SyncRegistry` → `evictOldestStaged`; hybrid search in the mounted context →
  **5 results**. **No data downloaded.**

  **Two limitations of the ENVIRONMENT, not of the product:** running the *core* binary outside the
  launcher payload leaves the grammar YAMLs and the `httpfs` extension outside where it looks — they had
  to be copied by hand — and `graphit setup` is interactive, so it hung until it was run with stdin
  closed.
- [x] **T16 — Documentation** — Rewritten where the design changed: `architecture/storage_layout.md`
  (tree, table of who-keeps-what, the consequences of the split), `specs/ast_module.md` (the whole
  Retrieval section, which described seven weighted passes and a prefix pass that do not
  exist), `specs/wiki_module.md` (four tables, and why the absence of `chunk_emb` IS the
  design), `specs/hub_collaboration.md` (Parquet bundle → mount). Plus pointed corrections in nine
  guides and specs, and `git` left the architecture diagram and the memory design.
  **The `docs/changelogs/` were NOT touched**, on purpose: they are dated records of what was
  true then, and rewriting them erases the history of the decision. The mentions left in the
  living docs are all historical — "SQLite managed that with triggers" — and they explain the why.
  ADR: `docs/decisions/2026-08-23-hub-em-s3-icebug-e-lancedb.md`.

## Phase D — T10 REDEFINED: the LanceDB native is compiled with the project (2026-08-22)

T10 was written as "download pre-compiled natives and put them in the launcher payload". Verification
before writing plumbing knocked that premise down, and the Engineer decided the path.

### What the verification found, measured and executed

**1. `lancedb-go` v0.1.2 — the only release — DOES NOT DO text search.** It is not a lack of hybrid: it
is the whole FTS. Proven in execution, not read:

| operation | result |
|---|---|
| create table, insert, count | OK |
| `CREATE FTS INDEX` (inverted index) | **OK — the native creates it** |
| `FullTextSearch` | **error: "Full-text search is not currently supported"** |
| `VectorSearch` | *real* Lance error (Utf8 column, not a vector) ⇒ it is genuinely wired |
| FTS + vector in the same query | took the vector branch and never reached the FTS |

The cause is in the binding's `rust/src/query.rs`: `// placeholder for future implementation`.
The Rust crate `lancedb` it compiles against **has** `full_text_search`, `rerank` and `norm` in the
`QueryBase` trait — the engine does it; the binding was not wiring the thread.

**2. The `main` branch already wired it.** Three commits from April 2026: hybrid vector+FTS in the
`VectorQuery` (#33), RRF reranker exposed in the query config (#32), `create_index_v2` with full tuning (#31).
`query.rs` goes from 226 to 580 lines, with `RRFReranker`, `NormalizeMethod` and the chaining
`vector_query.full_text_search(fts)`. The Go side exposes `QueryConfig{VectorSearch, FTSSearch,
Reranker, Postfilter, WithRowID}` — and the code's own comment says *"automatic RRF on hybrid
nearest_to + full_text_search queries"*. **The fusion is the engine's; no RRF in Go.**

**3. But there is no release with that.** Last release v0.1.2 from **2025-09-30**, eleven months before
those commits. No prerelease, no draft. The published natives contain the stub.

**4. The published artifact has 3 platforms, not 5.** `darwin_amd64`, `darwin_arm64`,
`linux_amd64`. Its `RELEASE_NOTES.md` promises `linux_arm64` and `windows_amd64`; they are not there.

**5. `main` dropped the `cdylib`.** `crate-type = ["staticlib"]` in commit `fa14ce2`
("drop unused cdylib"). The branches that copy `.so`/`.dylib`/`.dll` in `build-native.sh` are dead
code. So the dynamic path requires reactivating `cdylib` — one line of `Cargo.toml`, without touching
code.

### The Engineer's decision

**Compile the native through the Makefile, together with the project, each platform its own — with no
persistent artifact and no single published build.** Discarded: waiting for the upstream release (a
third party's schedule, and without SQLite there is no fallback), and compiling once and publishing to
our bucket.

Accepted consequences:

- **`go.mod` pins the pseudo-version of `main` at SHA `fa14ce29c7724354f2cea630a1d3488b56bbd64b`.**
  It is not a fork: it is upstream with no code patch. The SHA is pinned in `go.mod` AND in the Makefile,
  because Go and native MUST come from the same commit — if they diverge, the FFI breaks at runtime, not
  at compile time.
- **A Rust toolchain becomes a build prerequisite.** New in the project. No cross-compile: each
  platform compiles its own, which is what the decision asks for.
- `lancedb` crate v0.24.0 at the pinned SHA (v0.1.2 used v0.22.1).

### The plan's order CHANGED: T14 goes to the end

The Engineer decided that SQLite goes away entirely. That inverts the safe order: **T11 and the proof
that hybrid works on the real corpus come BEFORE removing SQLite.** Without a fallback, removing
first would leave the framework with no search at all for several tasks.

Legacy to remove in T14, sized: `mattn/go-sqlite3`,
`asg017/sqlite-vec-go-bindings`, the `fts5` build tag (today mandatory in every `go build`/`go
test`), the whole of `internal/sqlitestore/`, `internal/ast/search_sqlite.go` (1,229 lines), the
guard files — and `internal/ast/search_fusion.go`, which loses its caller when the fusion becomes
the engine's.

## END OF PHASE D: the native enters the build, and SQLite goes away (2026-08-23)

> **Name correction.** This section was born christened "PHASE E" and that was wrong: it is the rest of
> **Phase D** (T12–T14). Phase E is the *Closing* — T15 end to end and T16 documentation — and
> is still open. Renamed because a plan with two Phase Es is a plan you cannot read.

The Engineer's decision: "go ahead with all of them" — the four steps, in the order below.

### What motivated the order

The Engineer observed that **the `fts5` tag only exists because of SQLite**. Verified and correct:
the only two occurrences of `//go:build .*fts5` are the guard files
`internal/ast/fts5_required.go` and `internal/wiki/fts5_required.go`, both in packages that import
`mattn/go-sqlite3`. The tag does not turn on Go code — the driver's `sqlite3_opt_fts5.go` has no
code at all, only `#cgo CFLAGS: -DSQLITE_ENABLE_FTS5`. It decides which SQLite gets compiled.

**But the tag does not disappear, it changes places**, and that matters for the design:

| tag | posture today | cost |
|---|---|---|
| `fts5` | **mandatory** (the guards break the build) | one compiler flag in a vendored C |
| `lancedb` | **optional** (`store_disabled.go` returns `ErrNotBuilt`) | Rust toolchain, a `.so` per platform |

After T14 LanceDB is the only search, and there is no Go fallback by explicit decision. So
`store_disabled.go` stops being graceful degradation and becomes a broken installation: a search
that always returns `ErrNotBuilt` is not search. Its role becomes that of today's `fts5_required.go`.
**The guard-file pattern outlives its cause.**

### Why T14 cannot come first

`search_sqlite.go` carries weight right now: it has `RebuildFromCache` (full rebuild) and
`UpdateIncremental` (the delta), called from `json_rebuild.go`, `incremental_rebuild.go` and from two
points in `cmd/graphit/commands/ast.go`. Deleting it before LanceDB has those two methods means being
left with no search halfway through.

### Why step 1 comes before T12

`BUILD_TAGS := fts5` — that's all. The `fetch-lancedb` target is loose and **nothing depends on it**, and
that is why `go test ./internal/lancestore/` answers `[no test files]`: the tests exist and nobody
runs them. Porting the indexing path into a package the suite never exercises is writing
blind.

### The native's contract, measured

- the `lancedb-go` module declares **`CFLAGS: -I${SRCDIR}/../../include`** (the header comes from it) and
  **no `LDFLAGS`** — the library has to be supplied from outside;
- `liblancedb_go.so` exports 50 symbols and depends only on system libs (`libbz2`, `libgcc_s`,
  `libm`, `libc`) — `libbz2` is exactly the one that broke the static link;
- unlike ORT, which is `dlopen` at runtime with the path discovered in Go
  (`findORTLibrary`): LanceDB is cgo, so it is a **link** dependency and also a **loader** one.

### The four steps

| step | what | state |
|---|---|---|
| 1 | native in the default build: `lancedb` in `LOCAL_TAGS`, `test`/`vet`/`lint` depending on it, link resolved without an environment variable | **DONE** |
| 2 (T12) | port `RebuildFromCache` and `UpdateIncremental` to `lancestore`, from the same `ShardCache`, preserving the incremental | **DONE** (the callers still have to be rewired, which is T14) |
| 3 (T13) | wiki and memory search onto `lancestore` (no graph side) | **DONE** in the storage; the callers still have to be rewired (T14) |
| 4 (T14) | delete SQLite: ~4,140 lines, the two guard files, the `fts5` tag and `search_fusion.go` | **DONE** |

### STEP 1 DONE: the link is declared in the repository, not in the environment

**The link contract became code.** `internal/lancestore/cgo_lancedb.go` declares what the module
does not declare, using `${SRCDIR}`, which cgo expands to the file's absolute directory:

```
#cgo LDFLAGS: -L${SRCDIR}/../../.native -llancedb_go
#cgo LDFLAGS: -Wl,-rpath,${SRCDIR}/../../.native
#cgo LDFLAGS: -Wl,-rpath,$ORIGIN
```

**Two `rpath`s, because there are two kinds of binary:** the absolute one serves the test binaries,
which the toolchain links in a temporary directory where nothing sits next to them; `$ORIGIN` serves the
distributed binary, which travels with the library alongside it — that is what keeps the installation
relocatable. Verified in the ELF: `RUNPATH [/…/.native:$ORIGIN]`, and `ldd` resolves.

Measured result: `go test -tags "fts5 lancedb" ./internal/lancestore/` passes **with no
environment variable at all**, and **24 tests nobody was running started running** (15 PASS, 1 SKIP of the
remote S3 one that needs MinIO). Before that the suite answered `[no test files]`.

**The library does NOT live in `cmd/launcher/runtime`.** That directory is the launcher's staging
area and `build-linux` ends with `rm -rf cmd/launcher/runtime/*` — pointing the `rpath` there
means a `make build` silently breaks the next `go test`. It lives in `.native/`,
ignored by git, and `build-linux` copies from it into the package.

**`LOCAL_TAGS` is separate from `BUILD_TAGS`, and that is temporary.** The Rust native does not cross-compile,
so `build-darwin` and `build-windows` remain without the tag. **T14 closes this by force:** without
SQLite, a binary without `lancedb` has no search at all, so the tag has to go into `BUILD_TAGS`
and the release build has to run on each platform instead of cross-compiling from a single one.

**Defect found in the Makefile itself:** `$(shell case "$$(go env GOOS)" in darwin) …)` —
**make does not understand shell syntax**, and the first unbalanced `)` (which is what the legs of a
`case` are made of) closes the function and truncates the value in silence. The result was a path
`.native/ echo liblancedb_go.dylib ;; …`, which never exists, so the guard rebuilt the native on
every invocation. Swapped for make conditionals.

**Limitation measured, not hidden:** the *rebuild* path (`make fetch-lancedb`) could not
be verified on this machine — `~/.rustup` exists but has no toolchain, and there is no `cargo`. What
is verified is the link, the guard, the tag and the tests, because the already-built library is in
`.native/`. On a clean machine `fetch-lancedb` still needs a real pass.

### STEP 2 (T12) DONE: `internal/ast/search_lance.go`, the two write paths

10 tests, all against the real engine and a real `ShardCache` — no stub, because the point of the port
is that the engine does the search, so a fake would test exactly the part that is going to be deleted.

**What stayed the same as the SQLite index, on purpose:** two tables (`files` and `entities`,
because matching a file and matching an entity are different answers and ranking everything together buries the
entities); the row built **in a single place** for both write paths; and the indexes
created **after** the bulk load.

**What changed because the engine is a different one:**

- **no integer ids.** SQLite needed them to tie the external-content FTS tables to the
  content; here the uid is the key and nothing needs numbering;
- **no triggers.** Lance keeps freshly appended rows findable by scanning the fragments that
  are not yet in the index, so the incremental does not maintain a per-row index — it folds afterwards,
  once, for latency;
- **no separate vector table and no compacting of dead vectors.** The embedding is a column of the entity,
  so deleting the entity deletes the vector, and the whole class of bug where an obsolete vector answers
  for an entity that no longer exists **stops being expressible**;
- **one text column, not seven.** SQLite queried seven weighted fields (name split into
  10.0, docstring at 3.0, type at 2.0, path at 1.0, and three from the file) and fused the passes in
  Go. That does not port to an engine whose text query takes one column, and redoing the fusion in
  Go would be exactly the search-in-Go this project discarded. The fields become one document and
  BM25 ranks — it already weights by term rarity, which is what the manual weights approximated.

**The historical defect became inexpressible.** The SQLite rebuild's `INSERT` wrote `name_tri` and
the incremental's did **not**, so every file touched by an incremental silently lost trigram recall
until the next full rebuild. Now both paths call `buildEntityRow`, and
`TestLanceBothWritePathsProduceTheSameDocument` compares the documents the two produce instead
of trusting that they are the same.

#### Three defects found by measurement, not by reading

1. **`FoldNewRowsIntoIndexes` is latency, not correctness — and I had written the opposite.** I
   documented that a row appended after the inverted index was built stays invisible to
   text search until the fold. **False:** `TestFoldIsAboutLatencyNotVisibility` appends a row with
   a term that exists in no other one and finds it **before** any fold — the engine scans
   the unindexed fragments together with the index. Had I believed the intuition, the design
   would have a mandatory fold before every read. The comment was corrected to what was
   measured, and the probe stayed for the day the engine changes that.
2. **IVF-PQ requires 256 rows to train, and the failure took down the whole rebuild.** Measured:
   `Unprocessable: Not enough rows to train PQ. Requires 256 rows but only 2 available`. In other words:
   **a project with fewer than 256 indexed entities could not build any search index at
   all** — a new repository, a small service, almost every test fixture. Below the floor the
   vector index is skipped and semantic search keeps answering by scan, which at that
   size is what an index would degenerate into anyway.
3. **`DropTable` on a missing table was an error.** A rebuild against a new store failed on the
   very first run with `Table 'files' was not found`, which reads as corruption and not as an empty
   store. Dropping what does not exist is a no-op: the caller's intent is "this table must not
   exist".

Also added: a **filter-only** query (`Mode() == "filter"`), which is how you read a
row known by key — and it is how a test asserts what was actually written instead of what the
builder intended to write.

### STEP 3 (T13) DONE in the storage: `internal/wiki/store_lance.go`

12 tests against the real engine. **It serves memory too**, with no extra work: `internal/memory`
uses the same `wiki.WikiDB` (via `consolidate_similarity.go`), so one store serves both.

**Scope discovery: `WikiDB` was not only search.** It keeps five tables — `chunks`,
`chunk_emb`, `xrefs`, `sync_log` and `wiki_meta` — plus browse and embedding accounting. Since
SQLite goes away entirely, all of that had to go, not just the search.

**The xrefs looked like they needed a graph and did not.** `FindXRefs` does the BFS traversal **in
Go**, with one-hop lookups; what it asks of the storage is a filterable table of pairs, which
is the easiest case for a column store. The shape stayed identical to SQLite's on purpose — it was not
the part that needed to change.

**Four tables instead of five:** `chunk_emb` became a column of `chunks`, for the same reason as the
AST index — an embedding that lives next to its chunk cannot outlive it, so the failure
where an obsolete vector answers for a page that no longer exists stops being expressible.

**The sync log is the only table that survives a rebuild**, because it is the history *of* the
rebuilds: clearing it on every rebuild would leave it permanently with one entry, which reads as
"this wiki only synced once". Pinned by `TestLanceWikiSyncLogSurvivesARebuild`.

#### One defect that would have hit every caller

**A vector written as `[]float32` does NOT come back as `[]float32`.** The Arrow→Go bridge returns a list of
fixed size as `[]interface{}` of `float64`, so `v.([]float32)` fails — and the two-value
form of the type assertion **does not error, it returns nil**.

Real measured symptom: `StoredEmbeddings` returned an empty list while `EmbeddingStats` counted the
**same rows** as embedded — because one asked the engine and the other asked Go. Fixed in
`Table.normalizeRead`, which is the only layer that knows the schema, and pinned by
`TestVectorColumnRoundTripsAsFloat32`. Converting at the point of use would be the same error repeated in
every caller.

Also: the `slug` may contain an apostrophe (`what's-new`), and unescaped quotes in a filter **do not
fail — they change which rows match**. `lanceQuote` doubles the quote, with a test.

### STEP 4 (T14) DONE: SQLite went away entirely

**5,705 lines removed, 2,225 written, 83 files.** The whole suite green, and `go build ./...`
works with and without the tag.

Deleted: `internal/ast/search_sqlite.go` (1,229), `search_fusion.go` (331),
`internal/wiki/store.go` (954), `store_query.go` (860), `internal/sqlitestore/` (766), the two
guard files `fts5_required.go`, `premigration_db_test.go`, and the dependencies
`mattn/go-sqlite3` and `sqlite-vec-go-bindings` from `go.mod`. `BUILD_TAGS` went from `fts5` to
`lancedb`.

**The types were not SQLite's.** `WikiChunk`, `WikiSearchResult`, `XRefResult`, `SyncLogEntry` and
company went to `internal/wiki/types.go` — they describe what a wiki **is**, and nothing in them
was engine-specific, which is why the engine could be swapped without a single caller changing
shape.

**Renamed to take the engine out of the API:** `LanceSearchIndex` → `SearchIndex`,
`LanceWikiDB` → `WikiDB`. There is only one of each now, so "Lance" in the name was an
implementation detail leaking outward.

#### Publishing stopped converting, and installing stopped rebuilding

The Hub artifact carried the search tables exported as Parquet, and **every consumer
rebuilt the inverted and vector indexes** — engine structure did not travel. A Lance directory
carries its own, so publishing is copying and installing is copying. `parquet_transfer.go` and
`wiki/transfer.go` were rewritten for that, and the round-trip test got **stronger**: a
working search on the other side now proves the copied structure is usable, where before a rebuild
would mask whatever arrived broken.

#### Five defects the tests caught, all mine

1. **`EachFileSource` over a missing index walked empty instead of erroring.** `OpenSearchIndex`
   **creates** what it opens, so a store with no index walked cleanly over zero files — and a caller
   writing an artifact would publish it as complete. SQLite got that for free from a read-only open
   that failed; a store in a directory has to be asked.
2. **`chunkRow.ID` was the loop index.** It looked like a stable identifier and was not: embed a
   chunk and the next call hands that same number to another chunk. The field was **removed** —
   the slug is the identity. A test caught it.
3. **An empty query returned an error instead of nothing.** `"  "` is a question with no content, not a
   malformed request, and showing the user an error for that is reporting a failure that does not exist.
4. **A leftover SQLite `wiki.db` would be indexed as a source document.** Nothing deletes that file
   (this project does no migration), so `IsDerivedFile` keeps naming it **on purpose** —
   without that the wiki would index its own old database.
5. **A build without the engine answered silence.** `NewQueryService` swallowed the open error and the
   searches returned `nil, nil` — indistinguishable from a correct empty result, which is exactly the
   trap the `fts5` guard file existed to close. Now the reason is **reported**.

#### The guard file did NOT come back, and that is a decision

By my own earlier analysis, `lancedb` becoming mandatory would call for a guard like the `fts5`
one. It was not done, for two measured reasons: `ErrNotBuilt` **already names the tag and the fix**
("run `make fetch-lancedb` and build with -tags lancedb"), which is precisely what
`no such module: fts5` did not do; and keeping `go build ./...` working matters more now that the
native requires a Rust toolchain instead of a compiler flag. What **was** fixed is the part
that actually mattered — the failure being loud instead of silent.

#### Two quality gates re-derived, not lowered

`TestSearchIndexQualityFloor` measured **13/16** and dropped to 11/16. It is the same finding from this
migration, reapplied: five of the sixteen probes have no single defensible answer by the rule the project
itself wrote — `config` returning an entity literally called `Config` is *more*
defensible than `configLoader`. They became **recall** probes, and the strict floor is the eleven that
have one answer. Result: **11/11 strict and 5/5 recall** — identical to what the re-derivation in
`lancestore` gave, by an independent path.

`TestTruncatedQueryCoverage`: `valida` matches `PKG_VALIDACAO_PAGAMENTO` and `SchemaValidator`
comparably. The test **already excluded** `valid` and `db` for that exact reason; `valida` entered the
same category when the prefix pass went away. 8/8 strict, recall reaches at position 2.

`TestExpansionFieldCeiling` said it guarded "the prefix index", which no longer exists. It now
asserts the **conclusion** it established — an expansion field buys at most one probe, and
only with a lucky guess — instead of a deleted mechanism.

#### One thing that is NOT a regression

`internal/ast` gave `signal: segmentation fault` **intermittently** (it passed on the next
run, with no change). It is the buffer pool backlog item: this machine has 41 of 61 GB in use and
~19 available, and LadybugDB's write ceiling is 8 GiB per handle with no coordination between
processes. A second native in the process makes the peak higher, so the migration **aggravates** the
known problem without being its cause.

## MEASURED with real inference: the reranker is `bge-reranker-base` (MIT), and it stays OPT-IN (2026-08-23)

The Engineer asked for the measurement with real calls. It was done, and it changed two decisions:
the model and the very arithmetic that was measuring.

### Jina fell on license, not on quality

`jina-reranker-v2-base-multilingual` is **`cc-by-nc-4.0` — NON-COMMERCIAL**. For a commercial
product that is a blocker, and no benchmark number removes it. The previous choice was made
reading the benchmark table and not the license; the license is a requirement, not a footnote.

### The two candidates with a clean license, measured and not argued

Real inference, ONNX Runtime, 16 natural-language questions over 24 entities from **this**
repository with the real docstrings (`internal/ai/rerank_eval_test.go`, tag `rerankeval`):

| model | license | size | top-1 | MRR | nDCG@10 | per query |
|---|---|---|---|---|---|---|
| **`bge-reranker-base`** | **MIT** | 1.04 GiB | 12→**13**/16 | 0.833→**0.865** | 0.860→**0.883** | 720 ms |
| `ms-marco-MiniLM-L-6-v2` | Apache-2.0 | 87 MiB | 12→12/16 | 0.833→**0.828** | 0.860→**0.856** | 92 ms |

**ms-marco MAKES the ranking WORSE.** It is a tenth of the size and eight times faster, and even
so it comes last — it is trained on passage prose, and an identifier with a docstring is not
a passage. That result is exactly why the decision moved from a table of parameters to
measurement: by size and by latency, ms-marco was the obvious choice.

The two queries that move are the same in both models, and only the direction changes:

- `quoteIdent` ("why did my delete not remove a row and not error"): 3 → **1** in bge, 3 → 2 in ms-marco.
- `sanitizeUTF8`: 2 → 3 in bge, 2 → **4** in ms-marco.

### The arithmetic was unfair to the reranker, and in the direction that favored it looking worse

The first bge result came out with `improved 1, worsened 2`. Investigating: one answer
(`evictOldestStaged`) was at **rank 24 of 24** in the lexical stage. The baseline was measured over the whole
corpus and the reranked one over the window of 10 candidates — so the baseline got 1/24 of credit and the
reranked one got 0, **not because the reranker demoted the document, but because it never saw that
document**. A first-stage recall failure charged to the second.

Fixed: both sides are measured on the SAME window, and the window's cost is reported separately
(`first-stage miss: 1/16`). With the right arithmetic, `improved 1, worsened 1` in both models.

### The finding worth more than the reranker

**14 of 16 queries do not move.** The largest measured hole is not ordering — it is the answer that
is outside the candidate window, which no reranking reaches. Widening the window is cheaper
than 1 GiB of model and 720 ms per query.

### Decision: it goes in as is — opt-in, default `false`

+0.032 of MRR on a set of 16 questions is **one query changing places**. That does not pay for
1.04 GiB of download and 720 ms per query for everybody, and that is why the default stays `false`,
which is what was already built. The model becomes bge, with an MIT license.

Honest limits of this number: 16 questions, 24 documents, one right answer per question, and a
TF-IDF baseline instead of LanceDB's real hybrid. It is directional, not a verdict.

### Code fix the measurement forced: the inputs are DISCOVERED

The first real `Run` failed with `Missing Input: token_type_ids` inside a Gather node. The
ms-marco is BERT and **requires** `token_type_ids` — it is how it separates the question segment from
the document segment. bge is XLM-RoBERTa and **does not have** that input. A fixed pair of inputs
works for one and breaks for the other.

`newCrossEncoderFrom` now calls `ort.GetInputOutputInfo(modelPath)`, assembles the tensors in the order
the model declared, and feeds `token_type_ids` from `Encoding.TypeIds` only when the
model asks for it. Both architectures ran — that is the proof.

## HISTORICAL (license blocked it, see the section above): the initial choice of Jina (2026-08-23)

### Wiring ready, OPT-IN, default false

The Engineer's decision: use Jina, and the cross-encoder stays opt-in with default `false`.

### Why Jina, and why not the one from our own family

The research laid out the candidates with sizes:

| model | size | note |
|---|---|---|
| `ms-marco-MiniLM-L-6-v2` | 80 MB | ONNX ready, ~60 ms for top-100→top-20, but trained on **prose** |
| `jina-reranker-v1-tiny-en` | 130 MB | fast, English only |
| **`jina-reranker-v2-base-multilingual`** | ~1.1 GB | **the only small one with a published code-retrieval benchmark** |
| `bge-reranker-base` | 1.04 GB | strong on text, no code focus |
| `CodeRankLLM` | **7B (~4 GB)** | companion to our `coderankembed`, but **listwise by LLM** — an unfeasible size class |

The one from our own family is conceptually the right one and the wrong one on size: `CodeRankEmbed` has
137M and the reranker that accompanies it has 7B, because their reranking is listwise by LLM.

### Why default `false`, and it is measurement and not caution

- **It costs a second model**: 1.1 GB against the ~132 MB of the retrieval embedder.
- **It costs inference ON THE QUERY PATH.** An embedding is computed once at indexing time and cached
  by shard hash; a cross-encoder runs per query, over every candidate.
- **And the gate it would justify itself against is SATURATED**: 11/11 strict and 5/5 recall without
  it. Turning it on by default would be repeating good practice as a formula instead of applying it.

### What got built

`lancestore.Reranker` — a two-function interface (`Rerank`, `Name`), pluggable, `nil` by default.
`RerankConfig` in `Query` turns the stage on.

Three behaviors the tests pin, and each one is a decision:

1. **The first stage WIDENS** when rerank is on (`CandidateLimit`, default 50): a
   cross-encoder does not promote what retrieval did not return, so recall is retrieval's problem
   and not the reranker's. The result is trimmed back to the caller's `Limit`.
2. **A reranker failure DEGRADES to the engine's order**, and the error goes back to the caller along with the
   results. Losing all the results because a second-stage model did not load is worse
   than losing the reordering.
3. **A reranker that returns a different set is refused.** The safe reading is to distrust the
   reordering, not to serve a truncated answer as if it were ranked.

### The ONNX client, implemented

`internal/ai/rerank_local.go` — `CrossEncoderReranker`, on the path already trodden by the embedder:
`sugarme/tokenizer` for the `tokenizer.json`, `ort.NewDynamicAdvancedSession` for the `model.onnx`,
and the runtime initialization **reusing the embedder's `initONNXRuntime`** instead of a second
initializer, so the library path is resolved in a single place.

What differs from the embedder, and it is the part that matters: a bi-encoder reads one text and returns a vector, with
the similarity computed afterwards; **a cross-encoder reads the query AND the candidate together** and returns a
score. That is why it is better and why it is expensive — **you cannot pre-compute it nor cache it by content
hash, because the score belongs to the pair, not to the document.** So there is no pooling and no
L2 normalization; there is one logit per pair.

Details that are not obvious and are pinned:

- **The pair goes through the tokenizer's `EncodePair`**, not through string concatenation — it is what inserts
  the separator the model was trained with. Getting that wrong **does not error**: it produces a plausible score
  that ranks badly.
- **Batch of 16 and sequence of 512.** The batch exists because this runs on the query path: it keeps
  peak memory flat and lets a cancelled context take effect between batches.
- **A candidate that does not tokenize scores `-Inf`** instead of taking down the batch — a malformed document
  cannot cost the result set.
- **The output width is read, not assumed** (`len(data)/len(batch)`), so a model with a two-class
  head also scores correctly.
- **Panic recovery in the tokenizer**, with the same guard as the embedder — it panics instead
  of returning an error for certain inputs.

### THE GRAM BAG DOES NOT GO TO THE MODEL, and that was the trap

`internal/ai/rerank_adapter.go` assembles the candidate's text with identifier, split form,
type, docstring and path — **and not with the indexed column**. The bag of grams exists so BM25 can
match truncation; for a transformer trained on language it is hundreds of three-letter tokens that
smother the sentence and consume the sequence budget. Passing the indexed column straight through was
the obvious thing and would have been wrong. Pinned by `TestBuildRerankTextCarriesLanguageAndNotGrams`.

The adapter also lives in `internal/ai` and not in `internal/lancestore`, so the search package
does not gain a dependency on a model, a tokenizer or ONNX: `lancestore` declares the two-method
interface and `ai` satisfies it.

### THE DOWNLOAD IS LAZY AND GATED, which was the explicit request

`internal/ai/rerank_model.go` — `RerankModelManager`, deliberately a type separate from
`ModelManager` and not a mode of it, because they differ in exactly the thing that matters here: **when
they are allowed to touch the network.** `ModelManager` is called by `setup` and by the indexing
path; this one only after someone opts into rerank.

| entry point | behavior |
|---|---|
| `NewRerankModelManager()` | **does not touch the network, does not create a directory** |
| `Present()` | answers from disk, with a **size check** — a 16-byte HTML error page does not pass as a model |
| `NewCrossEncoderRerankerIfPresent()` | returns `(nil, nil)` if the model is not there: "no rerank", not an error and not a download |
| `NewCrossEncoderReranker(ctx)` | **this is the commitment** — it downloads if missing |
| `search.rerank` (config) | **default `false`**, and it is what gates everything above |

`graphit setup` does not touch this manager. Whoever leaves `search.rerank` off **never** pays the
280 MB — not at setup, not on the first query, not ever.

### Tests

`internal/ai`: nine — reorders without discarding anything, determinism on a tie, refuses a divergent
score count, degrades on scorer failure, the gram bag does not reach the model, a redundant split is
omitted, and **three about the download gate** (asking does not create a directory, `IfPresent` does not download,
a truncated bundle is refused).

`internal/lancestore`: four — off by default, widening with trimming, degradation to the engine's
order, refusal of an altered set.

`internal/config`: `search.rerank` is false by absence, and only `"true"` turns it on.

**And before turning it on by default, the evaluation set has to have slack.** The current one passes at 100%,
so it shows neither gain nor harm. Measuring over 19 synthetic entities does not decide 1.1 GB — it is
in the backlog, along with what the new set needs to have.

## ENGINE FIRST: the tokenizer is LanceDB's, and the gap that remains has a name (2026-08-23)

The Engineer's instruction: *"always prefer what the lancedb engine provides, it has priority
over go, for anything"*. First consequence: **I was not exposing the tokenizer** —
`lancestore.Index` had only column and type. Now it has `TextIndexOptions` with everything the engine
offers: `Language`, `Stem`, `RemoveStopWords`, `LowerCase`, `ASCIIFolding`, `WithPosition`,
`BaseTokenizer`, `NgramMin/Max`, `NgramPrefixOnly`, `MaxTokenLength`.

### The sweep, over the re-derived gate

| configuration | strict | recall@5 | empties |
|---|---|---|---|
| **expansion in Go, default tokenizer** | **11/11** | **5/5** | 0 |
| expansion in Go + engine `ngram` | 10/11 | 5/5 | 0 |
| engine `ngram` 2–4, with and without ascii | 10/11 | 4/5 | 0 |
| `ngram` 2–5 +ascii | 10/11 | 4/5 | 0 |
| `ngram` 2–4 `prefix_only` | 6/11 | 2/5 | **3** |
| default / `stem`+ascii | 6/11 | 3/5 | **4** |

### THE GAP, with a name — and it is what authorizes the exception

**The engine's ngram mode REPLACES word tokenization instead of adding to it.** Turning it on
buys substring matching and **loses** whole-token matching, so a query that is a
complete identifier can no longer beat a partial one — that is exactly why every line
with ngram loses one strict probe. **There is no token FILTER that emits sub-token grams ALONGSIDE
the words; there is only a different base tokenizer.**

So the grams are emitted into the document at write time and the engine **keeps its word
tokenizer**, indexing words and grams as ordinary terms. That is not a second search
implementation — **no ranking happens in Go** — it is the document carrying what the
tokenizer does not produce without giving something up.

And combining the two is **measurably worse** (10/11): the ngram tokenizer re-grams the bag of grams,
flooding the term space and diluting the signal.

**Trigger for deleting the exception:** if the engine gains an ngram token filter that composes with the
word tokenizer, run the sweep again — the engine-only line should reach 11/11 and the
expansion goes away. It is written in the comment of `chosenTuning`.

## THE QUALITY FLOOR WAS MEASURING TIE-BREAKING: 13/16 re-derived to 11/11 + 5/5 (2026-08-23)

The Engineer doubted the expected values — *"I don't even know whether the values that are expected today
make sense"* — and the doubt was right. **Five of the sixteen probes have no defensible
answer, and the project itself had already written the rule that disqualifies them.**

From `internal/ast/truncated_query_test.go`, about the `valid` probe:

> *"`valid` is deliberately absent: it is a prefix of both `validate` and `validacao`, so whichever
> of validateSchema and PKG_VALIDACAO_PAGAMENTO wins is tie-breaking, not coverage.
> **A probe with no defensible answer measures nothing.**"*

**And the floor test includes `{"valid", "validateSchema"}`.** Applying the rule consistently,
five fall:

| probe | why it is not defensible |
|---|---|
| `valid` | the case the project excluded — and included in the floor |
| `valida` | same: prefix of `validate` AND of `validacao` |
| `config` | `Config` is the entity literally called that; preferring `configLoader` is arbitrary |
| `schema` | `validateSchema` and `SchemaValidator` both carry it in the name |
| `configuration` | seven entities carry it, and `initConfiguration` carries it **in the name** |

**They were exactly the five that the engine-only design failed.** Without them, the design gets everything right.

### The re-derived gate, and the result

Eleven probes with one defensible answer stay **strict top-1**; the five ambiguous ones become
**recall** — the named entity has to be reached. The window is 5, and the number was not chosen
to pass: it is the window the old gate already used (`si.Search(c.query, 5)`).

```
strict top-1: 11/11    recall@5: 5/5    vazios: 0
```

Ranking entirely from the engine. The only work in Go is expanding the document and the query at
write and read time — identifier split and a bag of 2/3-grams — which is pre-processing, in the
same category as lowercasing, not ranking.

### What that resolves, and what remains as debt

**It resolves the cross-encoder question: it is not necessary for parity.** I was going to build a
reranker in Go to recover two points that did not exist — they were tie-breaking. The cross-encoder
remains LanceDB's path to quality *above* parity, and the Go binding only exposes
RRF, so it is recorded as a measured option and not as a necessity.

**The per-field weight debt remains.** The engine does not have it: a multi-column FTS index answers
`"Multi-column (composite) indices are not yet supported"`, the query does not name a field, and there is no
boost. Two attempts to work around it at write time were measured and **failed**: repeating the
field moves nothing (BM25 saturates frequency and normalizes by length, so lengthening the document
cancels the gain), and encoding the priority in the token (`zn`/`zs`, counting on IDF) did not either.
When the composite index lands upstream, it is worth re-measuring.

### The native tokenizer, measured

| configuration | top-1 (old set, 16) | empties |
|---|---|---|
| default | 8 | 4 |
| `stem=English` + ascii folding | 8 | 4 |
| `ngram` 2–4 | 10 | 0 |
| `ngram` 2–4 `prefix_only` | 7 | 3 |
| `ngram` 3–6 | 9 | 1 |
| **2/3-gram expansion in Go, default tokenizer** | **11** | **0** |

`prefix_only` **makes it worse** and reintroduces empties. And the docs warn about the cost of ngram: many more tokens
per document, larger index and memory. So write-time expansion beats the native
tokenizer **and** is cheaper in the index.

### Embeddings: nothing changed, and it should not

`lancestore` receives a ready vector and **does not call the LLM**. Generation remains
ONNX + `coderankembed` → `Embedder` → `ShardEmbCache` (keyed by `(relPath, uid, hash)`), with
the daemon keeping the model alive behind `embed.sock`. Two reasons not to delegate to the database: LanceDB's
"embedding functions" feature is Python and would fetch a model of its own instead of the one we already
embed; and the shard-hash cache is what makes the incremental worthwhile — delegating would re-embed the
corpus on every write.

**A note on the old number:** the floor test runs with a nil `embLookup`, so 13/16 was
**text only, no vector**. The measurements above are under the same condition, which makes them comparable —
but it means neither of the two numbers measures the full hybrid.

## THE ARCHITECTURE, as stated by the Engineer (2026-08-23)

Worth writing down because it decides T12 and T13, and because it has an asymmetry that is not obvious.

**AST — TWO databases:** Ladybug for the graph, LanceDB for the hybrid search.

```
local:     shards  ->  POPULATE BOTH databases, incrementally when something changes
export:    both databases PREPARE and EXPORT  ->  Hub persists to S3
consumer:  queries both on-the-fly at s3://
```

**Wikis (knowledge and memory) — ONE database:** only LanceDB, hybrid search. **There is no graph**, so there is
no Ladybug on that path.

### The asymmetry: only the graph needs conversion

| | local format | published format | conversion? |
|---|---|---|---|
| graph (AST) | native LadybugDB | **icebug-disk** in S3 | **yes** — T8, and it is where all the defects are |
| search (AST and wikis) | local Lance table | **Lance table** in S3 | **no** |

Lance's native format **is already queryable on-the-fly** straight from `s3://` — proven. So the
"export" on the search side is not a format conversion: it is writing the table to the S3 prefix, which
`lancestore.Store` already does by opening with the `s3://` URI and writing once. That is a big
difference in effort compared to the graph side, and it explains why Phase C cost what it cost and
Phase D should not cost the same.

### What that determines

- **T12** (AST index) ports `RebuildFromCache` and `UpdateIncremental` to populate the Lance
  table from the SAME `ShardCache` that today feeds SQLite, keeping the incremental path.
- **T13** (wikis) uses the same `internal/lancestore`, with no graph side.
- The search side's `ExportTables`/`ImportTables` stops being a file bundle and becomes
  "write the table to the published prefix".

## T11 DONE: `internal/lancestore`, and the two-mode architecture (2026-08-23)

The Engineer clarified the flow, and it determines the package's design:

```
local:      projeto -> banco populado (Ladybug nativo + índice de busca local)
publicação: banco populado -> EXTRAÇÃO -> Hub (icebug em S3 + tabela LanceDB em S3)
consumidor: instala contexto -> consulta s3:// on-the-fly, sem baixar
```

**The two modes are NOT symmetric**, and the package encodes that: local is where writes
happen in normal operation (it is what replaces SQLite); remote is a published version, written
ONCE by the publisher and only read by the consumers. `Store.Remote()` reports the mode and every
write on a remote store returns `ErrReadOnly` — a consumer that could write would fork the
artifact the registry names.

### What the package exposes

`Open(ctx, Config)` decides the mode by the URI's scheme. `Config.S3` carries region and endpoint and
**has no credential field** — the standard AWS chain resolves it, the same rule as
`internal/s3store`. The two derivations a compatible server requires come from there: a custom
endpoint implies **path-style**, and an `http://` endpoint requires `allow_http`.

Surface: `CreateTable`/`OpenTable`/`EnsureTable`/`DropTable`, `Append`, `Upsert`,
`DeleteByKey`, `DeleteWhere`, `EnsureIndexes`, `Search`. The search mode is decided by what is
filled in, not by a flag: `Text` alone is FTS, `Vector` alone is semantic, **both is hybrid**
— and then the fusion is the engine's.

### Arrow stops traveling here

`lancedb-go` compiles against `apache/arrow/go/v17`; the rest of the project uses `apache/arrow-go/v18`.
They coexist because they are different module paths, and **they never meet**: rows go in as
Go values and results come out as Go values. No v17↔v18 conversion anywhere.

### Build tag, so the tree keeps compiling

The native is ~230 MiB and is compiled from source. Requiring it on every `go build ./...` would break the tree
for anyone who has not run `make fetch-lancedb`. So the package has two halves — `store_lancedb.go`
(`//go:build lancedb`) and `store_disabled.go` (`//go:build !lancedb`) — with the SAME surface; the
second returns `ErrNotBuilt` and `Available()` reports `false`, so a caller degrades
deliberately instead of finding out through an error. `go.mod` pins the same SHA as the Makefile.

### THE SILENT DEFECT THIS FOUND, and it is data corruption

The filter dialect treats a name **in double quotes as a STRING LITERAL**, not as an
identifier. Measured:

```
"uid" IN ('u2')   3 rows -> 3 rows   err=<nil>   <- deletes NOTHING, no error
uid   IN ('u2')   3 rows -> 2 rows   err=<nil>
`uid` IN ('u2')   3 rows -> 2 rows   err=<nil>
```

The predicate it actually evaluates is `'uid' IN ('u2')`, false for every row. And since `Upsert`
is delete-then-append, **a delete that matches nothing leaves the old row and adds the new one**: every
incremental reindex would silently duplicate the index. `quoteIdent` uses a backtick, and
`TestIdentifierQuotingActuallyMatchesRows` pins that — including asserting that the double-quoted
form is still the trap, so nobody "fixes" it back to standard SQL.

### Ten tests, and what each group guarantees

| test | what it guarantees |
|---|---|
| `TestLocalStoreServesAllThreeSearches` | FTS, semantic and **hybrid** in local mode; the hybrid assertion is the **reordering** (the BM25 winner promoted above the vector winner) and that the vector winner **stays in the set** — a hybrid that discarded it would be FTS under another name |
| `TestRemoteStoreIsQueriedOnTheFly` | **on-the-fly against MinIO**: publishes, reopens as a consumer, recovers the schema from the table itself (no manifest), runs a remote hybrid, and **refuses a write** |
| `TestFilterIsAppliedByTheEngine` | the filter goes to the engine, otherwise the caller would post-filter and lose the ranking |
| `TestUpsertReplacesByKey` | upsert replaces instead of accumulating — which is what caught the quoting defect |
| `TestDeleteByKeyQuotesSafely` | a key with an apostrophe (`it's/odd.go`) does not terminate the literal |
| `TestIdentifierQuotingActuallyMatchesRows` | the guard against the silent corruption |
| `TestDeleteWhereRefusesAnEmptyFilter` | emptying the index has to be asked for explicitly |
| `TestSchemaValidationNamesTheColumn` | an impossible schema is refused with the column named, not as an Arrow error three layers below |
| `TestAppendRefusesAMissingRequiredColumn` | same for a row without a required column |
| `TestStorageKeysMatchTheVendor` | the `object_store` keys that `config.go` writes as literals stay identical to the vendor's |

`go build -tags fts5 ./...` and the whole suite remain green **without** the tag; with the tag, the ten
pass against MinIO.

## PROVEN: LanceDB's hybrid search works in Go, with the engine's RRF (2026-08-22)

Phase D's premise stopped being an assumption. The native was compiled from the pinned SHA and run:

```
FTS só    q="fusion rankings"        -> id=3  reciprocal rank fusion combines rankings
Vetor só  v=[0,0.95,0.05,0]          -> id=2, id=3, id=6
HYBRID    vetor + fts, RRF reranker  -> id=3, id=2, id=6
```

**The evidence is the reordering.** The vector winner was id=2; BM25's was id=3. In the hybrid,
id=3 rises to the top — which is the behavior of reciprocal rank fusion: 1st in FTS and 2nd in vector beats
1st in vector and absent from FTS. **The fusion is the engine's.** No RRF in Go.

Shape of the call, for reference:

```go
QueryConfig{
    VectorSearch: &VectorSearch{
        Column: "embedding", Vector: v, K: 3,
        FullTextQuery: "fusion rankings", FullTextColumn: "text",  // <- vira hybrid
    },
    Reranker: &RerankerConfig{Kind: RerankerRRF, RRFK: 60},
}
```

### The three pins the build requires, and why each one

1. **The commit SHA**, in `go.mod` AND in the Makefile: `fa14ce29c7724354f2cea630a1d3488b56bbd64b`
   (pseudo-version `v0.1.3-0.20260509194607-fa14ce29c772`). Go and native MUST come from the same commit;
   divergence breaks the FFI at runtime, not at compile time.
2. **The `rustc` version.** Upstream does not pin a toolchain, and the committed `Cargo.lock` pins
   `ethnum 1.5.2`, which **does not compile** on rustc 1.98:
   `E0512: cannot transmute between types of different sizes` (`()` of 0 bits to
   `TryFromIntError` of 8). `ethnum` comes from `jsonb`, which comes from `lance-arrow`/`lance-datafusion`/
   `lance-index` — three levels below what we chose. Fixed with
   `cargo update -p ethnum --precise 1.5.3`, two lines of delta in the lockfile that become
   a patch of ours.
3. **The system libraries of the static link.** A Rust staticlib does **not** carry its transitive
   C dependencies. Discovering them is canonical, not trial and error:

   ```
   cargo rustc --release -- --print native-static-libs
   note: native-static-libs: -lbz2 -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc
   ```

   Without `-lbz2` the link fails with `undefined reference to BZ2_bzDecompress`. **This changes the
   static/dynamic comparison**: static transfers to us the responsibility of getting that list right per
   platform (on macOS it is `-framework Security -framework CoreFoundation`); the `cdylib` would resolve
   inside the `.so` and the consumer would need nothing. The list is stable and discoverable, so it is
   manageable — but it is one more piece to maintain per platform.

### Measured numbers

| | |
|---|---|
| native build, from scratch | **4m55s** (plus the deps already compiled before) |
| `liblancedb_go.a` | **646 MB** |
| intermediates in `target/` | 1.2 GB |
| Rust toolchain | 597 MB |
| probe binary, static link | **260 MB** |
| `lancedb` crate at the SHA | v0.24.0 · `lance` v1.0.3 · DataFusion v50.3.0 |

## T10 DONE: `fetch-lancedb` compiles the native per platform (2026-08-22)

The Engineer's decision: **dynamic linking**, compiling together with the project, each platform its
own, with no persistent artifact.

### What went in

`make fetch-lancedb` — clones the pinned SHA into a cache in `/tmp`, applies the delta, compiles and copies the
platform's library to `cmd/launcher/runtime/`. Idempotent: with a warm cache it finishes in
0.27s. Without `cargo`, it fails naming what is missing and saying that **nothing else in the build needs
Rust** — the discipline the project already uses for the model download.

`make lancedb-cgo-env` — prints `CGO_CFLAGS`/`CGO_LDFLAGS` pointing at the cache. The header and the
library stay in the cache; **nothing is copied into the repository.**

### Why dynamic, with numbers

| | static | **dynamic** |
|---|---|---|
| core binary | 260 MB | **8.9 MB** |
| library | 646 MB `.a` | 217 MB `.so` |
| system libs on the consumer | `-lbz2 -lgcc_s -lutil -lrt -lpthread -lm -ldl`, per platform | **none** |

The `.so` resolves the C dependencies inside it — `ldd` shows only `libbz2`, `libc`, `libgcc_s`,
`libm`, all base. Hybrid proven by both paths, with an identical result.

**And the launcher did not need a single line.** `cmd/launcher/main.go` already extracts the payload and prepends
the directory to `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` / `PATH` before exec'ing the core — the same
mechanism as `libonnxruntime.so`.

### The delta against the pinned commit: 3 lines, no code

| file | change | reason |
|---|---|---|
| `rust/Cargo.toml` | `crate-type` += `cdylib` | upstream dropped it; we need the `.so` |
| `rust/Cargo.toml` | `features = ["aws"]` | **without this there is no `s3://`** — see below |
| `rust/Cargo.lock` | `ethnum` 1.5.2 → 1.5.3 | does not compile on the current rustc |

### THE FINDING THAT MATTERS MOST: the published native HAS NO S3

The binding depends on `lancedb` with `default-features = false`, and the crate declares `default = []`.
Object store support — S3 included — comes from the **`aws`** feature, which nobody enables. Measured:

```
lancedb.Connect(ctx, "s3://lance-otf/wiki", …)
-> No object store provider found for scheme: 's3'
   Valid schemes: file, file-…
```

**This holds for the PUBLISHED artifacts too**, which come from the same manifest — that is,
**no release native would ever serve a remote context**, in any version. It only showed up
because we compiled and tested against a real MinIO. It is the third argument in favor of compiling
together with the project: **we need features that upstream does not enable.**

### PROVEN on-the-fly, against MinIO

With the feature on, everything against `s3://` and downloading nothing:

```
CONNECTED ON-THE-FLY to s3://lance-otf/wiki
rows on s3 = 6
FTS index built ON S3
FTS      q="fusion rankings"   -> id=3
Vector   v=[0,0.95,0.05,0]     -> id=2, id=3, id=6
HYBRID   vetor + fts, RRF      -> id=3, id=2, id=6
```

The same reordering as the local test: id=3, the BM25 winner, is promoted above id=2,
the vector winner. **Inverted index built on the object store and the engine's hybrid, remote.**

The target applies both idempotently and reverts `Cargo.toml`/`Cargo.lock` when the SHA changes,
so the delta is never applied over another commit without review.

### A PRE-EXISTING gap this exposed, and it was fixed

**Nothing in `cmd/launcher/runtime/` was ignored by git** — only the `.keep` is tracked, and every
native the Makefile leaves there (liblbug, httpfs, ONNX Runtime) showed up as untracked. A
`git add -A` after a build would commit hundreds of MB of binary. Before it was ~15 MB of ONNX;
with LanceDB it would be 208 MB, so the fix became mandatory:

```gitignore
cmd/launcher/runtime/*
!cmd/launcher/runtime/.keep
```

## Four fixes coming out of a real `setup` (2026-08-22)

The Engineer ran `graphit setup` against MinIO and looked at the global directory. Out came two bugs of mine,
one design request and one verification. None of them showed up in a test.

### 1. MY BUG: the daemon was watching the wrong directory — memory recompilation NEVER fired

`~/.graphit/` had **`memory-raw` AND `memory-raw-wt`**. The origin:
`internal/daemon/memorysyncmodule.go` did `wtBase := store.Dir() + "-wt"`, because in the git store
`Dir()` was the repository and the worktrees sat next to it. After T6 `Dir()` **is** the raw root,
so the suffix pointed the watcher at an empty directory that it created along the way itself.

**Effect, and it is worse than the orphan directory:** a watch that never fires, and a memory wiki that
never recompiles. Fixed to `store.Dir()`, with a comment explaining the suffix so nobody
puts it back. The `memory-raw-wt` left on the Engineer's machine is residue and can be deleted.

### 2. MY BUG: `Registry sync failed` on every command, on a new bucket

```
✗ Registry sync failed (will retry on next command):
  rename ~/.graphit/hub/registry.partial ~/.graphit/hub/registry: no such file or directory
```

`SyncRegistry` downloaded the prefix into a staging area and did a `rename`. But `DownloadPrefix` writes one
file per object and creates the directories along the way — so in an **empty registry**, which is every
freshly created bucket, nothing was created and the `rename` failed. And it failed **forever**, because the
registry only starts existing when someone publishes.

Fixed with a `MkdirAll` of the staging area before the download. An empty registry is a normal state, not an error.
Pinned by `TestSyncRegistryOnAnEmptyBucketSucceeds`.

### 3. The unit identity left memory and went to the GLOBAL CONFIG

The Engineer's request: *"the user's unit is beyond memory, it would be more interesting if
you put it in the global config"*. He is right — "which installation is this" serves telemetry
attribution, the origin of a published artifact, a shared-resource lease; it is not a memory
concept.

| was | became |
|---|---|
| `memory.UnitID()` + `<global>/unit.json` + key `memory.unit` | **`config.UnitID()`** + key **`unit.id`** in `~/.graphit/config.json` |

Gains beyond the tidying: it shows up in `config get unit.id`, it is editable like any other setting, and
it has no sidecar file of its own. Generation is serialized and the override (env or config written by the
operator) **is not persisted back** — persisting would freeze what the operator wanted dynamic.
`memory.UserScopeID()` stayed as a derivation: sha256 of the unit, 16 hex, because `unit.id` may be an
e-mail or a name and that goes into a directory name and an object key.

### 4. The ignore: the project's `.gitignore` + the custom one, now WITHOUT depending on git

Request: *"that Ignore file used git's mechanism to make sure it worked, it needs to
work"*, and then *"it should consider the project's gitignore plus the custom one, that already
worked"*.

**What depended on git was only the BOUNDARY** — how far up to climb collecting ignore files was
answered by looking for a `.git`. And **every test in the `ignorer` package created a `.git` directory**
just to give that search something to find, which is the sign that the mechanism rested on an
initialized git.

**On investigating, I found that the git boundary never served any purpose.** `domainForFile` computes a
pattern's domain with `filepath.Rel(project, dir)`, so a file **above** the project gets a
domain of `..` and `gogitignore` never matches anything against that — it was collected and silently
inert. Worse, it created a risk: a project under a directory that happened to be a repository (a dotfiles
`$HOME`) climbed into it.

**The boundary is now the project.** Simpler, no git, without the risk, and with the same
observable result. Verified against THIS repository: `vendor/` and `coverage.txt` from the `.gitignore`,
`internal/ast/antlr/` from the `.astignore` **with the negations** of `common/` and `*/driver.go` working,
`.graphit/` and the lockfile from the defaults, and normal code not ignored.

**`gogitignore` STAYS, and it is not a git dependency:** it is a pure-Go implementation of the *semantics*
of gitignore patterns — negation, anchoring, directory-only pattern, per-file domain. It invokes nothing and
does not require a repository. It is what makes `.astignore` and `.wikiignore` behave the way anyone who
has ever written a `.gitignore` expects.

**KNOWN LIMITATION, which predates this and was left declared in a test**
(`TestAnIgnoreFileAboveTheProjectDoesNotApply`): in a monorepo, `node_modules/` in the `.gitignore` at the
repository root does **not** exclude it from a subproject's index. Making it hold requires computing the domain
against the collection root instead of the project — a bigger change than removing git, and not done here.

Five new tests cover the path without git, which was the hole: custom alone, `.gitignore` alone,
collection from `startDir` up to the project root (knowledge's scoped-build form), the limit above
the project, and **`.gitignore` + custom together in the same checker**.

## Telemetry: the event goes up when it happens; the queue was git's legacy (2026-08-22)

The Engineer's observation: *"that events-staging makes no sense accumulating with s3, before it
accumulated, with s3 it is just writing directly"*. Correct, and the code showed it was worse than the
statement.

### Three facts the code revealed

1. **`SyncEvents` did ONE `Put` PER EVENT.** There never was a batch. The queue **deferred** requests
   instead of reducing them — the whole "flushed in batches" argument in the comment was false.
2. **`SyncEvents` was called from ONE single place:** `graphit sync`. An event from any other command
   sat on disk until somebody ran sync. After the Engineer's `setup` there was already one sitting there.
3. **The key was destroyed on the round-trip.** Staging wrote under
   `strings.ReplaceAll(key, "/", "_")` and the flush reconstructed it with the inverse substitution — but the
   key **already contains** `_`, in the ULID and in the action. Every resent event went up under a
   mangled key.

**Why it made sense in git:** an event was a write to `refs/events/*` plus a push, expensive
enough per event for the batch to pay for itself. In object storage it is a PUT either way.

### How it ended up

- **`WriteEventFile` does the PUT, in the background.** No latency on the command's path, no queue on the
  happy path. Same pattern as memory's `Publish`.
- **`events-staging` is only the FAILURE path.** An event that did not go up is left for the next flush
  to try; nothing else is written there.
- **The key travels INSIDE the file** (`stagedEvent{Key, Body}`), not in the name — it kills bug 3.
- **Without a bucket, the event is DISCARDED** with a debug log. A queue with no consumer is a disk leak,
  not durability.
- **The failure path is bounded** to `maxStagedEvents` (256), evicting the oldest. A broken remote,
  and not just momentarily unreachable, would grow without limit.
- **`hub.WaitForPendingEvents()`** wired into `root.go` and `daemon.go`, next to
  `memory.WaitForPendingPushes()`, so the last command's event does not die with the process.

Four tests: direct upload without accumulating, retry under the **same** key, the staging limit, and
discard in local-only mode.

## T6 — DONE: memory leaves git (2026-08-22)

### The chain, before and after

```
before: remote (git branch memory/<scope>/<id>)  --fetch/rebase-->  worktree (TRUTH)  --compile-->  global wiki
after:  remote (s3://<bucket>/<prefix>/memory/<scope>/<id>/) --sync-->  raw dir (TRUTH)  --compile-->  global wiki
```

Only the **remote** changes. The local directory remains the truth and the global wiki remains
the authoritative one — the part every reader opens is untouched.

### Decisions, and the why of each one

1. **The local directory does NOT move and does NOT change name** — it stays
   `<global>/memory-wt/memory-<scope>-<id>/`. It stops being a git worktree and becomes an
   ordinary directory. Reason: it **is the truth**, and every reader resolves it through
   `store.MemoryWorktreeDir` / `RawDir`. Renaming orphans the raw store of anyone who already has one — and with an
   empty remote, orphaning is **losing memory**. "No backward compatibility" does not authorize losing
   data. The path helpers keep the name because they name a directory that keeps its name.
2. **A leftover `.git` inside the raw dir is ignored on read and EXCLUDED from the upload.** Whoever
   updates in place has one. Uploading that would publish git guts inside the memory prefix.
3. **Memory shares the Hub's bucket**, under the `memory/` prefix. It was one of the FIVE
   responsibilities of the `GitStore`, and the Engineer's decision was that all five go to S3.
   No new config key; `memory.repo` goes away.
4. **Branch → prefix, one for one:** `memory/<scope>/<id>`.
5. **Renames, because the names would start lying:** `MemoryGitStore` → `MemoryStore`,
   `NewMemoryGitStore` → `NewMemoryStore`, and the struct `MemoryWorktree` → `MemoryScope`. Same
   precedent as `GitStore` → `S3Store` in T4/T5.
6. **`Pull` MERGES, it does not mirror** — unlike the Hub's registry. Downloading the prefix on top
   of the raw dir preserves a local file that has not gone up yet; mirroring would delete freshly written
   memory that has not been published yet. Removal is driven only by `RemoveFile`.
7. **There is no commit, so there is no commit message.** `CommitAndPush(msg)` becomes
   `Publish(reason)`, and `reason` only goes to the log. See the pending item below.
8. **Conflict changes in nature, and for the better in the common case.** Each memory is a file with
   a ULID name, so two machines adding memories touch **different objects** and there is no
   conflict at all. Conflict only exists in editing/removing the SAME memory, and there it is
   last-writer-wins — which is what `rebase -X ours` already approximated.
9. **The push stays in the background** (`WaitForPendingPushes` remains), so that writing memory does not
   start blocking on the network. The local is the truth; the upload is asynchronous.

### What remained, and what was measured

| | before | after |
|---|---|---|
| backend files | `memory_git_store.go` (531 lines) + `memory_git_store_rebase_test.go` | `memory_s3_store.go` |
| git invocations before the first read | 8 (`init`, bootstrap commit, `config fetch.depth`, remote, `for-each-ref`, ref prune…) | **0** |
| `memory.repo` | config key | **removed**, along with `ResolveMemoryRepo`, `MemoryRepoURL` and `MemoryRepoDirPath` |
| callers | `NewMemoryGitStore` in 16 places | `NewMemoryStore`, same signature |

**Git in the `memory` package, what was left and why:** `git rev-parse --show-toplevel` and
`git config user.email`, in `memory.go`. That is git as **identity**, not as storage —
the `user` scope is keyed by the hash of the git identity by design. The criterion "no
`exec.Command("git")` in the package", which held for the Hub in T4/T5, does not apply here for that
reason, and the distinction is deliberate.

**New tests** (`memory_s3_store_test.go`), with T2's fake S3 server:

| test | what it guarantees |
|---|---|
| `TestMemoryPublishUploadsUnderTheScopePrefix` | **T6's acceptance** — the memory arrives in the bucket, under the prefix the branch used to name |
| `TestMemoryPublishDeletesRemovedMemories` | removal reaches the bucket (it is not inferred from the directory, the file is already gone) |
| `TestMemoryPullMergesAndKeepsUnpublishedMemories` | **Pull merges** — local memory not yet published survives |
| `TestMemoryPullOnAnEmptyScopeIsNotAnError` | a scope never published is a normal state |
| `TestMemoryPublishSkipsLeftoverGitMetadata` | a leftover `.git` is never published |
| `TestMemoryPruneLeavesTheRemoteAlone` | prune reclaims local disk and does **not** delete the remote prefix |
| `TestRemotePrefixMatchesTheBranchLayout` | branch → prefix is the identity, without a doubled `memory/` |
| `TestNewMemoryStoreWithoutABucketIsLocalOnlyNotAnError` | local-only remains a supported mode |

**Tests removed, and why it is not a loss of coverage:** seven tests exercised the git backend
directly (`createOrphanBranch`, `syncRemote`, `isRemoteEmpty`, `remoteBranchExists`,
`pushBranchInBackground`, the git helpers, and "nothing to commit"). They tested an
implementation that no longer exists; the behavior that mattered — publish, remove, sync,
prune — is covered above, and now against a real bucket instead of against a fake
repository.

`go build -tags fts5 ./...` and `go test -tags fts5 ./...` pass in full.

### CLOSED: git ZERO, and the audit trail inside the memory (the Engineer's decision)

Three instructions: *"you don't need anything from git, remove it completely"*, *"you don't need
backward compatibility nor to worry about preserving old data, even for identifying the user
look at another unit mechanism"*, and *"as for git carrying historical data, make those
data live in the memory's frontmatter, pointing even to the path of the previous version when
there is one"*. All three done.

**1. Git ZERO in the package.** `grep` for `internal/git`, `gitmod` and `exec.Command("git")` in
`internal/memory/` (production AND test) returns nothing. Those were the last two uses, which I had
defended as "git as identity":

| was | became |
|---|---|
| `git config user.email` → hash → `user` scope id | **unit identity**: a ULID generated once and persisted in `<global>/unit.json`, with an override through `memory.unit` |
| `git rev-parse --show-toplevel` → project root | **climbs looking for the lockfile** (`brand.LockFileName()`) |

Unit identity is **better than git's in two points**: it does not require another tool
configured (the old error told the user to run `git config` before being able to write a
memory), and the lockfile-based root gets right what `rev-parse` got wrong — a git repository with several
projects resolved to the repository root, now it resolves to the right project.

**What the override exists to solve, and it is the only loss of semantics:** git's e-mail made the
`user` scope follow the PERSON across machines. A unit per installation does not. Setting the same
`memory.unit` on both machines restores that, and it is the supported form. Without an override, two
installations are two user scopes.

**2. Audit trail in the frontmatter, with a pointer to the previous version.** Three new fields:

```yaml
revision: 3
updated_by: 01K9...          # a unidade que escreveu
previous: history/<id>/0002.md
```

Every write **archives the version it replaced** in `history/<id>/<revision>.md` and points to it.
Following `previous` walks the whole chain back to the original — pinned by
`TestRevisionChainWalksBackToTheFirstVersion`, which reconstructs `v3,v2,v1`. Removal also archives
(git left the blob reachable in the history); the honest difference is that nothing points to the
file afterwards, because the memory that would carry the pointer is the one that was removed — you find it
through `history/<id>/`.

**The archive is never confused with a memory:** `history/` is a subdirectory, and **every** listing in the
package reads one level with `os.ReadDir` and skips directories. Pinned by
`TestArchivedRevisionsAreNotMemories`, which checks the listing AND the wiki.

**3. Vocabulary corrected, now that backward compat was waived.** Without git, "worktree" and "branch"
were lying:

| was | became |
|---|---|
| `<global>/memory-wt/` | `<global>/memory-raw/` |
| `store.MemoryWorktreeRoot/Dir` | `store.MemoryRawRoot/MemoryRawDir` |
| struct `MemoryWorktree` | `ScopeStore` |
| `MemoryWorktree(b)` / `MemoryWorktreeLocal(b)` | `OpenScope` / `OpenScopeLocal` |
| `HasLocalWorktree`, `WorktreeDirForBranch`, `ExtractBranchDir` | `HasLocalScope`, `ScopeDir`, `ExtractScopeDir` |
| `HubBranch()` | `ScopePrefix()` |
| `RegisterBranch`/`DeregisterBranch`/`ActiveMemoryBranches`/`MemoryBranchSummary`/`ValidateMemBranchRefs` | `RegisterScope`/`DeregisterScope`/`ActiveScopes`/`ScopeSummary`/`ValidateScopeRefs` |
| `memory_branch_lock.go`, JSON field `branches` | `scope_lock.go`, field `scopes` |
| `worktreeShardDir*` | `shardMirrorDir*` |

The parameter became **`scopePath`** (`memory/<scope>/<id>`) and not `branch`. `prefix` was kept
reserved for the S3 key, otherwise the two collided in the same lexical scope.

Also corrected was the **prose** that described the git model in `paths.go`, `shardsync.go`, `wiki.go`,
`cycle.go`, `store.go` and — what mattered most — in `rule.go`, which **showed the user** the
path `<global>/memory-wt/...`, now nonexistent.

**Tests:** the ones that depended on git were removed along with the `gitAvailable` and
`setupGitTestEnv` helpers; no test in the package needs git. `TestMain` stopped disabling
git maintenance and exporting a git identity — the `HOME` isolation remains, and now it covers
`unit.json` for free. Eleven new tests between `history_test.go` and `memory_s3_store_test.go`.

## The label transpiler does not go away with an upstream fix: the restriction is the FORMAT's (2026-08-22)

The Engineer's question: is there an upstream pending item left, and is there a migration that dispenses with the transpiler?
The second was re-measured, because the measurement that supported it was **earlier** than the row
group fix — and three defects from that period were that fix.

**Re-measured, and it is still broken:** an edge table with **two** FROM/TO pairs, a CSR of 300
edges (`TestIcebugMultiPairRelTableCannotWork`):

| query | result | correct |
|---|---|---|
| `[:R]` | **600** | 300 |
| `(:NA)-[:R]->(:NB)` | 300 | 300 |
| `(:NA)-[:R]->(:NA)` | **300** | 0 — edges that do not exist |

**And now the reason, which is not a bug:** the reference tool emits **three files per
graph** — `nodes_<t>`, `indices_<rel>`, `indptr_<rel>` — keyed by the **TABLE** name, with no
pair at all in the name (`TestIcebugFormatHasNoPerPairFile`). There is nowhere to store the CSR of a
second pair. And a target id is a **position in the dense id space of ONE table**, so with
two TO tables the same number designates two different nodes. **No engine fix changes
that; it is the format.**

### The dichotomy, and it is structural

| | single node table (today) | node table per label |
|---|---|---|
| `MATCH (f:Function)` | needs a **label transpiler** | native |
| one edge type | **1 table, 1 CSR** | 1 table **per label pair** |
| `[:CONTAINS]` | native | union of ~62 tables |
| union with a filtered endpoint | not used | **broken, upstream, no workaround** |
| variable-length path | correct | **no correct form** |

So dispensing with the transpiler requires unioning pair tables, and unioning has only two forms: `[:A|B]`
— broken with a filter on an endpoint — or **client-side expansion** (the framework rewrites
`[:CALLS]` into a union of single-table queries, which are exact). The second works today, and it is
**the worse deal**: it rewrites the edge side instead of the label side, with a fan-out of up to 62
subqueries per pattern, and it has **no form at all** for a variable-length path.

The label transpiler is the cheaper of the two rewrites: it is mechanical, total, without fan-out, and
`(f:Function)` → `(f:Entity) WHERE f.label = 'Function'` has no semantic risk. **And it holds only
for the remote/icebug path** — the local native store keeps a table per label and is not
touched.

**For the transpiler to go away, the necessary upstream fix is the one for alternatives with a filtered
endpoint** — and even that only gives back the option of partitioning by pair, at the cost of fan-out and without
variable length. A format with a CSR per pair would really solve it; that is a feature request,
not a bug report.

## RESOLVED: `[:A|B]` is TWO defects, both UPSTREAM; the counting fixes itself by ordering (2026-08-22)

The requested step was to bisect the CALLS/CONTAINS pair and compare the four Parquet files byte by byte. The
bisection came first and changed the question: **it was not a pair.**

### What the whole matrix showed, and why 6 pairs misled

Enumerated **all 28 pairs** of the 8 types, not six:

- **9 wrong pairs**, not one. And in all 9 the result is `2 ×` the table **with the lowest id**.
- The 19 "correct" ones were not correct by being correct: **there was nothing to truncate in them.**

The rule that closes the 28: for `[:A|B]`, the engine caps **every** alternative by the row
count of the **first table created** (lowest table id). Hence

```
result = rows(first) + min(rows(first), rows(second))
```

First one smaller ⇒ `2 × first`. First one larger ⇒ exact sum. Equal ⇒ exact. **The order of the
query does not matter; the CREATION order matters.** With the tables in alphabetical order, the 9 pairs
whose alphabetical first was the smaller one failed, and only they — that is what made the defect look like it
belonged to one pair of tables.

### It is UPSTREAM, and the earlier test absolved the engine by luck

The previous round attributed this to our files because the tool answered 147,219 at the
same sizes. But that mount declared the **larger table first** — the case that cannot
fail. Asked in both orders, **on the tool's own files**:

| creation order | `[:Big_rel\|Small_rel]` |
|---|---|
| Big (92,396) → Small (54,823) | **147,219** = exact |
| Small (54,823) → Big (92,396) | **109,646** = 2 × 54,823 |

109,646 is exactly the number the real graph was reporting. **An engine defect, not the writer's.**

### The fix: one ordering

`sortRelsLargestFirst` in `internal/ladybugstore/icebug.go` — `schema.cypher` creates the edge
tables from largest to smallest. Since the cap is the **first** alternative (verified with three
tables: `100,1000,50` gives 250, which is the first-one cap, and not 150, which would be the minimum cap),
descending order guarantees that the lowest-id one in **any** subset is its largest. Nothing is
truncated, for any combination.

Result on the real graph: **28/28 pairs exact**, and the **8 alternatives at once = 204,353**,
exact. And the **identity** is also right, not just the count: with disjoint source id ranges
per table, the pattern reports the right sources for each one.

### The defect that REMAINS, and it kills partitioning by pair

With the counting fixed, a second defect appeared, distinct and **with no workaround**: alternatives
with a **filter on a bound endpoint**. Filtering by the most-called function in the real graph:

| query | result | correct |
|---|---|---|
| `[:CALLS]` | 3,769 | 3,769 ✓ |
| `[:CONTAINS]` | 0 | 0 ✓ |
| `[:CONTAINS\|CALLS]` | **0** | 3,769 ✗ — the 3,769 CALLS edges disappear |
| `[:CALLS\|WRITES_FIELD]` | **3,798** | 3,769 ✗ — WRITES_FIELD invents 29 |

Each alternative is matched against the node set **of the first one**, and ordering does not help.
**Reproduced on the tool's files** (`Big_rel|Small_rel` filtered: 13–14 where the per-table
sum is 9), so it is upstream too.

**Design consequence, and it is the answer to the objection to the transpiler:** partitioning by pair
does **not** become viable again. It turns every "who calls X" question —
`MATCH (f)-[:CALLS]->(g) WHERE g.name = …` — into this broken form. The single node table remains
the only correct form, and the label transpiler remains necessary. The folded table is
not affected by either defect: each type is ONE table, so no framework query
emits alternatives. They only appear when the user writes them — and there the counting
is now right.

### Hypotheses eliminated by measurement in this round

- **shared `storage` directory** — relaid the same bytes with one directory per table:
  the same 9 failures, identical. Pinned by `TestIcebugPairsSumWithPerTableStorage`.
- **the Parquet container** — schema, physical and logical type, nullability, encodings, metadata,
  row groups and page offsets compared table by table: one row group in all of them, everything
  consistent. The container was not the difference this time.
- **file content** — in the synthetic harness, neither properties (0, 2, 4, int, string), nor
  degree distribution (spread out versus concentrated), nor table size change the verdict.
  Only the order changes it.
- **size as the discriminant** — synthetic `2957|55040` fails and the real `HAS_FIELD|CALLS`, at the
  same sizes, is right. Same numbers, opposite verdict: it was the order.

## EARLIER ATTRIBUTION TO THE WRITER (historical, CORRECTED above): `[:A|B]` reduced to ONE pair (2026-08-22)

After the row group fix, the Engineer asked to re-measure the alternatives defect,
including against the Python tool. Done.

### On the tool: it does NOT reproduce, not even in our form

Generated two graphs with `uvx icebug-format` at the SAME sizes as the failing pair
(92,396 and 54,823 edges, 60,000 nodes each):

| mount | result |
|---|---|
| **separate** node tables (the tool's form) | 147,219 = **correct** |
| **shared** node table (our folded form) | 147,219 = **correct** |

So neither the format nor the sharing of the node table explains it. **The defect is in
our files.**

### In our export: only ONE pair among all tested

| alternatives | result |
|---|---|
| `[:CONTAINS\|CALLS]` | **109,646 against 147,219 — wrong by 37,573** |
| `[:CONTAINS\|IMPORTS]` | 96,120 ✓ |
| `[:CONTAINS\|WRITES_FIELD]` | 93,451 ✓ |
| `[:CONTAINS\|HAS_FIELD]` | 95,349 ✓ |
| `[:READS_FIELD\|REFERENCES]` | 39,371 ✓ |
| `[:CALLS\|READS_FIELD]` | 76,738 ✓ |

**Who contributes what:** `count(r.line_number)` over `[:CONTAINS|CALLS]` returns **109,646**, and
CONTAINS **does not have** `line_number` — so all the rows came from CALLS. **CALLS is read twice
and CONTAINS contributes zero.** The order changes nothing, and the deficit is always exactly
37,573 (= 92,396 − 54,823).

### What has already been discarded by measurement

- **row groups** — the files have 1, verified by test;
- **types and nullability** — `indices_CONTAINS` is structurally identical to the tool's
  (`target` INT64 unsigned optional, 1 row group);
- **dictionary encoding** — tested on and off, same deficit;
- **shared node table** — the tool is right in that form;
- **table size** — the tool is right with the same 92,396 and 54,823;
- **mixing a table with and without a property** — `[:CONTAINS|IMPORTS]` mixes 0 and 6 columns and
  is right.

**Concrete next step** (not done): bisect the pair by exporting only CALLS and CONTAINS in a
reduced graph, and compare the four Parquet files byte by byte against the tool's equivalents. It was
that method that found the row group bug.

## RESOLVED: three of the five "defects" were MINE — multiple row groups in the Parquet (2026-08-22)

The Engineer's instruction, and it was exact: *"if it works in the python tool and not in yours,
there is some problem with your generation, you should compare parquet by parquet the ones you generate with
his and understand the difference"*.

Done. The same graph (60,000 nodes, 200,000 edges) exported by both paths, and comparing
schema, physical and logical types, nullability, encodings, metadata and row group count:

| | mine | tool |
|---|---|---|
| **row groups** | `indices` **49**, `indptr` **15**, `nodes` **15** | **1** in all |
| `target`/`ptr` repetition | **required** | optional |
| logical type of `id` | `Int(64, signed)` | `None` |
| encodings | PLAIN, RLE | PLAIN, RLE, RLE_DICTIONARY |

**The cause: `pqarrow.FileWriter.Write` opens a NEW row group on every call.** I was writing in
batches of 4,096 rows, so dozens of row groups came out. And `parquet.WithMaxRowGroupLength`
does **not** merge them — I had added that option earlier and it changed nothing, which led me
to discard the hypothesis far too early.

**The effect was exactly the worst possible one:** the file mounts, the count over an anonymous pattern comes out
exact, and the moment a pattern **binds a node variable** the resolution fails in silence.
Nothing errors.

### Fixed, and what started working again

One line of real change — the whole table in a single record — plus nullability equal to the
tool's. Result on the real graph:

| before | after |
|---|---|
| filter anchored on the source: **0** against 583 | **583 ✓** |
| `count(<node variable>)`: 53,781 against 54,823 | **54,823 ✓** |
| target-anchored on the synthetic: 0 against 1 | **1 ✓** |

Pinned by `TestIcebugWritesOneRowGroupPerFile`, and the two tests I had written as
"engine defect" became regression guards with the name corrected.

**Accepted cost:** a single row group eliminates row-group pruning, so the scan by label
went from 12 ms to 42 ms. An edge query is still faster than the native one (0 ms).

### The final attribution, corrected

| defect | verdict |
|---|---|
| anchoring on a node variable | **WAS MINE** — row groups |
| `count(<node variable>)` | **WAS MINE** — row groups |
| `[:A\|B]` reapplying the indptr | **IS MINE** — to be re-measured; see the section below |
| **multi-hop traversal does not complete** | **UPSTREAM** — reproduced on the tool's output and still fails after the fix |
| **`=` on the primary key returns empty** | **UPSTREAM** — reproduced on the tool's output |

**ONE confirmed upstream defect that blocks us remains** (traversal), plus the primary key one
which has an exact workaround (`IN [v]`). `[:A|B]` needs re-measuring — if it was also row
group, **partitioning by pair becomes viable again and the label transpiler stops being
necessary**, which is exactly the Engineer's objection.

## EARLIER ATTRIBUTION (historical, corrected above): reproduced on the official tool, and an upstream search (2026-08-22)

At the Engineer's request: research the problems on the internet and test whether the Python tool
confirms them too. Done, and it **corrects an earlier statement of mine** — I had called the
five of them "reader defects", and only **two** are confirmed as upstream.

### What the upstream search found

LadybugDB is a continuation of **Kuzu**, and Kuzu has open issues that describe exactly
what was measured here:

| issue | what it says | corresponds to |
|---|---|---|
| [kuzu#4941](https://github.com/kuzudb/kuzu/issues/4941) | "The GDS join initializes data structures for every node, which performs poorly with large datasets" | **it is literally our `EXPLAIN`**: `TABLE_FUNCTION_CALL` over `a._ID` enumerating all the nodes before the `RECURSIVE_EXTEND` |
| [kuzu#4459](https://github.com/kuzudb/kuzu/issues/4459) | recursive join consumes too much memory on a variable-length path | defect 2 |
| [kuzu#5040](https://github.com/kuzudb/kuzu/issues/5040) | a query with a recursive relation **hangs** | defect 2 |
| [kuzu#4540](https://github.com/kuzudb/kuzu/issues/4540) | hangs with an undirected recursive join | defect 2 |
| [kuzu#4285](https://github.com/kuzudb/kuzu/issues/4285) | "GDS and Recursive Joins TODOs" — an open umbrella | defect 2 |
| [kuzu#2866](https://github.com/kuzudb/kuzu/issues/2866) | "Each REL table in Kuzu may only contain one node type for the FROM and TO specification" | **the architectural origin** of the multi-pair problem |
| [kuzu#5049](https://github.com/kuzudb/kuzu/issues/5049) | "Bug: Defining a rel table with multiple node table pairs" — "with only one pair there was no error" | multi-pair |
| [kuzu#4189](https://github.com/kuzudb/kuzu/issues/4189) | "Kuzu wrongly output non-existing relations in certain cases" | family of defect 3 |

Nothing found for the source-anchored filter nor for `count(<node variable>)` — the
icebug is recent (v0.17.0), so they may be new.

### What the official tool confirms, and what it does NOT confirm

Tests in `internal/ladybugstore/icebug_upstream_test.go`, over output produced by
**`uvx icebug-format`** (60,000 nodes, 200,000 edges), with the truth read from the CSR:

| defect | on the tool | verdict |
|---|---|---|
| 2 — multi-hop traversal | **does not complete in 100 s** | **UPSTREAM confirmed** |
| 5 — `=` on the primary key | `= 17` → 0, `IN [17]` → 1 | **UPSTREAM confirmed** |
| 1 — anchoring on the source | filter on a **non-key** column: 8=8 ✓ and 1=1 ✓ | **does NOT reproduce** |
| 4 — `count(<node variable>)` | 200,000 = 200,000 ✓ | **does NOT reproduce** |
| 3 — `[:A\|B]` | the tool's fixture has 2+2=4, ambiguous with 2×2=4 | **inconclusive** |

### Defect 1 is OURS, and I have not found the cause yet

In our export, anchoring on the source returns 0 in **all** forms (`=`, `IN`, `STARTS WITH`,
and even `entity_id IN [27766]`), while anchoring on the target works. On the tool's output,
both sides work.

**The data is right** — verified:
- the node is findable by name: `entity_id 27766`, label `Function`, correct path;
- `entity_id` is monotonic 0,1,2,… so dense id == row position;
- **the CSR has out-degree 583 at exactly that id**, which is the number the source reports.

Hypotheses already **discarded** by measurement: edge properties in the `indices` (removing them does not
change anything), a filter matching more than one node (it works on the tool with two), and the shape of the
predicate. What is left is the difference in **table shape**: 63,314 nodes × 20 columns and 8 edge
tables against 60,000 × 2 columns and 1 table. I have not bisected that yet.

**Honest consequence:** while defect 1 is not explained, it cannot be claimed that
Phase C is blocked only by upstream. Two defects are upstream; the one that hinders us most
may be ours.

## The source-side filter defect, measured on our export (2026-08-21)

Investigated at the Engineer's request, who asked whether it would not always be advantageous to generate a
reverse edge. The answer is no, and the reason is worse than a question of cost.

Measured on the real graph, the same questions on both storages:

| filter | expected | native | icebug |
|---|---|---|---|
| **TARGET** side ("who calls X") | 3,752 | 3,752 ✓ | **3,752 ✓** |
| **SOURCE** side ("what X calls") | 583 | 583 ✓ | **0 ✗** |
| source side of the **reverse** table | 3,752 | — | **0 ✗** |

**Filtering by the source side returns empty without an error.** And it is ironic: the CSR is organized BY
source, so that should be the fast path. `MATCH (a)-[:CALLS]->(b) WHERE a.name = 'X'` —
"what X calls" — is unanswerable on a mounted icebug graph. Pinned by
`TestIcebugSourceSideFilterReturnsNothing`, which also detects the future fix.

### Why a reverse edge does NOT solve it, on three levels

1. **In the same table it is actively wrong.** It is what the reference tool does, and it destroys
   the direction: 200,000 edges mount as 399,996, and `MATCH (a)-[:CALLS]->(b)` starts returning
   calls that do not exist. In a code graph the direction of CALLS is the meaning. That is why
   our writer emits the mirror in a **separate table** `<TIPO>_REVERSE` — the forward stays
   exact and the mirror does not count as an edge of the graph.
2. **The separate table is fast and still useless today.** 54,807 correct rows,
   `MATCH ()-[r:CALLS_REVERSE]->()` answers 54,807 in 1 ms, and the inbound query through it runs
   in **29 ms against 339 ms** through the forward — 11.7× faster. But it returns **0**, because
   asking through it is anchoring on the source, which is exactly the broken path.
3. **It does not fix the traversal.** Measured: 2 hops still does not complete with 399,996
   symmetric edges.

And the inbound **already works** without any of that, through the forward: 57 ms to count, 294–339 ms to
materialize 3,752 callers, against 2–4 ms native. So a reverse edge would buy latency,
not capability — and today not even that.

### The FIVE reader defects, all silent, all measured

None of them errors. All of them answer with the confidence of a right answer.

| # | defect | evidence |
|---|---|---|
| 1 | **a filter on the source side returns empty** | 583 → 0; target 3,752 ✓ |
| 2 | **multi-hop traversal does not complete** | native 2,133 ms / 867,766 paths; icebug >100 s. Reproduced on the official tool's output |
| 3 | `[:A\|B]` reapplies the `indptr` of the first alternative | 92,337 per table against 8,740 through alternatives; 2 alternatives of 92 and 746 = 184 = 2×92 |
| 4 | `count(<node variable>)` under-reports | 53,781 against 54,823; `count(r)` and `count(*)` exact |
| 5 | `=` on the primary key returns empty | `IN [v]`, `STARTS WITH`, range and `ORDER BY` work |

**Honest conclusion: the icebug read path in liblbug 0.18.2 is not fit for this framework's query
patterns.** Our export is demonstrably correct — verified by reading the
CSR and by target-anchored queries. What is not fit is the reader.

## Single node table: correct data, insufficient traversal performance (2026-08-21)

The Engineer decided to switch to a single node table, always native in Go and **with no
fallback to the Python tool**. Done. And measured against a **FROZEN COPY** of the real
graph (the daemon was rewriting the store during the read and that caused a segfault and a spurious
duplicate key — the Engineer's suggestion, and it was what stabilized the measurement).

### The layout

- **One** node table, `Entity`, with `label` as a column and `entity_id INT64` (the dense id
  itself) as the primary key. Not `_id`: the engine refuses with "reserved property name".
- **One** table per edge type, `FROM Entity TO Entity` — hence one pair, hence one CSR, hence
  **no `[:A|B]` alternation anywhere**, which was the defect that took down
  partitioning.
- Columns are the **union** across labels, null where the label does not have one. `LabelKeys` keeps the
  original key of each label, and `Pairs` the real (from,to) of each type, for reconstruction.

### DATA: 100% correct on the real graph

`TestIcebugAgainstARealGraph` against the frozen copy: **63,314 nodes in 30 labels, 203,776
edges in 8 tables, export in 2.0–2.5s**. It passes.

- The **30 labels** match node for node.
- The **8 edge types** match exactly: CALLS 54,823, CONTAINS 92,396, HAS_FIELD 2,953,
  HAS_PARAMETER 9,454, IMPORTS 3,724, READS_FIELD 21,915, REFERENCES 17,456, WRITES_FIELD 1,055.
- **Self-loops: 16 of 16**, verified **by reading the CSR back** and not by query — see below.
- **Artifact: 2.7 MiB** for the whole graph (the source store is 76 MiB). That is excellent
  for remote reading: the volume to transfer per query is small.

### CORRECTION OF WHAT I HAD REPORTED WRONG

The first measurement compared **`count(a)` on our side with `count(r)` on the native side** — that is not
the same question. Redone like with like:

| form | result |
|---|---|
| `count(r)`, anonymous endpoints | **54,823** ✓ |
| `count(r)`, **bound** endpoints | **54,823** ✓ |
| `count(*)`, bound endpoints | **54,823** ✓ |
| `count(<node variable>)` | 54,414 ✗ |

**The export is correct.** Bound endpoints work. The only wrong one is
`count(<node variable>)` — a narrow engine defect with an exact workaround (`count(*)` or
`count(r)`), pinned by `TestIcebugCountOfANodeVariableIsWrong`.

### PERFORMANCE: measured like with like

`TestIcebugRealGraphQueryCost`, the same question on both sides, on local disk (the network would add
latency on top; the ratio between them does not change):

| Query | native | icebug | ratio |
|---|---|---|---|
| count a label | 2 ms | 12 ms | 4.8× |
| filter a label by property | 1 ms | 11 ms | 6.7× |
| count an edge type | 13 ms | **0 ms** | **0.05× — 20× faster** |
| 1 hop with bound endpoints | 3 ms | **0 ms** | **0.13× — 8× faster** |
| multi-hop traversal | fast | **does not complete** | see below |

An edge query ends up **faster** than the native one (one CSR against 62 pair tables); a scan
by label ends up 5–7× slower in absolute terms of 11–12 ms, which is acceptable.

### Multi-hop traversal is a Ladybug OPTIMIZER BUG, proven with three experiments

Diagnostic script proposed by the Engineer: run `EXPLAIN` (it does not execute, so it does not
hang), and regenerate with `--add-reverse-edges` to see whether the optimizer requires a reverse index even
for a purely directed pattern. Both run, plus a control.

**1. `EXPLAIN` shows the difference between 1 and 2 hops.**

1 hop gets an optimal plan — a single operator:
```
RESULT_COLLECTOR[2] <- PROJECTION[1] <- COUNT_REL_TABLE[0]   Table: demo_rel
```
Hence the 0 ms.

2 hops gets:
```
RESULT_COLLECTOR[7] <- PROJECTION[6] <- AGGREGATE_SCAN[5] <- AGGREGATE_FINALIZE[4]
  <- AGGREGATE[3] <- PROJECTION[2] <- TABLE_FUNCTION_CALL[1] (Expressions: a._ID)
  <- RECURSIVE_EXTEND[0]
```
`TABLE_FUNCTION_CALL` over `a._ID` **enumerates all 60 thousand nodes** as the initial set, and
`RECURSIVE_EXTEND` expands from each one.

**2. A reverse edge does NOT solve it — hypothesis discarded.** The same graph reconverted with
`--add-reverse-edges` (399,996 edges, symmetric adjacency, `indptr` with the same 60,001
rows): nodes 60,000 ✓, edges 399,996 ✓, 1 hop 1 ms ✓, **2 hops still does not complete in
100 s**. It is not a lack of a reverse index.

**3. THE CONTROL, which is what closes the case.** The SAME data (60,000 nodes, 200,000 edges)
loaded into **NATIVE** storage via `COPY FROM`, and the SAME query:

| storage | 2 hops |
|---|---|
| **native** | **2,133 ms, 867,766 paths** |
| icebug | **does not complete in 100,000 ms** |
| icebug + reverse edges | **does not complete in 100,000 ms** |

**≥47× slower and effectively non-terminating, on a query the native resolves in 2 s.** The
query is modest (867 thousand paths); the problem is the execution path of `RECURSIVE_EXTEND`
over icebug storage. It is a Ladybug optimizer/execution bug, reproducible with the output of their
official tool, and there is no fix on our side.

### Multi-hop traversal is an UPSTREAM limitation, and that was PROVEN

The Engineer's suggestion: compare with the Python tool for diagnosis. That is what separated
"our defect" from "their defect".

A synthetic graph of **60,000 nodes and 200,000 edges** was generated, converted by the **tool
`icebug-format` itself**, and mounted:

| query on the TOOL's output | result |
|---|---|
| nodes | 60,000 ✓ |
| edges, anonymous endpoints | 200,000 ✓ |
| edges, **bound** endpoints | **200,000 ✓** |
| 1-hop bound fan-in | 200,000 ✓ (329 ms) |
| **2-hop traversal** | **did not complete in 100 s** |

**The official tool's output does not do 2 hops at that scale either.** So multi-hop
traversal is not a defect of our writer — it is the format's/reader's. And **a reverse edge would
not solve it**: the problem is not direction, it is path expansion (reasoning, not measurement).

Variable-length traversal **works** at fixture scale
(`TestIcebugVariableLengthTraversalIsNative` passes), so it is a scale limit, not a syntax one.

**The structural explanation, and it is the real cost of folding the labels:** in the native graph
`(a)-[:CALLS]->(b)` is typed by the pair tables, so the planner knows `a` is a `Function` and
prunes the search. In the folded table `a` can be any of the 63 thousand nodes — the planner **loses
the type information that made the pruning possible**. Counting by type gets faster (one CSR instead of
62); traversal with bound endpoints gets much slower.

The two designs fail in different ways, and that is the finding that matters:

| | partitioned by pair | single node table |
|---|---|---|
| data | correct | correct |
| `[:CONTAINS]` summable | **no** | yes |
| variable-length path | **no correct form** | correct but **does not complete** |
| type pruning in the planner | yes | **no** |

### Open, and now with the owner of the problem identified

1. **Multi-hop traversal — UPSTREAM.** Reproduced on the official tool's output. There is no
   fix on our side; it is a matter for the Ladybug project. In the meantime, a remote context
   answers lookup, property filter and 1-hop query well, and **not** deep
   traversal.
2. **`count(<node variable>)` — an engine defect, workaroundable.** Use `count(*)` or
   `count(r)`. Pinned by a test that also detects the future fix.
3. **Row group matters.** Writing in several row groups made the count with bound endpoints worse
   (53,741) against a single row group (54,823). `parquet.WithMaxRowGroupLength` keeps the table
   in a single row group.

### The Engineer's objection to the transpiler, and it is correct

> "I don't think it's right to have to rely on a transpiler for `MATCH (f:Function)` to work"

He is right, and it is worth recording why: **the label transpiler exists ONLY because we folded the
labels**, and folding exists only because the alternation form `[:A|B]` is broken. That is, the
need for the transpiler is a consequence of a reader defect, not of the design.

If the alternation defect is fixed upstream, **partitioning by pair** becomes the
better one again: a label remains a table, `MATCH (f:Function)` is native, `ast_schema` does not change, and
**no label transpiler is necessary**. The writer supports both groupings with a
localized change — the CSR, the manifest and the tests are the same.

It is worth opening an upstream issue about: (a) `[:A|B]` reapplying the indptr of the first alternative,
(b) multi-hop traversal not completing, (c) `count(<node variable>)` under-reporting, and
(d) `=` on the primary key returning empty.

### A new trap, and it is the reader's

**A mounted icebug graph cannot answer "these two are the same node".** Every
form tested returns zero while the edges are demonstrably present:
`a.entity_id = b.entity_id`, a range comparison on it, a repeated variable `(a)-[r]->(a)`, and
even a comparison of the **non-key** column `uid`. That is why the test verifies self-loops **by reading the
CSR** (`countSelfLoopsInCSR`) — which tests the export instead of the engine's planner.

### Two things the frozen copy revealed

- **`Comment` declares `uid` as PRIMARY KEY and has 951 repeated values** — the engine does not enforce
  the declaration. Keying the dense mapping by the PK would attach an edge to the wrong twin, in
  silence. That is why the mapping is keyed by **`offset(id(n))`**, unique by construction
  (17,408 of 17,408 distinct in the same table). This is a defect of the AST indexer, separate
  from this task.
- **The graph contains a string that is not valid UTF-8** (`"\xD8\x06"`), and a Parquet STRING column is
  UTF-8 by definition — the engine refuses the whole file. `sanitizeUTF8` repairs it and the manifest
  publishes the count in `repaired_strings`, so the repair is visible instead of silent.

## Two reader defects, measured on the REAL graph (2026-08-21) — history of the partitioning

The test `TestIcebugAgainstARealGraph` (`GRAPHIT_REAL_STORE=<ladybugdb>`) exports a populated
store — **63,314 nodes in 30 labels, ~198 thousand edges in 97 pair tables, in ~2s** — and
compares label by label and type by type. It exists because the 3-label fixture **hides the
two defects below**: in a small case the arithmetic coincides and the encoding does not change.

### Defect 1 — the engine's `COPY TO` produces Parquet that ITS OWN icebug reader cannot read

```
MATCH (x:Function) RETURN count(x)
-> Copy exception: Invalid string encoding found in Parquet file:
   value "\x00…\x5C\x02\x00\x00serv\x97…\x5C\x02\x00\x00test\x98…" is not valid UTF8!
```

Fragments of 4 characters interleaved with a constant counter (`\x5C\x02` = 604): a string
column read with the wrong offsets. **It is not our code** — it was reproduced after removing
`stampIcebugVersion`, which was the only thing of ours touching those files. On a 2-row table
it passes; at ~5,000 it breaks, so it is encoding/scale-dependent (dictionary, pages
or row groups).

Consequence: **the node table cannot be written by the engine's `COPY TO`.** It has to be
written by us, with arrow-go and explicit types (`cypherType` already maps them), probably with the
dictionary turned off. It is our work and it is feasible — it is just not done.

> Historical note: `stampIcebugVersion` was implemented to silence the warning about missing metadata
> in the node files and **removed**, because on an Arrow round-trip over the real corpus
> it *also* corrupted strings. Silencing a warning is not worth a corruption path. The warning
> stays; the manifest (`icebug.json`) records the version.

### Defect 2 — the alternation form `[:A|B|…]` is broken on an icebug table, and this is the serious one

Measured on the real graph, for `CONTAINS` (62 pair tables):

| Form | Result |
|---|---|
| Sum table by table | **92,337** ✓ (= manifest = origin) |
| `[:alt1\|…\|alt62]`, free endpoints | 8,740 ✗ |
| `(a:File)-[:alts_de_File]->()` | 27 ✗ |
| `[:alt1\|alt2]` (tables of 92 and 746) | 184 = **2 × 92** ✗ |

The `2 × 92` gives the mechanism: **the reader applies the `indptr` of the FIRST alternative to all of them.** It is
the same multi-pair defect, through another door. It works natively (checked on the live graph) and
breaks mounted. On a small fixture it can coincide with the right answer — that is what made me
state before, wrongly, that variable-length traversal worked.

**Consequence for the partition-by-pair design:** it depends on that rewrite to
preserve `MATCH ()-[:CONTAINS]->()`. Without it, what is left is a UNION per table, which is correct for a
fixed-length pattern and **has no correct form at all for a variable-length path
crossing pairs** — which is exactly what the framework's impact queries
use (`-[:CALLS*1..3]->`). Variable-length traversal over ONE pair table is
correct and is tested.

### What this means for "not losing anything"

- **Data: 100% preserved**, verified on the real graph table by table — manifest = origin
  = mounted on every sampled pair, and the sum over the 97 tables closes with the origin's total.
  Self-loops included.
- **Query possibilities: NO**, and defect 2 is not fixable on our side.

The design decision stays open, now with measurement: the **single node table with the label as a
property** makes every edge type have a single pair, hence a single CSR, hence **no
alternation is necessary** — `[:CONTAINS]`, `type(r)` and `[:CALLS*1..3]` become native again.
The cost migrates to the node side, where the rewrites use only predicates I HAVE ALREADY MEASURED
working (equality on a non-key column, and `IN [...]` for a key).

## How the icebug blocker was resolved: partition by pair, natively (2026-08-21)

The Engineer defined the constraint — **"I don't want to lose any functionality of my graph
when it is rebuilt, no relation, no data"** — and authorized the native path,
**with no fallback to the Python tool**, pointing at the spec and the reference code.

### The design, and why it loses nothing

`internal/ladybugstore/icebug.go` — native writer, `ExportIcebug`.

- **Node tables: one per label, intact.** Same name, same columns, same primary
  key. So `MATCH (f:Function)`, `label(n)`, `ast_schema` and every property access
  stay identical.
- **Edge tables: one per triple (type, from, to)**, named `TIPO__From__To`. Each one
  carries exactly ONE FROM/TO pair, which is what the format requires. Nothing is merged, nothing is
  discarded: every edge falls into exactly one table.
- **The separator is `__`** because an edge type is UPPER_SNAKE and a label is CamelCase — neither
  of them contains `__`, so the triple is always recoverable. Tested including with `READS_FIELD`,
  which has an underscore in its own name.
- **The query surface is preserved by translation**, in the `translateLadybug` that already exists:
  `[:CONTAINS]` expands into the pair alternatives (`[:A|B|C]` is **natively supported** —
  measured), and `type(r)` normalizes with `string_split(type(r), '__')[1]` (`string_split` and
  `regexp_extract` exist — measured). The list of pairs comes from the artifact's manifest,
  **never from a fixed list** — the lesson of `tradutor-cypher-sem-lista-fixa.md`.

### What the native implementation gained over the tool

1. **Partitioning by pair**, which the tool does not know how to do: it derives the type from the
   table name and maps one pair per type.
2. **Zero runtime dependency.** No Python, no uv, no intermediate DuckDB. Only
   `arrow-go/v18`, which was already a dependency.
3. **`--add-reverse-edges` reimplemented** and fixed for a heterogeneous graph: the reverse only
   applies to a homogeneous pair. The mirror of `File->Function` is `Function->File`, which is ANOTHER
   pair and another CSR — writing it in the same CSR **would invent edges**. It has a test.

### Divergences from the spec, deliberate and measured

| Spec says | Measured | What we do |
|---|---|---|
| "Self-loops are excluded. Any row where `source = target` is not emitted" | The reference implementation **preserves** self-loops; the `not_equal` filter is only on the reverse-edge path | **We preserve them.** A recursive function is a real edge. It has a test |
| `{lowercase_table_name}.parquet` | The reader derives the file name from the **table name in the DDL, with the exact case** — it asked for `nodes_File.parquet` | Verbatim name, preserving the real label |
| `icebug_disk_version` in the metadata (value not stated) | The value is **`"v1"`**; `"1"` is rejected with "current ladybug version does not support icebug_disk_version: 1" | `"v1"` |
| Tables `{prefix}_mapping_{type}` and `{prefix}_metadata` exported | The tool's real output **does not contain them**, and the mount works without them | We do not emit them; our own manifest (`icebug.json`) covers the accounting |
| The first column is the primary key | `RETURN n.*` expands in declaration order, which puts the key first **by luck** | We project the key first, explicitly |

### Two reader traps that cost silence, not an error

1. **A column without an alias comes back all null.** `RETURN n.path` produces a column called `n.path`, and
   the reader matches by the name the DDL declares. Without `AS path`, it mounts, queries, and returns
   `NULL` in everything. All projections are aliased.
2. **`=` on the primary key returns empty.** The engine routes the predicate through a primary-key
   index that icebug does not provide, and answers empty instead of scanning. **It does not
   error.** Everything else works on the same column, including **`IN [value]`**, which is
   semantically identical for a single value — it is the rewrite the reader applies, without loss.
   Pinned by `TestIcebugPrimaryKeyEqualityNeedsIN`, which also detects whether the engine gains the
   index in the future.

### What the engine's `COPY TO` does not do

`KV_METADATA` is an "Unrecognized parquet option", so the engine does not stamp
`icebug_disk_version` into the node files it writes — and without it the reader warns, once
per table, on every mount. `stampIcebugVersion` rewrites the file with the metadata via an
Arrow round-trip (it preserves the types exactly, with no remapping). Cost: one extra read and
write per node table, only at publish time.

## The original blocker (historical): one CSR per edge TABLE, and our graph has 97 pairs

Probe: `internal/ladybugstore/icebug_probe_test.go`, behind `GRAPHIT_ICEBUG_DIR`.
Tool: `uvx icebug-format` (installed by the Engineer in this session).

### What WORKS, and was validated end to end

1. **`--backend pyarrow` converts with no extra dependency.** The default tries DuckDB and fails with
   `ImportError: duckdb is required by the 'convert' feature`; `--backend pyarrow` needs nothing
   beyond the package itself.
2. **The `--source-db` (DuckDB) path is the only one that serves a heterogeneous graph.** It
   **discovers multiple tables** (`Discovered node tables: [...]`, `Discovered edge tables:
   [...]`), honors `--schema` for the FROM/TO, and emits **a combined `schema.cypher`** with
   `contains(FROM file TO function)` and `calls(FROM function TO function)` — heterogeneous, correct.
3. **The `--source-dir` path does NOT serve.** It expects `<name>-v.parquet` +
   `<name>-e.parquet` pairs and models **one homogeneous graph per pair** (`FROM demo TO demo`, a table
   named after the file). With two pairs in the same directory it produces **two independent
   subdirectories**, with no combined schema. `--schema` is **ignored** on that path — tested.
4. **The icebug DDL mounts and answers Cypher.** `MATCH (n:file) RETURN count(n)` → 2, and the
   heterogeneous traversal `(a:file)-[r:contains]->(b:function)` → 2. **T9 is validated as a
   mechanism.**
5. **Table names are CASE-INSENSITIVE in Ladybug.** `icebug-format` lowercases
   (`File` → `file`), and `MATCH (n:File)` matches the table `file`. So the lower-casing is
   harmless and does **not** require label rewriting.
6. **`--storage` is ignored on the `--source-db` path** (it comes out as `storage = ''`), and
   **so is `--output-dir`** (it writes into `<stem>_csr`). Both have to be handled on the Go side
   — rewriting `storage` is our work anyway, because the URI is ours.

### What BLOCKS

**icebug-disk keeps ONE CSR per edge TABLE, and Ladybug reapplies it to every FROM/TO pair
declared.** Measured:

- `CREATE REL TABLE multi(FROM file TO function, FROM function TO function) WITH (…icebug…)`
  is **accepted** — the initial failure was a missing file, not syntax, and it looks for **a single**
  `indices_multi.parquet` for both pairs.
- Fed with the CSR of `contains`, which has **2 edges**, the two-pair table answers
  **4**. That is: the same CSR is interpreted once per pair. **Wrong data, silently.**

Hence, in an icebug schema **each edge table needs to declare EXACTLY ONE FROM/TO pair**.

**And this graph is not like that.** Measured on this project's graph:

| Edge type | Distinct (From,To) pairs |
|---|---|
| CONTAINS | **62** |
| REFERENCES | 9 |
| CALLS | 7 |
| READS_FIELD | 6 |
| HAS_PARAMETER | 5 |
| WRITES_FIELD | 4 |
| HAS_FIELD | 3 |
| IMPORTS | 1 |

~97 pairs in total, and **only `IMPORTS` is single-pair**. Encoding one rel table per pair produces
~97 tables with names like `contains_file_function`, and destroys `MATCH ()-[:CONTAINS]->()` —
which is in the AST skill, in the documentation and in practically every query of the framework — turning it into
a UNION of 62 branches.

**It is not a code problem: it is a capability gap of the format.** That is why T8 stopped here instead
of producing something that passes on a two-table toy graph and corrupts the real one.

### The ways out, and why none is mine to choose

1. **Ask upstream / wait** for multi-pair CSR support in icebug. It costs nothing now,
   but it does not unblock today.
2. **Remodel the graph** so that every edge type has a single pair — one `Entity` table
   with `label` as a property, and `CALLS(FROM Entity TO Entity)`. It genuinely unblocks, and it is
   a big redesign: a label stops being a table, so `MATCH (f:Function)` becomes
   `MATCH (e:Entity {label:'Function'})`, and the documented surface of the AST skill, the
   `ast_schema` and all the queries change.
3. **Keep Parquet-per-table for the graph** (downloading at install time) and have on-the-fly **only in
   the search**, with LanceDB — which does not have that limitation. It contradicts part 4 of the request for
   half of the graph, and preserves everything else.
4. **Hybrid** — icebug on the single-pair tables, Parquet on the rest. Two mount
   mechanisms coexisting; discarded for incoherence, recorded for completeness.

## Measured findings about the httpfs extension (2026-08-21)

Probe: `internal/ladybugstore/httpfs_probe_test.go`, behind `GRAPHIT_HTTPFS_PROBE=1`.

1. **The extensions directory is NOT `~/.lbug/extensions`** (which is what the docs and the web search say).
   It is `~/.lbdb/extension/<engine-version>/<os>_<arch>/<ext>/lib<ext>.lbug_extension`. It was
   extracted from the template `{}/.lbdb/extension/{}/{}/` inside `liblbug.so` itself.
2. **Download URL**: `https://extension.ladybugdb.com/v<version>/<os>_<arch>/<ext>/lib<ext>.lbug_extension`.
3. **Platform tokens belong to the server and do NOT match GOOS/GOARCH**: `linux_amd64`,
   `linux_arm64`, `osx_amd64`, `osx_arm64`, and **`win_amd64`** — not `windows_amd64`, which gives a 404.
   `win_arm64` does not exist. The Windows binary is 14 MB against ~1.4 MB for Linux.
4. **There is no build for 0.18.2**, which is the engine version that `go-ladybug v0.17.0`
   embeds; the newest published one is **0.18.1**. The 0.18.1 binary **loads on runtime 0.18.2**
   and `show_loaded_extensions()` confirms it (`source: USER`). That is why
   `LBUG_EXT_VERSION` is a variable separate from `LBUG_VERSION` in the Makefile.
5. **`INSTALL httpfs` and `LOAD EXTENSION httpfs` are SILENT no-ops** when the version's
   directory does not exist: both return success, and `show_loaded_extensions()` returning 0
   rows is the ONLY way to know. Hence the mandatory verification in `LoadExtensions`.
6. **An invalid file in the payload TAKES DOWN THE PROCESS.** Pointing `LOAD EXTENSION` at a
   404 HTML page does not return an error — it kills with **SIGBUS inside cgo**, which no
   `recover` catches. Hence `validateExtensionFile` (minimum size + ELF/Mach-O/PE magic bytes)
   BEFORE the LOAD, and hence the `curl -f` in the Makefile being load-bearing and not style.
7. **`CALL http_cache_file=true` works** — it is the remote-read cache, a candidate for turning on
   by default after measuring latency (T15).

## Trade-offs & Decisions

- **Python/uv as a requirement for whoever publishes.** Consciously accepted so as not to block the
  migration on a reverse-engineered format writer. It is an explicit deviation from the requirement of
  "self-contained dependencies", and it is limited to the **publishing** path — whoever only
  consumes an artifact does not need Python. Backlog item opened for the writer in Go.
- **Search leaves atomic publication, again.** It had already been like that since 2026-08-19 (the index is
  in-place, the graph is copy+swap). With both remote, the problem changes shape but does not
  disappear: a crash between the icebug upload and the LanceDB one leaves graph and index
  describing different corpora. Chosen mitigation: the registry only starts pointing at the
  new version **after** both prefixes have gone up — the pointer is the commit.
- **Remote query latency was not measured.** This framework's queries are point
  lookups and traversals, not analytical scans, and no number exists yet for that
  shape of load against S3. Declared as not measured until T15.

## Technical Debt

- [ ] **`DownloadArtifact` still downloads `ast`/`knowledge`** — `TODO(T9)` in
  `internal/hub/s3_store.go`. It goes away when the mount by `storage='s3://…'` and the opening of
  the index by prefix land. `EnsureArtifactLocal` is already the destination behavior and refuses.
- [ ] **Memory is still in git** (`internal/memory/memory_git_store.go`), and `memory.repo`
  still exists. That is T6. `setup` already does not ask for it, so in the interval memory runs
  local-only — which is exactly what an empty `memory.repo` always meant.
- [ ] A native icebug-disk writer in Go, to remove the Python/uv dependency at publish time
  (item in the improvements backlog — see `graphit_improvements_backlog_list`).
- [ ] Measure traversal latency against `s3://` and decide whether a local read cache
  (`CALL HTTP_CACHE_FILE=TRUE`) should be on by default.

## System Knowledge

- **The launcher is this project's native distribution mechanism.** `cmd/launcher/`
  self-extracts the embedded payload to `~/.graphit/runtime/<version>/`: the core, the MCP proxy,
  `liblbug`, ONNX Runtime, ICU and the grammar YAMLs. Any new native — `httpfs`, the
  LanceDB binaries — comes in through there, and the `Makefile` already has the mold (`setup-lbug`,
  `fetch-ort-*`) for downloading per platform and copying into `cmd/launcher/runtime/`.
- **The Hub's GitStore has five responsibilities, not one.** Registry on `main`, one orphan
  branch per artifact/version (`WriteArtifactBranch`/`EnsureArtifactClone`), telemetry in
  `refs/events/*` (never on a branch), rule distribution through `main`, and memory
  worktrees. Whoever replaces it needs all five — swapping only the artifact one leaves the package
  half-git.

## Progress Log

### 2026-08-21

- Log opened before any edit. Feasibility research closed: `icebug-format`'s `--source-dir`
  accepts the Parquet form the project already produces, and LanceDB has an official Go SDK
  with FTS+vector+hybrid+S3 — the design's two biggest risks fell.
- Four decisions gathered from the Engineer (total scope of S3, `uvx` for now, `httpfs`
  pre-embedded, credential through the standard AWS chain) and recorded above with the
  discarded alternatives.
- Backlog item opened for the icebug-disk writer in Go.
- **Phase A concluded — T1, T2 and T3, compiling and with a green test.**

  **T1 (config).** New `internal/config/hub_s3.go`: `S3Config{Bucket,Region,Endpoint,Prefix}`
  with `Configured()`, plus `ResolveHubBucket/Region/Endpoint/Prefix` and the argument-less
  shortcuts. `ResolveHubRepo`/`HubRepoURL` left `config.go`.
  `brand.DefaultHubRepoURL`/`DefaultMemoryRepoURL` became `DefaultHubBucket`/
  `DefaultHubRegion`/`DefaultHubEndpoint`, and the `Makefile` followed
  (`DEFAULT_HUB_BUCKET`/`REGION`/`ENDPOINT` in the `-X`).
  `S3Config` **has no credential field, on purpose** — the secret is resolved by the
  SDK's standard chain and never read, written or logged by us.

  **T2 (`internal/s3store`).** `store.go` + `uri.go`: `Get`, `Put`, `Delete`, `Exists`,
  `List` (paginated by continuation token), `DeletePrefix` (in batches of 1000, because the
  unit of an artifact is a prefix, not a file), `UploadDir`, `DownloadPrefix`,
  `EnsureBucket`, `Key`, `URI`. Two sentinel errors that callers need to distinguish:
  `ErrNotConfigured` (local-only mode, not a failure) and `ErrNotFound` (missing object — first
  run — against a transport or permission failure).
  A configured `endpoint` implies `UsePathStyle` — MinIO and most compatible
  servers do not serve a bucket in virtual-host style.
  Test: `store_test.go` brings up an **in-process fake S3** (`httptest`) implementing
  HeadBucket, Get/Put/Head/Delete, ListObjectsV2 and DeleteObjects — **no test touches the network
  or a real bucket**. 13 cases, including the exact URI format the two engines mount.

  **T3 (`setup`).** Both git repository prompts left; bucket, region and
  endpoint come in. When the bucket is given, `verifyHubBucket` does a `HeadBucket` and **fails the
  run** naming the bucket, the endpoint, the `AWS_ACCESS_KEY_ID`/
  `AWS_SECRET_ACCESS_KEY` variables and the region as the probable cause — the same discipline as the
  model download, which the project's memory records as a rule ("a half-done setup does NOT report
  success"). The duplication of the three nearly identical prompt blocks was extracted into
  `promptValue`/`promptSimple`.
  Verified: `go build -tags fts5 ./...`, and `go test` green in `config`, `brand`,
  `s3store`, `hub`, `hub/adapters/ide`, `memory`, `mcpstdio`, `cmd/...`.

- **Phase B's contract written before the code:** `docs/specs/hub-s3-object-layout.md` —
  key convention for the five responsibilities leaving git (registry, artifacts,
  events, rules, memory), JSON Schema of the entry file, of the project file and of the telemetry
  event, which prefixes the two engines mount directly, and the publishing order that
  makes the entry file the commit (prefix first, pointer afterwards). T4 becomes mechanical.
  The `type` enum checked against `internal/projectlock/projectlock.go`, not deduced.

- **T7 (the httpfs half) concluded.**
  `Makefile`: new `LBUG_EXT_VERSION`/`LBUG_EXT_HOST`/`LBUG_EXT_CACHE`, plus the function
  `fetch_lbug_ext <token>` called by `build-linux` (`linux_amd64`), `build-darwin`
  (`osx_arm64`), `build-windows` and `build-windows-native` (`win_amd64`). It lands in
  `cmd/launcher/runtime/lbug/httpfs.lbug_extension`, and the launcher already embeds and extracts
  recursively (`//go:embed runtime/*` + `fs.WalkDir` + `MkdirAll`), so **no
  change was needed in the launcher**.
  New `internal/ladybugstore/extension.go`: `ExtensionDir`/`ExtensionPath` (under
  `brand.RuntimeDir(version.Version)`, the same pattern as `runtimeQueriesDir`),
  `LoadExtensions` rewritten to load by path **with verification**,
  `LoadedExtensions`, `validateExtensionFile`, `ConfigureS3` in option form, and
  `EnableRemoteCache`. The old `LoadExtensions` (which did `INSTALL` + `LOAD` by name and was
  dead code) left.
  8 tests, green, including the SIGBUS regression guard.

- **T4 concluded — `internal/hub/s3_store.go`.** A new type, complete, tested, and
  **deliberately without touching the callers** so the build is never broken in the middle of the
  phase. It covers the five responsibilities: registry (`ReadFile`/`WriteFile`/`RemoveFile`/
  `ListDir`), artifact (`ArtifactPrefix`/`ArtifactURI`/`PublishArtifact`/`DeleteArtifact`/
  `EnsureArtifactLocal`), telemetry (`WriteEventFile`/`SyncEvents`/`EventKey`), rules
  (`ReadRule`/`ListRules`/`WriteRule`), and a generic `ReadJSON` that **refuses** a manifest of a
  future version instead of parsing it.
  Three decisions worth recording:
  - **`EnsureArtifactLocal` REFUSES a mountable type** (`ast`/`knowledge`) instead of downloading.
    Downloading there would silently reintroduce exactly the transfer this migration
    removes; the error points at `ArtifactURI`.
  - **A download goes to `<dest>.partial` and then `rename`**, because the reuse check
    trusts "directory not empty" and an interrupted download would poison it forever.
  - **`ArtifactPrefix` mirrors `ArtifactBranchName` segment by segment**, including the
    rule that `ast`/`knowledge` do not carry the `id`. A test compares the two.
  The fake S3 moved out of `internal/s3store/store_test.go` into `internal/testsupport/fakes3.go`,
  because the `hub` package needed the same one and duplicating it would be the second thing to
  maintain.
  19 new tests, green.

- **T5 concluded — git left the Hub.** `internal/hub/git_store.go` (1000+ lines),
  `git_store_test.go` and `git_store_sync_test.go` **deleted**. Acceptance criterion verified:
  no `exec.Command("git")` is left in the `hub` package. **The repository's full suite green.**

  **What `S3Store` gained for the rewire to fit:**
  - **Local mirror of the registry.** `RegistryMirrorDir`/`AbsPath`/`SyncRegistry`. The registry is
    small JSON metadata and the code that reads it **walks a directory** (`BuildRegistryCache`
    → `os.ReadDir` → `loadProjectDir`); mirroring it preserves that code entirely. **This does NOT
    reintroduce the download the migration removes**: what can never come down is the heavy half
    (graph + index of a mountable artifact), and that one is still read in place.
    `SyncRegistry` **replaces** the mirror instead of merging — an entry file deleted
    remotely has to disappear locally, otherwise it announces a version whose prefix no longer exists.
  - **`RegistryRevision`** replaces `HeadCommit()` as the cache's validity marker: one
    listing over a prefix of small JSONs, hash of `key:size`. Accepted and documented
    limitation: it does not detect a rewrite of the SAME size — it does not occur, because an entry file carries
    the version in its name.
  - **`WriteFile`/`RemoveFile` write BOTH sides** (bucket + mirror). That is what keeps
    a read immediately after publishing consistent, which is what the commit guaranteed.
  - **`contexts/<kind>/<projectID>/<subdir>/`** and `PublishContextDir`/`FetchContextDir`, for
    `knowledge export`/`install`, which was an **unversioned** branch (`knowledge/project/<id>`)
    with a worktree + commit. Publishing **deletes the prefix before uploading** (mirror, not merge —
    a page deleted at the origin disappears). Fetching a context never published **is not an error**.
  - **`DownloadArtifact`** separate from `EnsureArtifactLocal`, with a `TODO(T9)`: until the graph is
    mounted by `storage='s3://…'`, installing `ast`/`knowledge` still needs the bytes. It is
    the exception that has to die, and that is why it has a name of its own instead of being a flag.
  - **`ArtifactCacheDirIn`** free of a store, because `resolveArtifactPath` only needed the
    path and built a whole `GitStore` to compute it.

  **What disappeared, and why:** `CommitAndPush` (every write is already durable — three
  calls removed), `Sync`/`EnsureCloned` (→ `SyncRegistry`), `HeadCommit`
  (→ `RegistryRevision`), `WriteArtifactBranch` (→ `PublishArtifact`),
  `DeleteArtifactBranch` (→ `DeleteArtifact`), `EnsureArtifactClone` (→ `DownloadArtifact`),
  `MemoryWorktree`/`ExtractBranchDir` on the knowledge path (→ `PublishContextDir`/
  `FetchContextDir`), `ArtifactBranchName`, and all the lock/rebase/worktree/
  event-refs machinery.

  **`RegistryManager` started keeping the constructor's `ctx`** (`baseCtx`). It already received one
  and discarded it; all of its methods are Hub I/O, called by the command that
  built it. The alternative was `ctx` in twenty signatures with no gain.

  **Callers swapped:** `internal/hub/registry.go` (~20 points), `event_tracker.go`,
  `lifecycle.go` (3), `service.go` (2, including `resolveArtifactPath`),
  `internal/mcpstdio/tools_knowledge.go` (4 + `installKnowledgeContext`),
  `tools_lifecycle.go`, `cmd/graphit/commands/{setup,lifecycle,runners}.go` (6).
  Tests adapted instead of deleted: `registry_test.go` (the persistence one now asserts
  **both sides**, bucket and mirror), `coverage_extra_test.go`, `event_tracker_test.go`.
  Three tests lost `t.Parallel()` because the Hub's config comes in through an environment variable and
  `t.Setenv` does not coexist with parallel.

- **Intermediate state, explicit:** between T3 and T4/T6 the Hub and memory run
  **local-only**. `GitStore.hubGitRemote()` returns `""` with a `DECISION` comment
  pointing at this log — the three points that consulted `hub.repo` already treated `""` as
  "no remote", so nothing pretends to work: the bucket is configured and validated, and it starts
  being used in fact when T4 replaces `git_store.go`.

### 2026-08-24 — T17

- Fix reopened because of a timeout observed on a three-hop query over the icebug artifact on S3.
- Memory and the wiki reconfirmed the earlier control: the same traversal finishes in 2.133 s on native
  storage and exceeds 100 s on icebug, including with reverse edges; the slow plan enumerates all the nodes
  before the `RECURSIVE_EXTEND`.
- Initial research in primary sources found explicit upstream pending items for recursive join:
  global initialization of structures, filter pushdown, on-disk graph header cache,
  sequential/batched scanning and bidirectional joins. Ladybug's current documentation confirms that
  S3 is lazy per row group.
- The local code already implements reverse edges in a separate and semantically correct table, but
  `ExportGraphToIcebug` does not activate `AddReverseEdges`; the Cypher `*1..3` goes through intact to the engine.
- The Engineer added as mandatory evidence the official notebook
  `https://github.com/LadybugDB/ladybug-icebug-notebooks/blob/main/index.ipynb`; the next step is to
  extract its cells and outputs before closing the design of the fix.
- Notebook investigated: `index.ipynb` is just a catalog. The relevant example
  `ladybug_icebug_disk_karate.ipynb` uses `--add-reverse-edges` to correctly represent
  an **undirected** dataset (78 edges become 156 CSR entries), builds local files and
  executes only one-hop patterns. No notebook in the index contains S3, remote query,
  multi-hop, benchmark, cluster ordering or row group tuning. Conclusion: it is evidence for
  exporting the reverse when the semantics call for symmetric adjacency, not evidence that it cures the
  `RECURSIVE_EXTEND` plan responsible for the timeout.
- The Engineer corrected the scope: reverse edges are mandatory in our own writer because the agent
  needs to be able to query without direction. Criterion fixed: publish `TIPO_REVERSE` separately and
  combine the two adjacencies only for `-[:TIPO]-`; the original directed relation will not be
  artificially duplicated.
- Delivery order fixed by the Engineer: conclude, test and **commit T17.3 in isolation**
  (export of reverse edges; the transparent use stays in T17.4) before starting the `ANALYZE` and
  any change to indexes/plan from T17.1/T17.4. The first commit will contain no performance
  optimization.
- The default was also fixed as a positive and intentional configuration:
  `hub.icebug.reverse_edges=true`. Resolution uses the existing hierarchy (inline → environment →
  project → global → compiled default) and only an explicit `false` disables it. `IcebugOptions{}`
  keeps the same safe default; the API receives the publisher's already-resolved decision. Spec/architecture
  documentation goes into the same first commit.
- Initial implementation of T17.3 applied: `IcebugOptions{}` now generates the reverse tables;
  `hub.icebug.reverse_edges` resolves through the normal hierarchy and only `false` disables it; the
  `RegistryManager` preserves the project's configuration map and hands it to the artifact
  preparation. The direct table remains intact and the mirror stays in `TIPO_REVERSE`.
- Regressions added for both sides of the contract: a standard publication must contain
  `_REVERSE`; `hub.icebug.reverse_edges=false` in the project map must remove it; the writer
  must also accept an explicit opt-out and mark the manifest correctly.
- First focused run: the writer passed; the configuration failed because the fixture modeled three
  nested maps. `ConfigMap` splits only at the first dot, therefore the correct representation
  of `hub.icebug.reverse_edges` is section `hub` + key `icebug.reverse_edges`. Fixtures corrected;
  the public key and its environment variable do not change.
- Second focused run green in `internal/config`, `internal/ladybugstore` and `internal/hub`.
  `docs/specs/config_module.md` and `docs/specs/hub_collaboration.md` now document the key,
  precedence, environment variable, lockfile format, two CSR tables, self-loop, logical
  count and the guarantee that directed queries do not receive the mirror.
- The expanded suite found and fixed pre-existing incorrect reverse metadata: the mirrored CSR
  was right, but the manifest copied the direct pairs and their counts. Now each pair records
  `To → From` and excludes self-loops from `Rows`; a regression checks orientation and the sum of the pairs.
  Tests that validated only the direct tables started distinguishing derived entries.
- `go test ./internal/ladybugstore -run TestIcebug -count=1` green after the fix; the default build
  of `internal/ast` also went back to compiling with the `hasName` helper fixed in a separate task.
- Focused native validation green: `internal/ast` and `internal/hub` with `-tags lancedb`, including
  publication, on-the-fly mount and the hybrid search floor.
- Full suite of the affected modules green with `-tags lancedb`: `internal/config` (0.009 s),
  `internal/ladybugstore` (4.067 s), `internal/ast` (86.344 s) and `internal/hub` (2.736 s).
  T17.3 closed for commit: reverse by default, layered opt-out, exact manifest and documentation.
- Pre-commit review made the configuration dependency explicit: `prepareASTPublish` takes a
  mandatory `ConfigMap` (accepts `nil`) instead of a variadic; the manager keeps the value as
  `projectConfig`. All the test callers were updated, avoiding an API that hid
  which project decides the opt-out.
- T17.3 was delivered in isolation in commit `42cc1af`; the fix to the `hasName` test helper,
  needed for the variant without `lancedb`, stayed isolated in commit `3c26cd8`.
- T17.1/T17.4 resumed only after those commits. Memories and the plan were re-read. Two
  proven restrictions remain: materializing the reverse does not on its own change the recursive plan that enumerates
  all the nodes, and splitting the Icebug files into several row groups silently breaks the binding
  of endpoints in the current reader. This phase's measurement will compare the original query, the
  reverse traversal anchored on the filtered node, unrolled fixed hops and the undirected pattern over the same
  real graph, before choosing the implementation.
- Reproducible diagnosis added in `TestIcebugRealGraphThreeHopPlans`, over the current real graph
  and the target `internal/ast/ladybug.go::runQuery` (4 direct callers). `EXPLAIN` shows that the
  original query and the recursive reverse remain on `READ_FTABLE → RECURSIVE_EXTEND`; the reverse
  gains a `SEMI_MASKER` over the filtered target, but still exceeds 30 s. The native control returns the
  7 transitive callers in 8.6–13.6 ms.
- Unrolling the expression into a single 2-hop pattern also exceeds 30 s: the plan becomes multiple
  `SCAN_REL_TABLE`/joins and does not use the adjacency as a selective expansion. In contrast, BFS on the
  caller, with three independent **one-hop** queries over `CALLS_REVERSE`, returned exactly
  the same 7 UIDs in 291.97 ms (101.20 + 97.45 + 93.33 ms). Therefore T17.4 should intercept only
  the semantically safe bounded-reach form and feed the frontier between queries; changing
  the orientation or generating a Cypher chain does not fix the upstream operator.
- Acceptance regression opened before the implementation in
  `internal/ast/ladybug_icebug_traversal_test.go`: it covers the documented public form with a label
  (`t:Function`), filters/params separated per endpoint, directed reach from the source and the
  undirected pattern. The baseline fails as expected: logical labels do not exist in the mounted catalog
  (`Present: CALLS, CALLS_REVERSE, Entity`) and the form without a label still returns empty on the
  recursive plan. The optimization will be deliberately narrow: only `RETURN DISTINCT` reach whose
  expressions belong to the reached endpoint; aggregates, paths and predicates crossing endpoints
  stay in the engine so as not to change semantics silently.
- First implementation of the planner applied in `internal/ast/ladybug_icebug_traversal.go` and wired in
  before `runQuery`: it identifies a mounted catalog by `Entity` + reverse tables, resolves the
  selective endpoint, expands up to 8 levels in batches of 512 UIDs and materializes the `RETURN DISTINCT`
  only on the reached endpoint. For the inverse direction it uses `TIPO_REVERSE`; for `-[:TIPO]-` it queries
  `TIPO` and `TIPO_REVERSE` separately, avoiding the upstream alternation defect. The form without
  `*1..N` is also served for one undirected hop. Queries with an edge/path variable, aggregation,
  `ORDER BY`/`LIMIT`, without an anchor or with a predicate crossing endpoints are not intercepted.
- The four acceptance tests went green after the implementation. A negative matrix was added
  to prove that the narrow planner refuses forms whose semantics it cannot preserve; the
  next validation is that matrix plus the real graph benchmark through the `LadybugBackend.Query` API.
- The negative matrix went green and the undirected case started using the normal public syntax
  `-[:CALLS]-`, without needing to write `*1..1`. An opt-in real-graph cost test was added:
  it compares the UIDs returned by the native and by the Icebug catalog for the same public query and
  fails if the planner's local execution exceeds 5 s.
- The first run of the real test exposed a false positive of the control itself: the map form
  `{uid: '…'}` answered zero on both sides. The test was switched to the proven workaround
  `uid IN ['…']` and now also requires the known cardinality of 7, besides comparing the sets;
  two empties never count as equivalence again.
- With `uid IN` the native control finds 7 again, but the first run of the real planner still
  returned zero in 53.8 ms, while the same manual strategy in the writer's test had returned
  7. The investigation was reopened at the anchor point: the test now records first the node-only
  lookup of `uid` and its `label`, separating a failure to resolve the target from a failure in the reverse CSR.
- The anchor proved the data and found the cause of the zero: `runQuery` is a `Method`, not a `Function`. The test had
  reintroduced exactly the assumption the public query avoids. The real query was corrected to
  `(label(t) = 'Function' OR label(t) = 'Method')`, keeping the `uid IN`; now it measures the
  documented case instead of a deliberately wrong label.
- With the anchor correct, the BFS reached the UIDs, but the batched node-only materialization
  `caller.uid IN [lista]` over the real `Entity` returned 5,922 rows; the same does not happen on the small
  fixture nor on the relational expansion. The separation of responsibilities was kept: frontiers
  remain in batches of 512 on the CSR, but each final UID is materialized in isolation with
  `IN ['uid']`, the reader's demonstrably exact form. The control's cardinality stopped being
  fixed at 7 because the planner's own new methods have already increased `runQuery`'s callers;
  the correct criterion is a set identical to the native one and not empty.
- Materializing one UID at a time did not finish in 60 s, indicating that the planner's `reached` set
  may already be inflated — not only the final query. Before a third change, the real test
  started executing and explicitly recording each `CALLS_REVERSE` frontier (cardinalities of
  hops 1, 2 and 3) through the same API, to locate exactly where the divergence begins.
- The manual execution isolated the CSR: 5, 6 and 4 UIDs on hops 1–3, with no explosion. Cardinality-only
  `DEBUG` logs (anchor, frontier, reached and materialization) were added to the planner and the
  real test activates that level, to compare the internal path without recording UIDs or content.
- The logs located the cause: the internal anchor had 6,298 rows. The partitioner removed the
  parentheses of `(label(t) = 'Function' OR label(t) = 'Method')` and then joined the predicate to
  `t.uid IN [...]` with `AND`; by precedence, that meant `Function OR (Method AND uid)`.
  Each partitioned predicate is now explicitly re-grouped before the join, preserving the
  original boolean tree.
- The Engineer reaffirmed the format restriction that governs any attempt at optimization by
  index/locality: **every Icebug Parquet must contain exactly one row group**. In the current reader,
  multiple row groups produce silently incorrect answers on large bases when a node
  variable is bound to the edge. Therefore T17 will not split the files to obtain pruning or
  smaller range reads; the optimization stays restricted to the reverse CSR, the selective plan by one-hop
  frontiers and, only if measured safe, the read cache. The test
  `TestIcebugWritesOneRowGroupPerFile` remains a mandatory regression criterion.
- The first validation on the real bucket did not reach the query: httpfs answered `400 malformed Host
  header`. The global configuration accepts `http://localhost:9000/`, but
  `resolvedLadybugS3Credentials` removed only the scheme and handed `localhost:9000/` to the engine,
  even though the contract requires `host[:port]`. Normalization now also removes trailing slashes and the
  regression covers HTTP endpoint, path-style and `DisableSSL`; the S3 probe will be repeated with the same
  self-contained temporary prefix, removed at cleanup.
- With the endpoint normalized, the real S3 validation went green: the bundle was exported with a remote
  URI, uploaded to the temporary prefix, mounted without download and queried through the public API. The
  3-hop traversal returned exactly the same set as the native storage and the cleanup removed the
  prefix. In the repetitions during the reindexing of our own code, it came in at **480–713 ms via S3** and
  **351–387 ms on the local filesystem**; the cardinality changed with the live graph, so the test compares
  non-empty sets instead of freezing a number. The original recursive query and the reverse one
  remained above 30 s in the earlier diagnosis.
- The UID-only return stopped re-reading `nodes_Entity.parquet`: the UIDs already deduplicated by the
  CSR are the result. Projections of other properties remain materialized UID by UID because
  of the defect measured in the batched node-only lookup; an additional regression guarantees that
  `RETURN DISTINCT` remains global even when several nodes project the same value.
- The conservative review of the parser closed one more false "anchor": constant predicates such as
  `WHERE 1 = 1` do not count as selective and now fall into the engine without interception. The
  materialization phase also observes context cancellation between individual reads.
- Conclusion on indexes and locality, closed with evidence and incorporated into the spec
  `docs/specs/hub_collaboration.md`: (1) the only index relevant to traversal is the CSR —
  `indptr`, `indices` and the pre-materialized reverse table; (2) LadybugDB does not provide a secondary
  index for that path — in `translateLadybug` (`internal/ast/ladybug.go`) `CREATE INDEX` and
  constraints are no-ops; (3) the LanceDB index is the AST's textual/vector search, does not take part in the
  expansion of Icebug edges and does not fix the `RECURSIVE_EXTEND`; (4) with exactly one row group
  mandatory there is no row-group pruning — the anchor may read the whole `Entity` file, a cost
  accepted for correctness, and splitting the file is forbidden; (5) ordering nodes by cluster/locality was
  discarded without a favorable benchmark, because a single row group eliminates the main benefit of
  pruning and the IDs are already dense with contiguous labels; the proven improvement came from the anchored
  selective plan plus the reverse CSR; (6) a UID-only projection does not re-read `nodes_Entity.parquet`.
- The httpfs cache (`CALL HTTP_CACHE_FILE=TRUE`) researched in the official documentation and deliberately
  NOT enabled by default: the cache downloads the whole remote file, is visible only during the
  transaction and is discarded on commit/rollback — since each planner expansion is a separate
  query/autocommit, it may download files repeatedly and worsen latency, contradicting the
  on-the-fly requirement. Any future cache test needs to be an explicit cold/warm remote benchmark,
  opt-in, without becoming the default without evidence.
- The expanded suite of the four affected modules (`go test -tags lancedb ./internal/config
  ./internal/ladybugstore ./internal/ast ./internal/hub -count=1`) showed an intermittent native
  failure: `internal/config`, `internal/ladybugstore` and `internal/hub` passed (0.008 s /
  4.063 s / 3.493 s) and `internal/ast` died with `SIGSEGV` after 47.828 s, with no useful Go stack —
  a native/cgo process failure. On the ISOLATED repetition (`go test -tags lancedb ./internal/ast
  -count=1 -json`) the same package passed in full in 71.485 s, with all the final tests
  recorded as PASS. Deliberately recorded as a native flake not attributed to the planner: no
  evidence links the SIGSEGV to the change, the isolated run covers the whole affected package and the next
  step of this verification is to repeat the expanded suite; if it reappears, isolate the active
  combination/test before concluding.
- Consolidated upstream research: `index.ipynb` is a catalog; the relevant notebook
  (`ladybug_icebug_disk_karate.ipynb`) proves only the SEMANTIC need for reverse edges in an
  undirected graph — it does not contain S3, multi-hop, benchmark nor row group tuning, therefore it is not
  evidence that the reverse cures the recursive plan. Upstream pending items classified in T17.2:
  recursive join backlog (kuzu#4285), global initialization (kuzu#4941), 2-hop explosion
  (kuzu#4459), filter placement (kuzu#5040) — all dependent on an upstream fix, not applicable
  here; httpfs S3 lazy per row group confirmed; Parquet row groups (Arrow blog) consulted.
  No upgrade of Ladybug/go-ladybug in this task (0.17.0 stays), by the Engineer's decision.
- Repetition of the expanded suite on 2026-08-25: the environment had lost `.native/liblancedb_go.so`
  (the build cache `/tmp/lancedb-native-cache` does not exist either and there is no `cargo` in PATH), so
  the library was restored from `~/.graphit/runtime/dev/liblancedb_go.so` to `.native/`. With it
  in place, the full suite (`go test -tags lancedb ./internal/config ./internal/ladybugstore
  ./internal/ast ./internal/hub -count=1`) passed TWICE consecutively: config 0.007 s /
  ladybugstore 2.561 s / ast 54.392 s / hub 1.442 s and then config 0.005 s / ladybugstore
  2.510 s / ast 50.070 s / hub 0.878 s. The earlier SIGSEGV did NOT reproduce; it remains classified
  as an intermittent native flake not attributed to the planner, to be investigated only if it reappears.
  `go vet` flags nothing in the changed files (only pre-existing unreachable code in the
  generated ANTLR parsers) and `git diff --check` is clean.
- Final benchmarks of this session, the same public 3-hop query anchored at
  `internal/ast/ladybug.go::runQuery`, set identical to the native storage and not empty:
  local filesystem **291.06 ms** (10 rows) and **real S3 429.13 ms** (10 rows, bundle exported to a
  temporary `diagnostics/` prefix of the configured bucket, mounted without download and removed at
  cleanup). Earlier controls: native 8.6–13.6 ms; the original recursive, the reverse and the fixed chain
  above 30 s.

### 2026-08-25 — reassessment of "table per label + real FROM/TO" at the Engineer's request

- The Engineer questioned the folded layout (`FROM Entity TO Entity`) and asked to test a node table
  per label with real FROM/TO, hypothesizing it might even dispense with the planner in Go. The protocol of memory
  `01M0MJEX7CS29H2NPCHF60Z764` was followed: the proofs were RE-RUN live before answering.
- `TestIcebugMultiPairRelTableCannotWork` PASS: a CSR of 300 edges declared over TWO pairs
  returns 600 (`[:R]` reads the CSR once per pair) and 300 phantom edges (NB ids read in the id
  space of NA). The restriction is the FORMAT's — `nodes_<t>`/`indices_<rel>`/`indptr_<rel>` are
  keyed by the TABLE name; there is nowhere to store a second CSR per pair, and a target id only
  makes sense in the dense space of ONE node table. No engine fix changes that.
- `TestIcebugAlternativesWithAFilteredEndpointIsWRONG`, run against today's real corpus,
  confirmed that the defect of alternatives with a filtered endpoint PERSISTS upstream:
  `[CONTAINS|CALLS]=0` and `[CALLS|WRITES_FIELD]` with ~+30 phantoms vs `[CALLS]` exact.
  The trigger for reconsidering partitioning by pair did NOT fire — the route stays discarded,
  now with evidence from today, not from 2026-08-22.
- BONUS — a bug of OURS found in the guard itself: since `42cc1af` the test built
  `rows[r.Type] = r.Rows`, and the REVERSE tables carry the `Type` of the base relation — overwriting
  the direct count with the reverse one (which excludes self-loops). The test accused a "+16 in the unfiltered
  union" that never existed in the engine; it was its own expected sum corrupted by the mirror.
  Fixed to ignore `r.Reverse`; two stable PASS repetitions. Nobody caught it before because the
  test skips without `GRAPHIT_REAL_STORE`. A temporary diagnostic (deleted) proved file-by-file
  integrity: manifest == rows of `indices_*.parquet` == last `indptr` for CALLS and
  CONTAINS, cold and warm unions exact, order of the alternatives irrelevant.
- Conclusion maintained WITH renewed evidence: a single node table on the icebug path; the local native
  store stays per label; the bounded planner remains necessary for multi-hop.

### 2026-08-25 — T18: canonical conformance with the icebug-format (real tables, no fold)

**Objective.** The Engineer fixed the direction: follow the format AS IT SHOULD BE, per
`docs.ladybugdb.com/import/icebug/` and the official tool's code
(`~/Downloads/icebug-format-main`, installed as the AST context `icebug-format`):
a node table PER LABEL with real FROM/TO in the relations — no `Entity→Entity` fold. Hypothesis to
prove: in that layout the reverse works without phantoms (each table has its own id
space) and perhaps the planner in Go becomes unnecessary.

**Facts already extracted from the official source (context `icebug-format`).**
- The tool emits ONE node parquet per type (`nodes_<tipo>.parquet`) and ONE
  `indices_/indptr_<rel>.parquet` pair per REL TABLE whose name is the same as the DDL's.
- Multi-pair in the input becomes MULTIPLE edge tables (one per pair) — the generated schema never groups.
- `--add-reverse-edges` DUPLICATES the rows inside the SAME CSR (self-loop once), making the
  relation symmetric — suitable for undirected graphs like Karate; it does NOT preserve directed
  semantics. For AST, the correct canonical equivalent is an explicit mirror rel table per pair
  (e.g., `calls_reverse(FROM method TO function)`), which in the canonical layout has no phantom at all.
- This project's NATIVE store already creates `CREATE REL TABLE GROUP CALLS(FROM Function TO Function,
  FROM Function TO Method, ...)` over node tables per label — the group mechanism exists in the
  engine for local storage.

**Plan.**
- [ ] T18.1 — Decisive experiments with a minimal bundle made by hand: (a) does the pure canonical layout as
  in the docs mount and query; (b) is `CREATE REL TABLE GROUP ... WITH (storage=..., format=
  'icebug-disk')` accepted by the engine? which files does it expect per member?; (c) does a mirror rel
  per pair answer without phantoms in both directions.
- [ ] T18.2 — Decide the export design according to the results: canonical-group (public name
  preserved) or tables per pair + query layer. Measure the recursive 3-hop on the real corpus in the
  chosen layout BEFORE porting the complete writer.
- [ ] T18.3 — Implement, test (round-trip, self-loop, properties, reverse, one row group),
  benchmark against native and against the current folded layout, document and commit separately.

Status: IN PROGRESS — T18.1 running.
- T18.1 CONCLUDED (2026-08-25, corpus frozen in /tmp/opencode/frozen-ladybug after instability
  of the live store during reindexing):
  (a) the pure canonical layout mounts and queries (E1); a mirror per pair answers exactly, zero phantoms
  (E2);
  (b) a preserved public name is IMPOSSIBLE over icebug-disk: `CREATE REL TABLE GROUP ... WITH`
  and the NEW syntax `CREATE REL TABLE knows(FROM user TO city, FROM user TO town) WITH` both
  require a single `indices_<tabela>.parquet` (E3/E4/E6) — the icebug reader treats any table
  as ONE CSR by name, group or not. The official docs confirm: GROUP deprecated since v0.8.0,
  an internal member is called `<Rel>_<From>_<To>` locally, but the file format has no per-pair notion;
  (c) recursion on the canonical layout works semantically but is SLOW: single-pair anchored 1m33s,
  cross-label alternation 1m51s vs native 16ms vs folded+planner 0.29s — the
  RECURSIVE_EXTEND plan does not push the anchor's filter in ANY layout;
  (d) DATA QUALITY discovered in the live native store: ~1,600 DUPLICATE uids in Function and
  145 invalid UTF-8 strings ("cmd/\xAB\x06") — a known backlog class (Comment), now
  confirmed in Function; a future exporter needs dense dedupe + sanitization.
  Scratch of the experiments: internal/ladybugstore/zz_canon_test.go and zz_canon_real_test.go
  (they will become regressions in the implementation).
- Self-loop research (the Engineer's request): no documented upstream reason (0 issues in the
  tool); reconstruction by evidence — the exclusion exists because the original use case
  duplicates edges in the same CSR under `--add-reverse-edges` and without a `rel_id` a duplicated self-loop
  would become two logical relations; the tool cut the node instead of building an edge identity.
  The OPEN issue LadybugDB/ladybug#505 formalizes the future model: a logical `rel_id` +
  `indices_bwd_<rel>.parquet`/`indptr_bwd_<rel>.parquet` as directional indexes (mirrors are not new
  relations) and the invariant "self-loops must not create duplicate logical relationships".
  Experiment E7 proved that the icebug-disk reader accepts a self-loop in a canonical CSR today
  (count=3 exact; the pattern (x)->(x) resolves). POLICY FIXED: the canonical exporter keeps a
  self-loop ONCE in the pair's direct CSR (a conscious deviation from the tool's current emission,
  aligned with the normative direction of #505); mirrors exclude it; equality with native preserved.
- Closing of the Engineer's two provocations (2026-08-25):
  (1) MULTI-ROW-GROUP measured with a control matrix on the current engine: `indices_<rel>.parquet`
  TOLERATES multiple row groups (count, sample and filters on both sides exact); it is
  `indptr_<rel>.parquet` that CORRUPTS when fragmented (6000→5049). Refined writer rule:
  indptr single-RG mandatory; indices may stream in multiple RGs (memory gain on large
  exports). Bonus discovered along the way: filtering a PRIMARY KEY column with `=` returns ZERO
  on an icebug-disk node table (`uid='x'`→0; `uid IN ['x']`→works) — it explains the historical
  workaround and requires an automatic rewrite in the rewriter (T18.2b);
  (2) THE `pair` PROPERTY ON THE EDGE (E10): single-hop with transpile works EXACTLY
  (`WHERE r.pair='method_function'` returned only methB→funcA); var-len is IMPOSSIBLE in the language —
  Binder exception "r has data type RECURSIVE_REL": there is no way to put a per-hop condition in
  `*1..N`, so the idea does not replace the planner for multi-hop. It is worth having as an optimization/self-description
  in single-hop and it aligns with the logical vocabulary of #505;
  (3) E8 closed it conclusively: a merged CSR under a multi-pair declaration produces a silently wrong graph
  (an edge crossed the id space and came out with an EMPTY LABEL).
- DESIGN DECISION recorded by the Engineer: the planner must NOT stay limited to 8 hops
  (the constant `icebugTraversalMaxHops = 8` today) — the canonical design + BFS frontiers terminates by
  saturation of the visited set, not by an arbitrary ceiling; accept `*1..N` with a large N and the open
  `*` pattern, always with a deadline/context cancellation.
- Native levers of the recursive MEASURED over the frozen folded layout (real corpus, anchor
  runQuery, native control 10 rows/~16ms): `SHORTEST 1..3` >60s; per-hop lambda filter
  `(rr,n | WHERE n.label=...)` >60s (probes isolated per process after the first experiment
  hung because of an abandoned goroutine holding the connection — a method lesson: a probe with a
  possibly long query runs in its own process with a short -timeout). The `{n.uid}` projection stays
  for a later probe, same family. CONCLUSION: no syntax extension makes the plan
  selective; the PLANNER REMAINS essential. The extensions go into the vocabulary of the
  tree-sitter-cypher transpiler for future forms/when upstream fixes pushdown.
- openCypher: the official docs declare alignment ("as far as possible follows openCypher");
  divergences relevant to our subset: WALK semantics by default (TRAIL/ACYCLIC available),
  var-length requires an upper bound (default 30, configurable), WHERE inside the node pattern not
  supported, singular label(), SHOW becomes CALL show_x(). Candidate grammars for the transpiler:
  taekwombo/tree-sitter-cypher (updated 2026-08) and simplificare-org (based on the openCypher
  grammar); the choice by measured coverage on our patterns during the implementation, SHA pinned.

#### T18 implementation plan — FROZEN (2026-08-25)

- [ ] **T18.2 — Canonical exporter** (`internal/ladybugstore/icebug_canonical.go`):
  node table per label (columns via `table_info`, dense ids by first occurrence,
  UTF-8 sanitization inheriting the `RepairedStrings` counter), rel table per pair with a
  deterministic name `<tipo>_<de>_<para>` + `_reverse` mirror, opt-out through
  `hub.icebug.reverse_edges`, self-loop once in the direct one, INDPtr single-RG mandatory,
  canonical `schema.cypher` and `icebug.json` v2 (`format: icebug-canonical`, map
  `TYPE → members`). Folded remains available; the default swap only after the consumer is ready.
- [ ] **T18.2b — Planner aware of the members**: resolves `TYPE → members` from the v2 manifest,
  WITHOUT a hop ceiling (saturation of the visited set + deadline), basic post-frontier aggregations
  (count/count DISTINCT), the rule ≥2 hops is always ours.
- [ ] **T18.2c — Compatibility through STRICT fail-closed REGEX** (the Engineer's decision:
  simplicity > tree-sitter; the grammar goes to the backlog as an improvement conditioned on the volume of
  errors): it translates ONLY exact recognized forms (`[:TIPO]`→alternation of the members when
  there is no filter on a bound endpoint; UNION per member with a filter; `uid='lit'`→`uid IN ['lit']`);
  an unrecognized form → an actionable ERROR listing the member tables, never a guessed translation.
- [ ] **T18.3 — Permanent regressions** (E1/E2/E5/E7/E10 as real tests), local/S3 benchmarks
  vs native, docs/specs updated, commits separated by phase.

Status: T18.2 RUNNING.
- T18.2 DELIVERED (commit e7bf77c): complete canonical exporter with permanent regressions
  (multi-pair round-trip, self-loop-once, exact mirror preserving direction, opt-out,
  PK-equality quirk pinned). T18.2b CORE DELIVERED in this commit: the backend loads icebug.json v2
  on connecting (a file next to the mounted catalog); the Hub's installer now downloads it along with the
  schema; the canonical planner resolves TYPE→members from the manifest, BFS WITHOUT a hop ceiling with the
  minimum depth respected (*N..M and open *), undirected via direct+reverse members,
  count([DISTINCT] endpoint.uid) post-frontier, fail-closed for unsupported forms
  (the error lists the plannable types). ast tests: unbounded == native on the 5-hop chain,
  count DISTINCT=3 on *1..3, collect() rejected with a message.
  PENDING (next increment): translation of single-hop outside the planner (alternation/union),
  a uid=→IN sanitizer over pass-through queries, local/S3 benchmark of the canonical path,
  the flip of the default folded→canonical in publish after end-to-end validation.
- SANITIZER delivered: `sanitizeCanonicalUIDEquality` rewrites `X.uid='lit'`→IN outside
  strings, with a regression of its own; applied only on canonical catalogs before any path.
- FLIP OF PUBLISH TO CANONICAL: implemented and REVERTED in this cycle — the flip changes the return
  type of ExportGraphToIcebug and breaks consumers of the folded manifest (bundle/
  search tests). The next increment needs to update those consumers TOGETHER with the flip. Failures
  observed in the suite without -tags lancedb are environmental, not regressions.
- REMAINING to close T18: (1) the flip with the consumers updated; (2) single-hop outside the
  planner: a bare pattern today falls into the fail-closed path — decide alternation vs planner 1..1;
  (3) canonical local/S3 benchmarks; (4) the hub_collaboration spec describing the canonical layout.
- T18 CLOSED (2026-08-25): publish flipped to canonical with the consumers updated
  (tests from the folded era migrated to the canonical contract — no label(), staged manifest,
  multi-item projections supported); bare pattern = exactly one hop through the same mechanism;
  candidate tables filtered by the columns the predicates/projections reference and
  members traversed only when BOTH sides have a uid; permanent benchmarks: local
  **182 ms** / real S3 **694 ms**, set identical to the native one (10 rows), vs folded
  291/429 ms and native ~16 ms. The hub_collaboration spec rewritten for the canonical layout,
  the folded one marked as legacy-readable.
- Final suites with -tags lancedb GREEN: ast 61.3s / ladybugstore 5.0s / hub 1.7s.
  Closing commits: flip+consumers (7c36a55), migration of the hub's expectations
  (7eba9ee), sanitizer (79f49af). T18 CONCLUDED: canonical publish is the default; folded
  remains readable for old bundles; optional pending items moved to future improvements
  (connected components, 2-hop edge, versioned cache, bidirectional, upstream PR).
- BACKWARD COMPATIBILITY REMOVED by the Engineer's decision: the folded query path
  deleted from the backend (Entity/CALLS regex parser, executor and label helpers) — canonical or
  native, with no third state. A permanent S3×Local battery added
  (TestMountedCanonicalS3Battery): 6 on-the-fly rounds against real MinIO, set identical
  to the native one in all of them, bundle PRESERVED in diagnostics/t18-canonical-battery-* for inspection.
  Numbers from this run: local native 6×≈60 ms total; S3 596–680 ms per round (10 rows).
- tree-sitter-cypher removed from the backlog by the Engineer's decision: strict
  fail-closed regex resolves 100% of the current needs with no native dependency. The item
  archived as "it does not make sense now".
