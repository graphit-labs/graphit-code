---
title: Rust AST Profile Catalog
description: "Declarative catalog, overlays, provenance, routing and input limits."
content-type: reference
audience: developers
references:
  - target: {type: task, id: tsk-fe0f}
    relation: implements
  - target: {type: task, id: tsk-0b48}
    relation: implements
  - target: {type: task, id: tsk-0ac6}
    relation: derived_from
  - target: {type: task, id: tsk-3eb5}
    relation: implements
  - target: {type: task, id: tsk-e4a9}
    relation: consumes
  - target: {type: task, id: tsk-bcf6}
    relation: uses
  - target: {type: task, id: tsk-e056}
    relation: uses
  - target: {type: task, id: tsk-13ae}
    relation: uses
  - target: {type: task, id: tsk-0006}
    relation: derived_from
  - target: {type: task, id: tsk-3f5d}
    relation: derived_from
---

# Rust AST profile catalog

Audience: catalog maintainers and consumers selecting profiles for AST compilation.
The current catalog API removes aggregate resource accounting under `tsk-fe0f`;
its focused validation is recorded below.
The catalog consumes real typed [DecodedProfile](rust_ast_profiles.md) values,
preserving Runtime/User/Project layers, Presence, defaults, ordered overlays,
provenance, selection, opt-in grammars and embedded language mapping.
The caller owns publication of a complete draft.

## Ownership and the consumer journey

The producer, tests and fixtures are `crates/graphit-ast/src/catalog.rs`,
`crates/graphit-ast/tests/catalog.rs` and `tests/fixtures/catalog/` relative to
the crate. Shared exports and manifests remain coordinated separately.

A consumer decodes real profiles, supplies ordered Runtime/User/Project layers,
builds an immutable draft with positive limits, creates an allow/deny policy,
and resolves a basename, extension or explicit grammar. Operations check the
original context. Building a new draft never mutates an existing draft.
Selected declarations are inputs to the native compiler.

The [foreground Raw Source consumer](rust_ast_raw_foreground.md) loads the actual
shipped, installed runtime, user and project profiles, then applies this catalog's
effective selectors and configured routing. It streams each selected native FILE
into the project's Raw branch using the existing supervisor. Raw publication does
not compile or publish NN; NN freshness remains a separate integration.

## Real decoder boundary

The producer is imported through `graphit_ast::profiles::*`; the real context is
`graphit_core::file_io::SnapshotContext`. Its relevant APIs are:

```text
ProfileId::new(&str, usize) -> Result<ProfileId, ProfileIdError>
ProfileId::as_str() -> &str
ProfileInput<'a> { id: &'a ProfileId, bytes: &'a [u8] }
ProfileLimits::new(ProfileLimitsInput) -> Result<ProfileLimits, ProfileLimitError>
ProfileDecoder::new(ScalarPolicy) -> Self
ProfileDecoder::decode(ProfileInput, &ProfileLimits, &SnapshotContext)
    -> Result<DecodedProfile, ProfileError>
```

`ProfileLimitError` is `InvalidLimits | ArithmeticOverflow`. `DecodedProfile`
exposes read-only `id()`, `usage()` and `patch()`; fields and constructors are
private. Integration tests must create it through the real public decoder.

Real type names include `ProfileParser::{TreeSitter, Antlr4}`, `QueryKind`,
`ExportStrategy`, `ExportPatch` and `TextNormalizerPatch`.

```text
Presence<'a, T> = Missing | Set(&'a Located<T>) | Clear { span }
Located<T>::value(); Located<T>::location()
List<T>::as_slice() -> &[Located<T>]
PatchMap<T>::iter() -> (&Located<String>, Presence<T>)
```

Do not add another `Located` layer to `List<T>`. All located types expose
`location()`; direct `span()` exists on String/Bool aliases, while enums use
`location().span()`. The closed typed nested schema and required-field validation
remain D1's responsibility. D2 does not implement another visitor or decoder.

D1 audits the complete same input before ordinary typed deserialization. Final
scalar values use exact audit lookup. `SourceLocation` is ScalarExact for one
validated scalar or NodeStart for the initial event of a compound. NodeStart
is not a complete container range. Missing/default fields get no invented site.

Core Null spellings and implicit empty values Clear only permitted slots.
`NuLl`/`nUlL` are String: Text maps retain them; compound Clears and removals reject
them. Quoted or explicitly tagged String null never Clears. The implicit Null
in `language: x\nexports:` has original span `19..19` at the colon; EOF is separate.
D2 does not adjust BOM origins. Native SDK rejection can produce Decode before
semantic lookup. Both absent error byte getters legitimately mean unavailable;
partial or invalid metadata is InvalidSpan. Output locations remain required.

## Current catalog API

```text
CatalogLimits::new(CatalogLimitsInput) -> Result<CatalogLimits, CatalogError>
ProfileLayer<'a> { kind: Runtime | User | Project, profiles: &'a [DecodedProfile] }
CatalogDraft::build(&[ProfileLayer], &CatalogLimits, &SnapshotContext)
    -> Result<CatalogDraft, CatalogError>
SelectionPolicy::new(allow: Vec<String>, deny: Vec<String>, &CatalogLimits,
                     &SnapshotContext) -> Result<SelectionPolicy, CatalogError>
FileSelector<'a> { basename: &'a str, extension: Option<&'a str>,
                   explicit_grammar: Option<&'a str> }
CatalogDraft::resolve_file(&self, FileSelector, &SelectionPolicy, &SnapshotContext)
    -> Result<Selection, CatalogError>
Embedded::mapped_language(&self, captured: &str, &CatalogLimits, &SnapshotContext)
    -> Result<Option<EmbeddedLanguageDecision>, CatalogError>
```

Drafts/effective values are opaque and owned; selection/mapping results borrow
those values. Layers are at most one each, ordered Runtime, User, Project;
omitted layers are allowed. Duplicate or out-of-order layers return InvalidLayer.
ASCII-casefold-equal language identities in one layer return DuplicateLanguage.
Declared bytes are preserved.

## Presence, overlays and defaults

Missing/false `merge` replaces the profile using defaults for Missing. True merge
inherits Missing; without a base it starts from defaults. `merge` is local control
and never an effective field. Set false replaces true. Empty lists clear; empty
dynamic maps clear. Nonempty maps replace touched values as complete values,
remove Clear keys and retain untouched keys. Replacements retain base positions,
new keys append in upper order and removed keys disappear. Keys remain exact and
case-sensitive except explicitly defined routing identities.

Exports objects replace the entire object with nested defaults; Clear restores
strategy none and empty maps. Complexity objects merge by subfield; an empty
object in a merge inherits. Complexity Clear empties lists and head calls; base
count 1 is consumer semantics, not a YAML field. HeadCalls objects replace the
complete object with defaults; Clear removes only head calls.

For a nonempty upper query list, remove every base query whose exact data_key
appears above, then append every upper query in order. Repeated upper groups remain
legitimate. Runtime `[a1,a2,b]`, User merge `[a3,a4]`, Project merge `[c]` yields
`[b,a3,a4,c]`; Project replacement yields `[c]`. Empty queries clear.
Nonempty upper embedded entries precede base entries for future first claim;
empty entries clear. embed_labels replaces the whole list with order/duplicates.
The catalog runs no regex, query, normalizer or source transform.

Defaults: parser tree-sitter, query.type entity, Boolean false, lists/maps empty,
optional Text empty, exports.strategy none, complexity.head_calls None.
Missing start_rule is None; explicit empty is Some(""). Omitted/empty grammar in
replacement or merge without a base derives backend+language. Merge with a base
and Missing grammar inherits even if parser changes; explicit empty rederives
from resulting parser/language. Backend/grammar mismatch is retained for explicit
future compiler rejection, without repair or fallback.

## Complete effective getters

Every field and nested field has a read-only getter. `Effective<T>` exposes `value()` and `provenance()` and has private
fields. EffectiveText/EffectiveBool hold String/bool. EffectiveList exposes a
slice; EffectiveMap exposes ordered iteration, len and is_empty.

| Root fields | Effective value |
|---|---|
| language, grammar | EffectiveText |
| parser, exclusive | Effective parser enum, EffectiveBool |
| start_rule | Effective optional String |
| extensions, self_keywords, anon_func_types, declaration_types, comment_types, embed_labels | Effective list of effective Text |
| queries, embedded | Effective list of effective Query/Embedded |
| exports, complexity | Effective Exports/Complexity |
| context_types, context_name_queries | Effective maps of String |
| antlr_contexts | Effective list of effective XPath owner definitions |
| target_rules, text_normalizers | Effective maps of TargetRule/Normalizer |

Nested getters cover all D1 fields:

- Query: source, select, keyword, child_rule, negate, captures, data_key, pattern, graph_label, name_capture, relation_type, value_capture,
  value_label, name_reject, span_capture, parent_capture, parent_label,
  qualifier_capture, target_fallback, type, name_is_data, target_labels.
- Exports: strategy, config, config_list; TargetRule: labels, fallback.
- Embedded: pattern, text_capture, lang_capture, default, wrap_prefix, wrap_suffix,
  normalize, languages, host_labels.
- Normalizer: replace, numeric_char_refs; Complexity: node_types, operators,
  head_calls; HeadCalls: node_type, names, pair_names, subject_pair_names.

`Query.source()` is `Effective<QuerySource>`: omitted/default `Primary`, or the
explicit `CobolPreprocessor` value with its declared scalar provenance. Query
merge/replacement and custom duplicates retain their existing behavior. The
[typed source schema](rust_ast_profiles.md#query-tree-source) and native consumers
validate routing; the catalog does not compile or repair queries.

Complexity.head_calls returns an effective optional HeadCalls. Removed values
do not gain live provenance. Draft getters are profiles()->&[EffectiveProfile],
source_profile(SourceIndex)->Option<&ProfileId>.
The draft retains owned declarations without a shared request context or native handles.

## Source provenance

SourceIndex is opaque Copy u32 with as_u32(). SourceSite is opaque Copy with
source_index(), location() and span(). ProvenanceView is Default, Declared(site)
or Derived(&[SourceSite]). The source table contains every input in layer/profile
order, including overridden inputs. Logical labels can repeat because SourceIndex
distinguishes entries; these are not physical identities.

Defaults have no fake SourceSite. Declarations retain ScalarExact/NodeStart.
Inheritance keeps old sites. Derived grammar retains ordered relevant declared
language/parser/explicit-empty-grammar sites, deduplicating identical pairs.
Merged containers order base sites then upper sites, also deduplicated. Individual
values preserve declared/inherited sites. Provenance records remain distinct from
resource accounting. Clear stays a patch instruction, not a live field.

## Limits and construction order

CatalogLimitsInput contains only positive `policy_entries` and `selector_bytes`.
The policy count fits u32; its addressable vector size is checked. These fields
bound the supplied allow/deny policy and individual selectors, not a corpus quota.
Each DecodedProfile has already passed ProfileDecoder's per-profile input,
node, depth, text, scalar and collection limits.

Construction checks layer ordering and declaration identities, resolves overlays
and defaults with borrowed text, validates collisions, then materializes the
complete draft once. CatalogUsage, aggregate quotas and the before/after
shape-counting traversals have been removed. Actual allocations remain fallible;
context checks preserve cancellation and deadline throughout construction.
Only a complete draft returns; a failed build preserves the previous draft.

## Original context

Construction, policy validation, selection and mapped-language lookup check the
original SnapshotContext during their operations. Interrupted maps to Cancelled,
TimedOut to DeadlineExceeded, and other context failures to ContextDenied.
The former ProfileOperationBudget/ProfileCharge bridge is removed.
Caller-supplied selection values do not establish source or backend authority.
The constructor is `SnapshotContext::new(deadline, Cancellation) -> Self`.

## Selector and policy validation

SelectionPolicy receives caller-owned vectors; admit count and Text before own
copy/index. policy_entries counts both vectors; selector_bytes applies separately
to each policy String and FileSelector component. Duplicate casefold entries
within one vector are InvalidPolicy. Cross allow/deny duplication is allowed and
deny wins. Syntactically valid unknown entries support future custom profiles;
they do not make unknown grammars known.

No trim, comma parser, spelling repair or arbitrary suffix matching occurs.
Caller supplies basename/extension; the catalog does not parse physical paths.
Basename allows spaces but rejects NUL/length violations. Grammar and extension
identities reject ASCII whitespace at edges. Extension must be nonempty with a
leading dot. Dockerfile basename matches its declared literal under casefold.
Repeated .R/.r within one profile remain legitimate wire values and deduplicate
only within selector indexing.

## Full grammar priority and automatic selection

Explicit selection compares full grammar identities across all profiles first,
including exclusive profiles. A full match selects one profile; apply allow/deny
to all its aliases. If filtered, return DisabledGrammar with no retry of another
profile's language/bare alias. Only without a full match collect language/bare
matches and deduplicate by profile: zero UnknownGrammar, one policy evaluation,
multiple AmbiguousSelector before policy. Only known tree-sitter-/antlr- prefixes
create bare aliases. Global casefold full-grammar collision is DuplicateGrammar
at build for any profile, including exclusive profiles.

Example: language x derives tree-sitter-x; language tree-sitter-x derives
tree-sitter-tree-sitter-x. Explicit tree-sitter-x selects x regardless of order
or case. Denying any x alias returns DisabledGrammar without alias retry.
Ambiguous aliases without a full match remain ambiguous before filtering.

Automatic routing excludes exclusive profiles. Same-backend route collisions
consider only nonexclusive profiles: two exclusive profiles sharing a route are
retained; exclusive+nonexclusive retains both and automatically selects the latter;
two nonexclusive same-backend extension/basename routes return AmbiguousSelector
before final assembly. Different backends can share routes. Stable Tree-sitter
profiles precede stable ANTLR candidates without scratch sorting.

No declared candidate returns Skip(NoSelector); all existing candidates filtered
returns Skip(Filtered). Empty allow means all; nonempty allow requires any alias;
deny any alias wins in automatic and explicit modes. Known declarations do not
promise backend availability. Selection<'d> is Candidates(Vec<&'d EffectiveProfile>)
or Skip(SelectionSkip::{NoSelector,Filtered}); vector growth is fallible.
Caller-created Selection values provide no proof or authority.

Embedded declarations remain ordered. Empty mapped language is deliberate Skip;
absent key in a nonempty mapping is Skip; empty mapping permits future declared
default/language-capture behavior. Host labels, wrappers, transforms and first-claim
execution belong to the compiled consumer. No failing configuration is dropped.

The concrete API is
`Embedded::mapped_language(&self, captured: &str, &CatalogLimits, &SnapshotContext) -> Result<Option<EmbeddedLanguageDecision>, CatalogError>`.
`EmbeddedLanguageDecision` is Skip or Language(borrowed nonempty Text). None means
the map is empty, leaving capture/default processing to the future compiler.
Some(Skip) means a missing key in a nonempty map or a deliberately empty value.
Keys compare under ASCII casefold after unique-key validation. Captured Text is
NUL-free, bounded by selector_bytes, and untrimmed. Comparisons reuse the real
original context; no regex or default is executed.

## Closed errors and recovery

CatalogError is Copy and exposes code(), site(), conflicting_site(), layer_index(),
profile_index(), schema_path()->Option<&'static str>. Codes: InvalidLimits,
QuotaExceeded, ArithmeticOverflow, AllocationDenied, InvalidLayer,
DuplicateLanguage, DuplicateGrammar, AmbiguousSelector, InvalidPolicy,
UnknownGrammar, DisabledGrammar, InvalidSelector, Cancelled, DeadlineExceeded,
ContextDenied. Identity conflicts carry both actual declaration sites when present.
Constructor/policy/selector/resource errors can legitimately have no sites.
No physical path, raw source, vendor Display or owned diagnostic string is exposed.

Keep previous drafts unchanged after failure. Correct configuration or follow
the caller's input policy and original cancellation/deadline state.
Distinguish unknown, disabled, absent route and filtered candidates. Do not turn a
disabled full match into an alias retry or native backend fallback.

## Independent expected fixtures

D1 qualified 45 language IDs, 44 documented noninternal entries (45 minus plpgsql),
40 Tree-sitter, 5 ANTLR, 5 exclusive and 40 automatic-eligible declarations,
999 queries, I198844/N8276/T152825. The all-scalar audit counter152845 includes
five Bool true scalars adding20 bytes, a different metric. D2 must build all45;
the documentation category44 does not justify dropping exclusive declarations.
Internal plpgsql stays explicitly addressable and automatically excluded.

The minimal x/y fixture independently specifies every effective default,
including derived grammar and declared/default provenance. It validates the
functional shape without counting nodes, text or allocation activity.

The matrix defines required scenarios. Qualification below records executed
catalog evidence and distinguishes reused D1 producer evidence; the matrix alone
is not a claim that every producer case was replayed by D2:

| Fixture | Required evidence |
|---|---|
| P01 | All45 real decoded profiles, IDs/grammars/declarations/query order, no drops |
| P02 | Ordered merge/replacement queries and embedded ordering, unrelated profile retained |
| P03 | Missing/false/empty/Clear, removals, exports/complexity/head-call defaults |
| P04 | Real joint scalar/SDK errors, duplicate/alias/merge/tag/multidoc/depth behavior |
| P05 | Query required/pair rules and opaque malformed native pattern retained |
| P06 | SQL/dialects/COBOL/PLpgSQL/Dockerfile routing |
| P07 | Aliases/case/deny wins, unknown/disabled/filtered, ambiguity before policy |
| P08 | Old draft immutable on failure; valid subsequent changes |
| P09 | Custom tracked/Markdown declarations, full-first, global/automatic collision rules |
| P10 | Every nested getter, default, provenance, embedded empty/missing-map Skip |
| L01 | Exact decoder boundaries, zero/overflow through real producer |
| L02 | Minimal x/y complete defaults and derived/default provenance |
| L03 | Original Unicode/CRLF/BOM spans, NodeStart, colon-null/EOF distinction |
| L04 | Closed sites/indices/static paths, absent metadata without vendor matching |
| L05 | Original cancellation/deadline/context errors; fallible allocation and no partial publication |

Integration consumes real decoded values. The artificial quota/copy-counter
harness has been removed. Allocation failures are reported only when exercised;
inspection alone is not an executed allocation failure.

## Affected validation

Historical leaf A `tsk-3f5d` qualified the earlier APIs. On Linux its affected
public suites passed: profiles 10/10, catalog 14/14 and symbols 9/9.
The affected private units passed 3/3, 5/5 and 5/5 respectively. The six
symbols doctests passed, including five compile-fail lifetime cases.
Strict Clippy for the library and three affected targets passed with -D warnings.
Scoped rustfmt of the six owned files and all seven durable checks passed.
These historical results do not qualify the subsequent catalog cleanup or another platform.

```text
cargo xtask cargo test -p graphit-ast --test profiles --test catalog --test symbols --jobs 1
cargo xtask cargo test -p graphit-ast --lib profiles::tests --jobs 1
cargo xtask cargo test -p graphit-ast --lib catalog::tests --jobs 1
cargo xtask cargo test -p graphit-ast --lib symbols::tests --jobs 1
cargo xtask cargo test -p graphit-ast --doc symbols --jobs 1
cargo xtask cargo clippy -p graphit-ast --lib --test profiles --test catalog --test symbols --jobs 1 -- -D warnings
```

## Catalog accounting cleanup

Correction `tsk-fe0f` removes aggregate input/effective quotas, CatalogUsage,
shape-counting traits and their copy-counter harness. Per-profile decoder limits,
selection payload limits and all functional catalog contracts remain.
The internal routing-text validator also drops its unreachable universal cap;
callers retain the actual selector byte checks and routing syntax validation. Current
validation uses only the catalog/profile suites, catalog units and scoped lint;
results are recorded separately from the historical qualification above.
On Linux, catalog 15/15, profiles 10/10 and private catalog units 4/4 passed.
The all-fields fixture explicitly declares `source: primary`, matching its
declared-provenance assertion; the dedicated COPY overlay fixture separately
checks missing source defaults and declared preprocessor source.
Scoped rustfmt passed for 13 changed Rust files and all 16 local document links
resolve. Strict Clippy for the library and catalog/profile targets passed after
the Tree-sitter owner corrected two concurrent lints; the failed lint run is
retained. Final catalog validation also passed with the original per-profile
collection limit of 256. These results cover this correction, not end-to-end product validation.
