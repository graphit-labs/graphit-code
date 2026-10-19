---
type: specification
title: Rust Runtime own Broker inventory
references:
  - target: {type: task, id: tsk-cab0}
    relation: supports
  - target: {type: task, id: tsk-e6e4}
    relation: implements
  - target: {type: task, id: tsk-f9c1}
    relation: derived_from
  - target: {type: task, id: tsk-6e07}
    relation: derived_from
  - target: {type: task, id: tsk-e07a}
    relation: uses
  - target: {type: task, id: tsk-5020}
    relation: uses
  - target: {type: task, id: tsk-6442}
    relation: uses
  - target: {type: task, id: tsk-84fd}
    relation: requires_contract
---

# Rust Runtime own Broker inventory

The daemon reads and publishes the running installation's historical Broker
inventory on Linux, macOS and Windows through its actual Auth issuer, Core captures
and bounded HTTP client. The producer uses the same implementation on each system;
existing Linux qualifications do not establish native Windows or macOS execution.
Inventory does not establish live presence, select a project/version, or grant
access to a project's contents. See [maintained Core facts](rust_runtime_maintained_snapshot.md),
[Auth inventory admission](broker_inventory_service_credentials.md) and
[DaemonAuth startup](rust_daemon_auth_startup.md).

The consumer takes an explicit `InventoryCredentialMode`: `LocalBrokerKey` or
`BrokerNative(&BrokerVerifierCache)`. A failed Native operation never tries a
local credential. Native verification does not perform login or refresh, create
a grant, or modify personal credentials, trust or settings.

The previous Local GET and PUT scopes were qualified in `tsk-6e07` and
`tsk-f9c1`. Native Auth inventory admission `tsk-e07a`, the reusable cache
`tsk-5020` and checked Original accessor `tsk-6442` are qualified producers.
The `tsk-e6e4` Code SDK → signed-token HTTP/PostgreSQL TLS journey passed
independently with the public `tsk-84fd` fixture (`native-tcp-01.log`, one test,
25.08 seconds). The affected App/lint/documentation qualification remains in
progress; the fixture producer's pass is not the consumer's evidence. Temporary SDK fixtures do not qualify
installed hosts, Windows/macOS or the migration as a whole. Agent integration targets Kiro V3, OpenCode 2 and AGY through their documented
contracts; its tests do not require vendor binaries, accounts or active plans.

## Captured ownership and historical facts

`DaemonAuth` supplies the live installation capture and installation ID.
Neither GET nor PUT accepts a caller-selected installation ID. Configured Broker
base paths are preserved when appending
`v1/instances/{captured-id}/inventory`, including escaped path segments.

PUT reads historical facts through `read_maintained_inventory` under the same
Original and captured service. It accepts no separately supplied projects,
versions or authority. Core inventory can retain facts about an absent checkout;
historical observation is distinct from current project-content access. Physical
installation, selected provider/profile and Original remain current checks.
Inventory does not authorize opening an absent or foreign project.

An absent Core inventory and a valid empty inventory produce an empty projects
array. They do not manufacture Broker ownership or authorization. Reading those
facts does not hold an inventory writer across HTTP awaits or freeze concurrent
maintenance. Publication is an observation of the captured facts at this operation.

## Admission and operation lifetime

The issuer captures exact `InventoryRead` or `InventoryPublish` for an actual
Runtime or Local caller, using its explicit service rule and unconditional
`External(RemoteRequest)`. Local mode requires an active Local provider with
the Broker endpoint and local broker key. Native mode requires the active Broker
provider and NativeAccess rule; its literal issuer, subject, endpoint,
`mcp_resource` and provider revision remain bound by the real Auth SDK.
Stored assertions, profile data and cache entries are not authentication.

Before cache lookup or Native discovery/JWKS, the consumer calls
`ServiceSelection::original()` and requires the same Arc snapshot as the supplied
Original. This checked accessor preserves the selection's existing request,
profile and policy gates and does not verify JWT or perform network I/O.
After obtaining the credential lease, the consumer still checks lease, Original
deadline/cancellation, physical live scope and binding. Checks run before request
preparation, around HTTP await boundaries and before returning decoded data or
a remote/invalid-response result. The absolute deadline and cancellation reach
`execute_guarded`; there is no rebased timeout or automatic retry.

For Native, the caller retains the cache alongside its configured issuer and
explicit immutable network policy. The operation holds an Arc verifier locally
through `native_credential`, the borrowed lease, gates, HTTP awaits and output.
Cache data does not authorize PROJECT reads. No lease is returned borrowing a
verifier created inside a helper.

Core scope mutex contention follows its existing implementation and checks
Original afterward; that wait is not preemptible. Physical rechecks detect
changed captures but do not promise continuous confinement against arbitrary
hostile pathname changes.

## Read own inventory

The public API is:

```rust
use graphit_daemon::broker_inventory::{InventoryCredentialMode, read_own_inventory};

let inventory = read_own_inventory(
    client,
    service,
    caller,
    InventoryCredentialMode::LocalBrokerKey,
    original,
).await?;
```

For Native, replace the mode with
`InventoryCredentialMode::BrokerNative(cache)`, where `cache` is the actual
retained `BrokerVerifierCache`. Mode selection is explicit, not inferred from
a token or attempted by fallback.

The client supplies validated origin policy and transport limits through the
[bounded HTTP transport](bounded_http_transport.md). Successful GET requires 200
and the strict own-inventory response. `OwnInventory` contains only
`instance_id`, `projects`, `created_at` and `updated_at`. Each project contains
its canonical nonzero ProjectId and distinct exact version labels. Version labels
are not normalized; invalid/blank labels, duplicate IDs/versions, malformed
fields, unknown fields, duplicate JSON keys and trailing JSON fail decoding.
Timestamps are nonnegative integer UTC epoch microseconds, not milliseconds or
client-generated dates. A legitimate empty projects array remains valid.

The instance ID must equal the captured installation. No owner, principal,
credential, private path, raw response or SQL diagnostic is returned. A 404
remains a typed failure: `not_found` also describes a missing route, so it cannot
be converted to an empty inventory or `None`.

`OwnInventoryReadFailure` preserves Admission, Network, InvalidResponse or Remote
and the actual `NetworkEffect`. Admission distinguishes Auth, verifier-cache
configuration/selection, physical scope, DifferentOriginal and UnexpectedBinding;
its Debug/Display is finite while `Error::source` retains the real cause.
GET status/code pairs are closed: 401 unauthenticated, 403 forbidden,
400 invalid_inventory, 409 owner_conflict, 404 not_found, 405 method_not_allowed,
504 deadline, and 503 authentication_unavailable/authorization_unavailable/
invalid_policy/inventory_unavailable/closed/outcome_uncertain. Unknown or
status-mismatched envelopes fail as InvalidResponse.

## Publish the maintained Core inventory

The public signature is
`publish_maintained_inventory(client, service, caller, mode, original, max_document_bytes)`.
`max_document_bytes` is the existing positive Core document bound, not a new
transport quota. The same exact Original precheck, real lease and physical
scope gates protect snapshot reading, encoding, native send, response awaits
and output.

The private Serde projection writes exactly
`{projects: [{project_id, versions}]}`, preserving Core order and labels.
The encoder uses the validated `BoundedClient::request_body_bytes()` limit,
checks it before buffer growth, uses fallible reservations and checks Original
while writing. Cancellation returns immediately without encoder retry. The
encoder cannot select an independent second transport limit.

`OwnInventoryPublication` contains `instance_id`, `change`, `created_at` and
`updated_at`. Change is Created, Updated or Unchanged. Success requires
201/created or 200/updated|unchanged, exact fields, the captured installation ID
and nonnegative integer UTC microseconds. Equal canonical inventories preserve
server timestamps; clearing an owned parent preserves creation time. The consumer
does not predict those changes. Server Publish ACL for all projects, or an
explicit Any-project grant for empty inventory, remains separate authority.

Publication failures preserve Admission, Snapshot, Encoding, Network,
InvalidResponse or Remote causes, finite diagnostics and effects. PUT remote
codes are closed: 400 invalid_json/invalid_inventory/body_too_large;
401 unauthenticated; 403 forbidden; 409 owner_conflict; 404 not_found;
405 method_not_allowed; 504 deadline; 503 authentication_unavailable/
authorization_unavailable/invalid_policy/registration_unavailable/closed/
outcome_uncertain. Invalid DTOs, unknown codes, status/code mismatch and
status/change mismatch fail. A 404 remains a failure.

## Effects and explicit recovery

`NotSent` and `MayHaveApplied` describe the inventory HTTP operation, not all
earlier Auth work. Native Auth may already have consulted public metadata before
inventory admission fails. Before inventory native send, admission/snapshot/
encoding failures are NotSent. Once send begins, late Auth/Original or final-gate
denial still blocks output and preserves MayHaveApplied.

A lost or invalid response does not prove rollback, authorize replay, or establish
SQL COMMIT uncertainty. The caller may start a new ownGET under actual current
authority to observe persisted inventory before choosing a consequential attempt.
No mode retries, changes credentials, refreshes tokens, or falls back to Go.

## Foreground composition

[Foreground Source](rust_source_foreground.md) starts one Auth owner. A configured
Native Broker publication adds Runtime NativeAccess InventoryRead and
InventoryPublish rules for the exact provider/endpoint and ordinary RemoteRequest.
Its private Option<Arc<BrokerVerifierCache>> stays alongside that Auth owner.
`BrokerVerifierCache::new(...)` returns the cache directly and only captures
settings. Its checked `get(&mut selection)` creates or reuses a verifier and
returns actual configuration/admission errors before Native credential use.
Local publication preserves the existing LocalBrokerKey rule. Without Native in
this composition, no verifier cache is retained.

The cache and inventory client share the existing TransportBytes/TransportInflight
network limits, URL8192, headers65536/count32, idle1/30s and
certificates8/bytes65536. HTTP loopback requires explicit readonly
`broker.allow_loopback_http=true`; default is false. There is no environment
proxy, personal root modification or TLS bypass.

The pure `admin_auth::verifier_limits(ContentLimits)` is reused by its existing
admin factory and Foreground cache: token=max_scalar_bytes, JSON ContentLimits,
max_keys=max_collection_entries, metadata TTL300s, key TTL300s, unknown-key
cooldown1s. These existing metadata policies do not renew signed JWT expiry,
Original deadline, refresh margin or PROJECT ACL. Foreground does not inherit
the admin factory's loopback allowance.

Publication failure retains the real Foreground owner for explicit cleanup,
preserving the Core commit and HTTP effect. Stop/cleanup does not claim rollback.
Continuous publication, live presence and Now composition remain their own
used callers and contracts.

## Public native fixtures and qualification

The existing Local native-process tests consume graphit-broker's public
`crates/broker-presence/examples/inventory_fixture.rs`, introduced by `tsk-0022`.
They take the prebuilt executable through call-local
`GRAPHIT_BROKER_INVENTORY_FIXTURE_BIN`; missing/invalid input fails setup.
They do not discover a default executable, recursively invoke Cargo or import
private Broker exports.

Build the example in the graphit-broker checkout:

```text
cargo build --locked -j1 -p broker-presence --example inventory_fixture
```

Run the consumer checks in graphit-code, with the prebuilt fixture supplied:

```text
cargo test --locked --offline -j1 -p graphit-daemon --lib broker_inventory
cargo test --locked --offline -j1 -p graphit-daemon --lib broker_inventory::tests::broker_inventory_native_process_journeys -- --ignored --exact --test-threads=1
cargo test --locked --offline -j1 -p graphit-daemon --lib broker_inventory::tests::broker_inventory_native_publication_journeys -- --ignored --exact --test-threads=1
```

The Local driver creates temporary Core installation/project identities and
version facts, starts the public process through strict NDJSON, then starts
DaemonAuth on the same captures and endpoint. Access/Service keys and profile
values stay in temporary Auth via the actual SDK. Ready audience/scopes describe
server checks, not a fabricated Local `mcp_resource`.

Those tests use real TCP, PostgreSQL TLS with an ordinary database role,
verifier, ACL and store. GET covers projects/empty, Read403 restoration,
opaque Access revocation401 restoration, separate-owner409, deletion404,
Original/physical/policy/cancel/deadline denial and transport bounds. PUT uses
actual Core inventory writers and covers Created/Updated/Unchanged and existing
recovery cases. The opaque fixture cannot deny Publish; decoding403 or supplier
ACL qualification does not prove Native consumer Publish403.

Native `tsk-84fd` provides a separate public fixture with real signed access,
HTTP namespace/openid metadata/JWKS and current read/publish ACL controls.
It represents provisioned warm verification, not a production OP or qualified
login/refresh. Token/UserInfo calls must remain zero in this journey. Its causal
loss of a successful PUT response tests transport uncertainty and explicit GET
observation, not SQL COMMIT lost acknowledgement. The producer is qualified in
`tsk-84fd`; this does not pass the consumer's integration check.

Build its public executable in graphit-broker:

```text
cargo build --locked -j1 -p broker-presence --example native_inventory_fixture
```

Supply that prebuilt executable through the call-local
`GRAPHIT_BROKER_NATIVE_INVENTORY_FIXTURE_BIN`. Compile the Code consumer before
starting the fixture; its explicit 600-second access TTL must not expire in the
Cargo queue. The test spawns the process after compilation and provisions only
a temporary Native profile via the actual AuthStore SDK. Invoke in graphit-code:

```text
cargo test --locked --offline -j1 -p graphit-daemon --lib broker_inventory::tests::broker_inventory_native_access_process_journeys -- --ignored --exact --test-threads=1
```

`broker_inventory_native_access_process_journeys` exercises actual signed access,
HTTP discovery/JWKS, GET absence, Created/Updated/Unchanged, current read/publish
ACL withdrawal/restoration, foreign-owner409, deadline/cancellation, current
service-policy revocation with a warmed cache, and lost PUT response followed by
a new admitted GET. Cold-cache SDK futures are actually polled to Pending before
Original cancellation or deadline, then resumed to require their typed failure;
a separate pending publication is dropped and a new GET uses the same cache.
These Pending observations establish suspension in the Native SDK, not inventory
send, server application or SQL rollback. Stats require zero token/UserInfo calls
and one causal loss.
The consumer execution passed in
`.build/rust-runtime-native-inventory-e6e4/native-tcp-01.log`: one real Native
test, 25.08 seconds. It exercises the actual SDK and server journey; it does not
qualify installed hosts, other operating systems, OAuth login/refresh or SQL
COMMIT acknowledgement loss.
The ordinary negative guard separately rejects a different Original before
Native verification and rejects a Local mode request against a Native profile.

IPC secrets remain transient in private pipes and temporary Auth, never receipts,
Task or stderr. Commands require matching Ack; missing Ack leaves uncertainty,
not replay/rollback. Driver teardown closes stdin, joins readers and reaps the
child. Shutdown Ack follows actual producer cleanup; EOF is graceful cleanup.
IPC waits use Original deadline; blocking reap/joins are not preemptible by it.
No SIGKILL or arbitrary native quiescence guarantee is inferred.

Previous receipts:
- GET: `.build/rust-runtime-broker-inventory-20261011/qualification-final-current.json`.
- Local PUT: `.build/rust-runtime-broker-publication-20261011/qualification-native-doc-strict-current.json`.
- Current Native consumer work: `.build/rust-runtime-native-inventory-e6e4/`.

The first ENOSPC compile and later Records/Now interface failures are preserved
in `daemon-focal-01/02/03.log`. The corrected ordinary consumer suite passed eight
tests in `daemon-focal-04.log`; its three process journeys were deliberately
ignored there. Native TCP/PGTLS passed separately in `native-tcp-01.log`, and the
Local GET regression passed in `local-get-01.log` (19.42 seconds). Remaining
Local publication also passed in `local-put-01.log` (54.56 seconds).
App ordinary guards passed in `app-focal-01.log` (4 PASS, 3 ignored).
Foreground Native owner/ACL/cleanup passed in `app-native-01.log`
(1 PASS, 8.99 seconds), and the affected Local Foreground journey passed in
`app-local-01.log` (1 PASS, 20.76 seconds). The Native journey and executable
command are documented on the linked Foreground page. The compiled CLI journey
passed in `app-cli-01.log` (1 PASS, 4.86 seconds), including publication and
SIGINT/SIGTERM cleanup. Strict clippy passed with `-D warnings` for Daemon
lib/tests and App lib/foreground_broker_inventory in `daemon-clippy-01.log` and
`app-clippy-01.log`. Focused edition-2024 formatting, diff whitespace and local
links passed. The final Source-to-docs review confirmed the public signatures,
Original gates, actual Native/Local separation and cleanup/effect contracts. Installed
hosts and other operating systems remain unqualified by these library tests.

## Documentation recovery

An ENOSPC write during e6e4 left the previous untracked document empty.
No integral backup was recovered from Knowledge or the bounded chat-history
search. This document was reconstructed from the current implementation,
verified producer Tasks, prior receipts and known contract excerpts under
coordinator authorization. It is a new current-contract document, not a
byte-identical restoration of the earlier SHA. The incident and validation
limits remain in e6e4; it does not change runtime authority or mark pending
journeys passed.
