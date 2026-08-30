# Task: `MATCH (n:Import)` Was an Error in a Repository with 2838 Imports

**Status: done** on 2026-08-05. Third round of the series that started in
`docs/tasks/schema-antes-da-query.md` and continued in `docs/tasks/tradutor-cypher-sem-lista-fixa.md`.

## The symptom, which was a call I made

```
MATCH (n:Import) RETURN count(n) AS n
→ Binder exception: Table Import does not exist.
```

`Import` **is** a label declared by the grammars — `go.yaml` has `graph_label: Import` on
the imports query. And this repository has 2838 `File-[:IMPORTS]->Module` edges for 215
modules. In other words: the imports were in the graph as edges, and the declared label
didn't exist as a table.

## The cause: a `continue` that threw the entity away

In `internal/ast/cache_convert.go`, in the `dataKey == "imports"` branch: it built the
import record (the edge to the canonical `Module`) and ended with `continue`. The `label`
was read three lines earlier and discarded. No entity, no table, no node.

Engineer's decision: **there's no reason not to store it in the database, and if a declared
label doesn't reach the graph, that's a bug.** The `continue` was removed.

Now an import is recorded twice, on purpose, because it answers two questions:

- the `File-[:IMPORTS]->Module` edge says **what this file depends on**, canonicalized —
  every file that pulls in the same module points to a single node;
- the entity says **where the statement is**, in this file, on this line — something the
  shared module node can't say.

## The label: three shapes, not one

The declarations were spread across three directions under the same `data_key`: 22 queries
said `Module`, 10 said `Import`, one said `""`, and 7 said nothing — while the pipeline used
none of them. `Module` was the worst: the entity's uid is per-file, so honoring it would
fabricate a second `Module` node next to the canonical one every file already points to.

Auditing pattern by pattern, two families turned out not to be a literal import, and the
Engineer decided to separate them instead of unifying everything:

| shape | queries | label |
|---|---|---|
| `import x` / `use` / `require` / `@import` — Go, Python, Java, Kotlin, Scala, Groovy, Dart, Rust, PHP, Ruby, CSS, Haskell, Julia, JS/TS `import_statement` | 23 | `Import` |
| `preproc_include` — C, C++, ObjC | 6 | `Include` |
| `export_statement source:` — JS, TS, TSX (re-export) | 3 | `Export` |

All three produce the `IMPORTS` edge, because all three pull in a module. `Export` was a
dead label until now: the constant existed and nothing produced it (`detectExportsTS` only
sets `is_exported`), so there's no collision. `Include` is new in `types.go`.

`importEntityLabel` honors the declared label for this family and replaces everything else
with `Import` — which covers `Module` and covers the missing label, something a
`type: relation` query is required to have.

## What I got wrong along the way, and who caught it

I normalized **all** 31 declarations to `Import` at once, including in `type: relation`
queries. `TestVerifyAllDefaultQueries`, which already existed, failed on four grammars:
*query imports has type=relation but non-empty graph_label "Import"*. A relation carries no
label, which is exactly why seven of those queries declared nothing — it wasn't an
oversight. Reverted.

A detail that this revert exposed: `processRelations` (`helper.go:138`) **does not remove**
entities from `result.Entities`, so an import query declared as a relation still passes
through the same `cache_convert` branch. Since the label is forced in code, those five
grammars still get an import node even though they declare no `graph_label` — covered by
`TestConvertToCacheForcesTheImportLabelWhenTheQueryDeclaresNone`.

## A hostile error, fixed along the way

`Query` now explains `Table X does not exist` instead of passing it through raw:

```
ladybug query: Binder exception: Table Import does not exist. — "Import" is not a label or
relationship type in this project's graph: nothing indexed here produced it, so it has no
table. Present: AtRule, Attribute, ..., Variable
```

A label that no indexed file produced has no table, so matching it here is a hard error —
whereas Neo4j would return zero rows — and the raw message reads like "the graph is broken"
instead of "this label doesn't exist IN THIS project." It stays an **error**, on purpose:
returning an empty result would make a typo indistinguishable from an honest absence. It
also covers a mistyped relationship type (`-[:CALS]->`) and the case of an empty or
rebuilding graph, which came up for real during this task when I queried a pre-swap
database.

## Tests

`internal/ast/cache_convert_imports_test.go` and `internal/ast/ladybug_missing_table_test.go`.

Worth noting about the design of two of them: the pipeline tests **stage** the repository's
YAML with `stageGrammar`. Without that, they measure the copy the last sync installed in the
runtime directory, and they pass while the repository file says something else — which is
exactly what happened on the first attempt, with the C test returning `Import` because it
read the installed YAML.

## The parse cache didn't invalidate, and the change reached no one

After `make install` and a full `sync`, this project's graph still had **not a single
`Import` node** — while running the new binary. The cause: the shard cache is keyed by the
file's **content hash**, and changing what the converter *produces* doesn't move the key.
Every file came back from the cache, with the old entries.

What fixed it manually was `ast index --reset`, and it's worth understanding why: it does
`os.RemoveAll` on the database directory (`runners.go:268`), and `CacheDir` **is that same
directory** (`runners.go:330`), so the reset wiped out the cache along with it. That was a
fortunate coincidence, not a design.

`shardCacheVersion` is the only lever that invalidates everything — a manifest written under
a different version is discarded, and then every file looks changed. It was bumped to `2`.

**Decision, prompted by the Engineer's right question ("we're in dev, is it worth touching
the version stamp?"):** yes, and being in dev is actually an argument *in favor*. What makes
a bump expensive is an installed base paying for a rebuild on update — a risk that doesn't
exist yet. The cost here is one reparse per cache, exactly what `--reset` already did. And
it's not just one cache: the binary indexes two registered projects, each with its own, plus
imported contexts when there are any; `--reset` reaches one at a time and depends on someone
remembering to run it. The failure the version stamp prevents is silent — a new binary with
an old cache returns an incomplete graph with no error anywhere, which is the failure mode
of this whole series.

`shard_cache_version_test.go` pins the semantics in both directions, because the bump is
worthless if the mismatch is ever tolerated.

## Left on record, not fixed

**objc, julia, and haskell declare the same import queries twice** — once as an entity and
once as `type: relation`, with the same tree-sitter pattern (objc 3+3, julia 2+2, haskell
1+1). It only avoids writing duplicates because the adapter's `seenNames` discards the
second match of the same name. It's inert duplication today, and fragile: it depends on a
dedup mechanism that exists for a different purpose. Since the entity path already produces
the edge, the `type: relation` copy is redundant — but removing it touches edge production
in five grammars and deserves its own verification.
