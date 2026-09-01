# Key and value as nodes in the data grammars (XML, JSON, YAML, TOML, HTML, HCL)

## Problem

The data-format grammars mapped only the **key**. The value — including
that of attributes — did not exist in the graph, and what did exist was wrong in three
different ways.

### 1. The value was neither a node nor searchable

An `env="prod"` attribute produced a single `Attribute` node named `env`. The `prod`
was nowhere: not as a node, not as a property, not in the search
index. Keeping the value only in the `value` property would not have solved it either — the
FTS index reads `name_split` and `docstring` of a node and **never** `value`
(`internal/ast/fts_sqlite.go`), so a value kept only as a property is
unreachable by the tool people use to find things.

### 2. Every capture became an entity with the same label

The tree-sitter executor iterated over **all** the captures of a match and created an
entity for each of them, always with the query's `graph_label`. The
`name_capture` field was read from the YAML, validated, propagated all the way to the executor — and ignored.
Only the ANTLR adapter honored it.

Measured consequences:

| Grammar | Pattern | Result |
|---|---|---|
| `hcl` | `(block (identifier) @_type (string_lit) @_rtype (string_lit) @name)` | `resource "aws_instance" "web"` generated **three** `Resource` nodes: `resource`, `"aws_instance"`, `"web"` |
| `hcl` | `variable`/`output`/`module`/`provider` with an identical pattern | a `variable "region"` block also became `Output`, `Module` and `Provider` named `region` |
| `html` | `(attribute (attribute_name) @_attr ... (#eq? @_attr "id"))` | `id="main"` generated a `REFERENCES` edge to the literal `id` in addition to the edge to `main` |
| `clojure` | `(list_lit (sym_lit) @_def (sym_lit) @name (#eq? @_def "ns"))` | `ns` became a `Namespace` node |
| `graphql` | `(operation_definition (operation_type) @_type (name) @name)` | `query` became a `Query` node |
| `json` | `(pair key: (string) @name)` | the node was named `"host"` — with the quotes, because the capture was the whole literal |

### 3. `.yaml` and `.yml` were not indexed

`yaml_lang.yaml` declares `language: yaml_lang` (the identifier is qualified
because `yaml` is also the format of the query files themselves) and
`grammar: tree-sitter-yaml`. `resolveTreeSitterLang` looked for the native grammar
**by the language name only**: `NativeLanguage("yaml_lang")` → nil. No YAML file
was parsed.

## Solution

### Engine: three roles per query

`ExternalQueryDef` gained four optional fields, and `tsQueryDef` mirrors them in the
same order (the cast between the two is positional):

| Field | Effect |
|---|---|
| `value_capture` | capture that carries the entity's value |
| `value_label` | label of the value node |
| `parent_capture` | capture that carries the name of whatever contains the entity |
| `parent_label` | label of that node |

A query can now declare **parent, key and value**. The value becomes a node of its
own, **named after itself** — that is what puts it in the search index —
contained by the key, and it is also written into the key's `value` property, so that
`RETURN n.name, n.value` answers without traversal.

`parent_capture` exists because `context_types` cannot express these
grammars: it walks up the tree and reads the ancestor's `name` field, and tree-sitter-xml's
`element`, tree-sitter-json's `pair` and tree-sitter-html's `start_tag` do not
have a field by that name. Naming the capture declares the containment the pattern already
matched instead of trying to re-derive it.

Capture indexes are resolved **once at query compilation time**
(`compiledQueryEntry.NameIdx/ValueIdx/ParentIdx`), not per match: the comparison on the
hot path becomes a `uint32` read.

### Engine: `name_capture` now applies in tree-sitter

Only the capture named by `name_capture` becomes an entity. The others exist for a
predicate to test — which is what the `@_` convention always signaled. That fixes, in one
go, the five cases in the table above.

### Engine: data text normalization

A query that declares `value_capture` or `parent_capture` is describing data,
not an identifier, so the text of **all** its captures goes through
`dataText`:

- outer quotes are dropped — `AttValue` (xml), `string` (toml) and `string_lit` (hcl)
  span their own delimiters; `string_content` (json) and `attribute_value`
  (html) do not;
- blank, multiline, or above `maxDataValueLen` (256) text is discarded, not
  truncated. A truncated value that still looks like a value is worse than an absent
  value, and a block scalar with a script inside it is not the name of anything.

### Engine: determinism in the conversion to cache

`ConvertToCache` iterated over `pf.Entities` — a `map` — to assemble the graph rows.
Two queries can legitimately produce the same entity (`xml.yaml` matches an
element for itself and again to reach its text), and the graph writer
keeps only the first row per `uid`+`label`, so **which of the two won changed
between runs over the same input**. Now the keys are visited in sorted
order, and a repeated entity **completes** the row that already exists (`value`,
`docstring`, `args`, `decorators`) instead of generating a row that would be discarded.

### Engine: grammar resolution by the `grammar` field

`resolveTreeSitterLang` now tries `NativeLanguage(grammar - "tree-sitter-")`
when the language name does not resolve. `.yaml`/`.yml` are indexed again.

### Grammars

Labels chosen by each format's terminology:

| Format | Key | Value | Extra |
|---|---|---|---|
| XML | `Attribute` | `AttributeValue` | `Element` → `Text` (character data) |
| HTML | `Attribute` | `AttributeValue` | `Element` → `Text` (visible content) |
| JSON | `Pair` | `Value` | array items belong to the key that names them |
| YAML | `Mapping` | `Value` | sequence items likewise |
| TOML | `Pair` | `Value` | array items likewise |
| HCL | `Attribute` | `Value` | block labels fixed with `#eq?` |

In XML and HTML the attribute also hangs off the element, via `parent_capture`:

```
Element "database" → Attribute "host" → AttributeValue "db.example.com"
```

The `REFERENCES` in `html.yaml` (`href`, `src`, `id`, `class`, `action`, `name`,
`for`, `role`) remain: containment says that a value belongs to a key, not that
it points somewhere.

### What was left out, and why

`Pair → Pair` (JSON) and `Mapping → Mapping` (YAML) nesting was not
implemented. It would require a second pattern per key, and since an entity's `uid`
is derived from `nome` + `contexto`, the same JSON member would start existing as two
distinct nodes (`f::host` and `f::database.host`). Representing that correctly
depends on changing the `uid` scheme, which is out of this scope. `Table → Pair` in
TOML has the same limitation — and it did not work before either, because tree-sitter-toml's
`table` also has no `name` field.

## Verification

`internal/ast/data_format_kv_test.go` (new):

- one test per format, checking the key node, the value node and the edge between them;
- `TestDataFormatGraphIsQueryable` — the full pipeline down to a real LadybugDB, with
  five Cypher traversals (`(:Attribute)-[:CONTAINS]->(:AttributeValue)` etc.)
  and the reading of `p.value`;
- `TestDataValuesAreReachableByFullTextSearch` — `OrderRepository`, `singleton` and
  `reporting-db` are values, not keys, and they are found by the index;
- `TestHelperCapturesAreNotEntities` — ruby, clojure and graphql;
- `TestHCLBlockLabelsAreNotAllEntities` — the `resource` block generates one node, not three;
- `TestOversizedAndMultilineValuesAreNotNodes` — the `dataText` limits.

The rejection tests were confirmed by disabling the `name_capture` filter:
they fail without the fix.

The `./internal/...` suite with `-tags fts5` passes. `TestHybridSearchQualityFloor`
depends on the embedder socket (`~/.graphit/daemon/embed.sock`) and is skipped when
it does not exist — unrelated to this change.

## Files modified

- `internal/ast/query_loader.go` — `ExternalQueryDef` +4 fields, validation,
  `captureIndex`, indexes in `compiledQueryEntry`
- `internal/ast/treesitter_adapter.go` — `tsQueryDef` mirrored, `name_capture`
  honored, emission of the value node and of the context per capture, `dataText`,
  `captureNodeAt`, grammar fallback by `grammar`
- `internal/ast/cache_convert.go` — ordered iteration, dedupe by `uid`+`label`
- `internal/ast/ladybug.go` — new labels in the Cypher escaping list
- `internal/ast/queries/xml.yaml`, `json.yaml`, `yaml_lang.yaml`, `toml.yaml`,
  `html.yaml`, `hcl.yaml`
- `internal/ast/data_format_kv_test.go` — new
- `docs/specs/ast_module.md` — query schema documented

## 2026-08-31 — YAML identity cleanup

The shipped YAML query now uses `language: yaml` and the filename `yaml.yaml`.
The older `yaml_lang` identifier existed only to avoid colliding conceptually with
the query-file format; it leaked that implementation detail into AST results,
configuration examples, tests, and documentation. The grammar remains
`tree-sitter-yaml`, so parsing and extension coverage are unchanged.

Verification: `go test ./internal/ast` passes.
