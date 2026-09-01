# Task: remove framework/ecosystem detection and `entry_point_score`, leaving that to the agent

**Status: done** on 2026-08-09.

## The problem

The AST module had two layers of heuristics whose only consumer, in the whole codebase, was an
agent reading a query example in the skill:

- **Framework/ecosystem detection** (`DetectFrameworks`, `DetectProjectConfig`) — scanned
  decorators, inheritance, imports and the presence of files like `go.mod`/`package.json`, and wrote
  the result only into the synthetic `__config__` node (`c.lang`, `c.source`). Nothing besides
  `RunEnrichment` itself called those functions, and nothing besides one query documented in the skill read
  `__config__` back.
- **`entry_point_score`** (`ScoreEntryPoints`/`scoreFunctionYAML`) — summed name/decorator rules
  coming from 44 framework YAMLs + every language YAML to score each `Function`/
  `Method`. Traced to the end: no search ranking, no automation without an agent in the loop
  (the "dream" is also an LLM agent, not a mechanical engine) depended on it — only the skill's
  documentation, teaching the agent to run `WHERE f.entry_point_score > 50`.

That is: the two layers existed to save the agent exactly the work it already knows how
to do for free — recognizing that `@RestController`/`app.get`/`Test*` are entry point conventions
of the framework in use — and they did it with real bugs found during the review: cumulative counting
without dedup between language and framework rules, `exported_bonus`/`max_score` shared
globally by "the last YAML loaded that sets a value > 0 wins" (not breaking today only
because all 44 YAMLs use the same 10/100 default), and the detection by decorator that stayed mute
without an error because LadybugDB has no `LEN()` function (documented in
`internal/ast/enrichment_tx_test.go`, now removed).

`cyclomatic_complexity` was analysed with the same rigour and was **not** removed: it is also a
heuristic (textual keyword counting, not a real CFG — it counts comments and string literals, and
depends on literal whitespace around the keyword), but its role is different: it needs to read the body of
every function in the repository to produce a comparative ranking, which the agent does not recreate for
free in a large investigation. Fixing the comment blind spot (excluding the `Comment`
spans before counting) was left for later — it is not the scope of this task.

## What was done

**Go:**
- `internal/ast/enrichment.go` deleted in its entirety (`DetectFrameworks`, `DetectProjectConfig`,
  `ScoreEntryPoints`, `scoreFunctionYAML`, `compileImportRegex`, `RunEnrichment`), along with
  `enrichment_test.go`, `enrichment_tx_test.go`, `enrichment_paging_test.go`.
- `internal/ast/query_loader.go`: removed `FrameworkFile`/`DecoratorRule`/`HeritageRule`/
  `ImportRule`/`EntryPointConfig`/`NameScoreRule`/`DecoratorScoreRule`, `EcosystemFile`/
  `EcosystemEntry`/`EcosystemExtract`, the resolvers (`ResolveFrameworks`, `ResolveEcosystems`,
  `ResolveAllLangConfigs` — this last one was left with no caller at all), the framework/ecosystem
  loaders/caches, and the `EntryPoints`/`ImportDetection` fields of `ExternalQueryFile`.
- `internal/ast/ladybug.go`: the `source` (File) and `entry_point_score` (entities) columns removed
  from LadybugDB's DDL.
- `internal/ast/rebuild_index.go` / `json_rebuild.go`: removed the initialization/serialization of
  `entry_point_score`; removed the call to `RunEnrichment` in the full rebuild.
- `internal/ast/incremental_rebuild.go`: removed the two calls to `RunEnrichment` (incremental
  and in-place rebuild) and the associated timing metrics.
- `internal/ast/rule.go` (source of `ASTRuleContent()`, which generates the skill): the ~12 references to
  `entry_point_score`/`__config__`/framework detection were removed or rewritten to
  teach the agent to name the convention of the framework in use (`f.is_exported AND (f.name = 'main'
  OR toLower(f.name) STARTS WITH 'test' OR toLower(f.name) CONTAINS 'handler')`) instead of
  looking for a precomputed property.
- `internal/ast/rule_cypher_test.go` / `rule_schema_first_test.go`: assertions adjusted to the
  real list of properties (without `entry_point_score`, without `source` on `File`).
- Hub: the `framework` artifact type removed (`internal/hub/registry.go`, `service.go` — 3
  places —, `reconcile.go`), since installing a framework YAML no longer had a consumer.

**YAML:**
- `internal/ast/frameworks/` (44 files) and `internal/ast/ecosystems.yaml` deleted.
- The `entry_points:` block (and `import_detection:` when present) removed from the 45 language
  YAMLs in `internal/ast/queries/`, along with the comment that preceded it.

**Docs and skill:**
- `.claude/skills/graphit-ast/SKILL.md`, `.kiro/skills/graphit-ast/SKILL.md`,
  `.agents/skills/graphit-ast/SKILL.md` — the three materialized copies, kept identical to the
  new `ASTRuleContent()`.
- `docs/specs/ast_module.md` — the "🏗️ Framework YAML Configuration" and "🌍 Ecosystem YAML
  Configuration" sections removed completely; `source`/`entry_point_score` taken out of the
  schema tables; example directory tree simplified.
- `docs/guides/user_manual.md` — the "Adding New Framework Support", "Customizing Framework
  Detection", "Customizing Entry Point Scoring", "Customizing Ecosystem Detection" sections and the two
  YAML reference tables (Framework Files, Ecosystems File) removed; the Hub artifact type
  count corrected from 11→10.
- `README.md` and `docs/specs/hub_collaboration.md` — the bullet/section for the "Framework Configs"
  artifact removed.
- `docs/specs/embedded_language_parsing.md` — the row of the decision table that cited
  `entry_point_score` adjusted to cite only `is_exported`, which still exists.

## Verification

`go build ./...` and `go vet ./...` clean. `go test ./internal/ast/... ./internal/hub/...` passes —
the `no such module: fts5` failures are environmental (SQLite driver without the FTS5 extension on this
machine) and predate the change, in search files not touched by this task.
