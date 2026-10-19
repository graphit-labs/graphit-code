# Rust AST JSON semantic model

This is the first bounded consumer of the effective-profile Tree-sitter
collector. It assembles the JSON profile's Pair/Value observations into genuine
syntax identities and original-source projections. It does not deliver the
full language-independent AST model, relationship resolution or publication.

Planning: `tsk-726c`; implementation: `tsk-6866`, under `tsk-7858` / `ses-c98a`.

## Actual input and ownership

[`ParsedJsonDocument`](../../crates/graphit-ast/src/semantic_model.rs) is created
only by `collect(factory, input, relative_components, limits)`. The caller
supplies `PreparedTreeSitterContexts` and the real `PreparedInput`. Collection
runs once and retains its tree, native context selections, nodes, query groups, captures, bindings, SDK
properties and original projections. The caller remains responsible for loading
the actual trusted grammar; a profile's grammar string is not a native proof.

The parsed owner creates `IdentityDocument` from that input's original bytes,
effective language/grammar and original request. `assemble()` returns a model
that borrows the parsed owner. Entity identities are produced by
`IdentityDocument::symbol`; there is no constructor accepting a serialized UID,
restored source handle or manually assembled semantic contributions.

`model.parsed().observations()` borrows the retained collector. Unsupported
occurrences and recovery information remain inspectable there and in model
diagnostics. Original cancellation and timeout apply to collection, assembly
and projection, including the final check on error paths.

## JSON boundaries and names

A Pair key is identified structurally: a captured `string_content` belongs to
a `string` with field `key` under a native `pair`, or the capture selects that
key string itself. The identity anchor and `name_raw()` are the entire original
string token. The decoded name comes from serde_json's string decoder. This
permits an empty decoded name without inventing an empty identity anchor.
Contributions retain each actual capture, including individual string fragments.

The stock grammar has no `string_content` child for an empty string and separates
escape sequences from text fragments. The stock profile therefore does not
capture every empty or escape-only key. The adapter does not inject replacement
queries or fabricate those occurrences. Explicit full-string custom queries can
select those tokens and are exercised by the fixtures.

A Pair's default extent is the actual complete `pair` node. A scalar or string
Value uses its complete native literal token. Configured span captures may select
another complete Pair on the key's native ancestor chain; disagreement between
that span and another contribution is an observable extent conflict. These
rules do not choose a hull or arbitrary parent as a declaration.

Namespace comes from actual ancestor pairs in outer-to-inner order. Native
Tree-sitter context queries select their names with `@context` and `@name`,
including nested captures and ordered pattern alternatives. Captured string
fragments resolve to their entire JSON string token before decoding, preserving
original anchors and escaped names. A Pair
excludes itself by native node identity. Explicit parent captures must identify
the key of a real ancestor pair; equal text alone cannot establish that link.
Configured `value_label` produces a literal child with its declaring Pair as
the immediate owner. Labels and data keys come from effective query metadata.

The Pair context is configured using the shared native query preparation:

```yaml
context_types: {pair: Pair}
context_name_queries:
  pair: '(pair key: (string) @name) @context'
```

A query must capture exactly one owner of the configured kind and one name per
match. Invalid syntax or bindings fail preparation/collection; a valid query
that does not match an ancestor produces an unresolved-owner diagnostic rather
than silently substituting its key. An explicitly empty context map remains
supported. Explicit parent/value labels may still name actual owner pairs using
the grammar's `key` field when no context is configured. Default context maps,
other context kinds, text normalizers and name-reject filters retain their
existing unsupported-configuration errors. Relation queries remain retained
observations with an unsupported-query diagnostic.

The native context collector traverses once with `TreeCursor`. JSON computes
nearest-pair and recovery indexes from the same parent-first observations;
namespace assembly follows only the relevant pair chain. It does not rebuild
a tree, invoke the removed ContextPath grammar, or repeat an ancestor search
for each literal. The same prepared contexts are used by the foreground daemon
and embedded-language consumers that feed the graph, SCIP and NN projections.

## Origins, values and merge

Only an Exact, nonempty lexical anchor can reach the identity factory. Extents
use the supplied-extent primitive against the same original allocation.
Compound, Synthetic, Unmappable and Ambiguous observations remain typed; none
becomes a fabricated source window. With the current origin producer, Ambiguous
is obtained for zero-width projections, including native MISSING recovery
captures, rather than a nonempty lexical token.

JSON strings, booleans and null use serde_json's mature decoder. Numbers retain
a borrowed, validated `RawValue` and their exact JSON lexeme. This representation
does not round huge integers or turn tiny exponents into a floating-point zero.
It does not claim arithmetic normalization. `1` and `1.0` are conservatively
different contributions. After RawValue validates the complete JSON value, its
lexical category must agree with the prepared Number node. An Exact mapping of
prepared `1` to original `true` is a kind-mismatch diagnostic, not a Number.
RawValue borrowing and formatting preservation are
documented by [serde_json](https://docs.rs/serde_json/1.0.151/serde_json/value/struct.RawValue.html).
Arrays, objects, CDATA and other language-specific codecs remain later work.

Entities group by their actual factory UID and effective data key. Duplicate
declaration/value queries enrich one entity while retaining every contribution
and its query/match/capture locator. Distinct paths, labels, anchors, owners and
data keys remain distinct. Conflicting values or extents retain both contributions
and expose their indices; getters do not silently select the first conflict.
Configured missing roles, malformed literals, nonexact origins and native
recovery are diagnostics rather than empty successful fields.
Diagnostics from an early failed parent selection are associated with existing
genuine declarations of the same native subject, label and data key after all
queries run. This works in either query order and does not manufacture an owner
or UID. An unresolved diagnostic cannot leave that declaration's projection
silently available through another query.

## CodeDocument projection handoff

`code_projections()` selects the effective `embed_labels`. An available result
borrows the entity, its exact original extent, source text, producing language
and grammar. Its text is the unchanged original extent slice. A conflict,
nonexact extent or diagnostic affecting that entity yields an explicit
Unavailable result with conflict and diagnostic indices. Unrelated occurrences
do not manufacture empty available text.

These views feed the native NN JSON encoder and graph assembler. They do
not invoke AI, emit Arrow rows, assign a model generation, publish GraphBatch,
or associate Raw versions. The existing Rust Function NN representation and
projection recipe must not be used as a JSON codec: decoded JSON names can
differ from their lexical anchors. SCIP identities remain a separate domain.

## Graph consumer

The [native AST Graph producer](rust_ast_semantic_graph.md) consumes this same
retained model through assemble_graph_json, independently of embed_labels and
NN inference. It retains every assembled entity, native namespace, query/capture
slots, value status, literal source/decoded JSON representation and diagnostics.
Source association uses the genuine parsed Original allocation and actual Raw;
it does not reconstruct ownership from rows or parse the document twice. JSON
relation queries remain ObservedOnly with UnsupportedQueryKind diagnostics,
without fabricated target edges. The foreground App/Data consumer publishes validated documents; the semantic model itself performs no storage writes.

## Validation and remaining scope

[`semantic_model` fixtures](../../crates/graphit-ast/tests/semantic_model.rs)
consume the real catalog, stock/custom profiles and JSON SDK. They cover
declaration enrichment, native namespaces, homonyms, custom labels/data keys,
full-token names, exact numbers, actual value/extent conflicts, original
projections, recovery, missing roles and request controls.

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test semantic_model
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test semantic_model -- -D warnings
```

The JSON fixtures also exercise native nested context-name capture, unresolved
owners and rejected missing capture bindings. The graph and NN consumers use
the same context-aware collector:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test semantic_graph_json --test native_nn_json
```

The [effective-profile collector](rust_tree_sitter_profile_matches.md),
[original extent primitive](rust_ast_entity_extent.md) and
[semantic extraction plan](rust_ast_semantic_extraction.md) remain separate
contracts. Full E01–E10 coverage still requires backend-specific delimiters,
general context queries, values, attachments, metrics, exports, embedding,
relationship targets and the external SCIP handoff. G2, Raw association,
publication/recovery and incremental refresh remain with their existing owners.
