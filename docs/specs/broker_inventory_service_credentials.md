---
title: Broker inventory service credentials
content-type: reference
audience: developers
status: implemented
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrected_by
  - target: {type: task, id: tsk-e07a}
    relation: implements
  - target: {type: task, id: tsk-3582}
    relation: uses_api
  - target: {type: task, id: tsk-7dd5}
    relation: uses_api
  - target: {type: task, id: tsk-749e}
    relation: implements
  - target: {type: task, id: tsk-b541}
    relation: implements
  - target: {type: task, id: tsk-9d27}
    relation: derived_from
---

# Broker inventory service credentials

The Auth SDK admits a finite Broker inventory operation using the existing
`RoleIssuer` and an actual authenticated caller. Runtime/Local callers may use
an active Local provider/profile with `LocalBrokerKey`; an active Broker
provider/profile supplies `NativeAccess` to the Runtime caller.
`ServicePurpose::InventoryRead` and `InventoryPublish` are separate from AI
Embedding/Rerank. Configuration and a borrowed key never grant the Broker's
server-side Auth, Read or Publish permission. This SDK performs no inventory
HTTP request, login, token refresh, credential publication or retry. Native JWT
verification uses the existing trusted discovery/JWKS cache when needed.

## Startup and capture

Trusted startup supplies an existing `ServiceAuthorityInput` with a Remote
rule whose binding is `RemoteServiceBindingInput::BrokerInventory { provider,
purpose, endpoint }`, credential is `LocalBrokerKey` or `NativeAccess`, origin
is Runtime or Local, and incoming is None. Provider is the literal persisted provider name;
endpoint is the configured Broker base URL. Duplicate origin/provider/purpose
rules are rejected. Empty or absent purpose rules deny admission.

`capture_service(&caller, purpose)` requires the actual caller's same-issuer
authority, original physical observation, live source, and
`External(RemoteRequest) == Unconditional`, plus the exact purpose rule. Role
and Incoming capture entrypoints deny both inventory purposes before loading
the local AuthStore. The old Local, Direct and Broker MCP bindings reject
inventory purposes at construction. Inventory bindings reject AI purposes,
Incoming origin, DirectKey, Anonymous or IncomingBearer.

LocalBrokerKey selection requires an active `ProviderKind::Local` with Broker
configured; NativeAccess requires an active `ProviderKind::Broker`. AI
selectors do not participate: Disabled and Direct configurations can coexist
with Broker for both provider kinds under their existing individual validation.
AI Broker still requires Broker configured. ONNX execution supplies no admission.
Local authentication itself requires an active Local profile and its MCP key;
it cannot coexist with an active Broker profile in the same AuthStore. Local
NativeAccess combinations therefore deny, rather than using another AuthStore,
changing the active profile or falling back to a key. Foreground NativeInventory
must authenticate through the real Runtime owner.

For LocalBrokerKey, the selected `ServiceMode` is BrokerKey. `credential()` borrows the real
profile `broker_key`; no key or an empty key returns MissingCredential. An
anonymous-capable provider never causes fallback. Direct AI keys are never
used for inventory. The SDK does not assert that an opaque key is current,
unexpired or owned by a particular server principal.

For NativeAccess, mode is BrokerNative and consumers use the existing
`native_credential(&BrokerVerifier).await`. It verifies the signed NativeAccess
from the actual Broker profile, bound to its stored issuer/subject, registered
client, intended audience, configured resource when present, time claims and
current provider revision. The native verifier must match the literal
BrokerService endpoint/resource/revision from the captured provider configuration;
the canonical inventory URL does not replace the literal credential issuer.
The same Original and policy/source/profile/physical gates are checked before
and after the await. NeedsRefresh remains a typed result, never automatic
refresh. The stored renewable OIDC expiry is interpreted as UTC microseconds;
Device expiry remains Unix seconds. The [profile refresh contract](native_broker_profile_refresh.md#caller-contract)
defines these distinct body units. Expired or near-expiry renewable profiles
return `NeedsRefresh` before JWT I/O, letting an explicitly authorized consumer
renew before an unsent inventory request. This does not reclassify invalid JWTs
or authorize retry of a possibly applied request. UserInfo is not required; a
valid signed JWT remains valid until expiry.

## Binding and limits

The URL SDK normalizes both the startup binding and configured Broker URL.
Both require HTTP/HTTPS and a host, with no URL credentials, query or fragment.
Text must be nonempty, unpadded and contain no control characters. Existing
profile validation still restricts HTTP to loopback and retains all provider,
MCP resource, S3 and credential requirements.

Only URL SDK normalization defines equivalence, such as host case, default port
or dot segments. Path and trailing slash remain significant when the SDK keeps
them distinct. The profile is never rewritten. The lease exposes
`ServiceBindingView::BrokerInventory { provider, purpose, endpoint,
provider_revision }` with the retained canonical endpoint and captured revision.
The existing Broker MCP binding keeps its literal endpoint/resource semantics;
an MCP resource is not converted to an inventory audience or scope.

Positive `max_rules` and `max_binding_bytes` bound actual rule input and
cumulative binding text; issuer `max_scope_bytes` bounds each input string.
Raw URL text and canonical expansion are validated before copying. There are
no issuer capacity charges, retained-byte ledger or inventory reservations.

## Borrow and live gates

The selection retains the same original request and AuthStore observation.
`credential`, `binding`, `original`, `user_subject` and `check` on the lease
repeat the existing authority, policy-generation and physical-store gates.
Key credentials expose no verified user subject; verified NativeAccess exposes
the real Native subject through the existing borrowed proof. Revisions never rebase and
the original cancellation/deadline is never replaced. Policy revocation,
caller-source changes and changed physical provider/profile/document state
invalidate use. The existing private lease has no Clone, Debug, Serde, restore,
authority conversion or owned credential factory.

Consumers must use the real lease gate before effects and after awaits. A raw
borrow cannot enforce permission at native send; the HTTP consumer must retain
its own real transport gates and distinguish not-sent from may-have-applied.
Server rejection and uncertain publication are not authorization to replay.

Errors retain the existing Auth, Store, Broker and Profile categories, plus
Disabled, MissingCredential, NeedsRefresh and WrongMode. Cancellation/deadline
retain Profile categories. Configured inventory URLs rejected by the new
grammar return typed Auth errors. Remote-only admission with no active profile
denies; it does not ensure a default or create an account.

## NativeAccess qualification

`tsk-e07a` extends the existing SDK with NativeAccess inventory rules. Four
focused cases passed using actual TCP/JWKS signatures and Core/AuthStore
fixtures: Runtime read/publish, exact verifier/provider tuples, signed claims
and refresh margin, and authority/profile/cancellation/deadline changes during
JWKS await. The current native-credential filter passed seven cases (these four
and three existing guards); all 17 public inventory integration cases passed.
Affected Auth library/tests Clippy with warnings denied, scoped formatting,
diff checks and all 14 documentation links also passed.
No UserInfo, login, refresh or credential publication is introduced.

Results are recorded in .build/rust-auth-native-inventory-20261011/. These do
not qualify the separate Runtime GET/PUT Native caller or public Broker SQL/JWT
endpoint. LocalNative remains denied by the active-provider invariant; the
LocalBrokerKey receipts below keep their original scope.

## Evidence and scope

Implementation: `crates/graphit-auth/src/issuer/service_credentials.rs`, narrow
provider validation repair in `crates/graphit-auth/src/profile.rs`, and public
native fixtures in `crates/graphit-auth/tests/inventory_services.rs`.
Prior Linux native runs in tsk-b541 recorded 17 public inventory integration
tests, 21 focused S1 service tests, eight verified-subject tests and 174 affected
issuer tests passing. These receipts precede the JWT-local contract revision in
tsk-3582; the interrupted pipeline did not qualify Profile, rustdoc or strict
Clippy. The current producers tsk-3582, tsk-749e and tsk-7dd5 are delivered.
The Incoming fixture now uses a real signed JWT without a username or UserInfo
endpoint: authentication succeeds with an optional username, while both inventory
purposes remain denied. Current affected qualification is recorded separately
from the earlier receipts below.
Logs and command receipts are in
`.build/rust-runtime-inventory-services-20261011/`. Existing S1 qualifications in
[Per-call service credentials](service_credentials.md) retain their historical
scope. Earlier Broker refresh fixtures corrected Disabled to Broker under the
then-current rule; those runs do not qualify this new independence contract.

After tsk-7dd5 completed, the current Incoming fixture passed all 17 inventory
integration cases in 1.06 seconds of test execution. The Local and service
contention/cancellation filters each passed one functional case, preserving the
Original, generation and physical profile; strict Auth all-target Clippy passed
with warnings denied. Scoped Rust 2024 formatting, diff and documentation links
also passed. Command wall times include the normal shared-target queue and do
not measure service latency. See current qualification (`../../.build/rust-runtime-inventory-services-20261011/current-cleanup/qualification.json`; historical local artifact unavailable).

The unchanged production lease API also retains the 133 Auth rustdoc cases from
tsk-749e, including its privacy/lifetime controls. Those cases were reused at
their recorded Source scope; no broad issuer, Broker or OAuth suite was repeated
for this fixture and documentation correction.

Portable focused invocation from the project root:

```text
cargo test --locked --offline -j1 -p graphit-auth --test inventory_services
```

Native SDK execution does not establish a Runtime HTTP journey, Broker ACL,
daemon/CLI operation, installed host lifecycle, Kiro V2/V3 operation or native
Windows/macOS qualification. These remain separate deliveries under ses-c98a.

The `tsk-749e` issuer ledger and state-quota removal is qualified in Main on Linux.
Input bounds, revocation and Original checks remain functional. See the
affected qualification (`../../.build/rust-issuer-retention-cleanup-20261011/handoff.md`; historical local artifact unavailable);
historical retention-capacity receipts describe their earlier Source.
