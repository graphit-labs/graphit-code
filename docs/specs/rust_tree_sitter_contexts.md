---
title: Rust Tree-sitter native contexts
doc_type: technical
description: Native context/name queries, cursor scopes and original-source projections over one collection.
references:
  - target: {type: task, id: tsk-e76e}
    relation: implements
  - target: {type: task, id: tsk-2922}
    relation: derived_from
  - target: {type: task, id: tsk-7890}
    relation: uses
  - target: {type: task, id: tsk-0953}
    relation: uses
---

# Rust Tree-sitter native contexts

`PreparedTreeSitterContexts` composes the existing native profile collector with
Tree-sitter queries. The grammar and parsed tree stay owned by the existing SDK
objects. A collection parses the input once and retains its original source
projection; it does not reconstruct a second syntax tree.

## Configure owner names

`context_types` maps visible native node kinds to semantic labels. Without an
explicit naming query, the SDK `name` field supplies the name. Declare a native
query when a grammar stores that name elsewhere:

```yaml
context_types: {pair: Pair, impl_item: Struct}
context_name_queries:
  pair: '(pair key: (_) @name) @context'
  impl_item: '(impl_item type: (_) @name) @context'
```

Use only node kinds from the loaded grammar; these two entries illustrate
separate JSON and Rust profiles. Queries require one `@context` and one `@name`
per pattern. `@context` must identify the configured context kind; `@name` must
belong to that same subtree. Invalid kinds, fields, syntax or capture bindings
produce configuration errors rather than falling back silently.

Tree-sitter query patterns express ordered alternatives. Pattern order decides
which complete match names a context; native anchors (`.`) and child patterns
express positional selection. Missing matches remain explicit missing-name
observations. A recovered or missing node preserves its native status and its
source mapping. A matching prefix never substitutes for a missing complete
name capture. Names remain source slices rather than text reconstructed from
the syntax tree.

## Scope association

A TreeCursor performs a conventional depth-first walk while an active context
stack records enclosing owners. Scope association excludes the captured node
itself and records owners nearest first. Entity captures at the owner name
therefore keep their previous self-scope exclusion. Anonymous functions use the
existing anonymous-kind policy and immediate variable-declarator `name` field;
unnamed/recovered contexts retain their diagnostics.

Defaults select only context/anonymous kinds exposed by the actual loaded
language. Declared maps/lists replace those defaults, including explicit empty
collections. Provenance remains on the effective profile. The implementation
uses native queries and field APIs for naming, with no path-expression parser,
second ancestor search per capture or copied context body for query matching.

Every collection and subsequent borrowed read checks the input's original
cancellation/deadline context. Byte ranges, Unicode and prepared-to-original
projections use the same retained tree/source contract as entity extraction.
Context errors do not authorize publication or substitute for semantic identity
validation.

## Consumers and verification

The typed, generic and JSON semantic models consume these same contexts. Their
existing graph/SCIP/NN producers retain source and identity validation.

```sh
cargo xtask cargo test --locked -p graphit-ast --test tree_sitter_context --test tree_sitter_model --test semantic_model
cargo xtask cargo test --locked -p graphit-ast --test tree_sitter_generic_model --test semantic_graph_json --test scip_syntax --test native_nn_tree_sitter
```

The tests cover default fields, explicit name captures, ordered alternatives,
positional matches, anonymous and nested owners, missing/recovered names,
Unicode/source projection and request termination. Execution results are
reported separately; the commands alone do not establish a platform pass.

Official references: [Tree-sitter query syntax](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html) and [ANTLR tree selection](https://github.com/antlr/antlr4/blob/4.13.2/doc/tree-matching.md).
