---
title: Rust AST ANTLR profile queries
description: Guards and nearest captures over official ANTLR trees.
doc_type: technical
references:
  - target: {type: task, id: tsk-0953}
    relation: implements
  - target: {type: task, id: tsk-a4cd}
    relation: derived_from
  - target: {type: task, id: tsk-08df}
    relation: consumes
  - target: {type: task, id: tsk-3f5d}
    relation: consumes
  - target: {type: task, id: tsk-1d33}
    relation: relates_to
  - target: {type: task, id: tsk-1f8a}
    relation: implements
---

# Rust AST ANTLR profile queries

ANTLR language parsing continues to use the maintained SQL/COBOL grammars and
official C++ runtime. Profile selection uses that runtime's XPath elements,
bound to the actual grammar during preparation. Rust coordinates requests and
processes the resulting node ordinals; tree selection remains in C++.

## Selection and filters

`pattern` is an official ANTLR XPath expression. `/rule` selects a direct child,
`//rule` selects descendants, and token selectors use the actual grammar's
vocabulary. Matches retain native preorder and are deduplicated per query;
duplicate queries retain distinct query indices.

Direct-child conditions are explicit configuration rather than XPath predicates:

```yaml
queries:
  - data_key: calls
    graph_label: Function
    pattern: '//general_element_part'
    child_rule: function_argument
    captures:
      name: [{xpath: '/id_expression'}]
```

`keyword` compares only direct terminal children using ASCII-insensitive equality.
`child_rule` tests a direct parser-rule child. `negate: true` inverts the configured
condition. These filters do not accept a matching descendant as a direct child.
`select` optionally continues with a native XPath expression after filtering the
anchor; this preserves guarded intermediate selections without introducing a
query grammar. Invalid vocabulary, paths or filter combinations fail preparation,
even for source files with no matching nodes.

## Captures and contexts

`captures` maps `name`, `value`, `span`, `parent` and `qualifier` to ordered small
operations. An empty list selects the matched node. An XPath operation selects
relative to the current node; a nearest-descendant operation uses breadth-first
order and returns the first matching rule at the smallest depth. A nearest-
ancestor operation starts at the parent and stops at its first matching rule:

```yaml
captures:
  name: [{xpath: '/identifier'}]
  qualifier:
    - nearest_ancestor: update_statement
    - xpath: '/general_table_ref'
```

Each operation consumes the previous result. A missing suffix remains missing;
it never retries a farther ancestor. Capture slots retain their order and do
not change the identities of selected nodes. `value`/`parent` captures require
the corresponding label. Unknown slots and ambiguous operations are rejected.

ANTLR owner definitions use `antlr_contexts`:

```yaml
antlr_contexts:
  - pattern: '//create_procedure_body'
    label: Procedure
    name: [{xpath: '/procedure_name'}]
```

Context definitions can use the same direct-child conditions. Empty `name`
selects the context node; explicitly captured names remain distinguishable for
owner association. Nearest scope and overlapping-selector validation remain
semantic-consumer responsibilities. Tree-sitter profiles use
[`context_name_queries`](rust_tree_sitter_contexts.md) and native captures.

## Ownership, controls and consumers

The effective profile owns prepared primary/preprocessor selector plans. The
prepared C++ objects retain immutable bound selectors, not parser, source or tree
pointers. Files share these plans; each parse has its own source, tree, result
collection and cancellation callback. A file-level rule/token index shares initial
descendant candidates across selectors instead of allocating tree-sized scratch
vectors separately for every query.

Cancellation and deadlines are checked during preparation, native walks and
capture operations. Failures preserve the original request error and query/slot
diagnostics. UTF-8 coordinates, source BOM handling, EOF and recovered/error nodes
continue through the existing native parser and original-source projection.

The semantic model feeds graph, SCIP and NN with these captures. No language
parser, runtime or cxx language bridge is replaced by this configuration change.

```sh
cargo xtask cargo test --locked -p graphit-ast --test antlr_profile_queries --test antlr_semantic --test antlr_model
cargo xtask cargo test --locked -p graphit-ast --test antlr_generic_graph --test scip_syntax --test native_nn_antlr
cargo xtask cargo test --locked -p graphit-app --lib ast_parser_resources::tests::
cargo xtask cargo test --locked -p graphit-app --test integration -- --exact foreground_ast_semantic::ast_only_publishes_current_graph_without_native_nn
```

The App checks compose the parser resources and publish through the real
foreground Source pipeline into the semantic graph. The AST-only case verifies
the Rust model and Java Tree-sitter fallback together, original source hashes and
Raw generations, capability records and the absence of NN publication when
disabled. NN projection and SCIP export have their own affected AST fixtures.

These are affected checks, not a performance campaign. Traversal and allocation
simplifications are structural; runtime performance gains require measurement
when a concrete decision calls for it.

Official references: [Tree-sitter query syntax](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html) and [ANTLR tree selection](https://github.com/antlr/antlr4/blob/4.13.2/doc/tree-matching.md).
