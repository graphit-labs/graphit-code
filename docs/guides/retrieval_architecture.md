# Choose a retrieval operation

Use this guide to choose deterministic evidence before asking an agent to synthesize an answer.
The selected PROJECT/version or exact installed context is part of every read. It selects data,
not authority. [Indexing](indexing.md) explains progressive availability and vector reuse.

## Match the question to the operation

| Need | Surface | What the result establishes |
|---|---|---|
| Locate code candidates | AST `search` with `fts`, `semantic`, `hybrid` or explicit vector | Ranked entities with source/fingerprint/identity, not verified call relationships |
| Read code or any known file | AST `source` | Original selected Source; authorized disk fallback when indexed Source is absent |
| Prove a relationship | AST schema then readonly Cypher query | Actual typed graph relationship in the captured publication |
| Find documentation | Knowledge `search` | Ranked pages and exact slugs/source selection |
| Read documentation | Knowledge `source` / `graphit_wiki_source` | Virtual Markdown and explicit selected slices, preserving original content |
| Inspect document synchronization | Knowledge `log` / `graphit_wiki_log` | Committed document history, not Task history |
| Retrieve durable guidance | Memory search | Current/historical matches according to explicit filters and returned match location |
| Find work | Task search or typed query | Current searchable Task public content; exact reads supply lifecycle evidence |
| Answer from pages | Knowledge Ask/Conversation | Agent synthesis grounded in explicitly selected or retrieved authorized pages |
| Investigate across installed artifacts | Live | An owned, temporary, multi-step agent run rather than a deterministic search |

## Lexical, semantic and hybrid

`fts` ranks lexical matches and needs no model. Semantic search embeds the query using the same
model/revision/dimensions/recipe as eligible document vectors. A different vector space is an
error/reindexing decision, not permission to reuse incompatible values. `hybrid` fuses lexical
and vector candidates with deterministic ranking; rows without embeddings still have lexical
coverage. Valid partial vectors are queryable while background indexing advances.

AST CLI search defaults to `semantic`; Knowledge CLI search defaults to `fts`. Memory and Task
searches default to FTS. Do not assume a single default across tools. CLI flags and MCP input
schemas define their exact enums, metrics and result bounds.

```sh
graphit ast search 'request cancellation' --mode fts --rerank=false
graphit ast search 'request cancellation' --mode hybrid
graphit knowledge search 'configure a parser' --mode fts --rerank=false
graphit task search --text parser --mode fts --page-size 10 --top-k 10
```

No corpus, empty corpus, incomplete semantic coverage and failed inference are distinct. A
requested service failure is not hidden behind an apparently successful empty or lexical result.
A model-free question should explicitly choose FTS, Cypher or source rather than pay for semantic
query inference unnecessarily.

## Optional rerank

`search.rerank=false` by default. Installing a model or provider does not enable it. An explicit
request boolean overrides effective configuration. Disabled rerank performs no rerank discovery,
preparation, download, loading or inference. Enabled AST/Knowledge rerank scores query/document
pairs after candidate retrieval, preserving candidate identities and the correspondence between
scores and results; it does not replace embedding or fabricate missing Source text.

Local uses BAAI/bge-reranker-v2-m3 GGUF F16; Broker and Direct use their configured contracts.
Broker provider selection cannot bypass Broker with local/direct inference. Errors remain visible.
See [AI settings and model installation](ai_models.md#rerank-modes-and-activation).

## Search, then verify selected Source

Use returned paths/UIDs and source fingerprints rather than guessing a source filename from a
symbol. Root AST Source is original text, with `head`, `tail`, line ranges, entity and pattern
selection. If indexed Source is unavailable, the authorized version checkout can supply the
requested file from disk, including unsupported extensions. It requires no Git command. Imported
fallback requires an explicit authorized project selection; a Hub artifact does not grant arbitrary
filesystem reads. Coordinates and Unicode validation remain tied to the selected source.

For graph questions, request the schema first, anchor an exact entity and project the reached
endpoint with `DISTINCT`. Unsupported query shapes fail explicitly; do not interpret a rejected
query as no callers. [AST contract](../specs/ast_module.md) describes supported traversal semantics.

## Project versions and installed artifacts

PROJECT reads select the retained project ID/checkout/version. Physical stores use one dataset
per corpus and native branches, with immutable Git-associated snapshots for PROJECT Memory/Task.
USER Memory follows its own verified identity and installation, never a selected project's branch.

Installed Native AST uses exact `context: id@version`; installed Knowledge uses the typed
`source:{kind:imported_knowledge,artifact:{id,version,...}}` selection. No automatic shallow Git clone,
ancestor selection or fallback to an unrelated local corpus is part of retrieval. Native Hub reads
capture authorized local/S3 data directly. [Storage](../architecture/storage_layout.md) is the
canonical layout reference.

## Pagination, consistency and errors

Continue opaque cursors with the exact same selection, filters, order and page parameters. A
cursor pins a publication; it is not an access token. Current authorization is checked again.
One page never proves EOF or complete export. Exact Task reads load current state first; history
and comments require explicit pinned pages. Session and Task history cursors are different.

Preserve cancellation/deadlines and distinguish protocol validation, domain failure (`isError` in
MCP), valid absent results and partial confirmed write effects. Structured MCP output defaults to
TOON; request `ai_optimized:false` for JSON consumers. CLI/HTTP stay JSON, and original Source text
is preserved. See [output contract](../specs/output_module.md).

## Synthesis and Live

In the browser, a new question starts with FTS evidence retrieval, so
you can write the question without first checking every page. Search still finds and previews
pages; evidence selection determines which authorized pages the answer may consume. Choose manual
evidence explicitly when you want to select a fixed set. Manual `selected_slugs: []` continues to
mean no documents. This browser choice does not change API, CLI or MCP defaults or enable
embeddings, semantic retrieval or reranking.

Knowledge Ask requires a literal model and an admitted completion CLI binding, plus explicit page
slugs or ranked retrieval. The CLI/protocol owns its model account; a configured embedding service
is not a completion service. Conversation adds owned native session continuation. Live prepares an
authorized temporary workspace and constrained tool environment; it does not grant Runtime power
or persistent ownership of unrelated stores. Review the returned sources and errors before using
a synthesized answer as evidence. [Knowledge AI](../architecture/rust-knowledge-ai-live.md) and
[conversation](../specs/rust_knowledge_conversation_app.md) specify those lifecycles.
