---
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrects
  - target: {type: task, id: tsk-d284}
    relation: implements
  - target: {type: task, id: tsk-de12}
    relation: implements
  - target: {type: task, id: tsk-1528}
    relation: implements
  - target: {type: task, id: tsk-861f}
    relation: implements
  - target: {type: task, id: 01M5C6SF19TSS3NE06D71HSM7N}
    relation: implements
  - target: {type: task, id: 01M5C9XXR0ZE2Q2GX2MMHA6ZBD}
    relation: implements
---

# AST grammars and parser extensibility

Graphit's AST module is language-driven. A language profile is a YAML file that
selects a parser, claims file extensions, declares extraction queries, and defines
language-specific behavior such as ownership, exports, complexity, target resolution,
semantic embedding, and embedded-language regions.

This guide covers the supported YAML fields, native selector operations, compiler
image extension, profile precedence, hot reload, distribution, validation and
current extension boundaries.

For the shipped languages, declared file extensions and parser-specific boundaries, use the [supported language matrix](../specs/ast_module.md#supported-languages). SQL dialect profiles require explicit selection; a registered extension alone does not promise complete support for every language variant.

## What can be extended without rebuilding Graphit

Project profiles and compiler configuration allow the following:

- add or replace Tree-sitter extraction queries;
- associate an extension with a custom Docker image that emits SCIP;
- customize extraction for the Tree-sitter and ANTLR grammars linked into Graphit;
- bind extensions to a specific grammar;
- enable only an allowlist of grammars or disable selected grammars;
- define new entity labels and relationship types;
- configure parent contexts, export rules, docstrings, and complexity;
- control which labels receive semantic vectors;
- parse JavaScript, TypeScript, CSS, SQL, or another linked language embedded
  inside a host file;
- merge a small project-specific delta onto a shipped language profile.

Graphit ships 45 indexed language profiles: 40 Tree-sitter profiles and 5 ANTLR
profiles. A compiled Markdown grammar is also available internally, but no shipped
YAML claims Markdown extensions because prose belongs to the Knowledge module by
default. A project may deliberately add its own Markdown profile.

Profiles configure the grammars linked into Graphit. Adding a new Tree-sitter
grammar to that syntax fallback requires a Graphit build; project-supplied native
shared libraries are not loaded. A custom compiler image can index an additional
language without rebuilding Graphit. ANTLR uses the same YAML extraction model,
but a new ANTLR grammar requires a Graphit contributor to add its generated C++
recognizers, typed native bridge and build registration. The five registered ANTLR grammar
names are `antlr-plsql`,
`antlr-postgresql`, `antlr-tsql`, `antlr-db2`, and `antlr-cobol85`.

## SCIP as the standard index format

SCIP is the common code-index format, not an opt-in third parser. The normal
Rust pipeline selects a complete compiler indexer for an eligible family and
uses syntax-derived SCIP when an admitted file has no usable compiler document.
`ast.scip.enabled` is removed, rather than retained as an always-true setting or
a hidden Docker selector. `ast.scip.version` selects the maintained-family image tag and defaults
to the compatibility alias `v1`. This alias describes the Graphit/indexer
contract, not the image release version: graphit-scip can publish release
`v0.2.1` as `0.2.1` and advance `v1` to that same image. Code consumes
`ghcr.io/graphit-labs/graphit-scip-<family>:v1` directly from GHCR; it does not
select `latest` or derive the compatibility alias from a release's major.
Keep this default for the current compatible images. Selecting a specific
release through `ast.scip.version` deliberately pins that tag instead.

This is the SCIP-default integration contract. The maintained Rust dispatcher/
entrypoint sources and local TypeScript/Rust images have been built. TypeScript
passed a local smoke and emoji UTF-16 position fixture. The real Rust Docker
Source journey passed in 34.98 seconds, covering compiler-to-Raw/Source/Graph,
cross-file CALLS, FILE target association, physical-change rejection and cleanup.
The integrated Source consumer also passed its Compiler/Absent/Failed scenarios
in 51.47 seconds, including official SCIP, Graph/Raw/NN association, exact Source
and CLI-to-MCP delivery. Final strict library Clippy passed in 36.58 seconds for
AI, Daemon, AST and App. On 2026-10-21, anonymous GHCR manifest requests confirmed
the compatibility alias `v1` for all ten maintained families. Registry
availability does not establish that every later local indexer change has been
published, nor does it qualify real indexing for the other eight families or
additional platforms. The default consumer pulls the alias on each compiler
invocation, so a compatible publication can advance its digest without changing
the Graphit configuration.

This contract distinguishes source and deployed artifacts. Earlier Rust-only opt-in
qualifications do not establish the new multi-family flow. Source materialization,
focused fixtures, real image execution, publication and platform qualification
remain distinct evidence. See the [native Graph contract](../specs/rust_ast_semantic_graph.md#scip-default-producers-and-the-common-join).

### Compiler semantics and syntax fallback

A complete compiler indexer can supply definitions and references informed by
build inputs, dependencies and a type system. Tree-sitter and ANTLR supply syntax
observations and locally justified relationships. Encoding both as SCIP does not
make those guarantees equal: syntax fallback must not invent types, import or
overload resolution, or cross-file bindings. Ambiguous, external and unresolved
targets retain their observed states.

Native extraction remains richer than a standard SCIP message. Labels,
configured relationships, ownership, exports, docstrings, calculated complexity,
embedding selection and embedded-language projections remain in the native graph
and Source contracts. The syntax encoder borrows those observations without
discarding fields that have no SCIP equivalent or hiding them in diagnostics or
documentation. A symbol string, display name or artifact path is not Source
authority.

The maintained complete Docker producers remain:

| Image family | Eligible extension routes |
|---|---|
| `go` | `.go` |
| `typescript` | `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs` |
| `python` | `.py`, `.pyi` |
| `java` | `.java`, `.kt`, `.kts` |
| `clang` | `.c`, `.h`, `.cpp`, `.hpp`, `.cc`, `.cxx`, `.hxx`, `.hh`, `.m`, `.mm` |
| `dotnet` | `.cs`, `.vb` |
| `rust` | `.rs` |
| `ruby` | `.rb`, `.rake`, `.gemspec` |
| `dart` | `.dart` |
| `php` | `.php` |

These are eligible routes, not a promise that every selected producer emits every
listed language. An effective language profile remains authoritative: `.m` with
a MATLAB profile does not route to Clang just because of its suffix.

This is the maintained family contract, not proof that every image, architecture
or Docker Desktop host has been requalified by the new Rust consumer. Keep the
pins and customizations, including the custom .NET producer; a smaller syntax
tagger does not replace them. Scala remains on its syntax profile: the pinned
Java/Kotlin indexer does not establish Scala compiler coverage.

The grammar matrix and compiler coverage are different. If an actual producer
emits a valid document for an admitted file without a native grammar, the common
assembler retains genuine Raw Source and compiler definitions under the honest
`scip-compiler` grammar identifier. This accommodates compiler-only source such
as VB from the maintained .NET producer. That producer supports C#/VB; it does
not advertise F# routes. Compiler-only assembly does not invent Tree-sitter
support or manufacture NN vectors. Native-only
admission requires `NoSelector` and empty grammar filters; it does not reinstate
stock routes replaced by custom profiles, exclusive routes or explicit overrides.

An admitted file without a usable compiler document follows its configured
Tree-sitter or ANTLR profile and emits an official SCIP `Document`. This applies
to every active grammar, not a reduced language allowlist. Existing Oracle
PL/SQL and DB2 models remain; generic PostgreSQL, TSQL and COBOL models complete
the other registered ANTLR paths. COBOL retains Raw Compound observations; its
Original `programUnit` body may have `SequentialOriginalCoverage`, allowing only
the maintained preprocessor's seven actual spaces of generated trivia at the
start of a line, without claiming an Exact projection. `ast.grammar` still selects
the syntax grammar.
Exclusive SQL dialects require explicit selection; a compiler failure does not
activate them implicitly. Parsing respects the actual Original/cancellation and
operation policy. The native parser uses its actual operation policy.

### Docker contract and failure boundaries

The producer binds the captured project read-only at `/source`. A fresh Linux
`tmpfs` at `/overlay` supplies empty upper/work directories for the writable
merged `/workspace`; the original project is neither copied nor made writable.
The maintained entrypoint uses `GRAPHIT_SCIP_OVERLAY=1`, requires the declared
`SYS_ADMIN`/`apparmor=unconfined` mount permissions and drops privileges after
mounting. Output is `/output/index.scip`; family caches are writable installation
state under `<global>/cache/scip/<family>`. Compiler targets are keyed by stable
PROJECT ID and selected version; worktrees of the same project/version reuse the
build tool’s content-validated cache rather than identifying it by checkout path.
Old-image compatibility mounts are not a second supported contract.

Unix passes the actual `GRAPHIT_SCIP_UID` and `GRAPHIT_SCIP_GID`. Windows uses
`GRAPHIT_SCIP_OWNER=output-mount` so the Linux entrypoint observes the output
mount's owner instead of inventing POSIX IDs. Positive integer
`GRAPHIT_SCIP_TIMEOUT_MS` comes from the finite compiler operation budget.
Family, PROJECT ID and version isolate compiler targets; each invocation owns a
separate temporary output directory.

The default pull policy is `always`: each compiler invocation asks GHCR for the
selected tag, downloading missing layers or updates to its compatibility alias;
unchanged layers are reused by Docker. No prior local image build is required.
A pull failure does not silently select an older cached image. Explicit
`GRAPHIT_AST_SCIP_PULL_POLICY=missing` permits a cached local image and does not
check that tag for updates. Explicit `GRAPHIT_AST_SCIP_PULL_POLICY=never` uses
only images already present locally and never attempts a registry pull. A missing
local image still follows the diagnosed native syntax fallback after cleanup;
authorization, cancellation and expiration of the caller's Original remain
terminal. The default continues to be `always`. Build metadata and dependencies remain family-specific:
C/C++ needs usable compilation inputs; PHP needs its pinned Composer/autoloader
contract. Their absence is not evidence of compiler semantics.

PHP requires `composer.json`, `composer.lock`, installed dependencies and an
up-to-date Composer autoloader. The pinned scip-php implementation uses the
project's autoloader and Composer 2 `installed.php` metadata; its dependencies
include `composer-runtime-api` `^2.2` and `phpstan/phpdoc-parser` `^2.3`.
Graphit preserves that implementation instead of substituting a static
loader or installing/updating project dependencies. See its
[pinned requirements](https://github.com/davidrjenni/scip-php/blob/71a5b117ec4c5dd2af302e363410e604e5df309e/composer.json)
and [Composer integration](https://github.com/davidrjenni/scip-php/blob/71a5b117ec4c5dd2af302e363410e604e5df309e/src/Composer/Composer.php).
An incompatible project autoloader is a real compiler failure, not permission to
claim missing semantic information was resolved.

The admitted Raw source set, with its grammar filters and ignore rules, bounds
which compiler documents enter the index. An indexer may read dependencies in
the read-only workspace; its output cannot enlarge source authority. Build-input
observations are separate from admitted sources. An image tag or SCIP metadata
alone does not establish source freshness.

Missing Docker, compiler unavailability after confirmed cleanup, or a missing
eligible document permits syntax SCIP for the affected file. The SDK visitor
rejects malformed compiler output. The pipeline can explicitly select syntax
SCIP after rolling back that compiler import and rechecking the parent Source
Original, authority, cancellation and deadline; no partial compiler index is
published.

The compiler keeps its own finite derived operation budget; the background
embedding RPC budget does not change it. A typed compiler Process timeout may
become compiler unavailability (`Ok(None)`) only after Process join and container
cleanup have completed, no remaining owner is pending, and the parent Original
and current authorization still allow fallback. Its safe diagnostic contains
only the family, failure reason, status and cleanup outcome. The existing native
syntax path then runs; the `Result<Option<_>>` API is unchanged. Expiration or
cancellation of the parent Original, integrity or authorization failures and
incomplete cleanup remain terminal errors.

Cleanup retains and joins the real child/container/output owners. A subsequent
normal admitted operation can try the compiler again; this introduces no retry
loop or silent shard repair. The timeout classification awaits its affected
SCIP tests; earlier component qualifications keep their recorded scope.

Maintained image recipes and release matrices declare their Linux architectures;
Clang/Ruby have Linux amd64 constraints. Configured CI, possible Docker Desktop
emulation and a tag name are not evidence of native Windows/macOS/arm64 execution
or published image contents. Existing Linux proofs cover only their recorded,
unchanged producer scope.

### Custom compiler images by extension

`ast.scip.parsers` is a string containing comma-separated `extension=image`
associations. Its default is the empty string. An explicit association wins over the maintained
compiler family for that extension. It also admits compiler-only extensions that
have no syntax profile, without enabling an unrelated grammar.

```sh
graphit config set ast.scip.parsers '.zig=registry.example/team/scip-zig:1,exm=localhost:5000/scip-example@sha256:<digest>'
graphit config get ast.scip.parsers
```

Replace `<digest>` with the real image digest before using the second association.
An extension accepts an optional leading dot and uses ASCII-case-insensitive
matching: `zig`, `.zig` and `.ZIG` identify the same route. Defining two of those
in one list is an error. Each item requires `=` with a nonempty extension and
image; missing assignments, trailing commas and empty segments are errors.
Whitespace around items and either side of `=` is trimmed. Extensions describe
one suffix, not globs, filenames or compound suffixes. Images must be single
references without internal whitespace,
control characters, a URL scheme or leading CLI option. Docker validates the
actual registry/name/tag/digest syntax. Private registries use Docker's existing
login/credential helper; no credentials belong in this configuration.

The exact image reference is passed to Docker unchanged. `ast.scip.version`
applies only to maintained Graphit families, and is never appended to a custom
image. Normal Docker pull policy applies to both kinds of image. A changed
compatibility tag can therefore advance on the next normal compiler invocation.
Use a digest when an immutable image is required.

Configuration uses the existing whole-value precedence:
Command > Environment > Project > Global > Distribution. Lists are not merged.
A higher-layer empty string explicitly clears lower-layer associations; an invalid
winning value fails instead of selecting a lower value. Configuration files keep
the normal JSON envelope with a string value, for example
`{"settings":{"ast.scip.parsers":".zig=registry.example/team/scip-zig:1"}}`.
The canonical environment setting contains the same plain list:

```sh
export GRAPHIT_AST__SCIP__PARSERS='.zig=registry.example/team/scip-zig:1'
# Or select the list for this daemon invocation:
graphit -c 'ast.scip.parsers=.zig=registry.example/team/scip-zig:1' daemon foreground
# Clear the Project list, including inherited associations:
graphit config set ast.scip.parsers ''
```

#### Build the image

Supply a real compiler/indexer that writes the official SCIP protobuf `Index` to
`/output/index.scip`. The container receives no language-specific command-line
arguments from Graphit. Its entrypoint must create the writable overlay and then
run the indexer from `/workspace`; it must not expect Graphit to pre-create a
writable source checkout or emit JSON in place of the protobuf.

The maintained Rust `scip-entrypoint` in the graphit-scip project already handles
overlay mounting and UID/GID selection. It executes
`/usr/local/bin/graphit-scip-run` after dropping privileges. A custom image may
reuse that entrypoint and place its own compiler executable at this path:

```dockerfile
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends mount util-linux \
    && rm -rf /var/lib/apt/lists/*
COPY --chmod=0755 scip-entrypoint /usr/local/bin/scip-entrypoint
COPY --chmod=0755 my-scip-indexer /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/scip-entrypoint"]
```

Build `scip-entrypoint` from the selected graphit-scip revision with
`cargo build --release --locked --bin scip-entrypoint`; use a binary and base image
with compatible architecture/libc, and include the real indexer's required
runtime and licenses. `my-scip-indexer` in this example is your actual compiler
producer, which must honor the following contract. This recipe does not provide
a generic language indexer or infer dependencies for it.

| Location or variable | Contract |
| --- | --- |
| `/source` | Read-only captured project mount. Never write here. |
| `/overlay` | Fresh tmpfs. Entrypoint creates empty `upper` and `work`, mounts their OverlayFS over `/source` at `/workspace`. |
| `/workspace` | Writable merged project used as compiler working directory. Changes are ephemeral. |
| `/cache` | Writable persistent image-specific cache; home/tmp/XDG state can live here. |
| `/cache/target` | Writable build cache isolated by stable PROJECT ID and version, reused across matching worktrees. |
| `/output` | Separate writable output directory owned by this invocation. Publish `/output/index.scip`. |
| `GRAPHIT_SCIP_OVERLAY=1` | Enable overlay setup; mounting needs the supplied `SYS_ADMIN` and `apparmor=unconfined` permissions. |
| `GRAPHIT_SCIP_UID`, `GRAPHIT_SCIP_GID` | Actual numeric Unix host owner; entrypoint drops to that identity after mounting. |
| `GRAPHIT_SCIP_OWNER=output-mount` | Windows selection instead of numeric variables; entrypoint observes the output mount's Linux owner. |
| `GRAPHIT_SCIP_TIMEOUT_MS` | Positive remaining compiler operation time. Honor cancellation/termination and finish within this budget. |

The image must put source-relative UTF-8 paths in `Document.relative_path`, using
SCIP's forward-slash convention, with no absolute path, parent traversal or NUL.
Coordinates and `position_encoding` must describe the exact original file text,
including Unicode, and ranges must remain within it. Use official SCIP symbols,
occurrences and relationships; do not infer cross-file bindings the compiler did
not establish. Graphit validates output and filters it against its admitted files;
an image's output does not grant filesystem or PROJECT access.

Write diagnostics to stdout/stderr, not into the protobuf. Exit nonzero on actual
compiler failure; exit zero only with a complete readable `index.scip`. Graphit
retains process/container cleanup and Original authorization/cancellation/deadline
checks. Compiler unavailability or missing documents permits configured native
syntax fallback only when those checks still allow it. A custom extension with
no linked syntax profile has no fabricated syntax fallback. Invalid output is
diagnosed and never partially imported. No custom image may turn an authorization
failure, cancellation, expired parent request or unfinished cleanup into success.
Caches are derived build state, not a second authoritative AST store. Custom
image caches are separate from maintained families and other image references;
output directories and overlays are removed after their invocation.

### YAML patterns, SCM and the Sourcegraph reference

Tree-sitter `queries[].pattern` is already native SCM Query syntax.
`PreparedTreeSitterProfile` compiles it against the retained grammar and binds
configured captures. YAML adds labels, query kind, parent/target rules, exports,
complexity policy, embeds and embedded-language mapping. Changing the wrapper to
`.scm` does not preserve those fields automatically or demonstrate a speed gain.
A compiling pattern still needs correct captures and semantic fixtures.

ANTLR executes its own rule-path selectors through the native C++ runtime. Keep
the complete [language-profile field contract](#top-level-fields) for both engines.
Advanced metadata stays with those language profiles and the native graph, while
compiler SCIP retains its actual symbols, documentation and relationships.

The reviewed Sourcegraph `docker-images/syntax-highlighter` snapshot (MIT) is a
conceptual reference, not an imported runtime. Its tags cover 15 languages and
locals cover Go/Java/MATLAB/Perl; the effective overlap with both analyses is
Go/Java. TSX has no tags/locals there. These queries do not establish compiler
parity or replace Graphit's richer grammar coverage. No Sourcegraph queries or
code are copied in this integration.

## The complete YAML shape

The following example illustrates common Tree-sitter fields. The field tables below also
cover ANTLR-specific selectors and optional native extraction controls. This is a schema
illustration, not a linked example grammar to copy unchanged; remove inapplicable fields.

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

context_name_queries:
  class_declaration: '(class_declaration name: (_) @name) @context'

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

Unknown YAML fields are rejected when decoding that profile. The loader diagnoses and skips
an invalid profile file; it does not silently accept a misspelled optional field. Validate
the intended effective profile and extraction before relying on fallback behavior.

## Top-level fields

| Field | Type | Required | Meaning and default |
|---|---|---:|---|
| `language` | string | yes | Stable language selector. Files at different precedence levels are paired by this value, case-insensitively. The filename is not the identity. |
| `extensions` | string list | for discovery | Extensions or filename selectors claimed by the profile, normally including the leading dot. Matching is case-insensitive. An omitted list does not discover new files; it only stops extension-filtering when the language is already selected. |
| `parser` | string | no | `tree-sitter` when omitted; `antlr4` selects the registered ANTLR engine. These choose syntax extraction, independently of the standard SCIP output. `parser: scip` is not a Native language-profile value. |
| `grammar` | string | no | Runtime grammar name. Defaults to `tree-sitter-<language>` or `antlr-<language>`. Explicit names are recommended and must match the linked grammar/registered driver. |
| `start_rule` | string | ANTLR metadata | Documents the ANTLR entry rule and participates in profile merging. The current registered `GrammarDriver` chooses its entry rule in code; this field does not dynamically invoke an arbitrary generated parser method. |
| `merge` | boolean | no | `false`/omitted replaces the same language at the lower level. `true` applies this file as a partial overlay. |
| `exclusive` | boolean | no | When `true`, keeps the grammar addressable by name but does not register its `extensions`. Use `ast.grammar` to select it explicitly. |
| `queries` | object list | conditionally | Extraction rules. A file with no valid queries is retained only when it contains recognized language configuration. |
| `exports` | object | no | Export/visibility strategy. |
| `self_keywords` | string list | no | Source prefixes such as `this.` or `self.` used to resolve receiver types on calls. |
| `context_types` | string map | no | Parser node/rule kind to graph label. These nodes can own nested entities. |
| `context_requires_entity` | string list | no | Rust generic consumer: listed native kinds require an actual Entity query candidate and must exist in `context_types`; default empty. Other kinds can retain native namespaces without Entity queries. |
| `context_name_queries` | string map | no | Native Tree-sitter queries binding `@context` and `@name`. |
| `antlr_contexts` | object array | no | ANTLR XPath owner definitions with structured name captures. |
| `anon_func_types` | string list | no | Tree-sitter node kinds recognized as anonymous functions assigned to names. |
| `declaration_types` | string list | no | Declaration node kinds eligible for preceding-comment docstrings. |
| `comment_types` | string list | no | Parser node kinds treated as comments. |
| `embed_labels` | string list | no | Labels that receive semantic vectors, in collision priority order. All entities still enter FTS. Empty means no semantic vectors for that language. |
| `target_rules` | map | no | Default target labels and unresolved-target policy per relationship type. |
| `complexity` | object | no | Decision shapes for models that calculate complexity. A calculator may define base `1`; capturing a decision alone, including generic ANTLR observations, does not create a calculated cyclomatic/cognitive metric. |
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
| `pattern` | string | yes | Tree-sitter query or official ANTLR XPath, according to `parser`. Invalid patterns fail preparation with the native SDK cause. |
| `name_capture` | string | no | Tree-sitter name capture, without `@`; defaults to `name`. ANTLR uses `captures.name`. |
| `type` | string | no | `entity` when omitted; `relation` routes the capture as an edge/call input. |
| `relation_type` | string | for relations | Relationship name. Special routing exists for calls, construction, decorators, exports, and DML; other names become generic references. |
| `value_capture` | string | no | Tree-sitter data-value capture. ANTLR uses `captures.value`. Requires `value_label`; ignored for relations. The value becomes a searchable node and the parent's `value` property. |
| `captures` | object | no | ANTLR slots `name`, `value`, `span`, `parent`, `qualifier` containing ordered `xpath`, `nearest_descendant` or `nearest_ancestor` operations. Empty list selects the matched node. |
| `select` | string | no | ANTLR XPath continuation after filtering the anchor. |
| `keyword`, `child_rule`, `negate` | string/string/boolean | no | ANTLR direct terminal or rule-child condition and optional inversion. |
| `value_label` | string | with value | Graph label for the captured value node. The Rust typed decoder rejects an incomplete value capture/label pair. |
| `name_reject` | regular expression | no | Rust generic consumer tests the raw name; an invalid regex is a typed preparation error. Anchor keyword filters to avoid rejecting substrings. |
| `span_capture` | string | no | Rust generic consumer requires an explicit native capture containing the name to prove full extent. Omission/absent/ambiguous capture remains unavailable; no parent-end default. |
| `context_capture` | string | no | Rust generic consumer: SDK scopes explicitly owned by an Entity. A native span or declaration must contain name and scope; missing/invalid/ambiguous evidence remains diagnostic. Default empty. |
| `declaration_capture` | string | no | Rust generic consumer selects a native declaration allowed by `declaration_types` for header exports and complexity, independently of span. Omission remains NoDeclaration. |
| `name_is_data` | boolean | no | Selects a Data search view: remove one matching outer quote pair and reject blank, multiline or more than 256 Unicode scalars. Raw name/ranges stay intact; escapes and Unicode are not decoded or canonicalized. Value/parent capture also selects Data. |
| `parent_capture` | string | no | Tree-sitter capture containing the explicit parent's name. Requires `parent_label`. |
| `parent_label` | string | with parent | Label of the explicit parent node. The Rust typed decoder rejects an incomplete parent capture/label pair. |
| `target_labels` | string list | no | Labels to which this relationship may resolve. Values are unioned with every query and the language-level rule for the same `relation_type`. Empty defaults to labels declared by this grammar. |
| `qualifier_capture` | string | no | Tree-sitter capture qualifying a target as `QUALIFIER.NAME`. ANTLR uses `captures.qualifier`, typically `nearest_ancestor` followed by `xpath`. A missing qualifier drops the match. |
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
`((node) @body (#eq? @body "x"))` so the predicate constrains the capture.
See the official [Tree-sitter predicate syntax](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/3-predicates-and-directives.html).

`name_capture`, `value_capture`, `parent_capture`, `span_capture`, `declaration_capture`, `context_capture`, and the Tree-sitter
form of `qualifier_capture` are YAML names without the `@` prefix. Embedded capture
names accept either form and are normalized by the loader.

## ANTLR selectors

`pattern` uses the official ANTLR XPath syntax (`/rule`, `//rule`, token selectors).
Configure direct terminal conditions with `keyword`, direct rule conditions with
`child_rule`, and inversion with `negate`. An optional `select` XPath continues
from each filtered anchor. Capture slots are structured operations:

```yaml
pattern: '//general_element_part'
child_rule: function_argument
captures:
  name: [{xpath: '/id_expression'}]
  qualifier: [{nearest_ancestor: update_statement}, {xpath: '/general_table_ref'}]
```

Nearest descendants use breadth-first order; ancestors exclude the selected node.
A missing later operation does not retry a farther ancestor. See the complete
[ANTLR selection contract](../specs/rust_ast_antlr_profile_queries.md).

## Parent context and names

Tree-sitter `context_types` maps node kinds to owner labels. Its SDK `name` field
is the default. Use native queries with explicit context/name captures otherwise:

```yaml
context_types: {element: Element}
context_name_queries:
  element: '(element (STag (Name) @name)) @context'
```

Each pattern binds the configured kind as `@context` and exactly one `@name`.
Use ordered patterns for complete alternatives and native anchors for positional
children. Invalid syntax/bindings fail configuration; absent names remain missing
observations. Scope association uses the native cursor and preserves self-exclusion,
anonymous functions and recovered nodes.

ANTLR profiles instead declare native XPath contexts:

```yaml
antlr_contexts:
  - pattern: '//create_procedure_body'
    label: Procedure
    name: [{xpath: '/procedure_name'}]
```

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
- `context_types`, `context_name_queries`, `target_rules`, and `text_normalizers` merge
  by map key;
- upper `embedded` blocks are prepended to lower blocks;
- `complexity.node_types`, `operators`, and `head_calls` inherit independently;
- `exclusive: true` is inherited, while `false` cannot clear an inherited `true` in
  the current boolean merge representation;
- `merge` itself is not inherited.

An empty list is indistinguishable from omission for the list-replacement rules. To
clear inherited list behavior, replace the whole profile rather than merge it.

The loader captures configured profile layers for each load. There is no two-second
filename/size/mtime cache contract. Invalid files or queries are diagnosed and skipped by
the loader; the remaining valid profiles can continue. Effective compiler/profile identities
participate in incremental reuse, so a changed producer cannot keep incompatible facts current.

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

## Extending syntax profiles

Tree-sitter and ANTLR fallback use grammars linked into the Graphit package.
Customize those profiles in `<project>/.graphit/ast/queries/<language>.yaml` or the
configured `ast.queries_dir`; Global profiles use `<GLOBAL_DIR>/ast/queries`.
Profile YAML and native queries retain their existing hot reload and validation.
An image association does not install a syntax grammar. To add compiler indexing
for an extension without a rebuild, use [custom compiler images](#custom-compiler-images-by-extension).

For the generic syntax route, declare a full native `span_capture` and, when
header policies or complexity require it, a separate `declaration_capture`.
Configured roles missing from the query are preparation errors; optional captures
missing in one match retain diagnostics. For an Entity-owned scope,
`context_capture` identifies its context; a native span or declaration must
contain its name as well as its body. `context_requires_entity` can require an
actual Entity candidate for a polymorphic context. Unclaimed anonymous nodes
remain scope barriers. These rules use native SDK nodes and do not infer a
declaration by spelling or capture-parent guesses.

## Adding an ANTLR parser as a framework contributor

YAML alone can customize extraction for the five registered ANTLR drivers, but cannot
register a sixth driver today. Adding one requires all of these implementation pieces:

1. generate the recognizers for the official ANTLR C++ runtime and include them
   in the native build under `crates/graphit-ast/src/antlr_engine/cpp/generated`;
2. extend the typed dialect boundary in `antlr_engine/bridge.rs` and its C++
   implementation with the actual generated start rule, source positions,
   diagnostics and operation-local cancellation;
3. register the recognizer sources in `antlr_engine/build_bridge.rs` and expose
   the dialect through the native profile/model dispatch;
4. add the complete YAML profile with `parser: antlr4`, grammar metadata,
   selectors, contexts, targets, complexity observations and embedding selection;
5. preserve original Source/ranges, ownership, extraction and relation fields
   through the native graph and the official SCIP syntax producer;
6. add focused fixtures for the driver, selectors, filtering, ranges, malformed
   input, cancellation and graph/projection integrity.

The Rust composition calls the native ANTLR bridge. YAML metadata cannot register a generated recognizer or invent a calculated
complexity metric. A new dialect needs its own native model/NN eligibility
contract; registering a parser alone does not establish an NN projection.

`start_rule` in YAML is descriptive metadata; the native dialect implementation
is the authority for which generated parser method starts the parse.

## Packaging and sharing a compiler

Publish the image to a registry your Docker installation can access, retaining
its upstream versions, local changes and licenses in the image's maintained
source. Configure its exact tag or digest in `ast.scip.parsers`. The string setting can
be shared in project configuration without machine-specific native library paths.
Custom images use the same process, output and Source authorization boundaries as
maintained images. Image publication and platform qualification remain separate
from local source tests.

## Validation workflow

There is currently no standalone grammar-schema validator, so validate at three
levels:

1. admit a small fixture through the configured native Source project and wait
   for its normal indexing outcome, retaining the selected project/version and
   published generation;
2. inspect actual entities/relationships with `graphit_ast_query` and Source with
   `graphit_ast_source`; inspect causal diagnostics for rejected files, fields,
   patterns or captures. These readers do not trigger indexing;
3. for framework contributions, select the affected Rust AST suites. For example,
   changes to source ranges or PostgreSQL composition use:

   ```text
   cargo xtask test graphit-ast/source_ranges graphit-ast/postgresql_semantic
   ```

   Choose targets by the behavior changed rather than porting the retired Go
   package tests one for one. See [Rust checks and Git hooks](../development/rust-fast-guard.md)
   for the registered selectors and native fixture prerequisites.

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
| Custom compiler does not run | Check `ast.scip.parsers` effective layer, exact image reference, Docker authentication and diagnosed pull/process failure. |
| YAML edit appears ignored | Verify the next captured profile load, filename suffix, strict YAML fields, `language`, `data_key`, and `pattern`; inspect the invalid-profile diagnostic. |
| A merged language loses behavior | Lists/scalars replace as a whole; empty lists inherit. Replace the full profile when you need to clear inherited values. |
| A Tree-sitter predicate matches decoys | Keep predicates inside the same pattern expression and verify capture names. |
| ANTLR captures the wrong text | Distinguish the anchor `pattern` from `captures` operations; use `nearest_descendant: rule` for the closest matching descendant, and `xpath: '//rule'` for ordinary XPath descendant selection. |
| Relationships resolve to the wrong kind | Narrow `target_rules.<REL>.labels` or `target_labels`, and qualify names when scope alone is insufficient. |
| Embedded text has wrong line numbers | Remove newline-changing normalization/wrapping; the loader warns and ignores invalid replacements. |
| A new ANTLR grammar is unavailable | YAML alone does not register a generated recognizer; add the native contributor integration described above. |

For storage paths and daemon observation behavior, see
[Filesystem and observation contract](filesystem_contract.md). For the complete AST
storage/retrieval design, see [AST module specification](../specs/ast_module.md).


### Generic NativeNN indexing

Generic profiles may select `embed_labels` once their entity and complete owner
extents are proven in the original Source. NN stores the original raw name and
anchor, including quotes and whole mapped atoms; Graph Data display may differ.
Missing or conflicting extents remain observable and produce no fabricated NN
row. A namespace with an exact original name can retain its native Graph identity
with a NonExact unit; its NN descendants remain unavailable until the full owner
extent is proven.

App uses an active local provider with CPU embedding, the existing configured
embedding slot and the normal model service. Enabling the embedding module alone
does not activate a provider; startup reports that missing configuration. The
Generic consumer retains parse/input mappings across await and publishes through
the shared format-3 codec. Semantic/hybrid results correlate by UID/data_key and
Source SHA. A format-2 derived index must reindex normally; changing a metadata
tag does not convert it. See the [NativeNN contract](../specs/rust_tree_sitter_native_nn.md).

Earlier resource-dependent Linux Generic NN qualification covered the former
installed-library loader and real local CPU EmbeddingGemma2 inference, incremental
vector reuse and semantic/hybrid association. That evidence remains scoped to its
unchanged semantic behaviors; it does not qualify custom compiler-image routing,
removed native-library installation, GPU execution or other platforms. Select
current affected tests by the producer and consumer changed, and distinguish
linked grammar extraction, actual compiler-image execution and model inference.

The retained Generic NN functional selector is:

```sh
cargo xtask test graphit-app/ast-generic-main-native-nn
```

Its current test is `ast_generic_nn_executable::generic_main_native_nn`, using a
custom extraction profile with the linked `tree-sitter-json` grammar. It does not
load a shared library or run a compiler image. Explicit execution requires
`GRAPHIT_TEST_MODEL_SERVER` and `GRAPHIT_EMBEDDINGGEMMA2_MODEL` to name the provisioned
model server and verified EmbeddingGemma2 GGUF asset; missing prerequisites fail
rather than download a replacement. The fixture selects CPU and exercises the
normal model service. Its source adaptation does not itself qualify a new run.
