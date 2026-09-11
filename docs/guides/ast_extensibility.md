# AST grammars and parser extensibility

Graphit's AST module is language-driven. A language profile is a YAML file that
selects a parser, claims file extensions, declares extraction queries, and defines
language-specific behavior such as ownership, exports, complexity, target resolution,
semantic embedding, and embedded-language regions.

This guide is the user-facing contract for extending that system. It covers every
supported YAML field, both selector languages, parser installation, precedence,
hot reload, distribution, validation, and the current extension boundaries.

## What can be extended without rebuilding Graphit

You can do all of the following in a project YAML file:

- add or replace Tree-sitter extraction queries;
- introduce a new Tree-sitter language when its shared library is installed;
- bind extensions to a specific grammar;
- enable only an allowlist of grammars or disable selected grammars;
- define new entity labels and relationship types;
- configure parent contexts, export rules, docstrings, and complexity;
- control which labels receive semantic vectors;
- parse JavaScript, TypeScript, CSS, SQL, or another installed language embedded
  inside a host file;
- merge a small project-specific delta onto a shipped language profile.

Graphit ships 45 indexed language profiles: 40 Tree-sitter profiles and 5 ANTLR
profiles. A compiled Markdown grammar is also available internally, but no shipped
YAML claims Markdown extensions because prose belongs to the Knowledge module by
default. A project may deliberately add its own Markdown profile.

Tree-sitter is fully data-driven at runtime: an arbitrary compatible shared library
plus a YAML profile can introduce a language without rebuilding Graphit. ANTLR uses
the same YAML extraction model, but a new ANTLR grammar currently also requires a
Graphit contributor to add and register its generated `GrammarDriver` and sidecar
build target. The five registered ANTLR grammar names are `antlr-plsql`,
`antlr-postgresql`, `antlr-tsql`, `antlr-db2`, and `antlr-cobol85`.

## The complete YAML shape

The following example intentionally shows every supported key. It is a schema
reference, not a profile to copy unchanged. Remove fields that do not describe your
language.

```yaml
language: example
extensions: [".exm"]
parser: tree-sitter
grammar: tree-sitter-example
start_rule: source_file
merge: false
exclusive: false

exports:
  strategy: modifier
  config:
    keyword: public
  config_list:
    keywords: [private, protected]

self_keywords: ["this."]

context_types:
  class_declaration: Class
  function_declaration: Function

context_name_paths:
  class_declaration: name
  resource_block: string_lit[1]|name

anon_func_types: [arrow_function, function_expression]
declaration_types: [class_declaration, function_declaration]
comment_types: [comment, line_comment, block_comment]

embed_labels: [Class, Function, Comment]

target_rules:
  CALLS:
    labels: [Function, Method]
    fallback: stub
  SELECTS:
    labels: [Table, View]
    fallback: Table

complexity:
  node_types: [if_statement, for_statement, case_clause]
  operators: ["&&", "||"]
  head_calls:
    node_type: call
    names: [if, unless]
    pair_names: [cond]
    subject_pair_names: [case]

text_normalizers:
  entities:
    replace:
      "&lt;": "<"
      "&gt;": ">"
      "&amp;": "&"
    numeric_char_refs: true

embedded:
  - pattern: '(script_element (attribute (attribute_name) @_key (quoted_attribute_value (attribute_value) @lang)) (raw_text) @body (#eq? @_key "lang"))'
    text_capture: body
    lang_capture: lang
    default: javascript
    languages:
      js: javascript
      ts: typescript
    host_labels: [Component]
    wrap_prefix: ""
    wrap_suffix: ""
    normalize: entities

queries:
  - data_key: functions
    graph_label: Function
    pattern: '(function_declaration name: (identifier) @name) @scope'
    name_capture: name
    type: entity
    relation_type: ""
    value_capture: ""
    value_label: ""
    name_reject: '^(?i)(if|else)$'
    span_capture: scope
    name_is_data: false
    parent_capture: ""
    parent_label: ""
    target_labels: []
    qualifier_capture: ""
    target_fallback: stub
```

Unknown YAML keys are currently ignored rather than rejected. Treat spelling as part
of the contract and validate changes with an index run and the project tests described
below; a misspelled optional field can otherwise appear to load while doing nothing.

## Top-level fields

| Field | Type | Required | Meaning and default |
|---|---|---:|---|
| `language` | string | yes | Stable language selector. Files at different precedence levels are paired by this value, case-insensitively. The filename is not the identity. |
| `extensions` | string list | for discovery | Extensions or filename selectors claimed by the profile, normally including the leading dot. Matching is case-insensitive. An omitted list does not discover new files; it only stops extension-filtering when the language is already selected. |
| `parser` | string | no | `tree-sitter` when omitted; `antlr4` selects ANTLR. Do not use other values: the current dispatcher treats anything other than `antlr4` as Tree-sitter. |
| `grammar` | string | no | Runtime grammar name. Defaults to `tree-sitter-<language>` or `antlr-<language>`. Explicit names are recommended and must match the installed binary/registered driver. |
| `start_rule` | string | ANTLR metadata | Documents the ANTLR entry rule and participates in profile merging. The current registered `GrammarDriver` chooses its entry rule in code; this field does not dynamically invoke an arbitrary generated parser method. |
| `merge` | boolean | no | `false`/omitted replaces the same language at the lower level. `true` applies this file as a partial overlay. |
| `exclusive` | boolean | no | When `true`, keeps the grammar addressable by name but does not register its `extensions`. Use `ast.grammar` to select it explicitly. |
| `queries` | object list | conditionally | Extraction rules. A file with no valid queries is retained only when it contains recognized language configuration. |
| `exports` | object | no | Export/visibility strategy. |
| `self_keywords` | string list | no | Source prefixes such as `this.` or `self.` used to resolve receiver types on calls. |
| `context_types` | string map | no | Parser node/rule kind to graph label. These nodes can own nested entities. |
| `context_name_paths` | string map | no | Per-context path for finding the owner's name when no `name` field exists. |
| `anon_func_types` | string list | no | Tree-sitter node kinds recognized as anonymous functions assigned to names. |
| `declaration_types` | string list | no | Declaration node kinds eligible for preceding-comment docstrings. |
| `comment_types` | string list | no | Parser node kinds treated as comments. |
| `embed_labels` | string list | no | Labels that receive semantic vectors, in collision priority order. All entities still enter FTS. Empty means no semantic vectors for that language. |
| `target_rules` | map | no | Default target labels and unresolved-target policy per relationship type. |
| `complexity` | object | no | Syntax shapes counted in cyclomatic complexity. Omitted means base complexity `1`, not keyword guessing. |
| `text_normalizers` | named object map | no | Reusable, host-language text decoding rules for embedded blocks. |
| `embedded` | object list | no | Ordered Tree-sitter selectors for regions written in another language. |

## Query fields

Each item under `queries` defines either graph entities or a relationship source.
`data_key` groups queries that produce the same kind of result; multiple entries may
share a key.

| Field | Type | Required | Behavior |
|---|---|---:|---|
| `data_key` | string | yes | Internal result group. It is also the replacement unit under `merge: true`: redeclaring a key replaces every lower-level query with that key. Custom keys are allowed. |
| `graph_label` | string | for nodes | Graph node label emitted by an entity query. Relationship-only queries normally leave it empty. Custom labels are allowed. |
| `pattern` | string | yes | Tree-sitter query or ANTLR rule-path selector, according to `parser`. Invalid Tree-sitter patterns are skipped when compiled; invalid ANTLR patterns are skipped when activated. |
| `name_capture` | string | no | Name source. Defaults to `name`. In Tree-sitter it is a capture name, without `@`. In ANTLR it is a child-rule path; `name`/omitted uses the matched node's qualified or first terminal text. |
| `type` | string | no | `entity` when omitted; `relation` routes the capture as an edge/call input. |
| `relation_type` | string | for relations | Relationship name. Special routing exists for calls, construction, decorators, exports, and DML; other names become generic references. |
| `value_capture` | string | no | Captures a data value. Tree-sitter uses a capture name; ANTLR uses a child-rule path. Requires `value_label`; ignored for relations. The value becomes a searchable node and the parent's `value` property. |
| `value_label` | string | with value | Graph label for the captured value node. If missing, `value_capture` is disabled with a warning. |
| `name_reject` | regular expression | no | Rejects the entire match when the extracted name matches. Invalid expressions are ignored with a warning. Anchor keyword filters to avoid rejecting substrings. |
| `span_capture` | string | no | Tree-sitter capture that defines the entity line span instead of the default name-node-to-parent-end span. It does not change export or complexity input. |
| `name_is_data` | boolean | no | Normalizes the name as data: removes matching surrounding quotes and rejects blank, multiline, or overlong values. `value_capture` and `parent_capture` already imply data normalization. |
| `parent_capture` | string | no | Tree-sitter capture containing the explicit parent's name. Requires `parent_label`. |
| `parent_label` | string | with parent | Label of the explicit parent node. If missing, `parent_capture` is disabled with a warning. |
| `target_labels` | string list | no | Labels to which this relationship may resolve. Values are unioned with every query and the language-level rule for the same `relation_type`. Empty defaults to labels declared by this grammar. |
| `qualifier_capture` | string | no | Qualifies a target as `QUALIFIER.NAME`. Tree-sitter uses another query capture. ANTLR uses `ancestor_rule/path/from/ancestor`. A missing qualifier drops the match. |
| `target_fallback` | string | no | Unresolved target behavior: `stub` (default), `file`, or a graph label to assign to the placeholder. |

### Relationship routing

`CALLS` and `INSTANTIATES` become call sites. `DECORATOR` and `EXPORT` are consumed
by their specialized processors. `SELECTS`, `INSERTS`, `UPDATES`, `DELETES`,
`ALTERS`, `DROPS`, `TRUNCATES`, and `REFERENCES` are database-oriented edges.
Other values, including custom ones, become generic references. Relationship names
ending in `_FIELD` require an owning context.

## Tree-sitter selectors

Tree-sitter `pattern` values use the grammar's query language. Capture only the name
that should become a node or target, and use additional captures for predicates,
parents, values, qualifiers, or spans.

```yaml
queries:
  - data_key: build_tags
    graph_label: Constant
    pattern: '((comment) @name (#match? @name "go:build"))'
```

Predicates must belong to the same pattern expression. For embedded selectors in
particular, `(node) @body (#eq? @body "x")` is interpreted as two patterns; write
`(node @body (#eq? @body "x"))` so the predicate constrains the capture.

`name_capture`, `value_capture`, `parent_capture`, `span_capture`, and the Tree-sitter
form of `qualifier_capture` are YAML names without the `@` prefix. Embedded capture
names accept either form and are normalized by the loader.

## ANTLR selectors

ANTLR query `pattern` is a compact rule-path language:

| Form | Meaning |
|---|---|
| `//rule` | Match any descendant with this rule name. |
| `/rule` | Match the root/direct position with this rule name. |
| `//a/b` | Match direct child `b` under any descendant `a`. |
| `//a//b` | Match descendant `b` at any depth under `a`. |
| `//a[KEYWORD]` | Match `a` only when it has the terminal keyword as a direct child, case-insensitively. |
| `//a[!KEYWORD]` | Match `a` only when it does not have that direct terminal. |

ANTLR `name_capture` and `value_capture` use a different, match-relative child path:

- `identifier` selects a direct child rule;
- `default_value_part/expression` walks direct children;
- `**/literal` selects the nearest descendant rule named `literal`;
- an omitted/default `name_capture` uses the matched node's qualified name or first
  terminal.

ANTLR `qualifier_capture` is ancestor-anchored. For example,
`update_statement/general_table_ref` climbs to the enclosing `update_statement` and
then walks to its `general_table_ref`. The first segment is tracked as an anchor but
does not become an owning context unless it also appears in `context_types`.

## Parent context and name paths

`context_types` maps a Tree-sitter node kind or ANTLR rule to the label of the entity
that owns declarations nested inside it:

```yaml
context_types:
  class_definition: Class
  function_definition: Function
```

By default Tree-sitter reads the context node's `name` field. When a grammar stores
the name elsewhere, `context_name_paths` supplies alternatives:

```yaml
context_name_paths:
  element: STag/Name
  resource_block: string_lit[1]|name
```

Segments are `/`-separated field names or named child kinds. `kind[n]` selects the
zero-based occurrence among same-kind children; an unindexed segment selects the
first. `|` separates fallback paths. ANTLR uses the same configured string through
its context-name resolver for supported rule paths.

## Target resolution

The language-level form avoids repeating the same rule on every relationship query:

```yaml
target_rules:
  CALLS: { labels: [Function, Method], fallback: stub }
  SELECTS: { labels: [Table, View], fallback: Table }
```

Each relationship key accepts:

- `labels`: the allowed target labels. Empty means all node labels declared by the
  language's `graph_label`, `value_label`, and `parent_label` fields;
- `fallback`: `stub`, `file`, or the label for an unresolved placeholder.

Query-level `target_labels` values are unioned per relationship type, not isolated per
query. Query-level non-empty `target_fallback` wins for that relationship. Resolution
only links a unique match within the allowed label set; ambiguity remains unresolved.

## Export strategies

The `exports` object has three fields:

| Field | Type | Required | Behavior |
|---|---|---:|---|
| `strategy` | string | yes when `exports` is present | Chooses the visibility rule listed below. An unsupported value behaves as `none`. |
| `config` | string map | strategy-specific | Holds one scalar option. `no_prefix` reads `prefix` (default `_`); `modifier` reads `keyword`. Other strategies ignore it. |
| `config_list` | string-list map | strategy-specific | Holds list options. `no_modifier` reads `keywords`; other strategies ignore it. |

`exports.strategy` supports these values:

| Strategy | Configuration | Result |
|---|---|---|
| `capitalized_name` | none | Names beginning with ASCII `A`–`Z` are exported. |
| `no_prefix` | `config.prefix`; defaults to `_` | Names without the prefix are exported. |
| `modifier` | `config.keyword` | Declaration source containing the modifier is exported. |
| `export_statement` | none | Names discovered under Tree-sitter `export_statement` nodes are exported. |
| `no_modifier` | `config_list.keywords` | Declarations containing none of the listed modifiers are exported. |
| `no_static` | none | Non-`static` declarations are exported. |
| `none` | none | Nothing is marked exported. |

Unsupported strategy names behave as `none`. Modifier checks inspect the declaration's
leading source region, so use source spelling exactly as the language emits it.

## Complexity fields

| Field | Type | Required | Behavior |
|---|---|---:|---|
| `node_types` | string list | no | Counts each matching named parser node inside an entity's syntax subtree. |
| `operators` | string list | no | Counts matching anonymous leaf tokens such as `&&` and `||`. |
| `head_calls` | object | no | Adds text-sensitive branch rules for grammars whose control forms share one wrapper node kind. |

Do not list the same decision in both `node_types` and `operators`.

`complexity.head_calls` handles grammars that represent control forms as the same
generic call/list node:

- `node_type`: wrapper node kind;
- `names`: first-child texts that each add one branch;
- `pair_names`: forms such as `cond`, adding `floor(n/2)` clauses after the head;
- `subject_pair_names`: forms such as `case`, adding `floor((n-1)/2)` after excluding
  the subject.

Nested declarations are scored independently and do not inflate their parent.

## Embedded-language fields

`embedded` is ordered. The first block whose Tree-sitter pattern claims a body owns
that body, even if language selection later yields no parser. Put attribute-specific
selectors before generic fallbacks.

| Field | Required | Behavior |
|---|---:|---|
| `pattern` | yes | Host grammar's Tree-sitter query selecting body and optional language value. |
| `text_capture` | yes | Capture whose text is sent to the inner parser. |
| `lang_capture` | no | Capture holding a language selector such as `ts`. |
| `default` | conditionally | Inner Graphit language when no selector is present/empty. At least `default` or `languages` is required. |
| `languages` | conditionally | Case-insensitive allowlist mapping captured values to Graphit language names. An unknown explicit value is skipped rather than parsed as `default`. |
| `host_labels` | no | Candidate labels for the host unit. Allows equal-span containment and chooses the innermost matching entity. Without it, strict containment and non-content entities are used. |
| `wrap_prefix` | no | Same-line prefix added before parsing a fragment. Newlines are rejected. |
| `wrap_suffix` | no | Same-line suffix added after a fragment. Newlines are rejected. |
| `normalize` | no | Name of a `text_normalizers` entry from the host language. Unknown names are ignored with a warning. |

Embedding depth is bounded by the parser. Inner entities keep the inner language and
have their line numbers shifted back to the host file.

## Text normalizers

Each named normalizer accepts:

- `replace`: literal source text to replacement. Replacements run deterministically,
  longest key first and lexical order for equal lengths;
- `numeric_char_refs`: decodes decimal and hexadecimal numeric character references.

A blank key, a replacement containing `\n`/`\r`, or a normalizer with nothing to do
is dropped. Numeric references that would produce a newline remain encoded. These
rules preserve line offsets for the inner parse.

## Discovery, precedence, and merging

YAML profiles are loaded from three levels:

1. project: `<project>/<ast.queries_dir>`, default
   `<project>/.graphit/ast/queries`;
2. user global: `<GLOBAL_DIR>/ast/queries`, normally
   `~/.graphit/ast/queries`;
3. versioned runtime defaults:
   `<GLOBAL_DIR>/runtime/<version>/ast/queries`.

At a level, regular files ending in `.yaml` or `.yml` are considered; subdirectories
are ignored. Project wins over user, and user wins over runtime. The resolver matches
`language` case-insensitively, then filters by the selected extension.

Without `merge: true`, the upper file is the complete language profile. With it:

- scalar/list fields inherit when omitted; an explicitly declared non-empty value
  replaces the lower one;
- `queries` merge by `data_key`, and redeclaring one key replaces that entire group;
- `context_types`, `context_name_paths`, `target_rules`, and `text_normalizers` merge
  by map key;
- upper `embedded` blocks are prepended to lower blocks;
- `complexity.node_types`, `operators`, and `head_calls` inherit independently;
- `exclusive: true` is inherited, while `false` cannot clear an inherited `true` in
  the current boolean merge representation;
- `merge` itself is not inherited.

An empty list is indistinguishable from omission for the list-replacement rules. To
clear inherited list behavior, replace the whole profile rather than merge it.

Profiles are checked for changes at most once every two seconds. The signature uses
YAML filenames, sizes, and modification times. A change invalidates compiled query,
embedding-label, extension, filter, and override caches. Invalid files/queries are
skipped with warnings; the daemon continues running.

## Grammar selectors and enable/disable controls

There are three separate controls:

```json
{
  "config": {
    "ast": {
      "grammar": ".sql=antlr-postgresql,.pks=antlr-plsql",
      "grammars_whitelist": "sql,postgresql,plsql",
      "grammars_blacklist": "tree-sitter-sql"
    }
  }
}
```

- `ast.grammar` maps comma-separated extensions to exact grammar names. A missing
  leading dot is added and extension keys are lowercased. Invalid/blank pairs are
  ignored. The map selects one backend with no fallback; names beginning `antlr-`
  select ANTLR, all others select Tree-sitter.
- `ast.grammars_whitelist` is exhaustive when non-empty.
- `ast.grammars_blacklist` always subtracts and therefore wins over the allowlist.

Filter entries are case-insensitive and may name the YAML `language`, the complete
grammar name (`tree-sitter-python`, `antlr-plsql`), or its bare prefixed suffix
(`python`, `plsql`). A disabled grammar cannot be revived by an explicit override.

The per-command `--grammar` flag overlays configured mappings for parsing. Persist
`ast.grammar` when the extension is exclusive or otherwise undiscoverable, because
discovery and the daemon router resolve project configuration before command-only
pipeline options exist.

When both non-exclusive backends claim an extension, Tree-sitter runs first. ANTLR is
attempted only when Tree-sitter errors or extracts no entities. An explicit grammar
override disables that fallback.

## Installing a Tree-sitter parser

### 1. Build the shared library

Build the grammar as a native shared library compatible with Graphit's Tree-sitter
runtime. It must export `tree_sitter_<language>()`. Use one of these filenames, in
search order within a directory:

```text
tree-sitter-<language>-<goos>-<goarch>.<ext>
tree-sitter-<language>-<goos>.<ext>
tree-sitter-<language>.<ext>
```

The platform extension is `.so` on Linux, `.dylib` on macOS, and `.dll` on Windows;
non-Linux platforms also accept a `.so` fallback filename.

The loader derives `<language>` by removing `tree-sitter-` from the YAML `grammar`
value. Underscores in the lookup language become hyphens in filenames, while the
exported symbol retains the grammar's actual symbol spelling.

### 2. Install the binary

Grammar binaries have two search levels:

1. `<project>/.graphit/grammars/treesitter`;
2. `<GLOBAL_DIR>/grammars/treesitter`.

Project-local binaries win. They are platform-specific and gitignored. There is no
separate versioned-runtime binary lookup; shipped languages normally fall back to
grammars compiled into Graphit, while Hub language artifacts install external
binaries into the global directory.

### 3. Add the YAML

Create `<project>/.graphit/ast/queries/<language>.yaml` (or the configured
`ast.queries_dir`) with at least `language`, `grammar`, `extensions`, extraction
queries, and appropriate `embed_labels`. The filename can differ, but matching the
language makes maintenance clearer.

### 4. Restart after binary changes

YAML hot-reloads; shared libraries do not. Language resolution, including failed
lookups, is process-cached because swapping a native grammar under active parses is
unsafe. The daemon fingerprints grammar filenames, sizes, and modification times and
replaces itself when installed libraries change. For direct CLI/MCP processes, start
a new process after adding, removing, or replacing a grammar binary.

## Adding an ANTLR parser as a framework contributor

YAML alone can customize extraction for the five registered ANTLR drivers, but cannot
register a sixth driver today. Adding one requires all of these implementation pieces:

1. generate the Go lexer/parser package under `internal/ast/antlr/<language>`;
2. implement `antlr/common.GrammarDriver`, including the actual start-rule method and
   conversion to the shared `TreeNode` representation;
3. register `antlr-<language>` in `nativeAntlrDrivers`;
4. add a sidecar build-tag registration file under `cmd/graphit-antlr-sidecar` and add
   the grammar to the build/release grammar list;
5. add the complete YAML profile with `parser: antlr4`, grammar metadata, selectors,
   contexts, targets, complexity, and embeddings;
6. add hermetic driver, selector, extraction, filtering, exclusivity, and sidecar
   protocol tests.

Supported sidecars are named `antlr-sidecar-<language>` (plus `.exe` on Windows) and
are searched in:

1. `<project>/.graphit/grammars/antlr`;
2. `<GLOBAL_DIR>/grammars/antlr`.

Graphit keeps a CPU-budget-sized pool of long-lived sidecar processes, starts them
lazily, sends length-prefixed requests over stdin, and retries once with a replacement
process after a protocol/process failure. The request is a little-endian 32-bit frame
length followed by a null-terminated grammar name and source bytes. The response is a
32-bit length, one status byte (`0` success, `1` error), and either JSON `TreeNode`
payload or error text. Response frames above 256 MiB are rejected.

`start_rule` in YAML is descriptive/mergeable metadata; the driver implementation is
the current authority for which generated parser method starts the parse.

## Packaging and sharing a language

Do not commit native binaries under `.graphit/grammars`. To share a parser across
machines and operating systems:

1. build one binary per target OS/architecture;
2. package them into a `.grammar` archive with `graphit-grammar-pack`;
3. put the archive and one or more YAML files at the root of a language artifact
   directory;
4. publish it with `graphit hub submit <id> <directory> --type language --version <version>`;
5. install it with `graphit hub install <id>@<version> --type language`.

Example pack command:

```bash
graphit-grammar-pack \
  -o tree-sitter-example.grammar \
  -symbol tree_sitter_example \
  -platform linux/amd64:dist/tree-sitter-example.so \
  -platform darwin/arm64:dist/tree-sitter-example.dylib
```

The archive is version 1, zstd-compressed, and selects the exact current Go
`GOOS/GOARCH`. Its basename must begin with `tree-sitter-` or `antlr-`. Installation
copies YAML into the global query directory and extracts the current platform binary
into the corresponding global grammar directory. Project YAML still has higher
precedence.

## Validation workflow

There is currently no standalone grammar-schema validator, so validate at three
levels:

1. parse a small fixture with a scoped index:

   ```bash
   graphit ast index path/to/fixture --reindex
   ```

2. query the resulting entities and relationships with `graphit ast query`, and
   inspect daemon/CLI warnings for skipped files, fields, patterns, or captures;
3. for framework contributions, run the focused AST suite:

   ```bash
   go test ./internal/ast/...
   ```

Useful invariants for tests include:

- every claimed extension selects the intended grammar;
- extraction emits the intended label, name, span, context, and language;
- relations resolve uniquely or use the declared fallback;
- `embed_labels` names only labels the profile can emit;
- query overlays preserve or replace the intended `data_key` groups;
- embedded selectors reject decoys and preserve absolute line numbers;
- disabled and exclusive grammars are not discovered accidentally;
- a parser error or empty result follows the documented fallback path.

## Troubleshooting

| Symptom | Check |
|---|---|
| Files are not discovered | The extension includes the expected spelling; the grammar is not exclusive/disabled; an exclusive extension is persisted in `ast.grammar`. |
| `unknown ... grammar` | The YAML grammar name does not exist in the registered tables, or the query profile did not load. |
| `grammar load failed` | Shared-library filename, architecture, exported symbol, ABI, and project/global installation path. Restart after fixing it. |
| YAML edit appears ignored | Wait for the two-second staleness window; verify filename suffix, YAML syntax, `language`, `data_key`, and `pattern`; inspect logs. |
| A merged language loses behavior | Lists/scalars replace as a whole; empty lists inherit. Replace the full profile when you need to clear inherited values. |
| A Tree-sitter predicate matches decoys | Keep predicates inside the same pattern expression and verify capture names. |
| ANTLR captures the wrong text | Distinguish query `pattern` paths from match-relative capture paths; use `**/rule` only when nearest-descendant behavior is intended. |
| Relationships resolve to the wrong kind | Narrow `target_rules.<REL>.labels` or `target_labels`, and qualify names when scope alone is insufficient. |
| Embedded text has wrong line numbers | Remove newline-changing normalization/wrapping; the loader warns and ignores invalid replacements. |
| A new ANTLR sidecar is installed but unsupported | External installation does not register arbitrary ANTLR drivers; add the contributor integration described above. |

For storage paths and daemon observation behavior, see
[Filesystem and observation contract](filesystem_contract.md). For the complete AST
storage/retrieval design, see [AST module specification](../specs/ast_module.md).
