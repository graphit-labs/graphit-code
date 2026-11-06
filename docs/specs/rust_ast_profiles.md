---
title: "Rust AST Profile Decoder"
description: "Typed YAML profile schema, original locations, input limits and decoder API."
content-type: reference
audience: developers
status: A_AFFECTED_MAIN_QUALIFIED
fixture-status: A_AFFECTED_TESTS_PASSED
references:
  - target: {type: task, id: tsk-0b48}
    relation: implements
  - target: {type: task, id: tsk-e4a9}
    relation: derived_from
  - target: {type: task, id: tsk-3eb5}
    relation: implements
  - target: {type: task, id: tsk-0006}
    relation: derived_from
  - target: {type: task, id: tsk-bcf6}
    relation: uses
  - target: {type: task, id: tsk-e056}
    relation: uses
  - target: {type: task, id: tsk-d83c}
    relation: reuses
  - target: {type: task, id: tsk-13ae}
    relation: uses
  - target: {type: task, id: tsk-0ac6}
    relation: relates_to
  - target: {type: task, id: tsk-3f5d}
    relation: derived_from
---

# Rust AST profile decoder

Audience: decoder maintainers and consumers building the AST profile catalog.
The current API is reconciled under `tsk-3f5d`; affected MAIN tests have passed,
and all seven durable checks passed after final documentation review.

## Scope and dependencies

D1 decodes captured YAML bytes into a complete owned typed patch. It preserves
presence, values, order, legitimate repeated queries and original source sites.
It performs no filesystem reads or native query/parser execution.

The repository checks out canonical profiles under `crates/graphit-ast/profiles/`
and text fixtures under `crates/graphit-ast/tests/fixtures/` with LF through
`.gitattributes`, keeping byte counts, source spans and fixture recipes stable
across hosts. The two `unicode-crlf` fixtures retain their explicit CRLF rules.
This checkout policy leaves captured profile and source bytes unchanged; the
decoder continues to audit and map the exact supplied input.

| Completed producer | Reused responsibility |
| --- | --- |
| `tsk-0006` | Inventory of active AST profiles and capabilities |
| `tsk-bcf6` | Integral scalar audit and exact scalar lookup |
| `tsk-e056` | UTF-8 ranges and original byte spans |
| `tsk-d83c` | Mature codec qualification |
| `tsk-13ae` | Original `SnapshotContext` |

[The catalog](rust_ast_profile_catalog.md) consumes real `DecodedProfile` values
through their typed API. Native query/grammar execution and generation
publication are separate consumers.
The producer, tests and fixtures are in `crates/graphit-ast/src/profiles.rs`,
`crates/graphit-ast/tests/profiles.rs` and `tests/fixtures/profiles/` relative to
the crate. Shared exports and manifests remain coordinated separately.

## Decode journey

The caller supplies a logical profile ID, captured bytes, validated positive
`ProfileLimits` and the original `SnapshotContext`. D1 checks the context and
input limits, audits scalars, deserializes a closed typed WireDTO from exactly
the audited input, validates semantic fields and representation limits, and
performs fallible allocation of the final patch. Failure returns no partial
`DecodedProfile`.

The actual ScalarAudit/input binding is preserved: it identifies the bytes
that were audited. Blocking SDK deserialization is checked before and after;
those checks do not provide interruption inside the SDK phase.

## Closed schema

Unknown and duplicate struct keys are errors. Dynamic map keys are audited Text.
Labels, captures, patterns and custom languages are not restricted to shipped
vocabularies. Text means an exact scalar-audit String, never codec coercion.

### Profile: all 20 fields

| Field | Type | Rule |
| --- | --- | --- |
| `language` | Text | Required, nonempty routing declaration |
| `extensions` | List<Text> | Preserve order and duplicates |
| `self_keywords` | List<Text> | Preserve values |
| `anon_func_types` | List<Text> | Preserve values |
| `declaration_types` | List<Text> | Preserve values |
| `comment_types` | List<Text> | Preserve values |
| `embed_labels` | List<Text> | Preserve order and duplicates |
| `parser` | Enum | `tree-sitter` or `antlr4` |
| `grammar` | Text | Missing differs from explicit empty |
| `start_rule` | Text | Missing differs from explicit empty |
| `merge` | Bool | Local patch control |
| `exclusive` | Bool | Explicit false remains Set |
| `queries` | List<Query> | Repeated `data_key` is legitimate |
| `exports` | Export | Nullable |
| `context_types` | Map<Text, Text> | Values may Clear |
| `context_name_queries` | Map<Text, Text> | Native Tree-sitter context/name queries; values may Clear |
| `antlr_contexts` | List<AntlrContext> | Official XPath selectors with structured names and optional direct-child conditions |
| `target_rules` | Map<Text, TargetRule> | Values may Clear |
| `embedded` | List<Embedded> | Preserve order |
| `text_normalizers` | Map<Text, Normalizer> | Values may Clear |
| `complexity` | Complexity | Nullable |

### Query

ANTLR additionally supports `select`, `keyword`, `child_rule`, `negate` and a
`captures` map of ordered `{xpath}`, `{nearest_descendant}` or
`{nearest_ancestor}` operations. An operation must declare exactly one nonempty
selector. Capture slots are `name`, `value`, `span`, `parent` and `qualifier`;
`value` and `parent` require their corresponding labels. Tree-sitter continues
using native capture names and rejects ANTLR-only configuration.


| Fields | Type | Rule |
| --- | --- | --- |
| `data_key`, `pattern` | Text | Required and nonempty |
| `graph_label` | Text | Required nonempty for entity; omitted/empty allowed for relation |
| `name_capture` | Text | Empty retained for future compiler default naming |
| `relation_type` | Text | Required nonempty for relation |
| `value_capture`, `value_label` | Text | Both nonempty or both absent/empty |
| `parent_capture`, `parent_label` | Text | Both nonempty or both absent/empty |
| `name_reject`, `span_capture`, `qualifier_capture`, `target_fallback` | Text | Opaque declarations |
| `type` | Enum | `entity` or `relation` |
| `name_is_data` | Bool | Audited Boolean |
| `target_labels` | List<Text> | Preserve values |

Malformed native query syntax remains declared text until mature compiler
validation. D1 does not compile patterns or silently drop invalid captures.

### Nested structures

| Structure | Complete fields |
| --- | --- |
| Export | `strategy`: enum; `config`: Map<Text, Text>; `config_list`: Map<Text, List<Text>> |
| TargetRule | `labels`: List<Text>; `fallback`: Text |
| Embedded | Required nonempty `pattern`, `text_capture`; Text `lang_capture`, `default`, `wrap_prefix`, `wrap_suffix`, `normalize`; `languages`: Map<Text, Text>; `host_labels`: List<Text> |
| Normalizer | `replace`: Map<Text, Text> with nonempty keys; `numeric_char_refs`: Bool |
| Complexity | `node_types`, `operators`: List<Text>; nullable `head_calls`: HeadCalls |
| HeadCalls | Required nonempty `node_type`; `names`, `pair_names`, `subject_pair_names`: List<Text> |

Export strategies: `capitalized_name`, `no_prefix`, `modifier`,
`export_statement`, `no_modifier`, `no_static`, `none`. Config maps are opaque.
Target fallback values such as `stub`, `file` or a label declare graph policy;
they do not authorize placeholder code. Empty `embedded.languages` values mean
deliberate Skip. Wrapping and normalization remain future consumer operations.

## Presence and catalog interpretation

Every getter preserves Missing, Set and legal Clear. Null is permitted only for
exports, complexity, head_calls and dynamic map values (key removal). Language,
parser, Booleans, arrays and array elements are nonnullable. Closed nested
fields gain no implicit Null permission. A null `config_list` map value removes
its key; a null element inside its list is invalid.

Core Null spellings are `null`, `Null`, `NULL`, `~` and implicit empty values.
`NuLl`/`nUlL` are Core String. Quoted `"null"` and `!!str null` are Text.
They never become Clear. Nullable compound slots reject these Strings with
InvalidField; ordinary Text maps preserve their exact bytes.

Implicit Null spans come from the producer's colon marker, not invented EOF:

| Input | Exact empty scalar span |
| --- | --- |
| `x:` | `1..1` |
| `exports:` | `7..7` |
| `language: x\nexports:` | `19..19` |

EOF coordinate conversion is a separate fixture. Original BOM mapping belongs
to the audited producer; D1 does not strip or rewrite source bytes.

D1 retains patches; D2 applies these rules:

- Replacement uses defaults for Missing. Merge inherits Missing from its base;
  without a base it starts from defaults. Merge itself is local control.
- Set(false) replaces true; empty lists clear; empty dynamic maps clear.
- Nonempty maps replace touched values, remove Clear keys, preserve untouched
  keys and positions, and append new keys in upper order.
- Exports objects replace the complete configuration with nested defaults;
  Clear resets to none and empty maps.
- Complexity objects merge by subfield; an empty object inherits during merge.
  Clear resets empty lists/head_calls None and consumer base count 1.
- HeadCalls objects replace the complete object; Clear removes only head calls.
- Nonempty upper queries remove every base query with an EXACT matching data_key
  and append ALL upper queries in order. Empty upper queries clear.
- Nonempty upper embedded entries precede base entries for first-claim matching;
  empty lists clear. embed_labels replaces the entire ordered list.

Defaults belong to D2: parser tree-sitter; query type entity; false Booleans;
empty lists/maps/optional Text; start_rule None when omitted and Some("") when
explicit empty; export strategy none; head_calls None. Effective schema excludes
merge. Replacement or merge without a base derives omitted/empty grammar from
backend and language. With a base, Missing grammar inherits even when parser
changes; explicit empty grammar derives from resulting backend/language.

Language and nonempty grammar routing declarations are UTF-8, NUL-free and have
no edge ASCII whitespace. No trimming or repair is allowed. ASCII-casefold
comparison does not rewrite stored bytes. Opaque paths, labels and patterns
do not inherit routing restrictions. Future backend mismatch is a visible error.
Same-layer duplicate language and effective full grammar identity checks belong
to D2. Embedded language map keys cannot be ambiguous under ASCII casefold;
other dynamic keys preserve case. Duplicate extensions within one profile stay
in output and are deduplicated only in the selector index.

## Current public decoder API

These APIs are active in the real library; affected validation is recorded below.

```text
ProfileId::new(&str, maximum_bytes: usize) -> Result<ProfileId, ProfileIdError>
ProfileId::as_str() -> &str
ProfileInput<'a> { id: &'a ProfileId, bytes: &'a [u8] }
ProfileLimits::new(ProfileLimitsInput) -> validated opaque limits or typed error
ProfileDecoder::new(ScalarPolicy) -> ProfileDecoder
ProfileDecoder::decode<'a>(&self, ProfileInput<'a>, &ProfileLimits, &SnapshotContext)
    -> Result<DecodedProfile, ProfileError<'a>>
DecodedProfile::id() -> &ProfileId
DecodedProfile::usage() -> Copy WireUsage
DecodedProfile::patch() -> &ProfilePatch
```

ProfileId is private, nonempty, NUL-free, byte-preserving and fallibly copied;
its positive cap is at most u32::MAX. It is a logical label.
ProfileIdError is Copy: InvalidLimits, InvalidId, InputTooLarge, AllocationDenied.
Limit validation errors are Copy InvalidLimits/ArithmeticOverflow.

All profile and nested fields have named readonly Presence getters, including
`r#type()` and `r#default()`. Patches, located values and decoded profiles have
no public constructors that fabricate validated output.

```rust
pub enum Presence<'a, T> {
    Missing,
    Set(&'a Located<T>),
    Clear { span: OriginalByteSpan },
}
```

Located<T> exposes value()/location(); LocatedText and LocatedBool are scalar
aliases with exact span(). List<T> exposes readonly slices/iterators.
PatchMap<T> exposes len()/is_empty() and iterators of located Text keys with
Presence values. ProfilePatch exposes its root location. WireUsage exposes
input_bytes, nodes, depth, text_bytes, max_scalar_bytes, max_collection_entries.

SourceLocation is opaque Copy with kind()/span(). LocationKind distinguishes
ScalarExact (one exact audited scalar record) from NodeStart (SDK initial node
event). Spanned compounds do not capture the entire container range. Missing
has no invented site; invalid metadata becomes InvalidSpan.

## Actual producer and SDK APIs

Reuse `scalar_policy.rs`: ScalarPolicy::new(RegexLimits), audit(bytes, limits),
ScalarAudit::input()/records()/usage()/original_span()/text_at()/bool_at()/null_at().
RegexLimits has public compiled_size_policy_bytes and dfa_cache_policy_bytes;
there is no RegexLimits constructor or ScalarAudit::decode<T> API.
SourceText is in `source_ranges.rs` and reexported by the AST crate.
Core SnapshotContext is `graphit_core::file_io::SnapshotContext`; Cancellation
is `graphit_core::file_lock::Cancellation`, without invented root reexports.

The SDK is pinned serde-saphyr 1.3.0. Typed deserialization uses its public
from_str_with_options and ordinary Serde Deserialize/visitors. Spanned<T>
requires T: Deserialize. Its public value/defined/referenced fields are real.
Original location conversion follows the qualified scalar test helper:

```rust
let span = wire.defined.span();
let original = audit.original_span(CodecSpan::from_parts(
    span.byte_offset(), span.byte_len(),
)?)?;
```

CodecSpan receives optional u64 offset and LENGTH, not start/end. Conversion
checks bounds, arithmetic and original UTF-8. There is no Spanned::span(),
Location::byte_offset() or public contextual SpannedSeed. The private
__yaml_spanned protocol/dispatcher must never be copied.

The ordinary closed WireDTO captures keys in Spanned<WText> (Option<String> transport) and retains
key/value slots. It rejects unknown fields before their values. Dynamic maps
retain ordered pairs until semantic key/kind validation. Nullable transport is
typed by the declared slot, preserving the full schema:

- WireNullableObject<T> transports a map via public MapAccessDeserializer or
  any scalar marker accepted by public native SDK dispatch. It applies to exports, complexity, head_calls and
  object-valued target_rules/text_normalizers entries. A sequence here is Decode.
- WireNullableList<T> transports a sequence of Spanned<T> via public
  SeqAccessDeserializer or a scalar marker accepted by public native SDK dispatch. It applies to dynamic
  exports.config_list values. A legitimate Text list becomes Set(List); CoreNull
  removes the map key. A map here is Decode; a null list element is InvalidField.
- WireNullableText handles scalar-valued dynamic Text maps. CoreNull removes the
  key; audited CoreString becomes Set(Text), including quoted/!!str null and
  NuLl/nUlL. Accepted Boolean/Numeric transport values become InvalidField through
  the strict audit lookup; SDK rejection before the visitor remains Decode.
  Compound values are Decode.

Every Clear requires exact audit.null_at confirmation. Other accepted scalar markers
are InvalidField for nullable object/list slots. Scalar transport never defines
final values. No generic sequence rejection may exclude config_list lists.

Bool slots use public deserialize_any scalar markers, so valid `!!bool true`
and `!!bool false` reach exact audit.bool_at validation. Ordinary Text, keys
and enum slots use Option<String> transport: plain Numeric text can reach the
strict audit rejection, while explicitly tagged non-String scalars may fail
natively before that semantic lookup. No wrong-kind final value is published.

The pinned SDK has visible dispatch limits. With nonfinite rejection enabled,
nullable `1e999`/`.inf`/`.nan` may fail as NonFiniteFloat before a visitor;
`!!int` beyond i64/u64 and invalid `!!float` may fail as InvalidScalar. These
native failures map to closed Decode, without retry, coercion or vendor text
matching. Huge plain hex may reach a String visitor, but its audited Numeric
kind still rejects it. This is a native syntax transport boundary, not a claim
that all Numeric representations reach the semantic pass.

Error location metadata with both byte getters absent is legitimately
unavailable. It preserves the native closed category and may use the public
parser marker fallback as a validated zero-length site. A partial byte pair,
out-of-bounds span, overflow or split UTF-8 boundary becomes InvalidSpan.
Output Spanned byte metadata is always required; optional error metadata never
relaxes final SourceLocation validation.

Integral audit runs FIRST. Typed SDK receives SAME audit.input bytes; validation
reads all keys/values through exact producer records. A readonly semantic pass
checks schema, N/D/T/S/C and required rules BEFORE retained allocation. A second
pass builds final values with fallible copies and exact audited lookups.
No generic Value parser, thread-local context or private codec extension is used.

## Input and representation limits

ProfileLimitsInput has positive usize fields input_bytes(I), nodes(N), depth(D),
text_bytes(T), scalar_bytes(S), collection_entries(C). I and C are at most u32::MAX;
I+1, 2N and 4N use checked arithmetic before SDK work. Opaque limits expose
readonly getters; no unlimited policy defaults are supplied.

WireUsage preserves I/N/D/T/S/C. Root semantic depth is 1. Each scalar/value/
container, including Boolean and Null, contributes one node; keys contribute
Text bytes but no node. Text keys and String values sum T and obey S.
Lists/maps obey C; Missing contributes nothing. Boolean/Null do not invent Text.

ScalarLimits::new(I, D, 4N, 2N, I, I, C) bounds scalar auditing. Typed
serde_saphyr options bound max_nodes 2N, max_events 4N, depth/flow depth D,
one document, scalar/comment bytes and simple-key lookahead I. Existing
anchor/alias/merge/include, duplicate/tag/Boolean and nonfinite policies remain.
The SDK `budget` supplies structural deserialization bounds.
SDK/Audit/WireDTO temporary allocation does not establish total heap or RSS
guarantees. Final owned copies use fallible allocation.

## Original context

Decode checks the caller's original SnapshotContext during the operation.
Interrupted maps to Cancelled, TimedOut to DeadlineExceeded, and other context
failures to ContextDenied; cancellation precedes expiry in Core. Checks preserve
the original cancellation signal and deadline. D2 consumes typed output through
the public API. The former ProfileOperationBudget and ProfileCharge bridge is
removed; no cumulative work or allocation accounting is required.

MAIN uses `graphit_core::file_io::SnapshotContext::new(deadline, Cancellation,
max_io_bytes) -> io::Result<Self>` and `check() -> io::Result<()>`.
The future two-argument constructor is adopted only after Core publication.

## Closed errors and retry

ProfileError<'a> exposes borrowed profile_id Option<&ProfileId>, Copy code,
optional OriginalByteSpan, static schema_path and [Option<u32>;4] indices through
readonly getters. It retains no source, physical path, owned diagnostic String
or vendor Display. Codes are InvalidLimits/InputTooLarge/InvalidUtf8/Decode/
QuotaExceeded/ArithmeticOverflow/AllocationDenied/InvalidField/InvalidSpan/
Cancelled/DeadlineExceeded/ContextDenied.

| Condition | Result |
| --- | --- |
| Missing/ambiguous/invalid scalar span | InvalidSpan |
| Wrong kind in known semantic slot | InvalidField |
| Parse/unsupported tag/anchor/alias/invalid scalar | Decode |
| Producer/native quota or multiple documents | QuotaExceeded |
| Invalid UTF-8 | InvalidUtf8 |
| Required field/enum/pair/null policy error | InvalidField with known static path |
| Native malformed shape/unsupported compound shape | Decode; path may be absent |
| Legitimately unavailable native error byte metadata | Preserve native closed category; validated Parser marker fallback if available, otherwise no span |
| Present invalid native metadata | InvalidSpan |

Resource/context errors may lack path. Native visitor rejection does not imply
a fabricated semantic path. Error ranges describe actual scalar/event sites,
not complete containers. The caller follows its original context and input
policy; a failed decode never returns a partial patch.

### Static schema paths

The decoder uses the following static API conventions.
Root fields use their field name. Nested fields use queries.<field>,
exports.<field>, embedded.<field>, target_rules.<field>,
text_normalizers.<field>, complexity.<field> or complexity.head_calls.<field>.
Dynamic key text never enters a static path. A list-element error uses the
containing list slot, such as extensions or exports.config_list.

Half capture/label pairs use queries.value_capture or queries.parent_capture,
with the actual site of whichever counterpart is present. A missing required
field may use the validated parent NodeStart, without inventing a closing range.
Native-shape/unknown-key Decode and resource errors may legitimately omit paths.

Indices hold ordered enclosing list/map positions from outermost to innermost
in four optional u32 slots; unused slots are None. Query/embedded list position
uses index0; embedded languages map then uses index1. text_normalizers map
position uses index0 and replace entry index1. exports.config_list map entry
uses index0 and its list element index1. Checked C bounds prevent index wrapping.

## Functional fixtures

The byte-preserving corpus manifest covers 45 original YAML profiles, total
198844 bytes. Functional boundary, schema-case and SDK-boundary fixtures cover
all fields, Presence/null/removal, tags and native versus semantic rejection.
Their historical preparation annotations remain fixture metadata; the current
execution evidence is the affected MAIN log below.

Corpus expectations are 45 language IDs, 44 documented public IDs plus internal
plpgsql, 40 Tree-sitter profiles (including Missing parser defaults), five
explicit ANTLR profiles, 999 queries and 8276 nodes. ScalarAudit measures 152845
bytes across scalar records, including five Boolean `true` records totaling
20 bytes; retained String/key text is 152825 bytes. Each decoded profile is
compared with String-only audit records. Five exclusive profiles are db2,
plpgsql, plsql, postgresql and tsql; ANTLR profiles are cobol85, db2, plsql,
postgresql and tsql. D2 retains all 45 profiles.

| Checks | Functional obligations |
| --- | --- |
| P01/P05/P10 | All45 getters/order/repetitions/full nested schema; required fields/pairs/enums/custom labels; opaque native patterns |
| P03/P04 | Presence/Clear/map removal versus list null; numeric/Text/Bool distinctions; duplicate/alias/anchor/merge/tag/multidoc errors |
| L01 | Exact semantic caps and independent excess, positive constructors and arithmetic overflow |
| L03 | UTF-8/BOM/CRLF original sites, ScalarExact versus NodeStart, colon-null and EOF |
| L04 | Closed codes/static paths/bounded indices; legitimate missing span/path; typed native errors |
| L05 | Original cancellation/deadline/errors; real fallible allocation and no partial patch |

For `language: x\n`: I12/N2/D2/T9/S8/C1. Exact caps accept; independently lowering
I/N/D/T/S by one fails. C=0 is invalid; a two-field map exceeds C1.
The public fixture caps I32768/N2048/D16/T32768/S4096/C256 are test policy.
Numeric controls include 123, 1e999, huge unsigned hex/octal values and explicit
String transport. Quoted/tagged/mixed-case null Text remains byte-preserving;
nullable-object semantic rejection retains the corresponding static field path.
Synthetic normalization keys are declarative; this producer performs no transform.

P02 overlays/P06 routing/P07 filters/P08 draft retention/P09 custom selectors
and L02 effective counts use real output in D2. A canonical effective minimal
profile has 19 root fields excluding merge, 26 nodes and 294 text bytes;
two minimal x/y inputs have input profiles2/I24/N4/T18 and effective profiles2/
N52/T588. The outer profile vector adds no semantic wrapper node.

Private units retain native error metadata validation, original context ordering
and actual Vec capacity rejection. Limits are input/representation policy,
not process-wide memory or CPU guarantees.

## Affected validation

Leaf A `tsk-3f5d` is being reconciled against MAIN. On Linux the affected
public suites passed: profiles 10/10, catalog 14/14 and symbols 9/9.
The affected private units passed 3/3, 5/5 and 5/5 respectively. The six
symbols doctests passed, including five compile-fail lifetime cases.
Strict Clippy for the library and three affected targets passed with -D warnings.
Scoped rustfmt of the six owned files and all seven durable checks passed.
These Linux results do not qualify another platform or the future Core API.

```text
cargo xtask cargo test -p graphit-ast --test profiles --test catalog --test symbols --jobs 1
cargo xtask cargo test -p graphit-ast --lib profiles::tests --jobs 1
cargo xtask cargo test -p graphit-ast --lib catalog::tests --jobs 1
cargo xtask cargo test -p graphit-ast --lib symbols::tests --jobs 1
cargo xtask cargo test -p graphit-ast --doc symbols --jobs 1
cargo xtask cargo clippy -p graphit-ast --lib --test profiles --test catalog --test symbols --jobs 1 -- -D warnings
```


## Query tree source

Each query optionally declares `source: primary` or
`source: cobol-preprocessor`. Omission selects `QuerySource::Primary`. A configured
source retains its original scalar location and catalog provenance; defaults
remain `ProvenanceView::Default`. Layered query merge/replacement and legitimate
duplicate queries retain their ordinary semantics. Unknown source strings,
non-scalar values and null are field errors at `queries.source` before SDK calls.
There are no aliases or inference from data keys, names or patterns.

```yaml
queries:
  - data_key: imports
    source: cobol-preprocessor
    type: relation
    relation_type: REFERENCES
    pattern: "//copyStatement/copySource"
    name_capture: name
```

`QueryPatch.source()` exposes presence and its scalar site;
`Query.source()` exposes `Effective<QuerySource>`. A consumer must support the
selected tree explicitly. The [COBOL consumer](rust_ast_antlr_semantics.md#cobol-copy-observations-and-native-program-scopes)
accepts the secondary source only for the ANTLR COBOL profile, reports incompatible
engine/grammar causally before SDK execution, and keeps preprocessor ordinals
separate from the main tree. The ordinary main-only semantic consumer rejects
secondary queries instead of silently dropping them.


The ordinary Tree-sitter prepared-profile factory validates every query source
before compiling any native query. `cobol-preprocessor` returns
`TreeSitterProfileError::UnsupportedQuerySource { query_index, source }`, even if
a preceding primary pattern has invalid native vocabulary. Default/explicit
Primary and the existing PG body parameters are unchanged. The new JSON-language
source fixture exercises this actual factory rather than relying on the COBOL
consumer to reject a wrong profile.
