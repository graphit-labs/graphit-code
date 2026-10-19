---
title: "Wiki Module Specification"
description: "Current authored Knowledge compilation, progressive text/vector publication, references, retrieval and AI consumers."
content-type: reference
audience: developers
keywords:
  - wiki
  - search
  - BM25
  - RRF
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/ast_module.md"
  - "docs/specs/memory_module.md"
---


# Wiki Module Specification

Knowledge compiles selected authored documents into a searchable native
publication. A document remains the unit of source identity and retrieval;
rendered metadata, references and semantic projections derive from that
publication. Indexing, querying, export and Live/Ask consumption have distinct
admission and lifecycle contracts.

## Indexed scope and compilation

The selected PROJECT/version, documentation root, format policy and ignore
rules determine input. See [ignore files](../guides/ignore_files.md),
[document lifecycle](rust_knowledge_document_lifecycle.md) and
[stream publication](rust_knowledge_source_stream.md).

Read each file through its captured physical source and current authorization.
Preserve authored Markdown and explicit source identity. Invalid input, changed
bindings, cancellation and corruption remain causal errors; they do not publish
a partial replacement or silently reuse a different source. Incremental no-op
retains unchanged content and embeddings instead of repeating inference.

## Published storage and readers

[Knowledge data boundaries](../architecture/rust-knowledge-data-boundaries.md)
and the [semantic data contract](../architecture/rust-knowledge-semantic-data.md)
define the shared searchable Source/semantic dataset, schema, generation, exact pin and EOF rules.
Text and compatible vectors publish progressively; there is no parallel authoritative NN corpus.
Bodies stream by file; readers and index workers do not retain a second complete
corpus. A read does not create an index, repair metadata or gain write authority.

Published and imported readers use the publication where it lives, with exact
artifact selection and its own authorization. They do not rebuild it or
substitute the current PROJECT directory. Local links preserve their origin
identity rather than turning a copied payload into a new source project.

## References and document metadata

Wiki links can point at absent pages. Cross-reference inspection must preserve
that distinction so lint can report unresolved links instead of dropping them.
Typed `references` in Markdown frontmatter carry target type, identity and
relation; omission and an explicit empty list retain their documented semantics.
See [record relationships](record_relations.md) and
[Knowledge references](rust_knowledge_reference_query.md).

Source body, slug, title, tags, breadcrumb and modification time are document
metadata, not access grants. Renderers preserve the configured format and report
invalid fields without fabricating missing content. Navigation and clustering
consume the same selected publication.

## Native search and context

[Knowledge search](rust_knowledge_search.md) defines lexical, semantic and
hybrid modes, pagination, selected generations, preview and causal empty states.
Absent embeddings do not become invented vectors. Semantic unavailability and
available lexical results remain distinguishable in hybrid responses.

MCP discovery returns bounded metadata; agents read selected page bodies with
the source tools. Knowledge answers contracts and documentation while Task
answers work state, prior decisions and execution history. One store does not
replace the other. Task-disabled projects still permit authorized Knowledge
retrieval.

[Ask](../architecture/rust-knowledge-question.md) and
[Live](../architecture/rust-knowledge-ai-live.md) consume actual selected
contexts with their own USER/PROJECT authorization, process custody and final
answer gates. A disconnected subscriber does not imply cancellation of a
server-owned Live turn; explicit cancellation and request-bound streams retain
their respective lifecycle contracts.

## Interfaces

The [MCP tools](../guides/mcp_tools_reference.md),
[CLI](../guides/cli_reference.md) and
[Knowledge explorer](rust_foreground_knowledge.md) expose these producers.
Browser selection, HTTP authorization, exported Markdown and native readers
share the domain contracts; a library test does not by itself prove the whole
interface journey or native execution on another OS.
