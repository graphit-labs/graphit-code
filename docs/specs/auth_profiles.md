---
type: specification
title: Rust authentication provider and profile data
references:
  - target: {type: task, id: tsk-5fb0}
    relation: implements
  - target: {type: task, id: tsk-63a3}
    relation: implements
  - target: {type: task, id: tsk-b541}
    relation: implements
  - target:
      type: task
      id: tsk-a1a9
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: implements
  - target:
      type: task
      id: tsk-3586
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: implements
  - target:
      type: task
      id: tsk-2f95
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: implements
  - target:
      type: task
      id: tsk-e57d
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: relates_to
---

# Rust authentication provider and profile data

This contract serves maintainers of `graphit-auth::profile` and consumers of the [private authentication store](auth_store.md). The Rust library implements bounded provider/profile data and in-memory compare-and-swap transitions. See [Authentication providers and account profiles](../guides/authentication.md) for login workflows. Profile data operations do not themselves perform login, token verification or runtime credential selection.

A stored username, subject, issuer, token, provider or selected profile is data. Neither decoding nor activation creates a verified Broker identity, authenticated caller, grant, RoleParent, role bearer or storage authorization. An authentication producer must validate credentials using the Broker client and the appropriate role authority before connecting this data to a credential consumer. `replace_profile_data` intentionally has no `login` or `authenticated` API.

## Fresh document and capacities

The only accepted envelope has `format: "rust-auth-v1"`, `epoch: 1`, a positive `document_revision`, `providers` and `profiles` arrays, and optional omitted `active_profile` and `web_hub_administrators` fields. There is no legacy-format reader, converter or implicit provider configuration. Null, unknown fields, duplicate object fields, duplicate provider/profile names, trailing content, wrong field types and invalid revisions are rejected. Optional fields are omitted rather than represented by null or empty strings.

`web_hub_administrators` is private installation policy for
[Web Hub FILE mutations and publication](web_hub_mutation_authority.md). Its
closed entries use exact provider/issuer/subject and nonempty distinct
`project`/`global` scopes. Omission or [] denies mutation. Decoding policy alone
does not authenticate or authorize a caller; the retained Auth proof and
PROJECT or remote permissions remain separate. Provider/profile mutations
preserve this policy. The existing collection and scalar bounds apply; there
is no PROJECT settings override or HTTP policy setter.

`AuthDocument::decode(bytes, limits, context)` accepts an explicit `ProfileLimits` and the original `graphit_hub::policy::RequestContext`. Every capacity must be positive:

| Capacity | Meaning |
| --- | --- |
| `bytes` | Input bytes and emitted document bytes; checked addition and representable allocation bound |
| `nodes` | Typed values, containers and JSON field names |
| `scalar_bytes` | UTF-8 bytes in each string, secret and field name |
| `collection_entries` | Fields per object and entries per array |
| `providers`, `profiles` | Additional limits on the respective record arrays |

The schema fixes nesting depth. Decoding uses typed Serde seeds in one parse, with no `serde_json::Value` intermediate or second parse. Field duplication and unknown fields are rejected before parsing their values. Scalar limits are checked before retaining owned strings or secret copies. Extra array elements are refused before decoding or allocating them. Vectors reserve bounded increments and are sorted by name without duplicating map keys. Lookup uses binary search. Trailing content is detected by ending the same deserializer; previously decoded secret fields are dropped on failure.

Serde JSON may retain temporary scratch while decoding an escaped string; this scratch is bounded by the admitted input byte limit and is not guaranteed to be wiped. The caller owns the input buffer and must manage its lifetime and zeroization. This contract does not claim all transient parser or allocator memory is erased.

The explicit `ProfileLimits` bound the typed document independently of Hub transport content limits. Cancellation and the original Hub deadline are checked at scalar, node, collection and writer boundaries. Physical storage I/O uses the original shared snapshot budget through AuthStore. Static typed errors contain no document contents, URLs, keys, tokens or parser diagnostics. No API loads configuration, starts a network request or creates a new deadline.

## Provider configuration

A provider record contains `name`, its own positive `revision` no greater than `document_revision`, UTC microsecond `created_at`/`updated_at`, and a typed configuration. Names must be nonempty without surrounding whitespace, control characters or path separators. Timestamps are nonnegative signed 64-bit microseconds; updates cannot precede the previous timestamp.

| Configuration | Preserved behavior |
| --- | --- |
| Local | Explicit AWS credential-chain permission; optional Broker service configuration |
| Broker | Endpoint and optional canonical MCP resource; Broker S3 credential source is required; anonymous login is rejected |
| AI Local | Explicit device `auto`, `cpu`, `cuda` or `metal`, with nonnegative `device_id`; CPU is the default |
| AI Disabled | No execution or direct-service fields |
| AI Broker | Discovery supplies service information; no direct-service fields |
| AI Direct | Typed protocol, literal endpoint/model, required `model_revision` and `recipe`, dimensions, optional OpenAI request dimensions and required Google batch endpoint |
| S3 location | Bucket, region, endpoint, prefix and typed source: local credentials, AWS chain or Broker scope |

For Local providers, optional Broker configuration is independent of the two AI service selectors. Broker providers select only Broker or Disabled inference services. Disabled and Direct AI remain subject to their own validation; selecting AI Broker requires a configured Broker. This permits Local Broker inventory credentials while AI is disabled or uses its own Direct keys. [Inventory service credentials](broker_inventory_service_credentials.md) require a static Local Broker key and a separate purpose admission. A local provider may explicitly allow anonymous Broker AI calls; this is not a login or grant. Its Broker service cannot configure an MCP resource. Broker providers require the Broker-scope S3 source and cannot allow anonymous login. Local providers cannot select the Broker-scope S3 source; AWS-chain selection requires explicit permission.

Service URLs must be absolute HTTPS or HTTP loopback, without URL credentials or fragments. MCP resources also reject query strings. Direct embedding supports `openai`, `openai-compatible`, Cohere, Voyage, `google-gemini-1` and `google-gemini-2` with positive exact output dimensions. Retired `openai-embeddings-v1` and ambiguous `google` values are rejected without aliases or conversion. Native rerank supports Cohere/v2, Voyage/v1 and Jina/v1 with zero dimensions. Embedding-only protocols and positive rerank dimensions are rejected before configuration publication. Disabled, Broker and Local variants cannot carry direct-service fields because the type and wire schema reject them.

Direct OpenAI configuration carries `request_dimensions: Option<u32>`. When present, it must be positive, equal to the expected output dimensions and smaller than the supported model's native width: 1536 for `text-embedding-3-small` or 3072 for `text-embedding-3-large`. It maps directly to the SDK's optional output-dimension reduction; it is not input-text truncation. Omission means no request extension. Compatible and other protocols reject this field.

Google family is explicit in the protocol, not inferred from a model name. Its `model` is the literal `models/name` resource with a nonempty ASCII alphanumeric/`-_.` name. `endpoint` and required `batch_endpoint` are supplied literally, ending in that resource plus `:embedContent` and `:batchEmbedContents` respectively. Both use the same origin and no query; no route is derived. Other protocols reject a batch endpoint. The typed codec and prospective output validation retain both options under the existing input/output bounds. These fields are captured and revalidated by the actual [service lease](service_credentials.md); configuration data is not authority.

Direct `model_revision` is the provider owner's explicit pinned model or deployment revision, and `recipe` identifies the reproducible input/output processing contract. Both are required nonempty literal strings under the existing scalar bounds; surrounding whitespace and control characters are rejected. A provider model name alone does not identify weights. Graphit does not invent a local revision or SHA for external services. The profile codec and actual Direct service binding preserve both fields and reject a policy/profile mismatch. Neither field is sent as an invented provider API parameter.

Local execution uses `LocalDevice` / `LocalExecution` and the canonical JSON `{"mode":"local","execution":{"device":"cpu","device_id":0}}`. The old `onnx` field and `coreml` device are rejected, without aliases or conversion. Metal explicitly selects the native GGUF device on macOS with device ID zero, subject to the model backend's applicability checks. Auto on macOS also requires device ID zero. These are platform configuration checks; they do not assert a runtime, GPU or model is available. Declaring a provider's rerank service does not enable reranking; its execution setting remains opt-in.

## Profile data and secrets

A profile record has independent positive `revision`, `token_revision` and captured `provider_revision`, creation/update timestamps, and a typed body. Each record and captured-provider revision must be no greater than the document revision. Decode, encode and prospective counting all enforce this bound, including stale records retained as data. Name and provider are common selectors. The complete Local/Code `ProfileData` body keeps its required username, optional organization and teams; `BrokerDeviceProfileData` has optional signed metadata. The stored credential variants are closed:

| Variant | Stored data |
| --- | --- |
| Local | Explicit anonymous assertion; optional MCP key, Broker key, embedding/rerank API keys and local S3 credentials or AWS profile |
| Broker Code | Issuer and subject assertions, access token, optional refresh token, ID token, expiry and explicit S3-disabled metadata |
| Broker Device | Issuer and subject assertions, access token, Unix-second expiry and explicit S3-disabled metadata; optional signed display metadata, no ID or refresh token |

Local anonymous data requires username `anonymous`, no organization or teams, no static Broker key, and explicit anonymous Broker configuration on its current provider. Named local issuer/subject assertions are derived from provider name and username; the anonymous assertion distinguishes them from issuer/subject `anonymous`. This preserves the active CLI anonymous mode without storing duplicate derived local identity strings.

Local S3 requires access key ID and secret key together; a session token requires that pair. A configured local S3 bucket requires local credentials, an AWS profile or explicit provider permission for the AWS chain. Direct AI mode requires the corresponding API key. Local Broker AI requires a static Broker key unless anonymous calls are explicitly configured. Local profiles cannot carry OIDC sessions. Broker profiles have no fields for static MCP/Broker keys or S3 grants: temporary Broker STS credentials cannot enter this schema, even as a placeholder. Stable storage location belongs to provider configuration.

Wire OIDC token type is exactly `Bearer`; token strings and issuer/subject are structurally validated data, not signature or grant proofs. Expiry is stored, but this module does not authenticate or authorize unexpired tokens. The Broker verifier owns those decisions.

`Secret` wraps `secrecy::SecretString` and has no `Clone`, `Debug`, public `Serialize` or `Deserialize` implementation. Deliberate `.expose()` is available to future bounded credential consumers. Record/document types containing secrets also have no automatic cloning, logging or public Serde. Compile-fail doctests enforce the secret container boundary.

`encode` returns `Zeroizing<Vec<u8>>`. Its first typed traversal validates/counts output without retaining secret bytes. It then reserves the complete encoded size before emitting any credentials and performs a bounded second typed traversal, with no reparsing and no plaintext-buffer reallocation. The writer returns `io::ErrorKind::Other` on cancellation, deadline or limit: `Interrupted` would cause Serde's retry behavior to hang. Bounded subprocess fixtures test all three failures.

## Data lifecycle and compare-and-swap

Every effective transition requires the exact current document revision and increments it with checked arithmetic. Record/token revision overflow, stale expectations, backward timestamps, context failures and prospective document capacity failures occur before changing semantic state. The same counting writer preflights the entire prospective document without cloning secret data. Allocations needed for the final insertion happen after validation and remain bounded.

| Operation | Result |
| --- | --- |
| `add_provider` | New name only; birth revision equals the prepared new document revision, with captured timestamps |
| `update_provider` | Requires exact provider revision; preserves creation time, increments provider revision and retains dependent profile data with its previous bound revision |
| `remove_provider` | Referenced providers conflict unless explicit cascade is supplied; cascade removes dependent profiles and clears their active selector |
| `ensure_default_local` | Creates only the `local` provider with independent CPU/0 AI settings; does not create profiles, keys or login; preserves existing explicit local settings and conflicts with a Broker named local |
| `replace_profile_data` | Requires expected document/provider/profile revision; new profile and token birth revisions equal the prepared new document revision; updates preserve creation time and increment existing profile/token revisions; does not activate or authenticate |
| `replace_broker_device_profile` | Replaces the typed Device body under expected document/provider/profile revisions and current limits; does not activate or authenticate |
| `activate` | Selects an existing profile bound to the current provider revision; a changed selection increments only the selected profile revision; produces no authority |
| `active` | Returns borrowed current data; missing, stale or absent selection fails without falling back |
| `logout` | Removes the explicitly named profile, or the selected profile; stale data can be removed without resolving its provider |
| `rotate_tokens` | For a Code profile, requires exact document/provider/profile/token revisions and the same active profile; replaces access/rotating refresh tokens atomically and applies the selected identity update in the same transition |

Provider updates deliberately leave stale records available for list/logout. Decoding permits a missing provider or missing active selector and retains that data; use/active operations reject it. Cross-provider credential compatibility is checked for current records and new writes. A stale local profile can therefore survive a provider's change to Broker type without becoming a Broker identity. Own record revisions remain positive and no greater than the document revision; there is no legacy zero-revision invalidation convention.

Birth revisions identify record incarnations; they are not counters that restart at one. Removing and recreating a provider with the same name, configuration and timestamps gives it a later own revision. Logout followed by recreation likewise gives the profile and token generation later birth revisions, even when the credential bytes and identity fields are identical. The document counter never rolls back within these typed transitions.

An actual selection change (`None` → A, A → B, B → A) increments the selected profile revision with checked arithmetic. Returning to A therefore invalidates its previous own binding even when no consumer observed B. Activation preserves the token and captured-provider revisions, credentials, identity claims and both timestamps. Selecting A again while A is active preserves all own bindings, while retaining the document operation fence. Editing unrelated B also preserves A's own binding; consumers may inspect a fresh physical snapshot for a new authorized operation, but cannot reuse or rebind an old AuthSnapshot. The document revision remains the whole-document CAS fence and is separate from those lifetime bindings.

The prospective activation writer counts both the selector and the selected profile's increment before mutation, including decimal digit growth. A valid document at `u64::MAX` can be decoded and encoded, but every typed mutation fails in `prepare` before effects. An own revision above the document revision is malformed data, not a reachable independent counter-overflow case. No incarnation field, active epoch, importer or compatibility path is added.

The live service selection exposes `NativeProfileRevision` as checked non-secret provider/profile names, issuer/subject and `TokenExpectation` revisions. It grants no refresh or Records authority. The daemon Records resolver uses this exact selection to coordinate demand renewal through the existing administrative profile manager, then captures a new service lease. The optional Records preflight performs the same preparation before Native USER authentication; resolvers without renewal support leave normal authentication checks authoritative.

For Code profiles, `TokenUpdate.identity` is a closed choice. `ProfileIdentityUpdate::Preserve` retains the ID token, username, organization and teams exactly. `Replace { id_token, claims: ProfileClaimsUpdate { username, organization, teams } }` replaces those fields together with the access and rotating refresh tokens. Organization may be removed and teams may become empty; duplicate team entries remain valid data. The profile name, provider binding, issuer, subject, creation timestamp, S3-disabled metadata and active selector stay unchanged. Document, profile and token revisions increment once; the provider revision stays unchanged.

Replacement validates the new ID and claims, then counts their actual prospective bytes, nodes, scalars and collections, including escaped output and organization presence. All four comparisons, provider freshness, active selection, timestamps, checked revision increments and final original-context checks precede the first mutation. No second profile replacement or secret-document clone is needed. The enum couples the data fields but supplies no cryptographic identity proof: an authentication consumer must verify the issuer, subject and token purpose before constructing it.

An in-memory CAS is not a durable cross-process publication guarantee. The implemented [AuthStore bridge](auth_store.md) forwards the private-file observation and CAS publisher from `graphit-core::auth_document`, preserving cancellation, physical privacy, publication effects and crash-cleanup proofs. Its mutation closure finishes the typed transition before a single physical publication; concurrent physical replacement remains a separate conflict. Physical roundtrip tests cover joint ID/claims/token replacement and show that each failed revision comparison writes no bytes or staging file. UI cookie sessions, OIDC login/refresh, role emission, CLI account commands, scoped STS and daemon consumers retain their own authentication and authorization contracts.

## Verification and source coverage

Focused commands:

```sh
cargo test --locked -p graphit-auth --lib profile::
cargo test --locked -p graphit-auth --lib store::
cargo test --locked -p graphit-auth --doc
cargo clippy --locked -p graphit-auth --all-targets -- -D warnings
cargo xtask check
```

`profile/tests.rs` covers complete Local/Broker roundtrip, current field inventory, fresh format and hostile JSON, exact byte/node/scalar/collection/record boundaries, cancellation/deadline, data lifecycle, stale/missing selection, explicit cascade, creation/update timestamps, revision overflow, prospective-document limits, independent token CAS, account switching, preserved ID and claims, joint ID/claims replacement, prospective identity limits, rejected replacement with unchanged bytes, explicit local anonymous data, local execution/platform validation, all active direct protocols and redacted static errors. `store/tests.rs` covers the physical joint-replacement roundtrip and four failed comparisons with unchanged file bytes and I/O counters. Lifecycle fixtures additionally cover same-name provider/profile recreation, A→B→A, None→A, repeated A selection, unrelated edits, exact/one-under activation digit-growth output, future-own revision rejection, and valid document MAX rejection before effects. Real AuthStore fixtures recapture and reload between transitions, reject old snapshots and CAS expectations, and show capacity/MAX denials with zero written bytes and no stage. The pre-fix ABA counterproof compares only the own binding tuple; the old complete document CAS already rejected an earlier document revision. The ignored writer child is invoked by its nonignored timeout harness; it is not an unexecuted external integration claim.

Implementation owners: `crates/graphit-auth/src/profile.rs` and `profile/codec.rs`. The user guide above describes authentication workflows. Data validation and CAS do not replace token verification or credential admission. Ownership avoids whole-document cloning and unnecessary retained buffers; no throughput or resource benchmark is claimed.

## Native administrative consumer

[NativeProfileManager](native_profile_manager.md) now applies these typed data
operations to the actual private issuer store under finite administrative or
verified Role ManageAuthProfiles authority. Its redacted views expose no raw
profile credentials. Local installation replaces only supplied credentials
and activates in one publication; omitted keys disappear and no key is minted.
The data schema itself still grants no authority. Broker OAuth/refresh and the
CLI offline producer remain separate required integration work.

## Device access-only profile

The [Native Broker Device login contract](native_broker_device_login.md) defines the separate `--device` journey, typed `broker_device` profile, optional metadata, actual-expiry service use and local logout. Its implementation is under qualification under tsk-5fb0. Existing Local username and Code ID/refresh requirements apply to their complete `ProfileData` body; `ProfileRecord::data()` returns that optional body, while `name()`/`provider()` select either body. No Device username, ID token or refresh token is fabricated.
