# Indexing writes to the correct project, and `reindex` deletes again

**Date:** 2026-07-28
**Scope:** `internal/mcpstdio/{context.go,tools_ast.go,tools_lifecycle.go}`,
`internal/ast/{writer.go,ladybug.go}`, plus two new test files
**Origin:** Engineer's reproduction — indexing `/tmp/probe` via MCP placed 16 probe nodes in
this repository's graph

---

## The problem

`brand.DotDir()` is literally `".graphit"`. Every module path builder consequently returns a
**path relative to the project root**:

| Builder | Returns |
|---|---|
| `ast.DefaultLadybugConfig()` | `.graphit/ast/project/ladybugdb` |
| `knowledge.WikiDir()` | `.graphit/knowledge/project` |
| `memory.ProjectLinkDir(scope)` | `.graphit/memory/<scope>` |

MCP handlers resolved this with `os.Chdir(projectDir)` + `defer os.Chdir(origWd)` — and let the
relative path **escape** the block. Since `LadybugBackend` opens the database lazily
(`sync.Once` in `connect()`, only on first query), resolution happened later, against the process's
cwd. That is: against another project.

The symptom is silent. Indexing reports `totalfiles:1|parsedfiles:1|...` and success; nodes go
to the wrong graph.

The detail that hid the bug for so long: in `openASTDB` (read) the `os.Stat` runs **inside**
the `chdir`, with the correct cwd. The check passed in the right place and opening happened in the wrong
place, which made the problem appear only on writes.

## The root, and why `chdir` was never needed

Builders are **pure with respect to cwd** — they only concatenate constant strings, never read
`os.Getwd`. `chdir` wasn't there to build them; it was only there to resolve the resulting relative.

So the fix is not "do `filepath.Abs` inside `chdir`": it's **anchor explicitly to
`projectDir`** and never depend on cwd.

```go
func anchorToProject(projectDir, path string) string {
	if path == "" || filepath.IsAbs(path) {
		return path
	}
	return filepath.Join(projectDir, path)
}
```

Already-absolute path passes through — that's the case for imported contexts (`~/.graphit/ast/<name>`, via
`brand.GlobalDir()`) and the `LADYBUGDB_PATH` override. `LadybugConfigForContext` was verified: it's
absolute on the normal path, and relative only in the `brand.GlobalDir() == ""` fallback, which the
anchor now covers.

As a side effect, `openASTDB`/`openASTDBReadWrite` no longer touch the process cwd. There was
a race window there — cwd is global mutable state and the server handles
concurrent requests — which simply ceases to exist.

## The other points with the same defect

Searching for the pattern found four more, two worse than reported:

- **`ast_index(reset: true)` deleted the wrong project.** `os.RemoveAll(filepath.Dir(cfg.DBPath))`
  ran **without any `chdir`**, with the relative path — it destroyed the AST database of whoever was in the
  server's cwd.
- **`CacheDir` of the pipeline** (in `ast_index` and in `sync`) pointed to the other
  project's directory, scattering the parse cache.
- **`resolveWikiDir`** returned `.graphit/knowledge/project` outside `chdir`: every knowledge wiki read via MCP resolved against the server's cwd. Here `chdir` **remained**,
  because `memory.WikiDir` does `os.Stat` to decide if the directory exists and that probe must
  run in `projectDir`; only the result is anchored before leaving.

`ast_embed` does all work **inside** `withProjectDir`, so it was already correct and wasn't
touched.

## `DeleteRepository` stops being a stub

`GraphWriter.DeleteRepository` returned `nil` without deleting anything. That's why
`ast_index(reindex: true)` didn't remove stale nodes and deleted-file entities survived until
someone ran a full `reset: true`.

Scope is by `File.path`, which the pipeline writes relative to the indexed root: `repoPath` equal to root
covers the whole graph, a subdirectory covers its own prefix, and a path outside the root deletes
nothing.

The non-obvious case is `Parameter` and `Field`. When the grammar doesn't declare them as labels, they
get a minimal DDL — `uid, name, lang, is_stub`, **without `path` column** — and hang off the owner, not
the `File`. You can't match them by file. The final sweep catches what was left without an owner, after
owners have been deleted:

```cypher
MATCH (n:`Parameter`) OPTIONAL MATCH ()-[r]->(n) WITH n, count(r) AS owners WHERE owners = 0 DELETE n
```

Scope still holds on a partial delete: parameter whose owner survived keeps its
inbound edge. There's a test for exactly that.

`Directory` nodes are also removed by prefix — not in the file list, but a tree
that left shouldn't leave folders standing.

And the error now **bubbles up**: in `tools_ast.go` the return was discarded with `_ =`. A cleanup that
fails silently makes reindex pile new nodes on old ones, which is the original defect.

## Tests

Both files were verified against the old code — they fail there, pass here.

`internal/mcpstdio/context_projectdir_test.go` runs with the process in one directory and
`project_dir` in another. Against the old code:

```
DBPath = ".graphit/ast/project/ladybugdb"; want "/tmp/.../001/.graphit/ast/project/ladybugdb"
openASTDB() succeeded; want a missing-database error for the target project
resolveWikiDir() = ".graphit/knowledge/project"; want "/tmp/.../001/.graphit/knowledge/project"
```

The second line is masking in its purest form: `openASTDB` **succeeded** because it found the
neighbor project's database.

The write test doesn't stop at the path: it forces lazy opening with a `CREATE NODE TABLE` and
then asserts the database was born in the requested project **and** that the cwd project still has no
`.graphit`.

`internal/ast/writer_delete_repository_test.go` seeds the graph by hand instead of indexing — the pipeline
only collects files whose grammar is installed, and a temporary directory has none
(`TreeSitterSupportedExtensions()` returns empty there). Seeding has the advantage of allowing the
schema to be built exactly in the hard case, with `Parameter`/`Field` without `path`. Without the orphan sweep:

```
DeleteRepository() left map[Field:1 Parameter:1] behind
```

## Verification

```
go test -race -tags fts5 -p 4 ./internal/ast/... ./internal/mcpstdio/...   ok
golangci-lint run ./internal/ast/... ./internal/mcpstdio/...               0 issues
make ui && make build                                                      exit 0
```

## `make ci` was green and lying

Running `make ci` at the Engineer's request, **30 tests failed** — and still:

```
  ✅ All CI checks passed.
MAKE_CI_EXIT=0
```

None of the failures is from this change: they are all from untouched files
(`search_index_test.go`, `abbrev_recall_test.go`, `fts_db_test.go`, …), all with the same cause:

```
open search index: search schema migrate: ... no such module: fts5
```

Two pre-existing defects in `Makefile`, independent of each other:

1. **Target `test` didn't pass `-tags fts5`.** Variable `BUILD_TAGS := fts5` exists at line 39 and
   is used by build targets, but `test` never referenced it. Without the tag, SQLite is compiled
   without the FTS5 module and every test that opens the search index fails. Proven on the same test, same
   code: fails without tag, `ok` with it.
2. **Target `test` swallowed the exit code.** `go test` calls were followed by `;` and more
   commands, so the recipe's status was that of the **last** command (the coverage `if`), never that of
   `go test`. That's why `ci` printed success with 30 failures on screen.

Fixed: `-tags $(BUILD_TAGS)` on both invocations and explicit propagation via
`status=1` + `exit $$status`. With the tag, the whole suite truly passes — project packages and
parser packages, both sets `make test` runs, both exit 0.

A third point, smaller: `ci: vet lint vulncheck test ui ui-lint` put `vet` **before** `ui`,
and `vet` needs `internal/ui/dist` (not versioned, `.gitignore:29`). On a clean worktree
`make ci` died on the first target with `pattern dist/*: no matching files found`. Reordered to
`ci: ui vet lint vulncheck test ui-lint`.

`make install` was not executed: depends on `make build` (exit 0, verified) and a `cp` to
`/usr/local/bin`, which is not writable by the user and would require `sudo`.

## Remaining debt

A relative `LADYBUGDB_PATH` is now resolved against `projectDir` instead of cwd. It's the
coherent semantics with the rest, and the one that makes the override work per project — but it's a behavior change for anyone depending on cwd. Absolute path, which is the expected usage, doesn't change.
