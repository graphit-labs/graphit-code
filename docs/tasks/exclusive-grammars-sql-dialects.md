---
title: Exclusive grammars — SQL dialects only when explicitly mapped
status: done
created: 2026-08-30
updated: 2026-08-30
tags: [ast, grammars, sql, configuration, resolution-chain]
---

# Exclusive grammars — SQL dialects only when explicitly mapped

## Objective

The ANTLR SQL dialect grammars — `db2`, `plsql`, `tsql`, `postgresql` — and the
tree-sitter `plpgsql` grammar must stop being reachable by extension. Today a `.sql`
file that tree-sitter parses with no entities falls through to ANTLR, which tries
PL/SQL, then PostgreSQL, then DB2, then T-SQL, and keeps whichever one happens to
extract something. That guess is wrong more often than it is right — a `.sql` file
in a PostgreSQL repository is not a PL/SQL package — and it is expensive: four full
ANTLR parses of the same buffer before the file is given up on.

These grammars must become **exclusive**: used ONLY when a configuration entry maps
an extension directly to them (`ast.grammar` = `.sql=antlr-plsql`, the override map
that already exists), never as the automatic choice for an extension and never as a
fallback after another grammar came back empty.

The flag that says so must be **generic and declared in the grammar's own YAML** —
not a hardcoded list of five language names in Go. Any grammar, shipped or installed
from the Hub, must be able to declare itself exclusive.

### Reasoning

- The mechanism the user asked to key off already exists end to end:
  `ast.grammar` → `config.ResolveGrammarOverrides` → `PipelineOptions.GrammarOverrides`
  → `CompositeParser.grammarOverrides`, and `CompositeParser.Parse` already short-circuits
  on it with *no fallback*. Nothing needs to be invented for the "intentional" half.
- What has to change is the *automatic* half: `rebuildExtTables` registers every query
  file's `extensions:` into `tsExtMap` / `antlrExtMap`, and those tables are what
  `HasTreeSitterForExtensionIn` / `HasAntlrForExtensionIn` answer from — which is both
  what discovery filters on and what `CompositeParser.Parse` branches on.
- So "exclusive" is precisely: **absent from the extension tables, present in the
  grammar-name tables**. Reachable by name, unreachable by extension.

### Why this approach over the alternatives

| Considered | Rejected because |
|---|---|
| Hardcode the five names in Go | Answers only for grammars the binary knows; a Hub-installed dialect could never opt in. Every other language-shaped decision already lives in the YAML (`comment_types`, `embed_labels`, `target_rules`) |
| Reuse `ast.grammars_blacklist` | Wrong semantics: a blacklisted grammar is off *everywhere*, including under an explicit override (`ParseWithGrammar` returns `grammar disabled by configuration`). Exclusive means "off by default, on when named" |
| Delete the `extensions:` field from the five YAMLs | An override needs the extension→grammar binding to survive somewhere, and discovery would never accept `.pks` again. It also makes the file lie about what it parses |
| Register exclusive grammars in the ext tables and filter at lookup | `tsExtMap` holds ONE config per extension, so an exclusive tree-sitter grammar would evict the non-exclusive one from the slot and rejecting it at lookup would lose both |

## Plan & Task Breakdown

- [x] **T1 — `exclusive:` in the query-file schema** — Spec: add `Exclusive bool` to
  `ExternalQueryFile` in `internal/ast/query_loader.go` and carry it in `mergeQueryFile`,
  so a `merge: true` file that stays silent about it inherits the base's answer. Done when
  a YAML declaring `exclusive: true` round-trips through the loader and through a merge.
  Constraint: opt-in and affirmative, like `merge:` — absent means the current behaviour.

- [x] **T2 — Keep exclusive grammars out of the extension tables** — Spec: `tsLangConfig`
  and `antlrLangConfig` gain `Exclusive`; `rebuildExtTables` and `projectTsExtMap` skip the
  `extensions:` registration for an exclusive file while still filling `tsGrammarMap`,
  `tsLangNameMap` and `antlrGrammarMap`; `HasAntlrForExtensionIn`'s project-query-file branch
  and `AntlrParser.Parse`'s project-YAML fallback branch skip exclusive files. Done when
  `HasAntlrForExtensionIn(dir, ".pks")` is false with no override configured and
  `ParseWithGrammar(path, "antlr-plsql")` still works. Constraint: resolution BY NAME must
  keep working — that is what the override, and an embedded block naming a language, use.

- [x] **T3 — Discovery honours the override map** — Spec: a cached per-project resolver for
  `ast.grammar` in the ast package (same shape as `grammarFilterState`), and
  `HasParserForExtensionIn` consults it first: when an override binds the extension, the
  answer is whether that grammar is known and enabled; otherwise the tables answer as before.
  Done when `.pks` is discovered again with `ast.grammar=.pks=antlr-plsql` set, and skipped
  without it. Constraint: this must not require a signature change — `collectFiles`, the
  watcher's `Accept` and `syncmodule` all have only `projectDir` at hand.

- [x] **T4 — Declare the five grammars exclusive** — Spec: `exclusive: true` in
  `internal/ast/queries/{db2,plsql,postgresql,plpgsql,tsql}.yaml`. Done when `.sql` resolves
  to tree-sitter-sql alone and no ANTLR dialect is tried after it. Constraint: `sql.yaml`
  (tree-sitter) stays non-exclusive — it is what `.sql` falls to by default.

- [x] **T5 — Tests** — Spec: cover the extension table exclusion, the by-name reachability,
  the override-driven discovery, and the merge inheritance. Done when `go test ./internal/ast/...`
  and `./internal/config/...` pass.

- [x] **T7 — The PL/pgSQL splice under an explicit PostgreSQL override** — Spec: prove, end to
  end through `ast.grammar=.sql=antlr-postgresql`, that a `LANGUAGE plpgsql` body is still
  re-parsed and its entities extracted. Done when a DECLARE local from inside the dollar-quoted
  body appears in the parsed file. Constraint: the assertion must go through discovery and
  `CompositeParser`, not the driver directly — the driver was already covered.

- [x] **T6 — Documentation** — Spec: `docs/specs/ast_module.md` (Grammar Resolution Chain,
  the ANTLR multi-dialect paragraph, Language Configuration Fields, the `--grammar` section)
  and this log. Done when the doc no longer claims `.sql` cascades PL/SQL → PostgreSQL → DB2
  → T-SQL and states the exclusive rule instead.

## Implementation Details

**`exclusive: true` means: absent from the extension tables, present in the grammar-name
tables.** Reachable by name, unreachable by extension. Everything else follows from that one
sentence.

- `ExternalQueryFile.Exclusive` (`internal/ast/query_loader.go`) is the declaration.
  `mergeQueryFile` inherits it when the upper file does not restate it — the same rule the
  other language-level fields follow, so a `merge: true` file that adds one query does not
  silently put the grammar back into extension resolution.
- `tsLangConfig.Exclusive` and `antlrLangConfig.Exclusive` carry it into the tables.
  `rebuildExtTables` (`internal/ast/treesitter_adapter.go`) skips the `extensions:` loop for an
  exclusive file and still writes `tsGrammarMap`, `tsLangNameMap` and `antlrGrammarMap` — which
  is what keeps `ParseWithGrammar` and embedded-block language resolution working.
- The three project-level doors got the same rule: `projectTsExtMap`, the project-query-file
  branch of `HasAntlrForExtensionIn`, and the project-YAML fallback inside `AntlrParser.Parse`
  (which also collapsed into `antlrConfigOf`, since it was rebuilding that struct by hand).
- `internal/ast/grammar_overrides.go` is new: a per-project cached resolution of `ast.grammar`,
  the same shape as `grammarFilterState` — same staleness interval, same reason (a config
  change lands on a running daemon without a restart). `grammarKnownIn` answers whether a
  grammar name is registered *and* not disabled.
- `HasParserForExtensionIn` now consults the override map **first**. When an override binds the
  extension the tables are not consulted at all; otherwise nothing changed. That single edit is
  what makes `.pks` discoverable again under configuration, because `collectFiles`, the
  watcher's `Accept` and the daemon's batch router all funnel through it and none of them has
  anything but a `projectDir`.
- `exclusive: true` added to `internal/ast/queries/{db2,plsql,postgresql,plpgsql,tsql}.yaml`.
  `sql.yaml` (tree-sitter) is untouched and is what `.sql` now resolves to.

### Behaviour change, stated plainly

An Oracle export indexed with no configuration used to produce a full PL/SQL graph and now
produces nothing until `ast.grammar` names the dialect. That is the requested semantics — the
dialects are intentional, not automatic — and it is the reason the troubleshooting guide got a
section of its own rather than a footnote.

## Use Cases

### UC-01: A `.sql` file is indexed with no grammar configuration
- **Actor**: the indexing pipeline (`RunPipeline` → `CompositeParser.Parse`).
- **Preconditions**: no `ast.grammar` entry for `.sql`; the shipped grammars are installed.
- **Main Flow**:
  1. `collectFiles` asks `HasParserForExtensionIn(project, ".sql")`.
  2. No override binds `.sql`, so the extension tables answer: `tsExtMap[".sql"]` is
     `tree-sitter-sql`; `antlrExtMap[".sql"]` is now empty.
  3. `CompositeParser.Parse` sees `hasTS && !hasAntlr` and parses with tree-sitter, once.
- **Alternative Flows**: the file yields no entities — it is still a tree-sitter result. No
  dialect is tried behind it.
- **Error Scenarios**: tree-sitter fails to parse — the error is returned as-is; there is no
  second engine to mask it.
- **Postconditions**: exactly one parse per `.sql` file.
- **Affected Files**: `internal/ast/composite_parser.go`, `internal/ast/treesitter_adapter.go`,
  `internal/ast/antlr_adapter.go`, `internal/ast/queries/sql.yaml`.

### UC-02: An Oracle repository opts into PL/SQL
- **Actor**: the user, then the pipeline.
- **Preconditions**: `ast.grammar` = `.sql=antlr-plsql,.pks=antlr-plsql` in the project
  lockfile (or global config, or the env var).
- **Main Flow**:
  1. `HasParserForExtensionIn` finds the binding and asks `grammarKnownIn` instead of the
     tables — `antlr-plsql` is in `antlrGrammarMap` and not disabled, so `.pks` is discovered.
  2. `CompositeParser.Parse` takes its override branch and calls
     `AntlrParser.ParseWithGrammar(path, "antlr-plsql")` directly.
  3. `parseWithConfig` resolves the PL/SQL queries and parses. No other dialect is tried.
- **Alternative Flows**: `--grammar` on the command line merges over the configured map for
  parsing. For an extension no other grammar claims, discovery has already run on the
  configured map alone, so the flag by itself finds nothing to parse.
- **Error Scenarios**: the grammar is blacklisted → discovery skips the extension and
  `ParseWithGrammar` would refuse with `grammar disabled by configuration`; the override names
  an unregistered grammar → `grammarKnownIn` is false and the extension is claimed by nobody.
- **Postconditions**: `.sql` and `.pks` files are in the graph as PL/SQL, with one parse each.
- **Affected Files**: `internal/ast/grammar_overrides.go`, `internal/ast/antlr_adapter.go`.

### UC-04: A PL/pgSQL function body is spliced under an explicit PostgreSQL override
- **Actor**: the pipeline, once `ast.grammar` names `antlr-postgresql`.
- **Preconditions**: `ast.grammar` = `.sql=antlr-postgresql`; the file holds
  `CREATE FUNCTION ... AS $$ ... $$ LANGUAGE plpgsql`.
- **Main Flow**:
  1. Discovery and dispatch go through UC-02 and land in the PostgreSQL ANTLR driver.
  2. `spliceCreateFunctionBodies` walks every `createfunc_opt_list`, reads the `LANGUAGE`
     option, and for `plpgsql` re-parses the dollar-quoted body with the vendored PL/pgSQL
     tree-sitter grammar, appending the subtree to the `anysconst` node.
  3. `postgresql.yaml`'s own queries run over the merged tree — `//decl_statement/decl_varname`
     for the DECLARE locals — and its `complexity:` block names PL/pgSQL's rules (`stmt_if`,
     `elsif_clause`, `stmt_for`, `stmt_foreach_a`, `case_when`, `proc_exception`), so the
     function's score walks into the spliced subtree.
- **Alternative Flows**: `LANGUAGE sql` / `plpython3u` / anything else — the body stays the
  opaque string constant it already was, which is correct: its language is a run-time property.
- **Error Scenarios**: the body fails to parse as PL/pgSQL — `parsePlpgsql` returns nil and
  nothing is spliced; the surrounding DDL is unaffected.
- **Postconditions**: locals declared inside the body are Variable entities of the `.sql` file.
- **Affected Files**: `internal/ast/antlr/postgresql/plpgsql_splice.go`,
  `internal/ast/queries/postgresql.yaml`.

### UC-03: A grammar author marks a grammar exclusive
- **Actor**: whoever writes a query YAML — this repository, a Hub language artifact, or a
  project's own `ast.queries_dir`.
- **Preconditions**: the YAML declares `language`, `extensions` and `exclusive: true`.
- **Main Flow**: the loader parses the flag; `rebuildExtTables` (or `projectTsExtMap`, for a
  project file) registers the grammar by name and not by extension.
- **Alternative Flows**: a `merge: true` file above it stays silent about `exclusive` and
  inherits it; restating `exclusive: true` keeps it. There is deliberately no way to un-declare
  it from above — that is what `ast.grammar` is for.
- **Error Scenarios**: none of its own; an exclusive grammar nothing names is simply unused.
- **Postconditions**: `HasParserForExtensionIn` returns false for its extensions unless an
  override binds one of them.
- **Affected Files**: `internal/ast/query_loader.go`, `internal/ast/treesitter_adapter.go`.

## Test Cases & Acceptance Criteria

### Feature: Exclusive grammars
Ref: UC-01, UC-02, UC-03 — `internal/ast/grammar_exclusive_test.go`

#### Scenario: an exclusive grammar does not claim its own extensions
```gherkin
Given a project whose queries directory declares an ANTLR grammar for ".dial" and ".excl" with exclusive: true
  And a tree-sitter grammar for ".fable" with exclusive: true
When discovery asks whether a parser exists for ".dial", ".excl" and ".fable"
Then the answer is no for all three
  And HasAntlrForExtensionIn is false for ".dial"
  And HasTreeSitterForExtensionIn is false for ".fable"
```

#### Scenario: exclusivity is per grammar, not per extension
```gherkin
Given an exclusive ANTLR grammar and a normal one that both claim ".dial"
When discovery asks whether a parser exists for ".dial"
Then the answer is yes
  And no exclusive config appears among the candidates enabledAntlrConfigsFor returns
```

#### Scenario: an override restores exactly the extension it names
```gherkin
Given a project whose config sets ast.grammar to ".dial=antlr-dialect_excl"
  And an exclusive ANTLR grammar registered as antlr-dialect_excl claiming ".dial" and ".excl"
When discovery asks about ".dial" and about ".excl"
Then ".dial" has a parser
  And ".excl" does not
```

#### Scenario: an override does not revive a disabled grammar
```gherkin
Given a project whose config sets ast.grammar to ".dial=antlr-dialect_excl"
  And whose config sets ast.grammars_blacklist to "dialect_excl"
When discovery asks whether a parser exists for ".dial"
Then the answer is no
```

#### Scenario: an override naming a grammar nobody registered claims nothing
```gherkin
Given a project whose config sets ast.grammar to ".dial=antlr-does_not_exist"
When discovery asks whether a parser exists for ".dial"
Then the answer is no
```

#### Scenario: a merging query file inherits exclusive
```gherkin
Given a base query file declaring exclusive: true
When a file with merge: true and no exclusive key is merged onto it
Then the merged language is still exclusive
```

### Feature: The shipped SQL dialects
Ref: UC-01, UC-02 — `TestShippedSQLDialectsAreExclusive`

#### Scenario: .sql resolves to tree-sitter alone
```gherkin
Given the query files this repository ships
When the extension tables are asked about ".sql"
Then tree-sitter claims it
  And no ANTLR dialect claims it
```

#### Scenario Outline: a dialect-only extension is not claimed without an override
```gherkin
Given the query files this repository ships and no ast.grammar configuration
When discovery asks whether a parser exists for "<ext>"
Then the answer is no

Examples:
  | ext       |
  | .pks      |
  | .pkb      |
  | .prc      |
  | .db2      |
  | .tsql     |
  | .pgsql    |
  | .plpgsql  |
```

### Feature: The PL/pgSQL splice
Ref: UC-04 — `TestPostgresOverrideKeepsThePlpgsqlSplice`

#### Scenario: a DECLARE local inside a dollar-quoted body becomes an entity
```gherkin
Given a project whose config sets ast.grammar to ".sql=antlr-postgresql"
  And a .sql file holding CREATE FUNCTION f(x INTEGER) ... AS $$ DECLARE spliced_local INTEGER; ... $$ LANGUAGE plpgsql
When discovery accepts the file and CompositeParser parses it
Then the parser used is antlr4
  And the parsed variables include "spliced_local"
```

#### Scenario Outline: every exclusive dialect stays reachable by name
```gherkin
Given the query files this repository ships
When grammarKnownIn is asked about "<grammar>"
Then the answer is yes

Examples:
  | grammar             |
  | antlr-plsql         |
  | antlr-postgresql    |
  | antlr-db2           |
  | antlr-tsql          |
  | tree-sitter-plpgsql |
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/ast/query_loader.go` | Modified | `ExternalQueryFile.Exclusive`; merge inherits it; `InvalidateQueryCaches` also drops the override cache |
| `internal/ast/treesitter_adapter.go` | Modified | `tsLangConfig.Exclusive`; `rebuildExtTables` and `projectTsExtMap` skip the extension registration |
| `internal/ast/antlr_adapter.go` | Modified | `antlrLangConfig.Exclusive`; project branches skip exclusive files; `HasParserForExtensionIn` consults the override map first |
| `internal/ast/grammar_overrides.go` | Created | Per-project cached `ast.grammar` resolution and `grammarKnownIn` |
| `internal/ast/grammar_exclusive_test.go` | Created | The scenarios above |
| `internal/ast/queries/db2.yaml` | Modified | `exclusive: true` |
| `internal/ast/queries/plsql.yaml` | Modified | `exclusive: true` |
| `internal/ast/queries/postgresql.yaml` | Modified | `exclusive: true` |
| `internal/ast/queries/plpgsql.yaml` | Modified | `exclusive: true` |
| `internal/ast/queries/tsql.yaml` | Modified | `exclusive: true` |
| `internal/ast/queries/postgresql.yaml` | Modified | `variables` query re-anchored at `decl_statement` — see below |
| `internal/ast/rule.go` | Modified | Agent rule: an empty graph on a SQL corpus is configuration, not a broken index |
| `docs/specs/ast_module.md` | Modified | New "Exclusive grammars" section; language table; `exclusive` in both field tables; the `.sql` cascade claim removed; `ast.grammar` section rewritten |
| `docs/specs/config_module.md` | Modified | `ast.grammar` key row and its own section |
| `docs/guides/user_manual.md` | Modified | Grammar Selection rewritten around `ast.grammar` |
| `docs/guides/troubleshooting.md` | Modified | "An Oracle / T-SQL / DB2 repository indexes nothing" |
| `README.md` | Modified | The ANTLR line notes the dialects are exclusive |

## Trade-offs & Decisions

- **The flag reaches parsing; the config key reaches discovery.** `HasParserForExtensionIn`
  has a `projectDir` and nothing else, and it is called from four places that have no pipeline
  options (`collectFiles`, the watcher's `Accept`, the daemon's batch router,
  `runners.go`). Threading a map through all four was the alternative; resolving `ast.grammar`
  from config inside the ast package covers every one of them, including the daemon, with no
  signature change. The residue is that `--grammar .pks=antlr-plsql` **alone** finds no files —
  documented in three places rather than silently left as a surprise.
- **An override to an unknown grammar now claims nothing**, where before the files were
  discovered and failed at parse time with `unknown ANTLR grammar`. Failing at discovery is
  quieter but it is the honest answer: the extension has no working parser.
- **Exclusivity is not expressible per project.** A project cannot make a shipped grammar
  exclusive, nor un-exclude one — the flag is the grammar's own statement about itself. Making
  it configurable would duplicate `ast.grammar`, which already answers "use this one here".

## Bug found while proving the splice

`postgresql.yaml`'s `variables` query was `//decl_stmt/decl_varname`, a direct-child XPath. The
grammar nests `decl_stmt` > `decl_statement` > `decl_varname`, so it matched nothing and **no
PL/pgSQL DECLARE local had ever been extracted** — a `CREATE FUNCTION` came back with its
`functions` and `parameters` entities and an empty `variables`. The existing coverage did not
catch it: `TestComplexityPlpgsqlSplicedIntoPostgresqlEntity` exercises the splice through the
complexity matcher, which walks node kinds and never runs the entity queries.

Unrelated to exclusivity in cause, found because proving the splice still works meant asserting
on something the splice produces. Re-anchored at `//decl_statement/decl_varname`.

## Technical Debt
- [ ] `--grammar` does not reach discovery. Fixable by adding an `extra map[string]string`
  argument to `collectFiles` and the watcher's `Accept` and threading
  `PipelineOptions.GrammarOverrides` into them. Deliberately not done: it touches four call
  sites for a flag whose documented use — correcting the grammar for an extension that is
  already discovered — still works.
- [ ] `TestShippedSQLDialectsAreExclusive` has to call `InvalidateQueryCaches` + `initTsExtMap`
  before asserting, because an earlier test in the package rebuilds the global tables while its
  own `HOME` is still redirected and `rebuildExtTables` reads `queryDirState.cached()`, which
  does not re-read the directory. Pre-existing isolation debt, not introduced here; the real
  fix is for `t.Cleanup(InvalidateQueryCaches)` to be registered *before* the `t.Setenv`.

## System Knowledge

- **`queryDirState.cached()` does not reload.** `InvalidateQueryCaches` marks the directory
  states stale and then calls `rebuildExtTables`, which reads `cached()` — so the tables are
  immediately rebuilt from the *previous* load, and the real reload only happens on the next
  `get()`. Invalidating while `HOME` points somewhere else therefore leaves the global tables
  empty until something calls `loadRuntimeCached` again. `initTsExtMap()` is that call.
- **`tsExtMap` holds one config per extension; `antlrExtMap` holds a list.** That asymmetry is
  why exclusivity had to be a registration-time skip rather than a lookup-time filter: an
  exclusive tree-sitter grammar registered into the single slot would evict the non-exclusive
  one, and rejecting it at lookup would then lose both.
- **`CALLS`-style resolution is not involved here at all** — this is entirely upstream of the
  graph, in which parser gets handed a file.

## Progress Log

### 2026-08-30
- Opened the log. Read the resolution path end to end before editing: `CompositeParser.Parse`
  (`internal/ast/composite_parser.go:34`), `rebuildExtTables`
  (`internal/ast/treesitter_adapter.go:168`), `enabledAntlrConfigsFor`
  (`internal/ast/antlr_adapter.go:961`), `HasParserForExtensionIn`
  (`internal/ast/antlr_adapter.go:992`) and its four callers — `collectFiles`
  (`internal/ast/writer.go:73`), `Watcher.Start` (`internal/ast/watcher.go:70`),
  `syncmodule` (`internal/daemon/syncmodule.go:197`), `runners.go:621`.
- T1–T2 landed: the flag, the two config structs, and every registration path that could
  re-admit an exclusive grammar by extension.
- T3 landed. Chose the config-resolved override map over threading a parameter through four
  call sites; the trade-off and its residue are recorded above.
- T4 landed: the five YAMLs.
- T5 landed: `internal/ast/grammar_exclusive_test.go`. `TestShippedSQLDialectsAreExclusive`
  failed at first for a reason that had nothing to do with the change — see System Knowledge on
  `cached()`. Full suite green: `go test ./...` exit 0.
- T6 landed: spec, config spec, user manual, troubleshooting, README, and the agent rule in
  `internal/ast/rule.go`. `AGENTS.md` is generated from that rule by the installed runtime, so
  it will pick the paragraph up on the next install rather than in this checkout.
- Interrupted: asked whether the PL/pgSQL splice still works when a user opts into
  `antlr-postgresql`. Re-read the splice rather than reasoning from the change: it binds the
  grammar at compile time (`sitter.NewLanguage(tsPlpgsql.Language())` inside the postgresql
  ANTLR package) and never touches the extension tables, the query loader or the grammar
  filter — so `exclusive: true` on `plpgsql.yaml` cannot reach it. `plpgsql.yaml` governs
  standalone `.plpgsql` files only; the spliced subtree is read by `postgresql.yaml`.
- T7 landed, and proving it surfaced a real defect in `postgresql.yaml` — see the section
  above. Full suite green after the fix: `go test ./...` exit 0.
