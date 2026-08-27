Task: Remove detection of frameworks/ ecosystems and __INLINE_0__, leaving it for the agent.

Status completed on August 9, 2026.

## O problema

The AST module had two layers of heuristic whose sole consumer was an agent reading a sample query in the skill:

- **Framework/Environment Detection** (`DetectFrameworks`, `DetectProjectConfig`) — scanned decorators, inheritance, imports, and the presence of files like `go.mod`/__INLINE_4___, and wrote the result only in the synthetic node environment `__config__` (__INLINE_6__, __INLINE_7__). Nothing beyond its own function called these functions, and nothing beyond a documented query returned by skill `RunEnrichment`. - **Skill Query** (`__config__`) — summed rules of name/decorator coming from 44 YAMLs of framework + all language YAMLs to score each `Function`/__INLINE_14__. Refined until the end: no search ranking, no automation without agent in loop (the "dream" is also an LLM agent, not a mechanical engine) depended on it — only the skill documentation taught the agent to run `__config__`.

In other words: The two layers existed solely to save the agent exactly the work he already knows how to do for free — recognizing that `@RestController`/`app.get`/`Test*` are conventions of entry points in the framework being used, and doing this with real bugs found during the review: cumulative count without deduplication between language rules and framework rules, `exported_bonus`/`max_score` shared globally by "the last YAML loaded that set a value > 0 wins" (not breaking today because all 44 YAMLS use the same default 10/100), and the detection via decorator that would have been changed without error because LadybugDB doesn't have function `LEN()` (documented in `internal/ast/enrichment_tx_test.go`, now removed).

Inline 23 was analyzed with the same rigor and **not** removed: it is also a heuristic (keyword count text, not real CFG — counts comments and string literals, depends on literal space around the keyword), but its role is different: needs to read the entire function body of the repository to produce a comparative ranking, which the agent does not recreate for free in a large investigation. The correction of comment blind spot (remove spans of `Comment` before counting) was left for later — it is not the scope of this task.

What was done

**Go:**
- Inline 25 deleted entirely (Inline 26, Inline 27, Inline 28, Inline 29, Inline 30, Inline 31), along with Inline 32, Inline 33, Inline 34.
- Inline 35: removed Inline 36/Inline 37/Inline 38/Inline 39/Inline 40/Inline 41/Inline 42, Inline 43/Inline 44/Inline 45, the resolvers (the last one without any caller), the framework/loaders caches, and the fields Inline 49/Inline 50 of Inline 51.
- Inline 52: columns Inline 53 (File) and Inline 54 (entities) removed from the DDL of LadybugDB.
- Inline 55 / Inline 56: initialization/de.serialization of Inline 57; call to Inline 58 in full rebuild removed.
- Inline 59: removed two calls to Inline 60 (rebuild incremental and inplace), associated timing metrics.
- Inline 61 (source of Inline 62, which generates the skill): ~12 references to Inline 63/Inline 64/detection of framework were removed or rewritten to teach agent to name the convention of the framework in use (`f.is_exported AND (f.name = 'main' OR toLower(f.name) STARTS WITH 'test' OR toLower(f.name) CONTAINS 'handler')`) instead of searching for a pre-computed property.
- Inline 65 / Inline 66: assertions adjusted to the real list of properties (without Inline 67, without Inline 68 in Inline 69).
- Hub: artifact type Inline 70 removed (Inline 71, Inline 72 — three points —, Inline 73), since installing a framework YAML file no longer had any consumer.

YAML:
- 44 files deleted and file `entry_points:` removed.
- Block `entry_points:` (and `import_detection:` if present) removed from the 45 YAMLs of language in `internal/ast/queries/`, along with the preceding comment.

**Docs and Skills:**
- **INLINE_79**, **INLINE_80**, **INLINE_81** — the three materialized copies, kept identical to the new **INLINE_82**.
- **INLINE_83** — sections "🏗️ Framework YAML Configuration" and "🌍 Ecosystem YAML Configuration" removed completely; **INLINE_84**/**INLINE_85** removed from schema tables; simplified example directory tree.
- **INLINE_86** — sections "Adding New Framework Support", "Customizing Framework Detection", "Customizing Entry Point Scoring", "Customizing Ecosystem Detection" and the two reference YAML files (Framework Files, Ecosystems File) removed; artifact Hub type count corrected from 11→10.
- **INLINE_87** and **INLINE_88** — bullet/section of artifact "Framework Configs" removed.
- **INLINE_89** — line in the decision table that mentioned **INLINE_90** adjusted to mention only **INLINE_91**, which still exists.

Verification

And `go build ./...` and `go vet ./...` are clean. `go test ./internal/ast/... ./internal/hub/...` passes —
the failures of `no such module: fts5` are due to the environment (SQLite driver without FTS5 extension on this machine) and preexist in files not touched by this task.
