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

## Authored page identity

For an authorized agent creating a new Knowledge page without an explicit ID,
the authoring sequence is:

1. Call `graphit_knowledge_id` with the actual relative page path. Root accepts
   `{"path":"docs/guide.md"}`; Role uses the existing selection envelope with
   `{"input":{"path":"docs/guide.md"}}`. Optional `ai_optimized` stays at the
   envelope root.
2. Receive the ULID from the tool. Do not invent an ID or use an empty placeholder.
3. Save that value as YAML frontmatter `id` together with the document. The agent
   writes the authorized page; neither the tool nor the indexer writes its ID.

Preserve a supplied valid `id` literally when editing or indexing an existing
page. A unique explicit ID preserves identity across a rename. If the tool or
authoring permission is unavailable, report the gap; do not silently skip the
identity step for a new agent-authored page. No CLI generator is required.

Knowledge also accepts external documents and sources without frontmatter.
During publication the existing source-identity factory supplies a deterministic
26-character ULID when no explicit ID is present, without modifying the source.
The MCP tool resolves this exact same default. It uses the first 128 bits of
SHA-256 over the domain `graphit.knowledge.default-slug.v1` plus a NUL byte followed
by the existing canonical relative `source_path`, encoded by the mature ULID
library. The same validated origin retains its fallback on reindexing and content
revisions; a rename changes it unless an explicit ID is retained. Unicode and
component boundaries remain significant. Its time component is hash-derived,
not the page's creation time.

The origin key and its reversible path encoding remain unchanged. Project,
version and source authorization continue to scope publication and readers;
the derived ID is data, not a global registry entry or an access grant.

Normal reconciliation can carry certified semantic fields to a changed slug
through the same unique active source origin when the effective model-input
fields remain equivalent. The old slug follows ordinary retirement, without an
alias. Input changes invalidate reuse, and the selected model and projection
recipe retain their existing compatibility checks; changing only an ID does not
require inference for an otherwise compatible stored vector.

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
