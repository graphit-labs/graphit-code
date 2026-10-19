---
title: Rust authenticated local Source handler
doc_type: technical
references:
  - target: {type: task, id: tsk-60a8}
    relation: implements
  - target: {type: task, id: tsk-c12a}
    relation: derived_from
  - target: {type: task, id: tsk-88f1}
    relation: derived_from
  - target: {type: task, id: tsk-2436}
    relation: derived_from
  - target: {type: task, id: tsk-e11b}
    relation: derived_from
---

# Rust authenticated local Source handler

This reference is for Rust integrators composing local file Source inspection.
All eight affected functional fixtures and strict affected Clippy passed in Main
on Linux. HTTP, MCP and CLI routing require separate consumer evidence.

Implementation: [source_handler.rs](../../crates/graphit-ast/src/source_handler.rs).
Index acquisition is specified in [local Source index acquisition](rust_ast_source_index.md).
Raw source and selection are specified in
[Rust AST and parser contracts](rust_ast_parsers.md#indexed-raw-source-snapshots).

## Composition and public API

The caller supplies the actual issuer and installation capture from the same
`DaemonAuth`, and a `ScopeInput` containing its already resolved local project
capture and version. Runtime and Local credentials alone do not bind a project;
the application must compose and validate the selected scope. Role verification
validates its supplied scope against the actual role credential. Captured scope
observations do not themselves grant access.

Three asynchronous functions return the existing owned
`Result<SourceInspection, SourceHandlerError>`:

| Function | Actual Auth producer |
|---|---|
| `inspect_source_runtime` | `RoleIssuer::authenticate_runtime` |
| `inspect_source_local` | `RoleIssuer::authenticate_local` |
| `inspect_source_role` | `RoleIssuer::verify` with the supplied `RoleCredentialReference` and scope |

Common parameters, in order, are `&RoleIssuer`, raw credential `&str`,
`ScopeInput`, requested relative `&Path`, `&text_slice::Request`,
`&SnapshotPolicy`, `SourceReadLimits`, `RetainedBytesLimits`,
`NativeDocumentFileLimits`, and `&graphit_hub::policy::RequestContext`.
The role function inserts `&RoleCredentialReference` immediately after the raw
credential. Limits and FILE policy come from the caller's existing operator
configuration; the handler provides no implicit policy or limit.

The request context supplies the sole original `Arc<SnapshotContext>`. The
handler neither accepts another request Arc nor constructs a replacement.
Borrowed issuer, scope and context must outlive the asynchronous operation.
The returned inspection owns its selected source and Core selection metadata.

Before inspecting paths or payloads, the actual authenticated caller or verified
role grant must authorize `Permission::Read(Module::Ast)`. The same actual proof
owner authorizes again before physical acquisition and final delivery. Auth
errors propagate without storing or formatting raw credentials.

## Selection and acquisition

The selected project is required. A file path must be nonempty, strictly UTF-8
for the SDK key and valid under Core's existing `RetainedFileSet` native relative
path validation. Empty segments, traversal, dot components, absolute paths and
NUL fail. The handler performs no trimming, case folding, lossy conversion or
separator normalization.

The handler opens the already resolved project's installation corpus and
selected branch using `open_local_source_index`. It retains its actual
`LocalSourceIndex` and `IndexedSourceSnapshot` throughout selection and delivery
checks. It does not reopen latest, publish source, repair storage or materialize
branches.

| Indexed observation | Result |
|---|---|
| Available nonempty or empty text | Inspect indexed raw text; do not capture the physical file |
| Explicit corpus absence, zero matching rows or matching NULL | Capture the physical file under the resolved project root |
| Authentication, scope, IO, SDK, schema, duplicate row, symlink, cancellation or deadline error | Return the actual error; do not fall back |

Physical acquisition uses the existing
`CapturedSourceReader::capture_project` with the selected live project scope,
requested relative path, `NodeKind::File`, supplied policies and the original
request. The inspector addresses the captured FILE with the empty internal path.
Physical fallback uses the runtime-resolved instance root and requested path.
The handler performs no Git, HEAD, refs, inventory or version proof lookup.
Fixture setup may observe the actual Core version producer; this is outside the
handler's acquisition flow.

`inspect_source` preserves raw text, including BOM, CRLF, NUL, multibyte content
and empty text. Core's `text_slice` applies whole-source head, tail, line range
and pattern selection. Invalid selectors and invalid physical UTF-8 remain
errors. Unknown file extensions require no parser to read their raw source.
The result records `SourceOrigin::Indexed` or `SourceOrigin::Physical` and the
existing selection result.

## Delivery and errors

The handler retains all observed owners while it revalidates the index,
physical content and original request, then authorizes ReadAst again and checks
the original request once more. It preserves the inspector's result until these
checks finish; no partial source is returned on failure.

`SourceHandlerError` preserves the existing error boundaries:

| Variant | Meaning |
|---|---|
| `Authentication(AuthError)` | Actual authentication, permission, scope or proof revalidation failure |
| `MissingProject` | No resolved local project supplied |
| `InvalidPath` | Empty path or path without strict UTF-8 representation |
| `Operation(io::Error)` | Core path, capture, content or request error |
| `Index(SourceIndexError)` | Corpus acquisition or retained indexed read error |
| `Indexed(IndexedSourceError)` | Indexed snapshot observation error |
| `Inspection(SourceInspectionError)` | Source reading, UTF-8 or selection error |

`Error::source()` retains underlying Auth, IO, index, indexed and inspection
errors. Auth's own static redacted error enum is preserved; it does not acquire
a fabricated IO cause. Storage errors cannot become an explicit miss. Recovery
belongs to the relevant producer or caller: a failed request must not widen
authority, replace the original deadline or turn an SDK failure into physical
text.

## Verification and limits

[Actual fixtures](../../crates/graphit-ast/tests/source_handler.rs) compose
persisted installation/project identities, RuntimeKeyOwner, AuthDocumentStore,
AuthStore and RoleIssuer using public APIs. Local credentials are published
through the actual store. The role fixture captures and parses a physical
AGENT.md, then registers an explicit trusted ReadAst definition and issues and
revokes its bearer through actual administration APIs. Parsing frontmatter or
observing SourceRef metadata does not automatically create a grant.

The eight fixture groups cover indexed empty/raw text against divergent
physical entries; explicit absent/NULL/file misses and raw FILE selectors; two
project identities and Default/named branches; referenced SDK data loss;
Runtime/Local denial and wrong/absent keys; role scope and revocation; path and
selector errors, cancellation and deadline; symlinks and replaced project
roots; invalid UTF-8, read limits and actual runtime key revocation.

Affected invocations are:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test source_handler --jobs 1
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test source_handler --jobs 1 -- -D warnings
rustfmt --check --edition 2024 crates/graphit-ast/src/source_handler.rs crates/graphit-ast/tests/source_handler.rs
```

The affected suite passed all eight fixtures in 0.39 seconds. Strict affected
Clippy passed with \`-D warnings\`. Initial fixture compilation and Auth store policy errors are preserved
separately from the passing run in \`.build/rust-ast-source-handler\`.
The already qualified Source inspector and index
locator establish their own foreign-original Arc checks; this handler exposes
only its request context's Arc. Final delivery authorization is present in the
implementation; no injected mid-await or mid-delivery revocation is claimed.
SDK pathname checks observe boundaries without continuous confinement.
Dataset ownership is not an external GC pin, and cancellation is cooperative
without hard SDK preemption. Consumer transport, imported contexts, entity
windows, index publication and Windows/macOS execution require separate proof.
