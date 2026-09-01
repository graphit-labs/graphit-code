Key-value pairs as we do in data grammars (XML, JSON, YAML, TOML, HTML, HCL)

## Problema

The grammars of data formats mapped only to the key. The value— including those of attributes— did not exist in the graph, and what existed was wrong three different ways.

The value was neither here nor there.

An attribute `env="prod"` produced a single node `Attribute` called `env`. The `prod`
was nowhere to be found: not as a node, not as a property, not in the index for searching. Storing the value only in the property `value` would also not solve — the FTS reads `name_split` and `docstring` from a node and **never** `value` (`internal/ast/fts_sqlite.go`), so a value stored just as a property is
unreachable by the tool that people use to find things.

All captures turned into entities with the same label.

The executor tree-sitter iterated over all captures of a match and created an entity for each one, always with the `graph_label` from the query. The field `name_capture` was read from YAML, validated, propagated to the executor — and ignored. Only the ANTLR adapter respected it.

Consequences measures:

Grammatical:
The inline 11 generates **three** nodes 11, 12, and 13.

Inline 18 with identical pattern | Inline 19/Inline 20/Inline 21/Inline 22 with identical pattern | A block 23 becomes also nodes 24, 25, and 26 called 27 |

Inline 28 with identical pattern | Inline 29 | An edge 31 to the literal 32 beyond an edge to 33 |

Inline 34 with identical pattern | Inline 35 | A node 36 becomes a node 37 |

Inline 38 with identical pattern | Inline 39 | A node 40 becomes a node 41 |

Node 42 is called 44 — with the quotes, since capture was the literal whole

3. _inline_45_ and _inline_46_ were not indexed

The inline function declares __INLINE_48__ (since it is qualified by the identifier, because __INLINE_49__ also defines the format of its own query files) and __INLINE_50__. The inline function was seeking the native grammar **only by the language name**: __INLINE_52__ → nil. No YAML file was parsed.

Solution

Motor: Three Roles per Query

**INLINE_53** won four optional fields, and **INLINE_54** mirrored them in the same order (the gap between the two is positional):

| Field | Effect |
|---|---|
| `value_capture` | captures the value of the entity |
| `value_label` | label of the node that holds the value |
| `parent_capture` | captures the name of what contains the entity |
| `parent_label` | label of this node |

The fields capture the following effects:

- `value_capture`: Captures the value of the entity.
- `value_label`: Labels the node that holds the value.
- `parent_capture`: Captures the name of what contains the entity.
- `parent_label`: Labels this node.

A query now declares **parent, key, and value**. The value becomes a node itself,
named by itself — that's what puts it in the search index — contained by the key, and also is stored in the property `value` of the key, so that `RETURN n.name, n.value` responds without traversing.

The inline 61 exists because the inline 62 cannot express these grammars:
he climbs up the tree and reads the field ___inline_63___ of the ancestor, and ___inline_64___ of the tree-sitter-xml, ___inline_65___ of the tree-sitter-json, and ___inline_66___ of the tree-sitter-html do not have a field with this name. Naming the capture declares the containment that the pattern already has instead of trying to rederive it.


Indices are resolved once during the compilation of the query (`compiledQueryEntry.NameIdx/ValueIdx/ParentIdx`). They are not resolved by matching; the comparison in the warm path is read from `uint32` instead.

### Motor: `name_capture` passa a valer no tree-sitter

Only the named capture by `name_capture` becomes an entity. The others exist for testing a predicate — as the convention `@_` always signaled. This corrects all five cases in the table above.

Motor: Normalization of Text Data

A query that declares __INLINE_72__ or __INLINE_73__ is describing data, not an identifier. Therefore, the text of all its captures passes through __INLINE_74__:

- the outer quotes fall — `AttValue` (XML), `string` (TOML), and `string_lit` (HCL)
  encompass their own delimiters; `string_content` (JSON) and `attribute_value`
  (HTML) do not;
- blank text, multi-line, or above `maxDataValueLen` (256) is discarded, not truncated. A truncated value that still appears to be a value is worse than an absent value, and a scalar block with a script inside is not the name of anything.

Motor: Determinism in Cache Conversion

The iterator iterates over the graph — an edge — to build the lines.
Two queries can produce a legitimate entity (`xml.yaml` which creates one element for itself and again to reach its text), and the writer of the graph stores only the first line by `uid`+`label`, so **which of the two would win in subsequent executions on the same input**? Now the keys are visited in order, and an entity repeated **completes** the existing line (`value`, `docstring`, `args`, `decorators`) instead of generating a line that would be discarded.

Motor: Grammar correction in the `grammar` field

It tries to `Attribute` when the language name doesn't resolve. `AttributeValue` and `Element` are re-indexed.

Grammars

Selected labels by each format's terminology:

XML: Inline 96, Inline 97, Inline 98 → Inline 99 (character data)
HTML: Inline 100, Inline 101, Inline 102 → Inline 103 (visible content)
JSON: Inline 104, Inline 105, items of array belong to the key that names them
YAML: Inline 106, Inline 107, items of sequence are identical
TOML: Inline 108, Inline 109, items of array are identical
HCL: Inline 110, Inline 111, block labels corrected with Inline 112

In XML and HTML, the attribute also extends from the element through __INLINE_113__:

```
Element "database" → Attribute "host" → AttributeValue "db.example.com"
```

The contents of the following 114 (115, 116, 117, 118, 119, 120, 121, 122, 123) continue:

Containment says that a value belongs to a key, not that it points to some place.

What was left out, and why

Indentation __INLINE_124__ (JSON) and __INLINE_125__ (YAML) were not implemented. This would require a second format per key, which as the entity's __INLINE_126__ is derived from __INLINE_127__ + __INLINE_128__, the same JSON member would become two distinct nodes (__INLINE_129__ and __INLINE_130__). Correctly representing this requires changing the schema of __INLINE_131__. TOML's __INLINE_132__ has the same limitation — it already didn't work, because the tree-sitter-toml's __INLINE_133__ also lacks field __INLINE_134__.

Verification

`internal/ast/data_format_kv_test.go` (novo):

- a format test, checking the key node, value node, and edge between them;
- `TestDataFormatGraphIsQueryable` — complete pipeline to a real LadybugDB, with five traversals in Cypher (`(:Attribute)-[:CONTAINS]->(:AttributeValue)` etc.) and reading `p.value`;
- `TestDataValuesAreReachableByFullTextSearch` — `OrderRepository`, `singleton`, and `reporting-db` are values, not keys, and are found by index;
- `TestHelperCapturesAreNotEntities` — ruby, clojure, and graphql;
- `TestHCLBlockLabelsAreNotAllEntities` — the block `resource` generates a node, not three;
- `TestOversizedAndMultilineValuesAreNotNodes` — the limits of `dataText`.

Note: The inline code blocks (`TestDataFormatGraphIsQueryable` to `TestOversizedAndMultilineValuesAreNotNodes`) are placeholders and should be replaced with actual values or references.

The rejection tests were confirmed by disabling the filter of `name_capture`:

Falha without correction.

Suite `./internal/...` with `-tags fts5` passes. `TestHybridSearchQualityFloor` depends on the embedder's socket (`~/.graphit/daemon/embed.sock`), and is ignored when it does not exist—unrelated to this change.

## Arquivos modificados

- `internal/ast/query_loader.go` - `ExternalQueryDef` + 4 fields, validation,
  `captureIndex`, indices in `compiledQueryEntry`
- `internal/ast/treesitter_adapter.go` - `tsQueryDef` mirrored, `name_capture` respected,
  value node and context capture emission by `dataText`,
  __NOINDEX__ fallback grammar by `grammar`
- `internal/ast/cache_convert.go` - ordered iteration, dedupe by `uid`+`label`
- `internal/ast/ladybug.go` - new escape list labels in Cypher
- `internal/ast/queries/xml.yaml`, `json.yaml`, `yaml.yaml`, `toml.yaml`,
  `html.yaml`, `hcl.yaml`
- `internal/ast/data_format_kv_test.go` - new
- `docs/specs/ast_module.md` - documented query schema

## 2026-08-31 — YAML identity cleanup

The shipped YAML query now uses `language: yaml` and the filename `yaml.yaml`.
The older `yaml_lang` identifier existed only to avoid colliding conceptually with
the query-file format; it leaked that implementation detail into AST results,
configuration examples, tests, and documentation. The grammar remains
`tree-sitter-yaml`, so parsing and extension coverage are unchanged.

Verification: `go test ./internal/ast` passes.
