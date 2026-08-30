# Embedded block carries its own language; and three cuts to repetition in what the agent reads

## Context

Real-world usage audit of the framework on a large database corpus (Oracle PL/SQL +
flow-configuration XML). The agent that ran the analysis reported where the tools helped
and where they fell short. This log covers what was verified, fixed, and deferred.

Finding #1 in the report was the most serious, and its diagnosis **was wrong** — verification
changed the root cause. The rest were measured one at a time before becoming a change.

## The main defect: embedded-block reference resolved with the wrong language

### Symptom

In a configuration XML with embedded SQL, `MATCH (a)-[r]->(t:Table)` filtered by
`r.source_file` from the XML returned **zero rows**. The natural reading — "this flow
doesn't touch any table" — is the opposite of the truth.

The symptom was NOT a missing edge. The edges existed, as `File → File` self-loops:
SELECTS 2617, UPDATES 545, INSERTS 181, DELETES 25. An edge present and **inverted**,
which is worse than absent: nothing announces it as a gap.

### Root cause

The measurement that isolates the problem:

```cypher
MATCH (t:Table) RETURN t.lang, count(*)     -- plsql, 10674
MATCH (n) WHERE n.path STARTS WITH '<xml dir>/' RETURN n.lang, label(n), count(*)
-- ALL xml — including Cursor, Procedure, Variable, which only the PL/SQL parser produces
```

Everything the embedded parser produced was stamped with the HOST file's language.
`mergeParsedInto` folded the inner parse into the outer one by concatenating lists, without
recording where each item came from. Two engine guards then failed together:

1. `resolveNamed` requires `d.lang == lang`. That's deliberate and correct — a `.tsx`'s
   `fill()` must not bind to a Go function of the same name. But the reference said `xml`
   while the declarations were `plsql`, so it never matched.
2. `refRule` picks the `TargetRule` BY LANGUAGE, and the DML rules with `fallback: Table`
   live in `plsql.yaml`/`sql.yaml`. `xml.yaml` declares none, so the fallback became
   `TargetFallbackStub`, which returns `(ref.Path, LabelFile)`.

Neither is an isolated bug. The bug is the wrong language reaching them.

### Fix

- `internal/ast/parser.go`: `Entity`, `CallInfo`, and `ReferenceInfo` gained `Lang` — the
  language that PRODUCED the item, empty when it's the file's own.
- `internal/ast/treesitter_embedded.go`: `mergeParsedInto` stamps `inner.Language` onto all
  three lists, filling in **only what's empty** so the innermost block wins in a nesting.
  Helper `langOr`.
- `internal/ast/parse_cache.go`: `Lang` on `cachedCall` and `cachedReference`.
- `internal/ast/cache_convert.go`: propagates it with `langOr(item.Lang, pf.Language)`.
- `internal/ast/rebuild_index.go`: `resolveRefTarget` does `lang = langOr(ref.Lang, lang)`;
  the three callers of `resolveCallee` pass `langOr(call.Lang, fe.entry.Language)`.
- `internal/ast/shard_cache.go`: `shardCacheVersion` 4 → 5.

Applies to any embedded language, not just SQL in XML.

### Tests — of the GRAPH, and that's the point

`internal/ast/embedded_lang_resolution_test.go`, four cases. Verified that the main test
**fails without the fix**, with exactly the production symptom (0 rows).

A test on `pf.References` passes with the whole defect still in place, and that's how this
survived two rounds: `TestEmbeddedANTLRBlockProducesDMLEdges`, despite its name, never
looked at the graph. Includes a **negative control** — a reference without its own `Lang`
must not cross languages — without which the test would pass even if resolution ignored
language entirely.

## Three cuts to what the agent reads every session

### `ast_schema` groups labels that share the same property list

Almost every label is an entity label carrying the SAME 16 properties; only `File`,
`Directory`, and `Module` differ. That was ~25 repetitions of the same list. Now the unique
shapes print one per line (the difference between `File.path` and an entity's `path` is
exactly what makes a query blow up), and the shared ones print once, naming the labels.
No information is lost. `internal/ast/schema.go`, test in `schema_shared_shape_test.go`.

### AGENTS.md: the invariant policy is stated once

Five modules repeated the same six sentences (precedence, CLI prohibition, the list of
native tools, "if in doubt, it applies," "re-apply on every request," the integrity
clause). **Measured: ~3,228 bytes, 18.6% of the file, were copies beyond the first
occurrence.** Hoisted into `mandatePreamble()`, which always precedes the blocks. Each
block now states only what VARIES: domain, skill, triggers, tools.
`internal/hub/adapters/ide/mandate.go`. The old test asserted the old design and was
replaced by two: the block carries what varies and **not** the policy; and the preamble
states it exactly once — without this second half, deleting the policy would still pass.

### AST skill: when hybrid search is noise

In a corpus with no prose and a rigid naming convention (`PRC_`, `PCK_`, `IX_`), both sides
of the hybrid rank on text and have nothing to tell them apart: the observed result was
fifteen scores flattened into 0.03–0.05. The skill now names the case and the signal for
recognizing it (flat scores, top-1 no better than top-10), and directs straight to Cypher
with `STARTS WITH` on the prefix. `internal/ast/rule.go`.

## The "leaking label" was not an indexing defect — it was a missing instruction

Reported as `PRC_X` coming back with label `Value` instead of `Procedure`. These are two
legitimate nodes in the same file: the `Procedure` on line 2, and a `Value` on line 9, which
is the string literal `:= 'PRC_X'` initializing `v_nome_progr`. This happens with 338
procedures in the corpus and does **not** break resolution — `plsql.yaml`'s `target_rules`
restrict `CALLS` to `[Function, Procedure, Package, Trigger]`, and 34 `CALLS` edges reach
"shadowed" procedures normally.

**The first version of this log classified this as an agent-query trap. Wrong.** The agent
had read the skill — and the skill INSTRUCTS it to run exactly the query that produces the
confusing result: `Phase 2: Pre-search (Grounding)` calls for
`MATCH (n) WHERE toLower(n.name) CONTAINS ...` with no label, and the multi-label table has
the line "Anything — full discovery." Nowhere did it say that labels named after their
**content** exist. Whoever followed the instruction got a result the instruction never
prepared them for: that's a defect in the skill.

The whole class was raised instead of invented: `Value`, `AttributeValue`, and `Text` come
from `value_label` in 37 shipped grammars, and `Comment` was already documented. All four
have `name` equal to their own content.

## Instruction gaps closed in the same round

Applying the same criterion — if the agent read it and still got it wrong, the instruction
is missing — three more:

- **A DML query that comes back empty or with only readers.** "Nobody writes to this table"
  is a conclusion with consequences, and the failure mode isn't a missing edge: it's an
  edge that resolved to another node. The skill now instructs inspecting `label(a)/label(b)`
  by `source_file` before concluding absence, and explains that `File → File` means an
  unresolved target.
- **An `Index` with no columns or uniqueness.** Forbidden to infer "the database doesn't
  enforce this" from a query that found no unique index — the graph doesn't index that, so
  an empty result proves nothing. It now instructs reading the DDL with the source tool
  before making the claim.
- **Cold-start memory** (`internal/memory/rule.go`): an empty store and a query that missed
  are indistinguishable in a search. `graphit_memory_list` reads the store directly and
  settles it in ONE call — instruction to use it on the FIRST empty search, not the third.

## Second round: the deferred items, done

### Attribution to the host entity

A statement inside an embedded block now has as its source the entity that HOSTS it,
instead of the file. `attributeToHostEntity` + `hostEntityAt` in `treesitter_embedded.go`,
running before the merge, while the block's position is still at hand — the embedded parse
is the file's last step, so the host's entities already exist with absolute line numbers.

Innermost by span, because documents nest; content-named labels (`Value`, `AttributeValue`,
`Text`, `Comment`) are excluded, otherwise the text node CARRYING the statement would always
be the innermost match and the source would be the statement's own text. Ties are broken by
line and then name, because `Entities` is a map and maps have no order.

This is the half that makes the project's own grammar pay off: the engine has no idea what
the host models — a step, a job, a handler are all just entities some grammar declared —
and because it doesn't know, it attributes whichever one fits.

> **FIXED on 2026-08-19, and what is written above describes the defective version.**
> "Innermost by span" wasn't enough: the caller passed the OFFSET as the block's line, one
> line above it, so the source became the sibling above it — in indented XML, the `<key>`
> that precedes the `<value>`. And "innermost that spans the line" was replaced with
> "innermost that CONTAINS the block, strictly," because in a data grammar an entity's span
> ends at the start tag. See `docs/tasks/embedded-block-host-must-contain-the-block.md`.

### Index with table, columns, and uniqueness

`Index` used to be a name and nothing else. Now it carries:

- the **table**, as `REFERENCES` coming out of the index (`create_index` entered
  `context_types`);
- the **covered columns**, in order, which is semantically meaningful in a composite index;
- **uniqueness**, in `value` — exactly the property the audit found empty.

`UNIQUE` is a keyword, not a rule, and that required a new engine capability:
`ChildByRule` falls back to the TOKEN when no rule matches. Generic — any ANTLR grammar
that carries a fact in a keyword can now capture it. The `Token` comes with the grammar's
own spelling (`'UNIQUE'`, quotes included), so the comparison strips the quotes; found by
dumping the tree, not deduced.

Tested with a control: a NON-unique index must not get the marker — "the database
guarantees this" is exactly the claim that must never be invented.

## The one item NOT done, and why

**Column-level DML.** Investigated thoroughly and deliberately left out: the naive version
produces the same class of error this round existed to eliminate. Capturing the column is
trivial; resolving it is not. `resolveNamed` requires exactly one candidate, and
`ORDER_ID`/`STATUS`/`ID` exist in dozens of tables — so almost every edge would fall back,
and a single `Column` node would aggregate the writes of ALL tables. "Who writes this
column" would answer with whoever writes the same-named column of any other table,
presented as if it were this one's.

The blocker turned out to be precise: it's not that capture is missing (the token capture
added this round is what resolved the index's uniqueness), it's that QUALIFICATION is
missing. The table is a sibling of the column in the tree, and captures resolve downward, so
a pattern that matches the column can't reach the table, and one that matches the statement
only reaches the first column. The design that resolves it — `..` traversal in captures,
`qualifier_capture`, and a declaration index qualified by `context.name` — is written up in
the backlog, with an acceptance criterion that detects the aggregation if it comes back.

## Boundary reaffirmed

The engine knows FORMATS (`xml`, `sql`, `json`), never TOOLS. Recognizing the structures of
a concrete flow orchestrator is the consuming project's own custom grammar. This engine's
job is to deliver the generic apparatus — and when the consumer can't reach the answer with
it, the gap is here, even if nothing is technically broken.

## Progress Log

- 2026-08-15 — Diagnosis, fix, graph tests, three repetition cuts, three backlog items.
  `go test ./...` green. Consumers still need reindexing: the shard cache is keyed by
  content hash, and only bumping `shardCacheVersion` invalidates it, with the daemon
  running the new binary.
- 2026-08-15 (same session, later) — Correction by the Engineer: I had attributed a finding
  to an agent query error, when the agent had actually read the skill. The criterion becomes
  **the agent read the skill and still got it wrong = the instruction is missing**.
  Reclassified and turned into instructions: labels named by content (Phase 2 of the AST
  skill), diagnosing an empty DML query, the ban on inferring a database guarantee from an
  `Index` with no columns, and the cold-start signal in the memory skill.
  `internal/ast/rule.go`, `internal/memory/rule.go`.

## Third round: qualification, and the item that had been left out comes in

Column-level DML was left out because the naive version aggregated. The blocker was
QUALIFICATION, and it now exists as an engine mechanism.

### `qualifier_capture` — generic, across both backends

A captured target now resolves as `QUALIFIER.NAME`, and `scan()` indexes every declaration
with a `Context` under `context + "." + name` as well. The field is the same in YAML, and
the semantics follow the backend, because the trees are different:

- **ANTLR**: the pattern matches ONE node and captures descend, but the qualifier is a
  SIBLING (an UPDATE's table sits beside the SET). The path is anchored on an ANCESTOR —
  the first segment is the rule to climb to. The chain already existed in
  `MatchResult.Context`; what was missing was `contextRulePredicate` accepting the anchors,
  derived from the queries themselves via `qualifierAnchors`, deliberately WITHOUT turning
  into `context_type`, otherwise `update_statement` would end up owning everything inside it.
- **tree-sitter**: the pattern is structural and matches the whole tree, so the qualifier is
  another CAPTURE (`QualifierIdx` alongside `NameIdx`/`ValueIdx`/`ParentIdx`).

**The decision that defines the quality bar:** a query that asks for a qualifier and can't
get one emits NOTHING. An unqualified edge isn't a lesser version of a good one — it's
harmful. Qualifying also makes the fallback honest: a `PEDIDO.ST_PROC` stub records a
column of one table, where `ST_PROC` alone would have merged every table's columns
together.

### The relation-type allowlist became an exclusion list

`validDMLEdgeTypes` was a fixed list of RELATION NAMES in Go code — grammar vocabulary
trapped in the engine. It was stale in both directions: it admitted `CREATES`, `EXECUTES`,
and `TRUNCATES`, which no shipped grammar declares, and it **silently discarded** any new
type — entities extracted, references cached, no edge and no error. It became
`engineOwnedRelTypes`: the exclusion list of what the engine routes through its own path
(CALLS, INSTANTIATES, READS/WRITES_FIELD, INHERITS, IMPLEMENTS, IMPORTS, DECORATOR,
EXPORT). What the engine owns is now a closed question; whatever a grammar invents reaches
the graph on its own.

### Per-language coverage, which is the question to always ask

`WRITES_COLUMN` declared in **plsql, tsql, postgresql** (UPDATE + INSERT), **db2** (UPDATE
only), and **sql/tree-sitter**. The trees are all different, and each path came from a dump,
not an assumption. In db2, INSERT doesn't descend into column nodes — declaring the query
there would be a pattern that matches nothing, which is worse than absence because it looks
like coverage.

## Fourth round: index parity closed, and what it uncovered

Index shape now exists in **plsql, tsql, postgresql, db2, and sql/tree-sitter**. Each path
came from a dump. Two things came out of it:

**The uniqueness guard turned out to be unnecessary.** Since `ChildByRule` falls back to the
token, the capture returns empty when the marker isn't there — so ONE query resolves both
cases, and `plsql` was simplified from two queries to one. In postgres the marker isn't even
a token: it's the `unique_` rule, which simply doesn't appear in an ordinary index. Same
field, same answer.

**`context_name_paths` was only read by the tree-sitter backend.** In ANTLR, a context's
name came from `declarationName` — the declared name field, or else the FIRST TERMINAL. That
doesn't fail loudly: the first terminal of `CREATE UNIQUE INDEX ...` is the word `CREATE`,
so every entity inside the statement ended up with a context named "CREATE". Measured: in
tsql, `create_or_alter_function`, `create_or_alter_procedure`, and `create_schema` all
answered "CREATE"; in postgres, `createtrigstmt` answered with the trigger's TABLE instead
of its name. The same YAML key now answers consistently on both backends, with the same
rule walker.

**What this repo's own guard test uncovered.** Declaring `context_types` in `sql.yaml`,
`tsql.yaml`, and `postgresql.yaml` triggered `TestEveryCallableContainerIsDeclaredAsAContext`
and its sibling, which require that EVERY container a grammar declares be listed in
`context_types` — otherwise parameters and columns get attributed to whatever surrounds
them, or dropped. `sql.yaml` came out of `flatLanguages` and started declaring
`create_table`, `create_view`, and `create_function` in addition to the index. In other
words: parity wasn't just adding the index — it closed containment gaps these three
grammars were missing.

## Fifth round: backlog tackled, and the repository goes English

### Language: decided and applied

The Engineer closed the question that had been open in the backlog — **code and comments
are 100% English.** Translated ~520 lines: `rebuild_index.go` (100 lines, the largest block
and the module's most valuable rationale), `cache_convert.go`, the AST tests, and the
comments across 45 grammar YAMLs. Much of the YAML was the SAME paragraph repeated across 26
grammars — translated once and applied to all.

Two traps in the translation, both caught by tests: a comment in `comment_entity_test` was a
**fixture**, and its assertion mirrored the text (translating only one side broke the test);
and multi-line block replacement leaves an **orphan line** when the block changed since it
was written — the final pass scanning for accented characters is what closes that.

### `unused` linter: turned on, and the backlog's measurement was wrong

The backlog claimed "costs zero (0 issues)." The measurement had been run with
`golangci-lint run --enable unused`, and **`--enable` doesn't override the config's
`disable:`** — the linter never actually ran. Turned on for real: **25 dead symbols**, all
removed (a third copy of `copyDirRecursive`, a whole `mockGit` that builds nothing, a
benchmark's `rssSampler` whose benchmark had disappeared, `resolveWikiScopeDir`,
`isRemoteEmpty` from GitStore, and others). `make lint` is green, and a dead test function
confirms the net actually catches things.

### Daemon: stable cwd at startup

`chdirToStableDir()` at the start of `Daemon.Start`. The daemon inherited its cwd from
whoever spawned it — including a test that had chdir'd into its own `t.TempDir()` — and it
survived the directory's removal, after which EVERY tool that calls `os.Getwd()` failed
while the ones that resolve via `project_dir` kept working. That split is what made the
symptom look like it belonged to just one module. Deliberately best-effort: a daemon that
can't chdir is still a working daemon.

### Evaluated and kept in the backlog

- **Graph integrity probe** — still valid and worthwhile: it detects the SILENT corruption
  mode of LadybugDB string data, which no current test catches. It's well-scoped work
  (`graphit ast verify`), not done here.
- **Two loose ends from the dream** — still valid; the first (surfacing in the report a
  session's warning that it didn't use a tool) is cheap and should come first.
- **Flake in `TestMemoryGitStore_CreateOrphanBranch_Full`** — reproduced once this session
  under load, passed on 3 consecutive runs afterward. The backlog's diagnosis still stands.
- **ICU in the bundle** — blocked by the Engineer's decision ("bring the lib back, I'll deal
  with it later") and needs verification on macOS and Windows, which can't be done from here.
