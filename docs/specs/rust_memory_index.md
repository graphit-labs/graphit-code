---
title: Explicit Memory index maintenance
doc_type: spec
description: Authoritative PROJECT and verified USER Memory index maintenance through the Rust CLI and Source MCP.
references:
  - target: {type: task, id: tsk-9d92}
    relation: implements
  - target: {type: task, id: tsk-67e7}
    relation: uses_api
---

# Explicit Memory index maintenance

`memory index` and the Source MCP tool `graphit_memory_index` maintain the
authoritative Memory indices. The default scope is PROJECT. `--scope user`
selects only the verified Native user's Memory; it rejects PROJECT directory
and version selectors. The Root MCP proxy preserves that explicit USER scope
even when its current PROJECT is selected.

```sh
graphit memory index
graphit memory index --refresh
graphit memory index --scope user
```

The equivalent MCP input is `{ "scope": "project", "refresh": false }`.
`project_dir` (NativePath) and `project_version` are optional PROJECT selectors.
They are not authority. The CLI validates its typed input before daemon
autostart and sends the same prepared operation to Source.

## Admission and configuration

The effective Memory module must be enabled before a corpus is captured or
written. PROJECT uses the live id/root/version and ordinary Memory write
admission. USER uses the active Native profile's signed JWT, trusted discovery
and JWKS cache, its verified issuer/subject and ordinary USER Memory write
admission. A Runtime key alone is insufficient to identify a USER. Display
names, input selectors, Role grants and fenced-only permissions cannot stand
in for these admissions.

The local Runtime's ordinary permission ceiling includes Memory writing and
USER read/write capabilities independently of embedding configuration. The
consumer still applies the selected module, actual subject and corresponding
record admissions. This supports a daemon started without PROJECT selections
that later maintains a checkout with Memory enabled. It does not widen Role,
fenced, Incoming or Web permissions.

Native JWT authentication uses the daemon's existing shared verifier owner.
Obtaining a cached verifier does not authenticate or authorize a caller.
`authenticate_native` independently verifies the active profile/binding and
access JWT. No UserInfo or token refresh call is required by index maintenance.
Remote revocation, account blocking and claim changes may remain unobserved
until the access JWT expires. Local profile and binding changes retain the
SDK's physical rechecks. This operation does not introduce a token lifetime.

## Lexical and semantic results

The response contains `scope`, `lexical` and `semantic`; PROJECT also identifies
the selected project id and version. Lexical `state` is one of:

- `no_corpus`: no Memory corpus exists. Both `lexical.state` and
  `semantic.state` are `no_corpus`; no model inference is attempted.
- `ready`: Ensure observed an existing compatible index, with its actual SDK
  generation. It does not commit another index.
- `published`: Ensure created a missing index, or explicit Refresh replaced a
  ready index. The result contains the actual generation and recovery key.

Refresh is explicit. An existing corpus with a missing or incompatible index
returns the SDK's causal `lexical_index_unavailable` result where Refresh is
not valid. Read-only Memory queries do not rebuild an index. Ordinary and
genuine fenced CRUD writers maintain their own once-initialization contract;
this operation does not manufacture ordinary authority for those writers.

With an existing corpus and no configured embedding service/default selection,
lexical maintenance completes and semantic `state` is `not_configured`. The
selected Local, Broker or Direct service resolves one portable vector identity.
Local borrows the existing model client, expectation and child. Remote execution
uses the admitted HTTP service without opening a local manifest or process.
The consumer supplies finite embedding operations through the shared dispatcher;
corpus read/write admissions stay bound to the original request. The Records cursor skips already covered revisions; inference
is only requested for documents that need coverage.

Semantic `complete` contains the source generation and `last_attachment`, if
any. Each SDK attachment is independent; that generation is not a receipt for
an atomic whole-index transaction. A configured service without a compatible
available owner fails causally. A remote provider is not silently served by a
residual local model slot.

Errors after lexical publication retain the lexical result under `result`.
Known index/embedding publications retain their generation and recovery key;
uncertain SDK commits retain their recovery key without claiming publication.
Embedding failures retain the last observed attachment. Cancellation uses the
same finite Original and the existing Source worker owner so its SDK commit
barrier can preserve those effects. No polling, retry inference, compatibility
importer or alternate model is added.

Once the CLI has received a Source response, a cancellation, deadline or
connection-cleanup failure still returns an error with the original error
kind. Its diagnostic retains the complete received tool result, including
known generations and recovery keys, together with the delivery causes.
Nothing is written to stdout on this error path. Cancellation before a
response is observed cannot report an unobserved publication.

## Consumer validation

`source_server::memory_index::tests` exercises actual Source HTTP through the
Root MCP proxy: ordinary PROJECT CRUD/Ensure/Refresh/FTS, disabled-module and
fenced-only refusals, and signed Native USER identity with a PROJECT selected.
It also checks warm discovery/JWKS reuse, an unavailable UserInfo endpoint and
rejection of a token intended for another service. Its configured-model case
reuses the existing Memory model fixture and child, including preservation of
a lexical publication when subsequent inference fails.

The compiled CLI journey is in `tests/source_executable.rs`; it uses the real
Foreground daemon and installation rather than a replacement Source fixture.
It covers PROJECT NoCorpus with actual id/version selection and global USER
Ensure/Refresh NoCorpus after activating a signed Native profile while the
daemon is already running. A subsequent CLI invocation reuses discovery/JWKS.
The missing-subject and mixed USER/PROJECT selector refusals remain covered.
The signed Broker fixture uses the existing explicit development option
`broker.allow_loopback_http=true`, captured before startup; the production
default remains unchanged. USER Ready/Published are qualified by the MCP
consumer; configured CPU inference is qualified by its PROJECT model case.
Evidence and any outstanding qualification are recorded in the existing
migration task, not inferred from library compilation.

The Memory index entry module and its PROJECT/USER `Scope` DTO select Linux,
macOS and Windows, composing the same authenticated Source request and
delivery/cleanup code. The affected Windows consumer check is pending; native
Windows/macOS execution remains unqualified.
