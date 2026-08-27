---
title: Disable specific grammars by config — ast.grammars_blacklist and ast.grammars_whitelist
status: done
created: 2026-08-24
updated: 2026-08-24
tags: [ast, config, grammars, indexing]
---

# Disable specific grammars by config

## Objective

Make it possible to turn individual grammars off for the AST index through the
configuration mechanism that already exists — inline `-c`, environment variable,
project lockfile, global `~/.graphit/config.json` — so the choice can be made
globally for a machine or per project, and committed with the repository when it
belongs to the repository.

Two keys, both comma-separated lists of grammar/language names:

- `ast.grammars_blacklist` — every grammar named here is **not** enabled: files
  it would have claimed are not discovered, not parsed, and its queries do not
  resolve.
- `ast.grammars_whitelist` — when present and non-empty, **only** the grammars
  named here are enabled. The blacklist still applies on top of it, so a name in
  both is disabled.

### Reasoning

Today the only way to stop a language from being indexed is to remove its query
file, because extensions are granted by the query file rather than by the grammar
— see the note under *Markdown is not on this list* in
[ast_module](../specs/ast_module.md). That works for this repository, whose
`internal/ast/queries/*.yaml` it owns, and is useless for a consumer: the query
files it wants to suppress live in the installed runtime directory
(`~/.graphit/runtime/<version>/ast/queries`), which is regenerated on every
install and is not the consumer's to edit. `.astignore` is not the tool either —
it excludes *paths*, and the request is about a *language*, which cuts across
paths.

So the switch has to be configuration, and it has to be read where the pipeline
decides a language exists.

### Justification of the approach, and the alternative that was dropped

**Chosen: enforce at the lookup boundary, per project.** The extension tables
(`tsExtMap`, `antlrExtMap`, `tsGrammarMap`, `antlrGrammarMap`, `tsLangNameMap`)
are process-wide and rebuilt from the runtime and user query directories, with no
project in the picture — `rebuildExtTables` says so explicitly. A per-project
filter therefore cannot live inside them: one daemon supervises many projects and
would need one table per project. It lives instead in the handful of functions
that take a `projectDir` and answer "what parses this", which is also exactly the
set of functions every discovery, watcher and parse path already goes through.

**Dropped: filter the query files as they are loaded.** It reads better —
one place, near `loadQueriesFromDir` — and it is wrong for the same reason: the
runtime and user levels are loaded once for the whole process and shared by every
project, so filtering there would apply one project's blacklist to all of them.
It would also make the caches lie: `belowProjectCache` is a single-entry cache
with no project key.

**What the lists match.** A query file carries both a `language:` and a
`grammar:`, and they are frequently different (`language: yaml_lang`,
`grammar: tree-sitter-yaml`; `language: plsql`, `grammar: antlr-plsql`). A name in
either list matches if it equals, case-insensitively, any of: the language name,
the grammar name, or the grammar name with its `tree-sitter-` / `antlr-` prefix
stripped. So `yaml`, `yaml_lang` and `tree-sitter-yaml` all disable the same
language, which is what someone writing the list actually means.

## Plan & Task Breakdown

- [x] **T1 — Resolve the two keys in `internal/config`** — Spec: add
  `ResolveASTGrammarsBlacklist` and `ResolveASTGrammarsWhitelist` beside
  `ResolveASTQueriesDir` in `internal/config/config.go`, returning the raw
  resolved string so the caller can both parse it and use it as a staleness
  signature. Done when both keys resolve through the full precedence chain
  (inline → `GRAPHIT_AST_GRAMMARS_*` → project lockfile → global config →
  compiled defaults). Constraint: `internal/config` stays below `internal/ast`;
  no import of ast from config.
- [x] **T2 — The filter itself** — Spec: new `internal/ast/grammar_filter.go`
  holding `grammarFilter` (whitelist/blacklist sets), `allows(language, grammar)`,
  and the alias rule above. Done when an empty configuration produces a filter
  that allows everything with no allocation on the hot path. Constraint: the
  no-configuration case must cost one map lookup at most, because this is
  consulted once per file discovered.
- [x] **T3 — Per-project caching, refreshed without a restart** — Spec:
  `grammarFilterState` mirroring `queryDirState` — a mutex, a signature, a
  rate-limited re-resolve (`queryStaleCheckInterval`) — keyed by project
  directory. Done when changing the key under a running daemon takes effect
  within the interval and drops the derived query caches. Constraint: resolving
  the config reads the global config file from disk, so it must NOT happen once
  per file; and `invalidateDerivedQueryCaches` must be called outside the state's
  own lock.
- [x] **T4 — Enforce it on the tree-sitter side** — Spec: `tsLangConfigFor`,
  `tsLangConfigByName`, `TreeSitterParser.ParseWithGrammar` in
  `internal/ast/treesitter_adapter.go`. Done when `HasTreeSitterForExtensionIn`
  and `TreeSitterLangForExtensionIn` report false/empty for a disabled language
  without either of them being touched — they both go through
  `tsLangConfigFor`.
- [x] **T5 — Enforce it on the ANTLR side** — Spec: `HasAntlrForExtensionIn`,
  `AntlrParser.Parse`, `AntlrParser.ParseWithGrammar` in
  `internal/ast/antlr_adapter.go`, and `antlrLangConfigByName` in
  `internal/ast/treesitter_embedded.go`. Done when a `.sql` file is skipped with
  `plsql` blacklisted. Constraint: `antlrExtMap` holds a *list* per extension, so
  the filter selects within the list rather than rejecting the whole entry — one
  dialect can be off while another stays on.
- [x] **T6 — Make the queries resolve to nothing too** — Spec:
  `resolveQueriesForLang` in `internal/ast/query_loader.go` drops the files of a
  disabled language at every level. Done when a parse that somehow reaches a
  disabled language extracts zero entities instead of falling through to the
  level below. Constraint: this is the function behind `mergedQueryCache` and
  `compiledQueryCache`, so the filter change has to invalidate both — which T3
  covers.
- [x] **T7 — Reset the filter with the other query caches** — Spec:
  `InvalidateQueryCaches` clears the per-project filter states. Done when a test
  that changes the config in a temp project sees the new value immediately rather
  than after the staleness interval.
- [x] **T8 — Tests** — Spec: `internal/ast/grammar_filter_test.go` covering the
  alias rule, blacklist, whitelist, whitelist+blacklist, discovery
  (`HasParserForExtensionIn`), query resolution, and the ANTLR
  one-dialect-of-several case. Constraint: the tests must set `HOME` to a temp
  dir, because the filter resolves the *global* config file.
- [x] **T9 — Documentation** — Spec: the key table and a section in
  [config_module](../specs/config_module.md); a section in
  [ast_module](../specs/ast_module.md) beside the resolution chain; a row and a
  section in [ignore_files](../guides/ignore_files.md), which is where "why is
  this file not in the graph" is answered; the troubleshooting entry; and the
  agent-facing rule in `internal/ast/rule.go` plus the three generated
  `SKILL.md` copies that are checked in.

## Implementation Details

### `internal/config/config.go`

Two resolvers beside `ResolveASTQueriesDir`:

```go
func ResolveASTGrammarsBlacklist(inlineCfg, projectCfg ConfigMap) string
func ResolveASTGrammarsWhitelist(inlineCfg, projectCfg ConfigMap) string
```

They return the raw resolved value, trimmed, and nothing else. Splitting it is
the AST package's job, for one reason: the same string is the *staleness
signature* the filter cache compares, and a parsed form would have to be
re-serialised to serve as one.

### `internal/ast/grammar_filter.go`

```go
type grammarFilter struct {
	whitelist map[string]bool
	blacklist map[string]bool
}

func (f grammarFilter) allows(language, grammar string) bool
func (f grammarFilter) allowsFile(qf ExternalQueryFile) bool
func (f grammarFilter) keepFiles(files []ExternalQueryFile) []ExternalQueryFile
```

`allows` short-circuits on `f.inert()` — both maps empty — so the default
configuration costs two length checks and no allocation. Otherwise it builds the
alias set for the pair and applies whitelist-then-blacklist.

`effectiveGrammarName(qf)` reproduces the defaulting that `tsConfigOf` and
`antlrConfigOf` apply (`tree-sitter-<language>` / `antlr-<language>` when
`grammar:` is absent), so a file that omits `grammar:` is still matched by the
grammar name a user would write.

The cache is `grammarFilterState`, one per project directory in a `sync.Map`,
and it is a deliberate copy of `queryDirState`'s shape: rate-limited staleness
check, a signature, and a `changed` return the caller turns into
`invalidateDerivedQueryCaches()`. The signature is the two raw config strings
joined by a NUL — the same trick `queryDirState.get` uses to fold
`ast.queries_dir` into the directory signature.

`projectDir == ""` is a supported key, not a bug: it resolves the environment
variable, the global config and the compiled defaults, which is the right answer
for the handful of callers that have no project at hand.

### Enforcement points

| Function | File | What changed |
|---|---|---|
| `tsLangConfigFor` | `internal/ast/treesitter_adapter.go` | filter applied to the project map hit and to the global `tsExtMap` hit |
| `tsLangConfigByName` | `internal/ast/treesitter_adapter.go` | same, for the language-name lookup an embedded block uses |
| `TreeSitterParser.ParseWithGrammar` | `internal/ast/treesitter_adapter.go` | an explicit `--grammar` override cannot revive a disabled grammar |
| `HasAntlrForExtensionIn` | `internal/ast/antlr_adapter.go` | selects within `antlrExtMap[ext]`, then within the project's own files |
| `AntlrParser.Parse` | `internal/ast/antlr_adapter.go` | filters the candidate dialect list before parsing any of them |
| `AntlrParser.ParseWithGrammar` | `internal/ast/antlr_adapter.go` | same as the tree-sitter override |
| `antlrLangConfigByName` | `internal/ast/treesitter_embedded.go` | embedded SQL inside XML honours the filter |
| `resolveQueriesForLang` | `internal/ast/query_loader.go` | a disabled language resolves to no queries at any level |

`HasParserForExtensionIn`, `HasTreeSitterForExtensionIn`,
`TreeSitterLangForExtensionIn`, `collectFiles`, `watcher.go` and the daemon's
`classifyBatch` needed no change at all: every one of them reaches the graph
through one of the eight functions above.

## Use Cases

### UC-01: Disable one grammar for a single project
- **Actor**: developer, or an agent acting for one
- **Preconditions**: the project is registered; the grammar is otherwise enabled
  (its query file is in the runtime, user or project queries directory)
- **Main Flow**:
  1. `graphit config ast.grammars_blacklist yaml` writes
     `{"config":{"ast":{"grammars_blacklist":"yaml"}}}` into the project's
     `graphit.lock.json`.
  2. Within `queryStaleCheckInterval` (2s) the next lookup re-resolves the key,
     notices the signature moved, and calls `invalidateDerivedQueryCaches()`.
  3. `collectFiles` → `HasParserForExtensionIn` → `tsLangConfigFor` now returns
     `false` for `.yaml` and `.yml`, so those files are not collected.
  4. `pruneVanished` drops every cached shard whose file is no longer live, and
     the pipeline deletes those files from the graph and the search index.
- **Alternative Flows**:
  - `graphit config --global ast.grammars_blacklist yaml` applies to every
    project on the machine.
  - `GRAPHIT_AST_GRAMMARS_BLACKLIST=yaml` applies to one process.
  - `graphit ast index -c ast.grammars_blacklist=yaml` applies to one run.
- **Error Scenarios**:
  - A name that matches no known grammar is inert — no error, and nothing is
    disabled. This is deliberate: the list is read long before the set of
    installed grammars is known to be complete, and failing on an unknown name
    would break a project whose grammar pack is not installed yet.
  - Whitespace and empty entries (`"go, , python"`) are skipped.
- **Postconditions**: no `File` node with that language remains in the graph
  after a full index; new files of that language are never added.
- **Affected Files**: `internal/config/config.go`,
  `internal/ast/grammar_filter.go`, `internal/ast/treesitter_adapter.go`,
  `internal/ast/antlr_adapter.go`, `internal/ast/query_loader.go`

### UC-02: Index only a named set of grammars
- **Actor**: developer on a large polyglot repository who wants the graph scoped
- **Preconditions**: as UC-01
- **Main Flow**:
  1. `graphit config ast.grammars_whitelist go,sql` writes the key.
  2. Every language whose alias set does not intersect `{go, sql}` is disabled —
     including languages the project itself declares in `ast.queries_dir`.
  3. Discovery, the watcher and the parse dispatch all narrow to those two.
- **Alternative Flows**:
  - Blacklist on top: `ast.grammars_whitelist=go,sql` with
    `ast.grammars_blacklist=sql` leaves Go alone. A name in both lists loses.
  - An empty or absent whitelist means "every language", which is the default.
- **Error Scenarios**:
  - A whitelist of only unknown names disables everything, and the index comes
    back empty. That is the honest consequence of the request, not a failure, and
    it is why the key is documented with the alias rule spelled out.
- **Postconditions**: the graph holds only the whitelisted languages.
- **Affected Files**: as UC-01

### UC-03: One SQL dialect off, the others on
- **Actor**: developer whose `.sql` files are PostgreSQL, not Oracle
- **Preconditions**: several ANTLR grammars claim `.sql` (`plsql`, `postgresql`,
  `tsql`)
- **Main Flow**:
  1. `graphit config ast.grammars_blacklist antlr-plsql`.
  2. `HasAntlrForExtensionIn` still answers `true` for `.sql`, because the list
     for that extension is not empty after filtering.
  3. `AntlrParser.Parse` drops `plsql` from the candidate list, so the expensive
     PL/SQL prediction stage is never entered for a file it was never going to
     claim.
- **Alternative Flows**:
  - `ast.grammar` (singular — the per-extension override) pins one dialect
    positively for a run; this key removes one negatively and persists.
- **Error Scenarios**:
  - Blacklisting *every* dialect that claims `.sql`, with no tree-sitter `sql`
    query file present, makes `.sql` undiscoverable. Consistent with UC-02.
- **Postconditions**: `.sql` files are parsed, by the remaining dialects only.
- **Affected Files**: `internal/ast/antlr_adapter.go`

### UC-04: Embedded blocks honour the filter
- **Actor**: the parser, on a `.vue`, `.svelte`, `.html` or XML file
- **Preconditions**: the host language is enabled; the embedded language is
  blacklisted
- **Main Flow**:
  1. The host file is parsed normally.
  2. `tsLangConfigByName` / `antlrLangConfigByName` is asked for the embedded
     language and answers "not found", so the block is left as host-language
     text instead of being sub-parsed.
- **Error Scenarios**: none — an unresolved embedded language was already a
  supported outcome, and the existing code path handles it.
- **Postconditions**: no entity carries the disabled language's `Lang`.
- **Affected Files**: `internal/ast/treesitter_adapter.go`,
  `internal/ast/treesitter_embedded.go`

## Test Cases & Acceptance Criteria

### Feature: Grammar enablement by configuration
Ref: UC-01, UC-02, UC-03, UC-04

#### Scenario: A blacklisted grammar stops granting its extensions
```gherkin
Given a project whose lockfile sets ast.grammars_blacklist to "yaml"
When the pipeline asks whether any parser handles ".yaml"
Then the answer is false
And TreeSitterLangForExtensionIn returns the empty string for ".yaml"
```

#### Scenario: A grammar absent from a non-empty whitelist is disabled
```gherkin
Given a project whose lockfile sets ast.grammars_whitelist to "go"
When the pipeline asks whether any parser handles ".yaml"
Then the answer is false
And the same question for ".go" answers true
```

#### Scenario: The blacklist wins over the whitelist
```gherkin
Given a project whose lockfile sets ast.grammars_whitelist to "go,yaml"
And the same lockfile sets ast.grammars_blacklist to "yaml"
When the pipeline asks whether any parser handles ".yaml"
Then the answer is false
And the same question for ".go" answers true
```

#### Scenario Outline: Every name a language answers to disables it
```gherkin
Given a project whose lockfile sets ast.grammars_blacklist to "<entry>"
When the pipeline asks whether any parser handles ".yaml"
Then the answer is false

Examples:
  | entry             |
  | yaml              |
  | yaml_lang         |
  | tree-sitter-yaml  |
  | YAML_LANG         |
  |  yaml             |
```

#### Scenario: An unknown name disables nothing
```gherkin
Given a project whose lockfile sets ast.grammars_blacklist to "cobol"
When the pipeline asks whether any parser handles ".go"
Then the answer is true
And no error is reported
```

#### Scenario: A disabled language resolves to no queries
```gherkin
Given a project whose lockfile sets ast.grammars_blacklist to "go"
When the query resolution chain is asked for the "go" language and the ".go" extension
Then it returns no query files
```

#### Scenario: One ANTLR dialect off leaves the extension claimed
```gherkin
Given a project declaring three antlr4 query files that all claim ".dial"
And its lockfile sets ast.grammars_blacklist to "antlr-dialect_two"
When the pipeline asks whether any parser handles ".dial"
Then the answer is true
And the query resolution for "dialect_two" returns nothing
And the query resolution for "dialect_one" returns its file
```

#### Scenario: The empty configuration changes nothing
```gherkin
Given a project with neither key set
When the pipeline asks whether any parser handles ".go"
Then the answer is true
And the grammar filter reports itself inert
```

#### Scenario: A config change lands without a restart
```gherkin
Given a running process that has already resolved the filter for a project
When ast.grammars_blacklist is written into that project's lockfile
And the caches are invalidated
Then the next lookup for the blacklisted extension answers false
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/config/config.go` | Modified | `ResolveASTGrammarsBlacklist` / `ResolveASTGrammarsWhitelist` |
| `internal/ast/grammar_filter.go` | Created | the filter, its alias rule, and the per-project cache |
| `internal/ast/treesitter_adapter.go` | Modified | enforcement in `tsLangConfigFor`, `tsLangConfigByName`, `ParseWithGrammar` |
| `internal/ast/antlr_adapter.go` | Modified | enforcement in `HasAntlrForExtensionIn`, `Parse`, `ParseWithGrammar` |
| `internal/ast/treesitter_embedded.go` | Modified | enforcement in `antlrLangConfigByName` |
| `internal/ast/query_loader.go` | Modified | enforcement in `resolveQueriesForLang`; filter states reset by `InvalidateQueryCaches` |
| `internal/ast/grammar_filter_test.go` | Created | the scenarios above |
| `internal/ast/rule.go` | Modified | the agent-facing rule gains the grammar-disabled exclusion |
| `internal/ast/rule_grammar_disabled_test.go` | Created | asserts the rule names both keys, the config tool, and the silent-typo warning |
| `.claude/`, `.agents/`, `.kiro/` `skills/graphit-ast/SKILL.md` | **Not changed** — see the Progress Log entry below. Generated from `internal/ast/rule.go` by the *installed* binary, so they land after `make install` |
| `docs/specs/config_module.md` | Modified | key table rows and a section for the two keys |
| `docs/specs/ast_module.md` | Modified | a section beside the resolution chain |
| `docs/guides/ignore_files.md` | Modified | the grammar-level exclusion, beside the path-level ones, and the three-axis table |
| `docs/guides/troubleshooting.md` | Modified | new entry: *A whole language is missing from the AST code graph* |
| `docs/guides/user_manual.md` | Modified | *Turning a Language Off*, after the per-project grammar customisation |

## Trade-offs & Decisions

- **Enforced at the lookup, not at load.** Chosen over filtering query files as
  they are read, because the runtime and user levels are process-wide and shared
  by every project the daemon supervises — see the Justification section. The
  cost is eight call sites instead of one; the benefit is that a per-project key
  actually behaves per project.
- **An explicit `--grammar` override does NOT beat the blacklist.** The two could
  have been layered the other way (the flag is more specific than the config, and
  the framework's own precedence chain puts inline above lockfile). It was
  rejected because discovery would still drop the files: `collectFiles` knows
  nothing about the override map, so `--grammar` on a disabled grammar would
  yield "no files matched" rather than the parse the flag asked for — a hole that
  looks like a bug from every angle. One rule, stated once: a disabled grammar is
  not usable, and the way to use it is not to disable it.
- **An unknown name is inert, not an error.** A blacklist is read on every
  lookup, in a process that may not have the grammar pack installed yet.
  Rejecting unknown names would turn "I have not installed that grammar yet" into
  a hard failure of the whole index.
- **The lists match language *or* grammar, with the prefix optional.** Strictly
  matching only `grammar:` would be more precise and would make `yaml` — the
  obvious thing to write — silently do nothing, because that language is called
  `yaml_lang`. Precision that fails the obvious input is not precision.
- **`TreeSitterSupportedExtensions()` is left unfiltered.** It has no
  `projectDir` and feeds the HTTP `/parsers/status` endpoint, which reports what
  the engine can do, not what a given project has switched on.

## Technical Debt

- [ ] `graphit config` still accepts any key with no validation, so
  `ast.grammars_blacklst` (typo) is written happily and silently does nothing.
This is the pre-existing defect recorded in the configuration memory, CLI.
  `graphit config <chave> <valor>`*; these two keys widen its blast radius,
  because the symptom of a typo here is "the grammar I disabled is still being
  indexed" rather than a visible error. A key registry with a did-you-mean would
  fix it for every key at once.
- [ ] There is no way to *re-enable*, from a project, a grammar its machine's
  global config blacklists. `ResolveConfig` treats an empty value as "unset", so
  a project cannot override a global list with an empty one — it can only add a
  whitelist. This is a property of the config mechanism, not of this feature, and
  fixing it means teaching `ResolveConfig` about an explicit "empty" sentinel.
- [ ] The filter is consulted per lookup rather than folded into the per-project
  extension tables. Measured cost is two map length checks in the default case,
  so this is not urgent; it becomes worth revisiting if the per-file path ever
  grows more filter consultations.

## System Knowledge

- **Extensions are granted by the query file, not by the grammar.**
  `rebuildExtTables` builds `tsExtMap` from the `extensions:` field of the
  resolved YAMLs. This is why "disable a language" has always meant "remove its
  query file", and why a config key had to intercept the *lookup* rather than the
  grammar registry.
- **The global extension tables have no project.** `rebuildExtTables` is
  explicit about it: "Project-scoped languages are not here — there is no single
  project". Any per-project decision has to be taken by a function that receives
  a `projectDir`.
- **Every discovery path funnels through `HasParserForExtensionIn`.** The full
  walk (`collectFiles`, `internal/ast/writer.go`), the AST watcher
  (`internal/ast/watcher.go`) and the daemon's batch router
  (`classifyBatch`, `internal/daemon/syncmodule.go`) all call it. Enforcing there
  covers discovery completely.
- **`pruneVanished` is what makes disabling retroactive.** On a full index the
  live set comes from `collectFiles`, so a file that is no longer discoverable is
  removed from the shard cache and deleted from both the graph and the search
  index. No `--reset` is needed. In *scoped* mode the tree is never walked, so the
  prune does not run and the old nodes survive until a full index.
- **Resolving config is a disk read.** `ResolveConfig` → `GetGlobalConfigValue` →
  `LoadGlobalConfig` reads `~/.graphit/config.json` on every call, with no cache.
  Anything on the per-file path that resolves a config key must sit behind a rate
  limit, which is precisely why `queryDirState.get` takes the directory as a
  function rather than a string.
- **`antlrExtMap` maps an extension to a *list*.** Several dialects can claim
  `.sql` and each is tried in turn. A filter over that list is therefore a
  narrowing, not a rejection — which is what makes UC-03 work.

## Progress Log

### 2026-08-24

Read the memory first: The correction is to detach a language from the Abstract Syntax Tree (AST).
Query file, not grammar*, is the immediate precursor — it establishes that
  registered ≠ indexed, and that the query file is where a language's extensions
  come from. That is what pointed the design at the lookup boundary rather than
  at `nativeGrammars`.
- Traced the enforcement surface through the AST graph: readers of `tsExtMap`,
  `antlrExtMap`, `effectiveProjectQueryFiles` and `HasParserForExtensionIn`.
  Eight functions cover every path; the discovery and watcher call sites need no
  change because they all go through them.
- Wrote this log, then implemented T1–T7.
- T8: added `internal/ast/grammar_filter_test.go` — 11 test functions covering
  the alias rule, blacklist, whitelist, both together, the ANTLR multi-dialect
  narrowing, query resolution, the inert default, and pickup without a restart.
  `go test ./internal/ast/ -run 'Grammar'` passes; `go build ./...` and
  `go vet ./internal/ast/ ./internal/config/` clean.
- T9: documented in `config_module.md` (table rows + section), `ast_module.md`
  (section beside the resolution chain + note on the markdown precedent),
  `ignore_files.md` (a third exclusion axis: grammar, beside path and extension),
  `troubleshooting.md` (the "language missing from the graph" entry),
  `user_manual.md` (*Turning a Language Off*), and the agent rule in
  `internal/ast/rule.go`.
- Revised the enforcement to resolve the configuration **only after** a table hit
  (`withGrammarEnabled`, `enabledAntlrConfigsFor`, `antlrConfigByLanguage`). The
  first version resolved the filter at the top of `tsLangConfigFor`, which put a
  `sync.Map` load plus a `time.Now()` on every extension a full walk sees —
  including the majority that no grammar claims. Behaviour is identical; the
  default case now costs nothing on those.
- Verification: `go test -tags lancedb ./internal/ast/ ./internal/config/
  ./internal/knowledge/ ./internal/hub/` all pass (ast 62s). `-race` on the new
  paths and on the query-directory tests passes. `go vet -tags lancedb` clean on
  the three touched packages. `golangci-lint` reports 4 findings, all
  pre-existing and all in `internal/ast/search_common.go` (unused `rrfK`,
  `identifierTrigrams`, `queryTrigrams`, `indexedText`) — untouched by this work.
- Verified the three checked-in `SKILL.md` copies contain the new paragraph
  byte-identically to what `ASTRuleContent()` now renders, so no drift until the
  next regeneration.
- **The hand-applied `SKILL.md` copies did not survive, and that is the same
  two-speeds trap this repository already knows about.** The three checked-in
  files were edited to match the new `ASTRuleContent()` output byte for byte, and
  verified. The closing `graphit_sync` then regenerated all three from the
  **installed** binary — `~/.graphit/runtime/dev/graphit-core`, which predates this
  session — and reverted them to their previous content. `git status` shows them
  unmodified. Nothing is lost: `internal/ast/rule.go` is the source of truth, it
  carries the paragraph, and `rule_grammar_disabled_test.go` fails if it ever stops
  doing so. **The generated copies pick it up on the first sync after
  `make install`**, which was not run here (a full build: UI, grammars, ORT
  download). Re-applying the edit by hand would be worse than leaving it: any sync
  before that install reverts it again, and in the meantime the checked-in files
  would disagree with what the installed binary produces. Config and query files
  are DATA and propagate in seconds; rule text is Go CODE and propagates at
  `make install`.
- Left on the improvement backlog:
  `docs/tasks/backlog/graphit-config-aceita-qualquer-chave-sem-validacao-entao-um.md`
  — `graphit config` writes any key without validation, so a typo in either new
  key is silent. Pre-existing, but these keys widen its blast radius, because the
  symptom of a typo is "the grammar I disabled is still indexed" rather than an
  error. The dream module reports `status: inactive`, so nothing will pick the item
  up on its own; it is committed with the project for a human to find.
- Next: nothing outstanding for this task. The two debt items about
  `graphit config` key validation and the un-blacklist gap are pre-existing
  properties of the config mechanism and are recorded above.
