# A project's grammar directory becomes configurable, and `merge: true` merges instead of replacing

## Date
2026-08-12

## Problem
Two defects in the same mechanism, both of which made a project-level grammar
override cost far more than it should.

**1. The override could not be versioned.** A project customizes a grammar by
dropping a query YAML into its own queries directory, which the resolution chain
reads before the user's and the runtime's. That directory was fixed at
`.graphit/ast/queries`, and `graphit init` adds `.graphit/` to `.gitignore` — so the
one kind of customization that belongs to the *repository* rather than to the
machine lived in the one directory the repository does not track. The next clone,
and every other developer, silently got the shipped grammar back.

**2. Any file at any level replaced the whole language.** `resolveQueriesForLang`
returned the first level with a match and nothing else, so overriding one pattern
meant copying the entire shipped file — several hundred lines for `go.yaml` — and
then owning every future fix to it. Worse, the copy silently takes over the fields
it does not restate: a copy that forgets `extensions` unregisters the language
(`rebuildExtTables` and `projectTsExtMap` build the extension tables from that
field alone), and one that forgets `grammar` sends `resolveTreeSitterLang` looking
for `tree-sitter-<language name>`, which for `csharp` does not exist. Both fail
by silence: files of that language stop being discovered, with no error anywhere.

## Root Cause
`projectQueriesDir` was `filepath.Join(projectDir, brand.DotDir(), "ast", "queries")`
— a constant, with no configuration behind it.

For the second, `resolveQueriesForLang` was a three-branch "first non-empty wins".
There was a vestigial `Replace bool` field on `ExternalQueryFile`, parsed from
`replace:` and read by nothing but a test assertion, which suggests merging was
intended at some point and never implemented.

## Changes

### `internal/config/config.go`
- `DefaultASTQueriesDir()` — `.graphit/ast/queries`, where it has always been, so
  nothing regresses for a project already using it.
- `ResolveASTQueriesDir(inlineCfg, projectCfg)` — the `ast.queries_dir` key, a path
  relative to the project root. Same precedence as every other key: inline,
  `GRAPHIT_AST_QUERIES_DIR`, project lockfile, global config, compiled default.

### `internal/ast/query_loader.go`
- `projectQueriesDir` resolves through the key; `ProjectQueriesDir` exports it for
  the Hub. The configured directory **replaces** the default rather than adding to
  it — a project has one grammar directory, and two would mean two answers for the
  same language with no rule to choose between them.
- `queryDirState.get` takes the directory as a **function** instead of a string, and
  evaluates it inside the lock, past the rate limit. Resolving the project's
  directory reads the lockfile, and this is called once per file parsed; the
  staleness check already runs at most every 2s, so the config read now costs what
  the directory sweep costs. The directory path is folded into the signature, which
  is what makes changing `ast.queries_dir` land on a running daemon like an edit to
  the files themselves.
- `Replace bool` → `Merge bool` (`merge:`). Opt-in and stated in the affirmative, so
  the file says what it does rather than which behaviour it switches off — and so a
  plain bool is enough, with no third state to encode. Absent replaces, as every level
  always has.
- `filterByLangExt` matches the language with `strings.EqualFold` instead of `!=`. It
  was the one exact comparison in a chain that folds case everywhere else — `mergeOnto`,
  `projectTsLangMap` and `rebuildExtTables` all key by `strings.ToLower(Language)` —
  which meant a file spelling the language differently from the level it overrides could
  merge onto that level and then not be selected.
- `mergeOnto(base, over)` folds one level onto the one below, matching by language.
  It returns the **upper level**, never the union — which level answers for a
  language stays `resolveQueriesForLang`'s decision, and a language the upper level
  says nothing about has to stay unanswered there so the fallthrough still works.
  A level in which no file asks to merge is returned as the same slice, so the
  common case is one pass over a handful of structs and no allocation.
- `mergeQueryFile` is what `merge: true` means, field by field — three rules for
  three kinds of statement:
  - scalars and lists: declared replaces, omitted inherits. This is what makes a
    partial file a working language, and it is also the only way to *shorten* a
    list, which a union could not express.
  - maps (`context_types`, `context_name_paths`, `text_normalizers`): merged key by
    key. They are catalogues; adding one entry should not mean restating forty.
  - `queries`: merged by `data_key` (`mergeQueryDefs`). A redeclared key replaces
    that whole group — `go.yaml` captures `calls` with two patterns, and half a
    definition of "how calls are found" is not a thing a language can have.
  - `embedded`: the upper level's blocks go **first** (`mergeEmbedded`). Order is
    precedence there — the first block whose pattern matches a body claims it — so a
    project's `<script lang="ts">` would never be reached behind the generic
    `<script>` it was written to precede.
  - `complexity`: the same declared-replaces-omitted rule one level down, so
    restating `node_types` does not silently drop `operators`.
- `userLevelQueryFiles()` = the user's files folded onto the runtime's.
  `belowProjectQueryFiles()` = `overlayByLang(runtime, userLevel)` — every language a
  project inherits, one effective file each. `effectiveProjectQueryFiles(dir)` = the
  project's files folded onto that.

  The overlay is the part that is easy to get wrong, and the first draft did: with
  the base being only the level immediately below, a project merging onto a grammar
  the user's directory says nothing about — which is every grammar, since that
  directory is usually empty — had nothing to merge onto and came out as a bare
  partial file. A merge's base has to be the whole chain beneath it, keyed by
  language; a level's *identity* is still only its own files.
- `belowProjectCache` and `projectEffectiveCache` memoize those folds and are
  cleared by `invalidateDerivedQueryCaches` with the rest. Both are read **after**
  the loads they depend on, because a load that noticed a change drops them.

### The consumers of project-level files
Every reader of `loadProjectCached` outside the loader now reads
`effectiveProjectQueryFiles`, because a partial file is not a usable language on its
own: `projectTsExtMap` and `projectTsLangMap` (`treesitter_adapter.go`),
`HasAntlrForExtensionIn` (`antlr_adapter.go`), `antlrLangConfigByName`
(`treesitter_embedded.go`). `rebuildExtTables` registers `mergeOnto(runtimeQ, userQ)`
on top of the runtime for the same reason — it uses the `cached()` accessors, not
`get`, so it stays out of the reload path that calls it.

### `internal/hub/service.go`
- Linking a language artifact out of a local source project reads
  `ast.ProjectQueriesDir(absSource)` instead of a hardcoded path: where a project
  keeps its grammars is that project's own configuration.

### `internal/hub/rule.go`
- One row in the agent-facing configuration table: a committed grammar YAML with no
  effect is `ast.queries_dir` pointing at the gitignored default.

## Tests
New — `internal/ast/query_override_test.go`:
- the configured directory is where project grammars are read, and it replaces the
  brand directory rather than adding to it;
- moving `ast.queries_dir` under a running process is picked up;
- `merge: true` inherits `extensions` and `grammar`, keeps the lists and map
  entries it said nothing about, merges `queries` by `data_key`, and restates
  `complexity.node_types` without dropping `operators`;
- without the flag the project file still replaces, dropping what it does not
  restate;
- the fold applies at **every** level: user-over-runtime and project-over-user
  compose, and the runtime's `grammar` survives both;
- a merged project file keeps its extension registered — the failure mode that is
  invisible until files stop being discovered;
- `merge: true` with no lower-level language is just the file;
- `mergeOnto` returns only the upper level and mutates neither input, since a
  level's parsed files are shared by every project and every parse.

Updated — `internal/ast/query_loader_test.go`: the `replace:` assertions become
`mergesOnto()`; `TestProjectQueriesDir` redirects `HOME` so the developer's own global
config cannot answer for the key; and `TestFilterByLangExtFoldsCase` covers the folded
comparison, including that folding does not turn it into a prefix match.

Suites run green with `-tags fts5` and `LD_LIBRARY_PATH` pointed at the LadybugDB
module: `internal/{ast,config,hub,knowledge,daemon,mcpstdio}`,
`cmd/graphit/commands`, `cmd/launcher`. `internal/ast` also under `-race` for the
query, grammar and language tests.

## Migration Notes
Nothing breaks. `ast.queries_dir` defaults to the old path, and a file with no `merge`
key behaves exactly as before.

Two things to know when moving the directory into the tracked tree:

- **Those YAMLs will be indexed as code.** `.graphit/` is excluded from the AST
  pipeline by default; an ordinary project directory is not. Add it to `.astignore`
  if the noise is unwanted.
- **`replace:` is gone.** It was parsed and never honoured, so no behaviour depended
  on it; a file still carrying the key is now an ignored unknown field rather than a
  silent lie sitting next to `merge`.

## Documentation
The specs described this mechanism **incorrectly**, which is very likely where the belief
that merging already existed came from:

- `docs/specs/ast_module.md` documented `replace: false # false = append to
  lower-priority queries` in the YAML schema example, and a `replace` row in the Query
  Fields table reading "Default: `false` (append)". The behaviour was full replacement at
  every level, and the flag did nothing. Both now describe `merge`.
- `docs/guides/user_manual.md` twice told a project embedding SQL in XML to copy the whole
  file because "override is per language, not a merge". Both notes now name `merge: true`
  and what it keeps.

Also filled in what was missing rather than wrong — the reference had no entry for several
keys the engine reads:

- `context_name_paths` and `complexity` were absent from the Language Configuration Fields
  table and from its example; both are in now.
- New **Complexity Scoring** section: `node_types` vs `operators` (and the rule that a
  grammar wrapping `&&` in a named node belongs in the former, never both), and
  `head_calls` with `node_type` / `names` / `pair_names` / `subject_pair_names` — the last
  two documented nowhere before.
- New **Text Normalizers** section: `replace`, `numeric_char_refs`, the newline invariant
  and what gets dropped at load time, and why normalizing is opt-in per block rather than
  per language.
- The `embedded` row gained `normalize` and the load-time validation that drops a
  half-written block.
- The merge section states the pairing rule, which was documented nowhere: files pair on
  `language` alone, case-insensitively, and `extensions` is inherited rather than matched.
- `docs/specs/embedded_language_parsing.md` — the "declare it in the project override"
  snippet now says where that override lives and that it needs `merge: true`.
