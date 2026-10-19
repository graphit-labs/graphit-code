---
title: "AST Module Specification"
description: "Technical specification of the AST database, detailing LadybugDB, Cypher translation rules, parser adapters, and indexing pipelines."
content-type: reference
audience: developers
keywords:
  - AST
  - LadybugDB
  - Cypher
  - parser
  - Tree-sitter
  - ANTLR
  - SCIP
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/wiki_module.md"
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrects
  - target: {type: task, id: tsk-de12}
    relation: implements
  - target: {type: task, id: tsk-8286}
    relation: implements
  - target: {type: task, id: tsk-bcfd}
    relation: implements
  - target: {type: task, id: tsk-1b06}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: uses_api
  - target: {type: task, id: tsk-1fd1}
    relation: implements
  - target: {type: task, id: tsk-350c}
    relation: relates_to
  - target: {type: task, id: 01M5C6SF19TSS3NE06D71HSM7N}
    relation: implements
  - target: {type: task, id: 01M5C9XXR0ZE2Q2GX2MMHA6ZBD}
    relation: implements
---

# AST Module Specification

AST publishes structural code evidence, selected Source and ranked entities for one authorized
PROJECT/version or installed Native context. SCIP is the standard interchange format. Parsing,
publication, semantic attachment and reads are separate lifecycles. The [indexing guide](../guides/indexing.md)
explains availability; [parser extensibility](../guides/ast_extensibility.md) defines all maintained
profile fields and the custom image contract.

## Standard SCIP indexing and preserved native data

A configured `ast.scip.parsers` extension/image route wins over automatic family selection.
Maintained specialized Docker indexers produce compiler-aware SCIP. The generic fallback extracts
Tree-sitter/ANTLR facts and also produces SCIP; it is not an opt-in second graph implementation.
A Docker document must match the captured authorized Source, paths, encoding and producer before
its facts are published. Compiler-only/custom extensions need no builtin syntax grammar, but
fallback is possible only where a linked grammar actually exists.

`ast.scip.version` defaults to compatibility alias `v1` for maintained Graphit GHCR images. It is
not a package/release version and is never appended to custom image references. Docker pull
defaults to `always`; the branded `AST_SCIP_PULL_POLICY` environment suffix permits explicit
`missing` or `never`. Never cannot download an absent image. Docker credentials/registry syntax
remain Docker's concern; no image credentials belong in project config.

C++ owns generated SQL/COBOL ANTLR parsers, the official runtime and tree adaptation; cxx carries
small extracted results/diagnostics to Rust. Tree-sitter uses native queries/captures/fields and
cursor traversal. The retired ContextPath/ProfileQuery grammars and `antlr-rust-codegen` are not
part of current profile interpretation. No custom shared-library loader or native-grammar opt-in
is preserved. Adding a linked builtin language is a contributor build change; installing a
Docker image route is a user configuration change.

## Supported languages

The bundled generic fallback profiles below come from `crates/graphit-ast/profiles/queries/`.
They describe extension routing and extraction, not compiler-equivalent semantic completeness.
A custom/specialized Docker route has its own SCIP producer. Explicit SQL dialect profiles
are exclusive selections; ordinary `.sql` uses the generic SQL profile unless configured otherwise.
PL/pgSQL is an embedded extraction profile, not a separately linked Tree-sitter grammar.
Embedded HTML/Vue/Svelte and PostgreSQL language handling is described in
[embedded parsing](embedded_language_parsing.md).

| Profile | Parser / grammar | Routed names or extensions |
|---|---|---|
| `bash` | Tree-sitter / `tree-sitter-bash` | `.sh`, `.bash`, `.zsh`, `.ksh` |
| `c` | Tree-sitter / `tree-sitter-c` | `.c`, `.h` |
| `clojure` | Tree-sitter / `tree-sitter-clojure` | `.clj`, `.cljs`, `.cljc`, `.edn` |
| `cobol85` | ANTLR C++ / `antlr-cobol85` | `.cob`, `.cbl`, `.cpy`, `.cobol` |
| `cpp` | Tree-sitter / `tree-sitter-cpp` | `.cpp`, `.hpp`, `.cc`, `.cxx`, `.hxx`, `.hh` |
| `csharp` | Tree-sitter / `tree-sitter-c_sharp` | `.cs` |
| `css` | Tree-sitter / `tree-sitter-css` | `.css`, `.pcss`, `.postcss` |
| `dart` | Tree-sitter / `tree-sitter-dart` | `.dart` |
| `db2` | ANTLR C++ / `antlr-db2` | `.sql`, `.db2` |
| `dockerfile` | Tree-sitter / `tree-sitter-dockerfile` | `Dockerfile`, `.dockerfile`, `.containerfile` |
| `elixir` | Tree-sitter / `tree-sitter-elixir` | `.ex`, `.exs` |
| `go` | Tree-sitter / `tree-sitter-go` | `.go` |
| `graphql` | Tree-sitter / `tree-sitter-graphql` | `.graphql`, `.gql` |
| `groovy` | Tree-sitter / `tree-sitter-groovy` | `.groovy`, `.gvy`, `.gy`, `.gsh`, `.gradle` |
| `haskell` | Tree-sitter / `tree-sitter-haskell` | `.hs`, `.lhs` |
| `hcl` | Tree-sitter / `tree-sitter-hcl` | `.tf`, `.hcl`, `.tfvars` |
| `html` | Tree-sitter / `tree-sitter-html` | `.html`, `.htm` |
| `java` | Tree-sitter / `tree-sitter-java` | `.java` |
| `javascript` | Tree-sitter / `tree-sitter-javascript` | `.js`, `.jsx`, `.mjs`, `.cjs` |
| `json` | Tree-sitter / `tree-sitter-json` | `.json`, `.jsonc`, `.json5` |
| `julia` | Tree-sitter / `tree-sitter-julia` | `.jl` |
| `kotlin` | Tree-sitter / `tree-sitter-kotlin` | `.kt`, `.kts` |
| `lua` | Tree-sitter / `tree-sitter-lua` | `.lua` |
| `objc` | Tree-sitter / `tree-sitter-objc` | `.m`, `.mm` |
| `php` | Tree-sitter / `tree-sitter-php` | `.php` |
| `plpgsql` | embedded / `embedded profile` | `.plpgsql` |
| `plsql` | ANTLR C++ / `antlr-plsql` | `.sql`, `.pks`, `.pkb`, `.pls`, `.plb`, `.prc`, `.fnc`, `.trg`, `.typ`, `.bdy`, `.spc`, `.vw` |
| `postgresql` | ANTLR C++ / `antlr-postgresql` | `.sql`, `.pgsql`, `.plpgsql`, `.pg` |
| `protobuf` | Tree-sitter / `tree-sitter-proto` | `.proto` |
| `python` | Tree-sitter / `tree-sitter-python` | `.py` |
| `r` | Tree-sitter / `tree-sitter-r` | `.r`, `.R`, `.Rmd` |
| `ruby` | Tree-sitter / `tree-sitter-ruby` | `.rb` |
| `rust` | Tree-sitter / `tree-sitter-rust` | `.rs` |
| `scala` | Tree-sitter / `tree-sitter-scala` | `.scala`, `.sc` |
| `sql` | Tree-sitter / `tree-sitter-sql` | `.sql` |
| `svelte` | Tree-sitter / `tree-sitter-svelte` | `.svelte` |
| `swift` | Tree-sitter / `tree-sitter-swift` | `.swift` |
| `toml` | Tree-sitter / `tree-sitter-toml` | `.toml` |
| `tsql` | ANTLR C++ / `antlr-tsql` | `.sql`, `.tsql` |
| `tsx` | Tree-sitter / `tree-sitter-tsx` | `.tsx` |
| `typescript` | Tree-sitter / `tree-sitter-typescript` | `.ts`, `.mts`, `.cts` |
| `vue` | Tree-sitter / `tree-sitter-vue` | `.vue` |
| `xml` | Tree-sitter / `tree-sitter-xml` | `.xml`, `.xsl`, `.xslt`, `.xsd`, `.svg`, `.wsdl`, `.plist`, `.xhtml` |
| `yaml` | Tree-sitter / `tree-sitter-yaml` | `.yaml`, `.yml` |
| `zig` | Tree-sitter / `tree-sitter-zig` | `.zig` |

Use [grammar selection and enable/disable controls](../guides/ast_extensibility.md#grammar-selectors-and-enabledisable-controls)
for dialect overrides, whitelist and blacklist behavior. Unsupported or recovered syntax produces
the profile's actual diagnostics; a registered extension does not promise every variant.

## Source, graph and searchable corpus

| Data | Granularity and authority | Contract |
|---|---|---|
| Raw Source | Complete file, `ast/source.lance` | One source/fingerprint/version, not a copy per entity |
| Entities/text/vectors | `ast/corpus.lance` with native version branches | Lexical rows and nullable compatible semantic association together |
| Structure | Icebug manifest and immutable Parquet node/fact/CSR shards | FILE publications preserve unrelated shards and stable sparse ranges |
| Query engine | Shared in-memory Ladybug owner | Reads local or authorized remote ranges on-the-fly; no productive `.lbug` file |

The graph directory is `<global>/projects/<project-id>/ast/live/<version-key>/graph/`; it is not a
checkout-path identity or a second native-database store. [Storage layout](../architecture/storage_layout.md)
defines lifecycle and native maintenance. Native corpus branches share fragments until writes
diverge. Query snapshots and graph publication guards protect current readers during retirement.

## Indexing pipeline: full and incremental

A full pass captures source selection/settings, discovers eligible files and compiles the actual
producer documents. It publishes completed FILE state progressively. A single file cannot expose
half-replaced graph facts, but neither repository EOF nor all embeddings gates its Cypher/FTS
availability. Partial, absent, empty, failed and complete states remain distinguishable.

Incremental work compares source/compilation identities and producer association, processes changed
files and real affected dependencies, and retains unrelated source/graph/lexical/vector state.
Renames/deletes retire old facts. A changed compiler image/profile cannot leave incompatible old
compiler output pretending to be current. Specialized compilers can operate over a language project;
per-file generic parsing does not imply every compiler can run with a one-file work unit.

Embeddings run in background batches outside the writer. Attach only if uid/data key/source or
public-input fingerprint and selected model/revision/dimensions/recipe still match. Changed/deleted
content makes stale vectors ineligible immediately. Unchanged compatible vectors survive edits,
branching and native maintenance. Semantic queries use available compatible partial coverage;
FTS/Source/Cypher need no embedding model. [Semantic publication](rust_ast_source_index.md) and
[parser composition](rust_ast_parsers.md) give producer details.

## Configuration and customization

| Control | Default / meaning |
|---|---|
| `modules.ast` | true; orchestration switch, not authorization |
| `modules.sync` | true; automatic source synchronization where consumed |
| `modules.embedding` | true; false preserves structural/textual reads without embedding work |
| `ast.index_docs` | false; opt in to structured files under the configured docs directory |
| `ast.index_source` | true; controls indexed Source, not arbitrary filesystem authority |
| `ast.scip.parsers` | empty CSV string; exact extension-to-image associations |
| `ast.scip.version` | `v1` compatibility alias for maintained images |
| `ast.queries_dir` | `.<brand>/ast/queries`, relative to the project |
| `ast.grammar`, `ast.grammars_whitelist`, `ast.grammars_blacklist` | Empty CSV by default; explicit grammar/extension/selection controls |
| `ast.query.buffer_pool_bytes` | 0/native Ladybug default (approximately 80% system memory), per shared native pool maximum |
| `ast.query.threads` | 2 per native query handle; positive |
| `search.rerank` | false; request-level boolean can override |

The native pool maximum is not preallocated page data or a total daemon budget; pool metadata
still costs memory. Explicit positive bytes must be at least 4096 and sufficient for actual query
workspace. Do not instantiate an independent large database pool per retained project merely to
read on-the-fly shards. [Graph query architecture](rust_captured_cypher.md) describes capture/owners.

Profiles use configured language/grammar, extraction rules, entity labels/names/contexts, relation
resolution, embedding labels and embedded-language handling. Unknown YAML fields are errors in that
profile, not silently ignored settings. The loader diagnoses/skips invalid profile files; it does
not bless their configuration or claim the entire source pass failed. Layers are captured by each
load, not a documented two-second filename/mtime cache. [Complete profile reference](../guides/ast_extensibility.md)
describes replacement/merge behavior and native Tree-sitter/ANTLR selectors.

## Source selection and disk fallback

Root `graphit_ast_source` returns original text. CLI `ast source PATH` supports entity selection,
line ranges (`--start`/`--end`), `--head`, `--tail`, literal/regex pattern, surrounding lines and
line numbers. Selectors compose entity → range → head → tail → pattern. There is no `top` field;
search result count and source slicing are different options.

Prefer exact returned path/entity/span/fingerprint. If indexed Source is absent, read that file
from the selected authorized version checkout, even for an unsupported extension. No Git lookup
is necessary. Confinement, current PROJECT permission, relative path checks, NUL/Unicode validation,
original source and coordinate integrity still apply. An imported context requires explicit
project selection for disk fallback; its artifact name is not a host filesystem grant.
See [Source handler](rust_ast_source_handler.md) and [entity extents](rust_ast_entity_extent.md).

## Retrieval

FTS supplies lexical candidates; semantic supplies compatible vector candidates; hybrid fuses both.
CLI AST search defaults to semantic; exact schema/query/Source operations do not infer a query.
An unavailable requested service is explicit rather than an empty successful result or silent
FTS fallback. Candidate identity, Source association, ordering, pagination and cursor/publication
binding survive optional rerank.

Rerank is opt-in and uses real query/document scoring. Disabled rerank does no discovery/preparation/
loading/inference. Enabled results retain retrieval score separately from rerank score. Missing
exact Source must follow the producer's current source association contract, not guessed paths or
concatenated unrelated entities. [Rerank consumer](rust_search_rerank_consumer.md) governs errors.

### The rules and what each refusal says

Read the actual schema before writing Cypher. Use its labels/properties and exact relationship
names. Anchor a precise entity and project the reached endpoint with `DISTINCT` for traversals.
Do not guess a relationship type or treat FTS hits as proof of callers. Query parameters are typed
data and do not permit mutation. Unsupported syntax/planner shapes, missing/incompatible graph,
input/output bounds and admission failures are explicit query failures, not empty graph evidence.

[Captured Cypher](rust_captured_cypher.md) is authoritative for supported query composition and
projection constraints. Sample/neighborhood are separate bounded readers; they do not grant
arbitrary Cypher or a complete graph export. Preserve reached endpoint direction, requested depth
and source identity. Empty results establish only absence within the selected captured graph.

## Local and Hub on-the-fly readers

PROJECT local and installed remote graphs use the same in-memory Ladybug/Parquet path. The remote
variant adds exact artifact/version admission and authorized S3 range transport; it does not
hydrate a checkout or persist a local `.lbug`. Installed contexts are listed by `ast imports` /
`graphit_ast_imports`, then selected as exact `id@version`. Publication/export captures real
Source/graph/corpus snapshots under current authority; an installed artifact cannot authorize a
read of arbitrary local files. [Native packages](../development/native-packages.md) define format.

## Browser and other consumers

CLI, MCP, native HTTP and the AST Explorer consume these same producers. Selection carries the
retained PROJECT/version or exact installed context. The Browser header shows authorized project
names while IDs/versions remain selection keys. Search/Source/query/schema/graph views keep errors
visible, cancel obsolete requests and preserve identity across navigation. JSON API contracts are
independent from MCP's TOON presentation. [Browser contract](ui_dashboard.md) defines UI behavior.

## Implementation and validation

`graphit-app/src/foreground_ast_sources/` owns productive composition and specialized compiler
execution; `graphit-ast` owns profiles/SCIP/syntax extraction; `graphit-data` owns Lance and native
Icebug queries/publication. Source and semantic consumers must retain authorization, Original
cancellation/deadline and joined native/process ownership. Integration tests use real controlled
producers/consumers without vendor account requirements. A library test or prepared image tag
does not qualify every platform, Docker architecture or published release.
