---
title: Rust Now and Presence integration
status: qualified
references:
  - target: {type: task, id: tsk-ea469}
    relation: implements
  - target: {type: task, id: tsk-f253}
    relation: implements
  - target: {type: task, id: tsk-255e}
    relation: implements
  - target: {type: task, id: tsk-c814}
    relation: derived_from
  - target: {type: task, id: tsk-0137}
    relation: derived_from
  - target: {type: task, id: tsk-b7fc}
    relation: uses
  - target: {type: task, id: tsk-60d0}
    relation: uses
  - target: {type: task, id: tsk-8923}
    relation: uses
  - target: {type: task, id: tsk-7f8c}
    relation: uses
  - target: {type: task, id: tsk-ccd4}
    relation: uses
  - target: {type: task, id: tsk-a437}
    relation: uses
  - target: {type: task, id: tsk-9a23}
    relation: uses
  - target: {type: task, id: tsk-9e52}
    relation: uses
  - target: {type: task, id: tsk-e6e4}
    relation: uses
  - target: {type: task, id: tsk-f37c}
    relation: uses
  - target: {type: knowledge, id: Entrada_Rust_de_Source}
    relation: uses
  - target: {type: knowledge, id: Warm_Native_inventory_integration_fixture, scope: project, scope_id: 01M1YEAE1WYMWG9KJKV1XQC4T4}
    relation: uses
  - target: {type: knowledge, id: Rust_Inventory_integration_fixture, scope: project, scope_id: 01M1YEAE1WYMWG9KJKV1XQC4T4}
    relation: uses
  - target: {type: knowledge, id: Rust_realtime_WebSocket_server, scope: project, scope_id: 01M1YEAE1WYMWG9KJKV1XQC4T4}
    relation: uses
---

# Rust Now and Presence integration

The consumer is [now_presence_integration.rs](../../crates/graphit-app/tests/now_presence_integration.rs).
It exercises compiled Graphit processes against private temporary installations.
It does not use personal profiles, credentials, trust or settings.

## Independent Runtime journey

The first case starts two actual Graphit daemon foreground processes, with distinct Core installation and PROJECT identities and selected versions. It creates native Task records and acquires two independent target claims per Runtime through the compiled MCP stdio proxy and the running Source service, reads local PROJECT Now through the same public tool, and checks that heads and private bodies do not cross project boundaries. The same actor may claim both distinct Tasks concurrently, and both live claimed heads must appear in local Now. Unclaimed Tasks must not be promoted into active heads. Stopping the first process must join it and retire its discovery/key; the second remains usable until its own signal and join.

The registered runner selects this target with cargo xtask test
graphit-app/now_presence_integration. Run the independent case directly from
the repository root:

~~~text
cargo xtask cargo test --locked --offline -j4 -p graphit-app --test now_presence_integration compiled_two_runtime_projects_versions_native_heads_and_owned_shutdown -- --exact --test-threads=1
~~~

The independent Linux case passed: one test, no ignored cases, 16.68 seconds
of test runtime. Its result does not certify Broker
registration or realtime, installed agent hosts, or other operating systems.

## Broker integration boundary

The complete journey also requires two real Runtime processes, their distinct
projects and versions, and the existing Broker Native and Local/opaque
composition with PostgreSQL TLS and publish/subscriber WebSockets. It must
observe registration, realtime replacement, ordering, isolation, explicit
reconnection and cancellation/join through real MCP and continuous CLI.
The remote fixture grants the real task:read capability and a separate PROJECT
grant. Memory and Knowledge capabilities are deliberately absent: their
forbidden sections must have no items or known counts. This is distinct from
an available empty section or a physically absent corpus.
Only the heads needed for those scenarios are required; there is no numeric
scale threshold, accounting campaign or fabricated Now snapshot.

The inventory fixtures currently used by Code provide real Auth and PostgreSQL
TLS inventory APIs. The Broker owner qualified the public example executables
with the existing productive realtime composition; this supplies
the real producer artifacts for the Code journeys. The protocol peer in
[now_executable.rs](../../crates/graphit-app/tests/now_executable.rs) remains a
transport test, and in-process Foreground journeys remain component evidence.
Neither substitutes for the combined process journey.

Current contracts and component evidence are in
[Runtime Now publisher](rust_runtime_now_publisher.md),
[Runtime inventory](rust_runtime_broker_inventory.md),
[Source Now](rust_source_now.md) and
[Foreground Source](rust_source_foreground.md).

## Explicit Broker cases

The Broker owner has qualified its public fixture artifacts. The Native and
Local Code process journeys are qualified separately on Linux. They are explicitly ignored by
the ordinary target run; an ignored case is not a passing integration. Provide
the actual fixture executable through GRAPHIT_BROKER_NATIVE_INVENTORY_FIXTURE_BIN
or GRAPHIT_BROKER_INVENTORY_FIXTURE_BIN, respectively, and run:

~~~text
cargo xtask cargo test --locked --offline -j4 -p graphit-app --test now_presence_integration compiled_native_two_runtime_now_registration_reconnect_and_join -- --ignored --exact --test-threads=1
cargo xtask cargo test --locked --offline -j4 -p graphit-app --test now_presence_integration compiled_local_two_runtime_now_registration_reconnect_and_join -- --ignored --exact --test-threads=1
~~~

The public Start.realtime request supplies heartbeat_timeout_ms=60000,
broadcast_capacity=32, required_scopes=[presence:read], command_bytes=65536,
outbound_bytes=65536, operation_timeout_ms=30000 and task_read=true. These are
explicit scenario inputs, not new product defaults. task_read produces the
real SQL TaskRead rule; it is separate from JWT transport scopes and PROJECT
grants. Ready and shutdown/EOF IPC retain their existing shapes. The fixture
retains one real RealtimeServerRuntime until its awaited shutdown.

Use the public `native_inventory_fixture` executable for Native and
`inventory_fixture` for Local. The storage fixture has a different Start
contract and is not interchangeable. Local derives TaskRead grants from
Start.projects, so this scenario supplies both actual captured project IDs
and their versions. That initial fixture inventory establishes test permissions;
it is not counted as Runtime registration. Each running daemon still publishes
its own maintained project/version inventory through the production registrar.

## Current execution and causal coverage

The Runtime PresenceRead service rule uses the existing provider and credential conditions. PROJECT ACL and Role grants remain independent.

Inventory callers select credentials from the active provider kind: Broker uses its trusted verifier; Local uses its configured credential. The presence of a shared lazy verifier cache does not select Native mode.

The public Broker fixtures expose `deny_read`/`allow_read` in both modes,
`deny_publish`/`allow_publish` and `replace_with_foreign_owner` in Native,
and `revoke_access`/`restore_access` in Local. Local revocation affects only
the access credential, leaving the service credential valid. `delete_inventory`
affects the initial instance only. SQL acknowledgements do not prove a
WebSocket has already processed the change.

Native `lose_next_successful_put_response`, followed by the real PUT and
`stats.put_response_losses`, can establish response-body loss after a confirmed
200/201 and reconciliation by a fresh own GET. This is distinct from SQL
COMMIT acknowledgement loss. There is no public IPC to force lag, expire a
heartbeat or change the Local identity revision. Native expiry uses the
explicit Start access TTL; Local fixture credentials last one hour. A buffer
capacity of one alone is not deterministic proof of lag.

Existing component proofs remain separately identified: `tsk-b7fc` qualifies
the public realtime fixture and owned shutdown; `tsk-60d0` qualifies real lag,
current ACL/revocation, expiry and subscription cleanup; `tsk-8923` qualifies
publisher registration, supersession, owner conflict, signed expiry and
lifecycle; `tsk-7f8c` qualifies HTTP view invalidation, body cancellation and
retained join. A publisher send completing is not a remote acknowledgement.
Native10 and Local24c supply the combined-process evidence; these component
receipts supply only their explicitly identified causal cases.

## N5–N9 coverage

| Requirement | Producer and consumer evidence |
|---|---|
| N5 project versions in the global lock | `tsk-a437` and `tsk-9a23`; real selections/inventory in both combined journeys. |
| N6 instance registration and inventory | `tsk-e6e4`, `tsk-8923`, `tsk-f37c` and `tsk-9a23`; two distinct Runtime registrations and updates in Native10/Local24c. |
| N7 realtime Now | `tsk-60d0` and `tsk-7f8c`; Native10/Local24c exercise WebSocket delivery, reconnect and withdrawal. Expiry and deterministic lag retain their component proof. |
| N8 dynamic CLI | `tsk-9e52` supplies terminal/pipe/cancel evidence; both combined journeys exercise the continuous CLI. The combined cases do not claim a TTY proof. |
| N9 filtered instance tools | `tsk-7f8c` and Native10/Local24c exercise MCP/Source with explicit instance/project/version filters. |

Response loss after a successful PUT and fresh own GET is the `tsk-e6e4`
Uncertain case. It is not SQL COMMIT acknowledgement-loss evidence. Reuse these
valid results without rerunning unchanged tests to recreate deleted build logs.

## Local Source caller and failure diagnostics

The compiled `mcp --stdio --non-interactive` command is the public local
Runtime caller. It ensures Source is Running, reads its installation discovery,
uses the real Core-owned runtime key and loopback MCP endpoint, maintains the
checkout and lists remote tools before serving stdio. An active Broker profile
does not select another endpoint or bearer on that path. No alternate flag,
mock key or Auth producer is needed for this caller.

The consumer now labels preparation, startup, IPC, MCP and stream failures. Initialization errors retain bounded sanitized stderr and wait for the same child; a failed or timed-out wait requests kill and explicitly awaits that child rather than treating kill-on-drop as a join receipt. Broker public shutdown/ack/join also runs after ordinary journey errors, preserving both the primary and cleanup failure.

Attempts 01–07 allowed 60 seconds for Ready. The current helper allows 300
seconds for Broker Ready, shutdown acknowledgement and the Broker process
join, matching the external control contract qualified by `tsk-ccd4`.
That producer observed Docker create taking 85.842 seconds and PostgreSQL
setup taking 89.206 seconds. Its synchronous Docker create/start/TLS-copy/port
commands precede the 20-second PostgreSQL readiness loop. This changes only
the test fixture's external controls: Runtime/CLI waits, product request
deadlines, heartbeat and token lifetimes retain their existing values.
A timeout at this consumer boundary alone does not establish a stalled command
or an authority failure. The joined Now CLI stderr drain is retained until
actual EOF instead of dropping its handle on an internal timeout.

The test enables the producer's GRAPHIT_BROKER_FIXTURE_DIAGNOSTICS=1 only
for the temporary fixture child. Its stderr drain retains at most 32 lines of
160 bytes, accepting only the published phase/event/status enums and decimal
elapsed milliseconds. Other stderr is discarded. A Start failure closes stdin,
requests termination and awaits that same child plus its stderr task before
returning the primary error and cleanup results. This process join does not
prove PostgreSQL/container cleanup or the public shutdown acknowledgement.

The Broker launcher uses its own Command, rather than the temporary
Installation command. It inherits environment and working directory, adding
only the diagnostic opt-in; it neither clears the environment nor overrides
Docker or PATH. The producer helper also inherits them. The documented
Broker Cargo recipe starts from its repository root, whereas this direct
launcher inherits the test process directory. Certificate paths originate
in the helper TempDir, without checkout-relative paths. These source facts
do not establish equal effective environments or explain the Docker wait.

## Web Now consumer

The qualified Web consumer in `tsk-255e` adds the maintained `/workspace` journey
to the Rust stream. Its Web ingress is validated separately from the Native/Local
producer qualifications above.

Sign in through the configured Browser OAuth provider and use the native Project
and Version menus. `POST /api/now/web/contexts` accepts an empty JSON object,
retains exact native paths, and discovers authorized working contexts through
short-lived subscriptions using the same Broker protocol. Forbidden projects
are omitted; authentication or transport failure is reported instead of an
apparently complete catalogue. Discovery happens at page entry or explicit
refresh, with no background catalogue polling.

`POST /api/now/web/subscribe` accepts the existing native
`{selection:{project_dir,project_id,project_version}}` shape. It selects one exact
live project/version and returns `application/x-ndjson` frames shaped as
`{selection,snapshot:{type:"snapshot",project_id,instances:[...]}}`. Each instance
contains its stable ID, selected version and validated Now heads. The current
daemon's publisher and other instances use this same channel. An empty instance
list means no authorized online publisher currently contributes to that selected
version; Broker unavailability is an explicit connection error.

The access token remains server-owned. The subscription retains the locally
verified Web caller, configured verifier and same Original at existing admission
barriers. The native ingress still rejects Browser Origin. The destination comes
from the configured verifier endpoint, never a token claim or request URL. The
Broker separately admits PresenceRead PROJECT access and recomputes
Task/Memory/Knowledge masks on each replay; a valid JWT alone grants no project
access. The Broker also applies the publishing identity's read mask before
storing the snapshot. Task/session heads therefore need TaskRead on the
publisher as well as the reader; a later reader cannot restore stripped data.
Partial forbidden sections remain available without three redundant Code
authorization calls or Task-read as a prerequisite. No UserInfo or introspection
check is added; expiry and refresh keep the existing OAuth policy and token TTL.

The stream reuses the subscriber codec, heartbeat and latest-snapshot delivery.
It keeps its current pending snapshot and one outgoing frame, does not hydrate
record bodies, and never buffers a corpus or per-record history. Links open those
bodies through their own authorized readers. Context change, logout, response
drop and source/host shutdown cancel the stream; the Browser owner retains and
joins its jobs. Authentication and liveness are checked before delivery,
including when a subscriber is slow. Delivery also checks the verified OAuth
credential expiration on queued frames and wakes the worker at that existing
expiration; a quiet or backpressured stream cannot outlive its credential. No
additional token lifetime is introduced.

React uses native integer parsing, the shared progress component and existing
design-system states. It withdraws disconnected data, shows reconnection honestly
and retries transient stream failure after two seconds. Invalid selection,
missing ingress, authentication and PROJECT denial stop automatic retries; the
header refresh explicitly retries. It performs no periodic Now GET and never
sends bearer credentials from frontend code.

Affected validation targets are the Browser Now composition, subscriber proof consumer, native context navigation/refresh, `now.test.ts`, `WorkspaceNow.test.tsx`, TypeScript/Vite and shipped assets. It relays an actual local SDK snapshot through two authorized publisher sockets; full independent Runtime publishers remain covered by `tsk-f253`.
