Task: #Tarefa was an error in a repository with 2,838 imports.

Status: completed on August 5, 2026. Third round of the series that began in
`docs/tasks/schema-antes-da-query.md` e seguiu em `docs/tasks/tradutor-cypher-sem-lista-fixa.md`.

## O sintoma, que foi uma chamada minha

```
MATCH (n:Import) RETURN count(n) AS n
→ Binder exception: Table Import does not exist.
```

It is declared by grammars as a label — `go.yaml` has `graph_label: Import` in the query.
of imports. This repository has 2,838 edges (`File-[:IMPORTS]->Module`) for 215 modules. Or
be: The imports were represented as edges in the graph, and the declared label did not exist as a table.

## A causa: um `continue` que jogava a entidade fora

In the branch of `dataKey == "imports"`, it constructed the register of `internal/ast/cache_convert.go`.
The code imported a canonical edge for the `Module`, ended at `continue`. The `label` was read three times.
Lines discarded before analysis. No entities, no tables, no nodes.

Decision of the Engineer: "Why not keep it in the drawer? And if a declared label doesn't arrive"
into the graph is an error. ** The `continue` exited.

Now an import is recorded twice intentionally, because it answers two questions:

- a aresta `File-[:IMPORTS]->Module` diz **de que este arquivo depende**, canonicalizada — todos
The files that pull the same module point to one node;
The entity says where the statement is, in this file on this line, that it's at the module node.
Shared cannot say.

The label: three forms, not one

The statements were derived from three directions in INLINE 0: 22 queries said
Here's the translation:

"INLINE_0", ten said INLINE_1, a INLINE_2, and seven nothing - while the pipeline did not use any of them.
The inline 0 was the worst: the entity's ID is by file, so it would honor her to create another node.
In line with the canonical that all files already point to.

Auditing standard by standard, two families are not literally equivalent, and the Engineer decided
separar em vez de uniformizar:

| forma | queries | label |
|---|---|---|
| `import x` / `use` / `require` / `@import` — Go, Python, Java, Kotlin, Scala, Groovy, Dart, Rust, PHP, Ruby, CSS, Haskell, Julia, JS/TS `import_statement` | 23 | `Import` |
| `preproc_include` — C, C++, ObjC | 6 | `Include` |
| `export_statement source:` — JS, TS, TSX (re-export) | 3 | `Export` |

The three produce the edge `IMPORTS`, because they pull module. `Export` was dead labeled until
Here: The const existed and nothing produced (`detectExportsTS` only marks `is_exported`), so there is none.
Collision. `Include` is new in `types.go`.

The inline 0 honors the declared for this family and replaces the rest with inline 1 - what
covers INLINE_0 and covers the missing label, which is what a query INLINE_1 must cover.
tem.

## O que eu errei no caminho, e quem me pegou

Normalize all 31 declarations into INLINE_0 once, including in queries
Here's the translation from Portuguese to idiomatic English:

"`type: relation`". "`TestVerifyAllDefaultQueries`", which already existed, failed in four grammars:

This translation maintains the structure and meaning of the original sentence while making it sound more natural in English.
The query's import has a type of relation, but there is an empty graph labeled "Import".
And that's why seven of those queries didn't declare anything—there was no oversight. Reversed.

Detail that this reversal revealed: `processRelations` (INLINE_1) does not remove entities.
From `result.Entities` onwards, a declared inline query as a relation continues through.
even part of `cache_convert`. As the label is forced in the code, these five grammars gain
It is even declared as `graph_label` without any coverage - covered by
`TestConvertToCacheForcesTheImportLabelWhenTheQueryDeclaresNone`.

## Erro hostil, corrigido no caminho

Here is the translation from Portuguese to idiomatic English:

Now it explains `Table X does not exist` instead of passing it raw:

This translation maintains the essence of the original sentence while using more natural phrasing in English.

```
ladybug query: Binder exception: Table Import does not exist. — "Import" is not a label or
relationship type in this project's graph: nothing indexed here produced it, so it has no
table. Present: AtRule, Attribute, ..., Variable
```

A label that no indexed file produced has an empty table, so linking it is a hard error here.
where Neo4j would return zero lines — and the harsh message reads as "the graph is broken" instead
"From 'this label does not exist IN THIS project' continue **error** for purpose: return result"
empty would make a mistake indistinguishable from an honest absence. It also covers types of dishonest relationships.
digitado (`-[:CALS]->`) e o caso do grafo vazio ou em rebuild, que apareceu de verdade durante
This task when I consulted a pre-swap bank.

## Testes

`internal/ast/cache_convert_imports_test.go` e `internal/ast/ladybug_missing_table_test.go`.

O que vale registrar sobre o desenho de dois deles: os testes de pipeline **encenam** o YAML do
repository with `stageGrammar`_. Without this, they measure the copy that the last sync installed on
runtime directory, and they wait while the repository file says something else - exactly
o que aconteceu na primeira tentativa, com o teste de C devolvendo `Import` porque leu o YAML
instalado.

The parser's cache did not invalidate, and the change didn't reach anyone.

After completing `make install` and a fully `sync`, the graph of this project continued without having one.
It is running the new binary. The cause: the shard cache is keyed by the **hash of
Content of the file remains unchanged; anything that the converter produces does not change the key. All files returned from the process are intact.
cache, com as entradas antigas.

What he resolved in his hands was INLINE_0, and it's worth understanding why: he does
In directory of the database (`os.RemoveAll`), and `CacheDir` is this very same directory.
(`runners.go:330`), then the reset wiped the cache along with it. It was a fortunate coincidence, not design.

The only lever that invalidates everything is `shardCacheVersion`.
Discarded, and now every file seems altered. It was for `2`.

Decision, with the right question from Engineer ("Are we in development, should we touch the seal?"): Yes, and
Being in Dev is an argument for. What makes a bump expensive is the installed base paying for rebuilds on it.
Update - risk that has not yet materialized. The cost here is a reparse from cache, exactly what the...
It has already been done. And it's not just a cache; the binary indexes two registered projects, each
com o seu, mais contextos importados quando houver;
The inline 0 achieves one at a time and depends on someone remembering. The failure that the seal avoids is
Silent — a new binary graph with an old cache returns a partially complete graph without any errors anywhere.
What is the way this series fails all together?

The _INLINE_0_ fixes semantics in both directions because the bump doesn't matter if it's not used.
tolerance is sometimes accepted.

It is registered, not corrected.

**objc, julia e haskell declaram as mesmas queries de import duas vezes** — uma como entidade e
A like `type: relation` with the same tree-sitter pattern (ObjC 3+3, Julia 2+2, Haskell 1+1). Only
does not write in double because the `seenNames` of the adapter discards the second match with the same name. Is
Inert duplication today, fragile: depends on a deduplication tool designed for another purpose. Like this.
By now, it has produced the edge, the copy `type: relation` is redundant — but removing it messes with
Production of five grammatical structures deserves independent verification.
