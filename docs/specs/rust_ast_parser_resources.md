---
title: Rust AST parser resources
description: Linked syntax grammar owners, prepared profile factories and custom Docker SCIP producers.
doc_type: technical
references:
  - target: {type: task, id: tsk-861f}
    relation: implements
  - target: {type: task, id: tsk-a5f4}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: consumes
  - target: {type: task, id: tsk-4840}
    relation: supports
  - target: {type: task, id: tsk-04ab}
    relation: relates_to
  - target: {type: task, id: tsk-de12}
    relation: relates_to
  - target: {type: task, id: tsk-d284}
    relation: relates_to
  - target: {type: task, id: tsk-1528}
    relation: relates_to
---

# Rust AST parser resources

The App's [parser resources](../../crates/graphit-app/src/ast_parser_resources.rs)
acquire the Tree-sitter grammars linked into the installed Graphit package.
Project/global YAML profiles customize extraction and native context queries for
those grammars. The resources retain each actual GrammarLanguage owner while
prepared query factories and parsers use it. They do not discover or load native
`.so`, `.dylib` or `.dll` files from project or global directories, and there is
no native-library execution opt-in.

Adding a syntax grammar remains a framework build change, including its runtime,
license and extraction-profile tests. SQL and COBOL continue to use their
customized generated ANTLR C++ parsers, official runtime and existing cxx bridge.
This does not remove Tree-sitter/ANTLR fallback or their extension by profiles.

## Custom compiler producers

For runtime extension without loading native code into Graphit, configure
`ast.scip.parsers`, a string of comma-separated `extension=image` associations.
The [configuration resolver](../../crates/graphit-app/src/foreground_ast_sources/scip/config.rs)
uses `ConfigSnapshot.get<String>`; its winning whole-value layer is Command >
Environment > Project > Global > Distribution. An absent or explicitly empty
string clears lower associations. Whitespace around items and `=` operands is
trimmed; missing `=`, empty operands and empty comma-separated segments fail.
Config files keep the standard JSON `settings` envelope with a string value.
No JSON-object reader, additional environment decoder, deep merge, image registry
framework or native plugin loader is involved.

Keys normalize an optional leading dot and ASCII case, rejecting collisions and
invalid extensions. Values are single Docker arguments: registry host/port, tags
and digests are retained verbatim. Values with whitespace/control characters,
leading options or URL schemes are rejected; Docker validates the final image
reference. `ast.scip.version` continues to select maintained Graphit family tags
and is not appended to custom images.

The foreground resolves the custom association before the maintained family
route. A custom extension can provide compiler-only SCIP without a native syntax
profile. The existing Process host, captured Source authorization, operation
cancellation/deadlines, output validation and joined cleanup own its execution.
Only admitted documents enter the index. A compiler image cannot grant access or
invent syntax fallback when no linked profile exists.

See [AST extensibility](../guides/ast_extensibility.md#custom-compiler-images-by-extension)
for exact configuration examples, image creation, `/source` read-only input,
`/overlay`/`/workspace`, persistent image cache, `/cache/target`, `/output/index.scip`,
UID/GID and timeout environment, positions and failures. Compiler-image publication
and native platform execution are independent qualifications.

## Prepared syntax extraction and semantic publication

`contexts(profile, scope, Original)` prepares `PreparedTreeSitterContexts`;
`profile(profile, scope, Original)` prepares `PreparedTreeSitterProfile`. Factories
bind the effective profile's native queries to its linked grammar and can be
reused across the operation's files. Preparation checks Original; collection
checks the actual input's Original. Mutable parsers remain independent.

The [profile loader](../../crates/graphit-app/src/ast_profile_loader.rs) keeps
captured profile guards local to each operation. Shipped native models and the
[generic semantic model](rust_tree_sitter_generic_model.md) determine which
entities and projections are justified. Acquiring a grammar alone does not create
semantic entities or eligible embeddings. Generic profiles preserve configured
labels, native extents, contexts, diagnostics and Source associations.

The [semantic callback](../../crates/graphit-app/src/foreground_ast_sources/semantic.rs)
prepares factories before processing their files. Rust model values that are not
Send remain local to their blocking worker; the existing Tokio Handle can await
the configured model service there. No unsafe Send implementation or second model
server is needed. Graph/text publication does not wait for inference. Semantic
updates retain model-recipe and content-fingerprint checks and reuse compatible
vectors. Background jobs cooperate with cancellation and the supervisor joins
started workers before releasing their ownership.

## Historical qualifications

The earlier installed-library implementation had a Linux x86_64 resource test
run with 10 passing cases, including actual Global configuration, native JSON
libraries, ABI/export errors, query preparation, cancellation and process
replacement. Those results describe the removed loader and are historical, not
acceptance checks or proof of the current custom compiler-image consumer. Its
native-library fixtures and opt-in are not retained for compatibility. Current
changes are validated through the affected linked-resource, configuration and
SCIP consumer suites; unexecuted Windows/macOS or Docker Desktop paths are not
qualified by Linux source tests.
