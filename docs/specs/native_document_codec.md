---
title: Rust native document codec
type: reference
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-427f}
    relation: supports
  - target: {type: task, id: tsk-e493}
    relation: derived_from
  - target: {type: task, id: tsk-d83c}
    relation: derived_from
  - target: {type: task, id: tsk-ced2}
    relation: derived_from
  - target: {type: task, id: tsk-bc28}
    relation: relates_to
---

# Rust native document codec

`graphit_hub::native` provides a pure in-memory `NativeDocument` for
JSON, JSONC and TOML. `tsk-427f` tracks its library validation and evidence. This page does not claim native-client qualification or product cutover. The codec is consumed by the Rust installation paths; historical codec-only evidence does not qualify every native host.

The codec supplies bounded inspection and field edits to the future planners
described in [shared native document transactions](native_shared_documents.md).
Its policy comes from [Hub resource policy](hub_resource_policy.md). Native
bindings and consumer delivery remain part of
[Hub collaboration](hub_collaboration.md). Parsing a document or changing a field
does not establish ownership of a file or authorization to publish it.

## Public data and operations

`DocumentFormat` is a closed enum: `Json`, `Jsonc`, `Toml`.
`NativeDocument::parse(format, input, limits, request)` accepts UTF-8 bytes,
explicit `SharedWriteLimits` and the captured `RequestContext`. The format,
limits, request reference and native tree are private; callers cannot replace
them or access a mutable DOM to bypass validation.

| Operation | Contract |
| --- | --- |
| `render()` | Returns a complete bounded `String`, or a static error without publishing partial output. |
| `get(path)` | Returns `Ok(None)` when the path is absent; returns `WrongType` for an incompatible intermediate or a value the managed representation cannot express. |
| `set(path, value)` | Creates missing object/table parents and sets the exact field. An existing incompatible intermediate is an error, not an instruction to replace it. |
| `remove(path)` | Removes an exact JSON property or TOML value assignment, retaining parent containers, comments and other fields. TOML Table/AoT removal is `WrongType`: callers remove their owned leaves. An absent field or parent is a no-op subject to the output bound. |

A path is a nonempty slice of literal key segments. It is not a dotted-path
parser, JSON Pointer or array-index syntax. Each segment must be nonempty and fit
the scalar-byte limit; path length must be less than the configured depth.
For example, `["mcpServers", "graphit", "command"]` selects three exact keys,
whereas `["server.with.dots"]` selects one literal key.

`FieldValue` generates only `String`, `Boolean`, `Integer(i64)`,
`Array(Vec<FieldValue>)` and `Object(BTreeMap<String, FieldValue>)`. Object keys
are literal strings. There is no managed null, float or datetime generator.
`get` performs no string/number/boolean coercion: a foreign JSON number such as
`1e1000`, a null, a TOML float/datetime or an array-of-tables cannot be converted
into an invented managed value. These native values remain in the document and
can survive unrelated edits. Inspecting a compound value fails if a descendant
cannot be represented.

The JSON root must be an object. TOML is a native document/table tree. A JSON
scalar or array root is `WrongType`. Existing TOML values can be replaced while
retaining decoration. An `Object` can replace a regular table while retaining its header decoration and position. Other generated types and array-of-tables replacement are `WrongType`.

## Explicit policy and accounting

All ten `SharedWriteLimits` fields must be positive. Policy validation checks
depth at most 80, `input_bytes + 1`, `nodes × 4 + 1`, and the relationships
`scalar_bytes ≤ input_bytes`, `scalar_bytes ≤ output_bytes` and
`entries/hook_tuples/references/documents ≤ nodes` before parsing.
The wider shared-writer packet remains explicit even though this codec does not
write a journal or enforce a batch of shared documents.

| Quota | Meaning in the codec |
| --- | --- |
| `input_bytes` | Maximum bytes accepted from the source input. |
| `output_bytes` | Maximum bytes returned by rendering or committed by an edit, including actual escaping and formatting. It is independent of the input cap, larger or smaller. |
| `depth` | Root depth is 1; nested values and containers increase depth. |
| `nodes` | Values and containers count; object/table keys do not add nodes. TOML array-of-tables containers and their tables count. |
| `scalar_bytes` | Native decoded string/key text uses UTF-8 byte counts; lexical preflight also bounds applicable scalar token text. |
| `entries` | Maximum entries in each object/table/array, including arrays of tables. |

These are per-document/tree quotas. They do not debit the request's content
renderer or snapshot data-I/O counters. The codec nevertheless checks the same
captured request cancellation and deadline throughout parsing, traversal,
mutation validation and output. Repeated edits do not create a fresh deadline.
`Interrupted` and `TimedOut` remain distinguishable. These checkpoints do not
make upstream synchronous parser calls preemptible or impose a hard RSS bound.

## Native preflight and parsing

The source byte cap and UTF-8 validation precede tree construction. Native
scanners and parser events apply token, depth and scalar bounds before the
editable DOM/CST; an exact semantic walk then applies node, collection, key and
value quotas to the native representation.

JSON preflight uses the native scanner with a token ceiling of `4 × nodes`.
The AST walk checks decoded duplicate keys before constructing the editable CST.
Strict JSON rejects comments and trailing commas. JSONC enables only comments
and trailing commas beyond JSON: loose property names, missing commas,
single-quoted strings, hexadecimal numbers, unary plus, bare decimal points,
non-finite numbers and extended string escapes remain disabled.

TOML preflight collects at most `4 × nodes + 1` lexer tokens to detect overflow,
uses the push parser with a recursion guard derived from `depth - 1`, and counts
decoded UTF-8 key/scalar text through a bounded `StringBuilder`. Only then is
`DocumentMut` constructed and walked, including tables and arrays of tables.
Foreign numeric literals and native datatypes stay in their native nodes;
there is no intermediate JSON conversion that rounds, stringifies or erases
them.

The selected dependencies have default features disabled:

| Dependency | Exact version | Enabled features |
| --- | --- | --- |
| `jsonc-parser` | 0.34.0 | `cst` |
| `toml_parser` | 1.1.3 | `std` |
| `toml_edit` | 0.25.15 | `parse`, `display` |

These pins come from the qualification in `tsk-d83c`. That qualification does
not certify this codec's consumers or the native clients receiving its output.

## Transactional in-memory edits and preservation

Before cloning generated values into native nodes, managed-value validation
checks depth, nodes, collections and scalars and computes a checked raw output
lower bound. Escaping can increase the result, so this early bound supplements
the exact output check rather than replacing it.

Inspection and the internal edit copy use a byte cap of `max(input_bytes, output_bytes)`. A source admitted above the output cap can therefore be inspected or shrunk by a successful edit. Public rendering and the final edit candidate still obey `output_bytes`.

A set reparses the bounded internal text into an independent candidate, changes the native node, renders through the output writer, reparses and revalidates the result, and checks the request before swapping the private tree. A CST clone alone is insufficient isolation:
the upstream JSONC CST shares mutable `Rc` nodes. An error leaves the previous
document intact.

The output writer checks the exact next byte count before appending each chunk.
An oversized or canceled render returns no partial `String` to the caller.
This bounds published output, not every upstream allocation or allocator
overhead. Candidate construction can temporarily retain the old document,
rendered text and candidate tree.

JSONC edits operate on native property nodes; untouched foreign literals,
comments and order remain native content. TOML replacement clones the existing
value's decoration and mutates its existing `Item`, preserving the key and its
position rather than removing and reinserting it. Unrelated native tables,
datetimes, float representations and arrays of tables are not rebuilt as
managed values.

Removal uses native AST/key/value spans to delete assignment syntax and native scanner/lexer tokens to select the necessary comma. It retains attached and embedded comments, including first, last and only entries, and revalidates before committing. It does not call upstream removers that discard trivia, strip comments with regular expressions or parse a second grammar. TOML dotted assignments are located through native visual key paths.

`Object` replaces the entire selected object. Shared-writer consumers must patch owned leaves to retain user extensions within an object. Missing parents inside a TOML inline table are generated as native inline tables. JSON CST preserves unchanged CRLF content; TOML native parse/display may normalize line terminators to LF. TOML output is not promised to be byte-identical to CRLF input.

## Errors and delivery boundary

`DocumentError` is a fixed enum: `InvalidPolicy`, `InvalidEncoding`,
`InvalidDocument`, `DuplicateKey`, `LimitExceeded`, `InvalidPath`, `WrongType`,
`Interrupted`, `TimedOut`. Debug/Display expose these categories without vendor
diagnostic excerpts, source snippets, keys or submitted values.

| Contract evidence | Source |
| --- | --- |
| Private document, typed paths and managed values | `crates/graphit-hub/src/native/document.rs`: `NativeDocument`, `DocumentFormat`, `FieldValue` |
| Pre-parser and semantic quotas | Same file: `validate_policy`, `json_preflight`, `json_validate`, `toml_preflight`, `toml_validate` |
| Candidate isolation and bounded publication | Same file: `edit`, `validate_field`, `render_tree`, `Limited` |
| Native field preservation | Same file: `json_edit`, `toml_edit_field`, `remove_field`, `json_removal`, `toml_removal` |
| Codec fixtures | `crates/graphit-hub/src/native/document/tests.rs` |
| Qualified pins | `crates/graphit-hub/Cargo.toml` |

The codec performs no filesystem I/O, lock acquisition, ownership adoption,
whole-file journal, conditional file replacement or native-client protocol.
Shared writers still need their own cooperative concurrency, staging,
reconciliation, permission and recovery contracts. No benchmark gain, product
integration follows from this library validation.
