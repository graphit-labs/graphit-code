---
title: Rust Runtime Now publisher
status: native-linux-qualified
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrects
  - target: {type: task, id: tsk-ba70}
    relation: corrects
  - target: {type: task, id: tsk-6d5a}
    relation: relates_to
  - target: {type: task, id: tsk-8923}
    relation: implements
  - target: {type: task, id: tsk-f37c}
    relation: relates_to
  - target: {type: task, id: tsk-b98d}
    relation: derived_from
  - target: {type: task, id: tsk-4970}
    relation: derived_from
  - target: {type: task, id: tsk-bcf2}
    relation: uses_api
  - target: {type: task, id: tsk-f382}
    relation: relates_to
---

# Rust Runtime Now publisher

The borrowed publisher sends fresh PROJECT Now observations to a registered
Broker channel. It borrows the caller's real authority, scope captures,
PresencePublish credential lease and Original request. It owns one socket and
creates no Graphit background jobs.

The publisher and local compositor source are enabled for Linux, macOS and Windows using the same Auth, Records and Data producers. This source composition does not qualify native execution on another host.

## Caller and API

The caller constructs `ProjectRecordAuthority::new(auth.issuer())`, authenticates
a Runtime or Local caller through that issuer, and captures
`capture_presence_publish_service` for the active provider. Native callers
retain `BrokerVerifierCache::get` and `selection.native_credential`; Local callers
explicitly use `selection.credential`. Role grants do not become Runtime
publisher authority.

`NowPublisherInput` contains the borrowed authority, authenticated caller,
actual `ScopeInput` and explicit `ProjectNowLimits`. The caller retains the Core
project capture, binding and selected `VersionContext` for the whole operation.
The channel's instance id comes from the installation capture, its project id
from the PROJECT capture and compositor, and its version from that selection.
Caller-provided snapshot DATA is not accepted.

`NowPublisherLimits` supplies positive whole escaped command and inbound
control byte limits. `start_now_publisher(client, lease, original, input, limits)`
returns a borrowed `NowPublisher` after the actual first publish and welcome.
`next()` drives one real recomposition alongside the socket and publishes once
on the negotiated tick. If that read is still pending, it sends the channel's
last genuine observation with its original `generated_at`. It starts no second
read while that recomposition is pending.
Successful `next()` completes the transport send and final local checks; it is
not a Broker acknowledgement of that publication. A remote rejection can arrive
on a later receive. The caller must keep driving this future while it intends to
retain presence.
Consuming `shutdown()` closes the owned socket.

The [local compositor](rust_runtime_project_now.md) creates fresh Task, Memory
and Knowledge read admissions before every publication and reapplies their masks
to a copy of the last genuine observation. Read denials produce valid
`failed/forbidden` sections. PresencePublish denial or a physical identity,
Original or whole-operation failure ends the publisher. This API does not add
Read(Task) or any other permission to a caller.

## Protocol and freshness

The [captured WebSocket transport](rust_http_websocket.md) uses the real lease's
Broker endpoint and sensitive Authorization header. The caller must first
register actual maintained inventory. The initial `publish` carries the complete
fresh `NowLocal` and precedes the server's `welcome`. Initial local composition
finishes before opening that WebSocket; fresh module admissions and masks are
applied after connection immediately before the first send. A slow first read
therefore does not consume the liveness of an already-open publisher channel.

The decimal `heartbeat_timeout_ns` is parsed as checked unsigned nanoseconds.
The publication interval is half the advertised timeout, strictly positive
and shorter than that timeout. The first deadline is anchored immediately before
the actual initial send, so receiving Welcome later does not restart the slot's
heartbeat window. A due deadline waits for the same pinned socket receive before
polling a recomposition; otherwise an already-ready socket is polled first.

This is negotiated protocol cadence, with no additional knob or invented TTL.
One actual recomposition may remain pending across ticks; sends retain the last
genuine observation and its original timestamp with fresh admissions and masks.
The publisher sends no standalone heartbeat and does not replay a retained
observation when reopening a channel for recovery.

Publish commands serialize once through a private physically bounded byte
writer. Escaping counts toward the actual byte cap. The first request,
allocation or byte-limit failure is retained, and no partial payload escapes.
Inbound controls must be JSON objects with the exact fields, without duplicate
or unknown keys, and fit the complete control cap. A second welcome or a
subscriber snapshot is invalid for an established publisher.

## Errors and lifecycle

The same immutable Original snapshot, cancellation and optional deadline govern reads,
serialization, every send and receive, and their final checks. The publisher
does not replace or renew this Original or a signed token's expiry.

The actual bcf2 context producer represents ordinary requests as Some(deadline)
and a caller-owned cancelable service operation as None. None is absence of a
clock deadline, not a far-future sentinel or a new Auth grant. Both cases use the
same captured WebSocket checks. Initial welcome awaits the real transport
receive(), and negotiated ticks use receive_until(Instant). Signed token expiry
remains independent of the Original deadline.

The socket and any pending read are taken before an awaited `next` and restored
only after a successful publication and final checks. The same transport receive
future stays pinned when recomposition completes, because abandoning that receive
would close its socket. A returned error or abandonment drops both the channel
and pending read; later `next` returns Closed. Cancellation, deadline and terminal
read errors stop renewal of the previous observation. `shutdown()` drops the
pending read before closing the socket. The publisher spawns no jobs; the
Foreground owner retains its existing cancellation and join lifecycle.

The twelve actual Broker notice codes are invalid_data, too_large,
unauthenticated, forbidden, deadline, authorization_unavailable, closed,
superseded, backpressure, outcome_uncertain, not_registered and owner_conflict.
Actual pre-upgrade HTTP status is retained as a numeric typed cause. Local,
Auth, Serde and transport causes stay behind finite diagnostic boundaries;
paths, credentials and payloads are not printed by the publisher.
Admission diagnostics and their standard error source expose only the actual
`ServiceUseError` and its static enum categories, including physical `Auth(Io)`
or `Store(Core(Io))`. Arbitrary retained I/O errors and other causes remain
private. An operational failure of physical identity inspection is not reported
as proof of an invalid token.
The existing WebSocket SDK cause reports its static SDK variant and reason,
numeric handshake status when available, and IO `ErrorKind` or Reqwest's finite
network category. Its Debug and Display never format the retained raw SDK error
or expose its source chain. These details distinguish upgrade, TLS, protocol and
IO failures without changing the borrowed publisher's failfast behavior.
The continuous owner separately decides recovery from those typed categories.

Only backpressure or close 1013 returns `RecoveryRequired`. This borrowed
primitive never reconnects: an error ends its socket and the caller must discard
that operation. Finite callers retain their original request validity; they do
not rebase a deadline or retry an uncertain effect.

The continuous [Foreground owner](rust_source_foreground.md) separately rebuilds
a failed connection using the same typed recovery policy before and after its
first successful publish/welcome. Readiness does not gate transport recovery.
An abrupt TCP
drop or recognized connection I/O failure is typed as Closed by the transport;
invalid frames, opcodes and UTF-8 remain Protocol, and TLS verification failures
remain Transport. HTTP registration and Broker admission propagate a distinct
Connection category only for native timeouts, recognized connection I/O in the
SDK source chain, or Hyper's typed closed/incomplete-message condition. HTTP EOF
before a complete response is a concrete disconnect even without a nested
std::io::Error. HTTP parsing failures have terminal precedence; malformed headers
and chunk frames do not become disconnects. Generic Transport is not a retry
signal, so TLS and unknown failures during inventory registration or verification
remain terminal. Close 1000, 1001, 1012 and 1013 are recoverable for this owner.
The JSON `closed` and `deadline` notices also permit recovery in this continuous
owner. Each attempt requires new authority and a genuine
snapshot after discarding the old operation. The Broker uses `closed` for
heartbeat-only expiry and checks authentication before its operation budget.
Authentication expiry remains terminal, including simultaneous budget or
heartbeat expiry, and a real takeover remains `superseded`. Local SDK/Original
deadlines, cancellation, Close 1008, Auth/JWT expiry, supersession and all other
JSON notices, including `outcome_uncertain`, remain terminal even before the
first welcome. The borrowed SDK and ordinary finite requests remain failfast;
only the continuous owner can readmit a classified recoverable failure.
The same existing owner loop readmits physical admission I/O failures, including
descriptor exhaustion during a fresh PROJECT capture. It discards the failed
socket, lease and operation first, reports the failure, waits using the existing
cancelable backoff, and establishes fresh authority on the next attempt. An
inventory admission I/O failure is readmitted only with `NotSent`; uncertain
effects are not reinterpreted. Actual `Denied`, changed identity, invalid tokens,
poisoned state, cancellation and local deadlines remain terminal. The borrowed
publisher never retries, and no failed check grants authority or publishes a
fabricated successful observation. This does not replace fixing the underlying
descriptor/resource retention.
The close-code meanings follow [RFC 6455](https://www.rfc-editor.org/rfc/rfc6455)
and the [IANA registry](https://www.iana.org/assignments/websocket/websocket.xhtml).

After loss the owner drops the old socket, lease and payload. A cancelable wait
starts at one second and doubles to at most thirty seconds; successful welcome
resets it. Each attempt creates a genuine service request, checks the current
selection and credentials, and composes a fresh snapshot. After a previously
successful welcome, recovery also republishes the complete current maintained
inventory under the stable installation id. Before that first welcome, startup
has already confirmed the maintained inventory and publisher readiness remains
pending until the real welcome. The registration
is a new idempotent state publication, including after a network outcome that may
have applied; it does not replay an old Now sequence or payload. Remote
OutcomeUncertain and refresh/rotation uncertainty remain terminal. There is no
health probe, additional InventoryRead grant or retained snapshot. Native credential
renewal is a separate owner operation described below.

## Native credential renewal in the Foreground owner

The App owner consumes the SDK's actual `ServiceUseError::NeedsRefresh` from
credential acquisition or publisher admission, including admission during the
first publish/welcome. It recognizes the original typed service error directly
or through the known `NowPublisherError::Admission` wrapper. It does not call
`lease.check()` again to reinterpret a failed operation: a second physical check
could see another I/O failure or a changed selection and mask the original
renewal request. Arbitrary nested errors or matching diagnostic strings never
request renewal. Inventory admission requests renewal only when its cause
is that same Auth error and `NetworkEffect::NotSent`. Other inventory effects,
remote `Superseded`, protocol errors and uncertain refresh effects remain terminal.
The borrowed publisher itself still performs no token renewal.

The owner quiesces and joins its current provider's publisher jobs, then retains
one administrative `NativeProfileManager::refresh_broker` operation. It uses the
real Runtime caller, bootstrap AdminParent, active Native profile, verifier and
existing OAuth limits. Core publication effects, CAS failures and owned pending
cleanup remain observable. A failed rotation is terminal for that owner and is
never retried automatically. Cancellation signals the retained operation and
awaits its result; dropping a waiter does not detach the job. Shutdown joins it
before authentication cleanup. A failed Source startup returns its live cleanup
owner; the caller consumes that owner's `shutdown()` while preserving the
original failure, without another token POST.

After success, new selections, credentials and snapshots are captured for the
current maintained projects. A pre-start inventory renewal preserves the normal
first publisher startup and welcome. Rotation and new channel admission use the
configured finite request policy; publisher service operations retain their
cancelable service lifetime. There is no new expiry margin, timer or token cache.
A missing refresh token requires administrative recovery; it does not authorize a
Local credential fallback.

HTTP admission does not wait for the first Now welcome. A terminal publisher
rejection, including HTTP 401 before its first welcome, retires that publisher;
it does not stop Source HTTP or unrelated project publishers. The local HTTP
listener continues enforcing Runtime authentication until explicit service
cancellation or administrative stop, which joins its accepted work and listener.
The App startup fixture covers this pending/rejected publisher boundary without
model files or provider accounts; typed Now errors remain covered by the
publisher contracts and the existing foreground isolation journeys.

The cab0 delta is implemented in [the owner](../../crates/graphit-app/src/foreground_now.rs) and [its retained rotation](../../crates/graphit-app/src/foreground_now/native_refresh.rs). Focused regressions cover retained expiry events, sibling terminal errors, unsent inventory admission, two-project renewal, expired bootstrap and cancellation/remote failure without retry. The live proof keeps a 15-second JWT, zero refresh margin and four-second heartbeat; it requires zero token POSTs after both initial snapshots, exactly one rotation and new snapshots on both channels after the response.

The two-project JWT test disables AST, Memory and Knowledge through production
configuration, isolating real Native renewal and two publishers/subscribers with
no corpus. Expired bootstrap has an independent test. The affected historical
journey keeps its active corpora and indexers and remains an open regression:
heartbeat expiry was observed while the JWT was valid, and Runtime is investigating
read/composition/send phases. The isolated passes do not qualify that broader
Native journey. Admission expiry between lease acquisition and first welcome was
reviewed in source; this packet did not execute a dedicated controlled fixture
for that narrow window. Historical publisher and reconnect results below remain
proof only for their recorded scenarios.

The continuous Foreground owner, local selection changes, stop/join of its
jobs and service-lifetime context belong to `tsk-f37c`. Ordinary finite
requests are not a substitute for that service context. Core and wrapper
producers in `tsk-bcf2` supply a real cancelable service context. The owned
consumer has Source in [Foreground](rust_source_foreground.md), with one
job per retained Core selection. Source HTTP readiness is independent of the
first Now publish/welcome; the publisher's ready signal still requires that
actual welcome. A recovered connection has a new operation inside that same job,
including when a prior connection failed before the first welcome.

A terminal publisher failure remains terminal for that PROJECT publication.
The composing Source observes the error after the job has joined and been
removed, reports its cause and keeps local Task, HTTP, administrative commands
and sibling publishers available. This also applies before the first welcome.
It neither grants Broker access nor retries Forbidden or invents readiness.
The active Memory integration uses an authorized Native write followed by the real
authenticated Kiro Stop hook to request the owned background rescan. It requires a
new semantic generation after denial; a direct Store write alone does not dispatch
that work. This proves the Source hook/model boundary, not Root MCP Memory CRUD or
a native vendor-host run.
`NeedsRefresh` keeps its separate Native rotation path; global rotation failures,
owner join failures and unclassified cleanup failures retain the Source failure
path. Explicit service cancellation still shuts down and joins every owner.
The current Linux focal qualification passed all four affected cases: two-project
isolation/shutdown, Broker-loss recovery, Native refresh negatives and active GGUF
Memory after denial plus the authenticated Stop hook. Each ran one test with no
failures or ignored tests; timings and exact names are retained in
[the Source qualification](rust_source_foreground.md). This does not qualify a
vendor-host run, Windows/macOS, Root Memory CRUD or S3 Enabled.

After Native credential rotation, publishers resume with the same background
lifecycle as startup. The service owner does not wait for every new welcome
under a finite request deadline. Readiness still requires a real publication;
publisher admission and authority failures remain observable through the owned
jobs. Rotation, CAS, cleanup and cancellation failures are preserved. Explicit
PROJECT reconciliation continues to use its caller's deadline.

Its actual-consumer journeys are
`native_foreground_now_two_projects_and_owned_shutdown` and
`native_foreground_now_local_owned_shutdown` in
[foreground_now.rs](../../crates/graphit-app/tests/foreground_now.rs).
The final Linux consumer run passed both tests in 256.35 seconds. Current
Clippy with warnings as errors passed in 34.60 seconds, and three Source
configuration regressions passed. The Local journey was isolated after an
earlier observer upgrade returned HTTP 504; isolation does not establish the
cause of that deadline. Reproducible qualification commands:

~~~text
cargo xtask cargo test --locked --offline -j4 -p graphit-app --test foreground_now native_foreground_now_two_projects_and_owned_shutdown -- --ignored --exact --test-threads=1
cargo xtask cargo test --locked --offline -j4 -p graphit-app --test foreground_now native_foreground_now_local_owned_shutdown -- --ignored --exact --test-threads=1
~~~

This consumer qualification is separate from the completed primitive
publisher proof in `tsk-8923`. The absence-of-model/global-configuration case
has no active M jobs. The real `tsk-6d5a` proof covers M supervisors without
active Now. The historical
`native_foreground_now_with_active_memory_terminal_joins` passed in 104.39
seconds: compiled model server, qualified CPU ORT, global configuration only
in the temporary installation, identity from the real model resolvers and SDK
semantic generations published for both physical projects. An ACL terminal
in Now withdrew both channels and the Source. That global shutdown expectation
is superseded by publisher isolation. This historical ORT/ONNX execution does
not qualify the current GGUF consumer. The maintained case is now
`native_foreground_now_with_active_memory_isolates_publishers_until_stop`:
it uses the real GGUF fixture with `GRAPHIT_EMBEDDINGGEMMA2_MODEL` and
`GRAPHIT_TEST_MODEL_SERVER`, preserves active Memory after Now denial and
requires a fresh semantic publication before explicit service stop. Its query
identity comes from the current metadata/Memory producers; no model lease or
vector is fabricated. This consumer adaptation is not a new model-journey PASS.

An earlier attempt returned inventory 504/Deadline before M started. The
qualified run first calls the real Local verifier, which passed in 117 ms
and warms that path. This neither identifies the earlier deadline cause nor
proves cold reconnection. Startup retains its cause and owner for explicit
recovery; product timeouts were not increased.
Producer availability on Linux does not qualify agent compatibility or Windows
and macOS. Agent compatibility follows the current documented protocol and does
not require an installed binary, account or provider plan in these tests.

## Verification boundary

The final current-source publisher journey passed on Linux: one native test in
66.95 seconds, after a 4.37-second compilation. The fixture uses real DaemonAuth,
AuthStore, Core PROJECT captures, Native JWT/JWKS and explicit Local credentials,
Lance Task/Memory/Knowledge corpora and the compositor through TCP WebSocket
Broker backed by PostgreSQL TLS VerifyFull with a NOSUPERUSER application role.
The command and final log are:

~~~text
cargo test --locked --offline -j4 -p graphit-daemon --test now_publisher now_publisher_broker_postgres -- --ignored --exact --test-threads=1
.build/rust-runtime-now-publisher-8923/native13-final.log
~~~

This journey covers two channels, actual inherited branch state, fresh heads,
full EOF, decimal precision, payload privacy, partial and complete corpus denials,
invalid corpus, physically absent corpus, registration withdrawal, owner conflict,
supersession, service denial, byte caps, pending-future abandonment, cancellation,
finite deadlines, service Original without a deadline, independent signed-token
expiry, drop and shutdown. It retains all real Task/session claims and runs with
the default stack. It does not inject Broker backpressure or close 1013; those
recovery mappings are covered by the library units and source contract.

Current Linux library units also passed: three tests for exact controls and all
twelve real notices, checked decimal cadence, escaped byte bounds, one-pass
serialization, private causes and close 1013. The command and log are:

~~~text
cargo test --locked --offline -j4 -p graphit-daemon --lib now_publisher::tests
.build/rust-runtime-now-publisher-8923/unit04.log
~~~

Scoped strict Clippy passed for the daemon library and both publisher/subscriber
test targets (`--no-deps -D warnings`, 1.03 seconds). The affected subscriber's
native regression also passed independently: one test in 33.67 seconds after
its four optional-deadline/welcome adapters. Logs are `strict02.log` and
`subscriber-native01.log` in the same qualification directory. Scoped fmt passed.
Six current subscriber library tests also passed (`subscriber-unit01.log`),
covering strict envelopes, cadence, escaped bytes and the DATA snapshot adapter.
The subscriber proof covers its real transport lifecycle; it is distinct from
the publisher's real compositor journey.
Foreground and Windows/macOS remain separate integration proofs. Agent
compatibility follows its documented current protocol; no provider account, plan
or installed agent is required by these transport tests.

The executions above are retained in the durable Task record tsk-8923. A later
external deletion removed the qualification directory, including its logs and
receipt; these paths identify the artifacts observed during qualification and
are no longer available. No historical logs were reconstructed. Initial required
Task checks, Knowledge summary, SDK branch inheritance and asynchronous remote
rejection expectations were corrected in the owned fixture. Earlier oversized
fixture poll frames aborted with stack overflow; six smaller async phases
preserved scenario coverage, authority, claims and Original. The independent
Records investigation tsk-f382 passed its public Task-claim baseline and did not
establish an SDK defect. Native09 reached the claim return; Native10 and Native11
were subsequent fixture failures. Native12 passed before diagnostic cleanup;
Native13-final passed after removal of markers and diagnostic panic handling.
No SDK patch, stack setting, mock authority or skipped claim was used.

The byte limits are logical payload limits. They do not claim a hard process
RSS cap, preemptive interruption of synchronous native I/O or atomic reads
across the three independently pinned corpora.

The combined process journey is tracked separately in
[Now and Presence integration](rust_now_presence_integration.md).

## Native Foreground fixture operation window

The ordinary two-PROJECT Native Foreground journey passes an explicit
`RealtimeHttpConfig` with a 30-second operation timeout to its real Broker
fixture. It deliberately holds `resource_acl_state` in `ACCESS EXCLUSIVE` mode,
observes an actual admission waiter within its existing 30-second observation
window, checks authenticated HTTP status while the Now welcome remains blocked,
then commits. HTTP readiness does not establish publisher readiness. The former
assertion that HTTP must remain unavailable is superseded by concurrent Source
startup; its ordering proof belongs to the affected App startup focals, while
the unchanged Native15 channel/lifecycle evidence is reused. A shared
2-second operation window imposed an unintended startup latency requirement on
that synchronization; it was not a deadline assertion of this journey.

The shared `realtime()` configuration and all three NativeRefresh callers keep
their existing 2-second operation timeout. This fixture reconciliation changes
neither Original nor JWT expiry, heartbeat/cadence, ACL/HTTP ordering, cancellation
or the dedicated expiry checks. The operation window is explicit configuration,
not a public default or an authentication TTL. The technical activation
configuration is reconciled separately by its owner.

That journey uses the productive Source Runtime `Read(Task)` permission. Its seed
creates and claims one Task and its linked session per PROJECT. The initial
snapshot must expose their actual SDK IDs, titles and linkage as available heads,
with session counts of zero completed and one total. It has no Task-denial setup.
The obsolete forbidden/null expectation failed Native13 and remains recorded;
the fixture now checks the authorized seed data. Actual denied-module masking
keeps its unchanged component proofs, including fresh-admission revocation.
The separate signed-expiry subcase authors no records and has the same actual
read permission. Task, sessions and Memory remain `no_corpus` with
[]/0/false/null. Knowledge may already have an empty document generation committed
by the Source job: `available` with those same empty fields is distinct from
`no_corpus`. Neither state accepts an error or foreign/nonempty data.
Native14 retained a second obsolete Task-forbidden expectation there; its failure
is preserved after the fixture reconciliation. The signed JWT expiry and terminal
authentication assertions remain unchanged.

The earlier combined Native Foreground diagnostic confirmed that the Broker
removed both channels for heartbeat expiry. Its measured consumer polls remained
below 400 milliseconds; awaited recomposition time does not establish a blocked
poll, and nested measurements cannot be added. That diagnostic alone did not
qualify the Native journey. Temporary Runtime instrumentation was removed before
the final functional execution.

Native15 passed the complete two-PROJECT journey: exact seeded Task/session IDs,
fresh Memory heads, a repeated genuine observation with unchanged time followed
by a newer composition, version changes/restarts, actual takeover/ACL/registration
negatives, signed authentication expiry, no-corpus states, cancellation and owned
shutdown. This proves the tested Linux scenario; it does not install or activate
the release, repeat model inference or qualify native Windows/macOS hosts.

The affected Local owned-shutdown and active-Memory fixtures also use the current
Source Runtime `Read(Task)` permission. Neither seeds Task/session records, so
their Task/session sections must be `no_corpus` with []/0/false/null. The older
ordinary-Runtime ceiling without Task read permission does not describe this
current Source policy. These fixtures retain their real Memory publication,
authority negatives and owned lifecycle; their focused results are recorded
separately from Native15.
From one final integration ELF, Local owned-shutdown passed in 24.53 seconds.
The active-Memory case failed with HTTP 504 during Local subscriber upgrade,
before its absent-Task or Memory-generation assertions, under its previous
2-second operation window. Source maps this response to the Local operation
deadline before WebSocket upgrade; it does not isolate verification, SQL or
scheduling delay. That ordinary active-Memory journey has no 2-second latency
requirement. Its caller now passes an explicit 30-second `RealtimeHttpConfig`
through the existing Local fixture helpers. Shared `realtime()`, all other Local
callers and registry/read/list windows keep 2 seconds. Original, token expiry,
heartbeat/cadence and security negatives are unchanged. The failed attempt and
unchanged Native15 and Local owned-shutdown passes remain recorded; active-Memory
qualification remains separate. The focused 30-second rerun reached both initial
subscriber snapshots and their Task/session assertions, then failed in 93.66
seconds while awaiting actual Memory publication: Source ended with
`Remote(Deadline)` and completed owned cleanup. The log does not identify the
original Broker command or establish Memory-generation success. This attempt
remains historical and unqualified. That recorded consumer revision recovered
remote `closed`/`deadline` only after readiness through a fresh admission; it did not
extend the old operation's deadline or retry local SDK/Original expiry. The
affected classifier passed its focused test, including all twelve remote codes
and local terminal errors. The first consumer attempt after that change failed
with HTTP 504 `Deadline`/`MayHaveApplied` while waiting for a combined snapshot;
it preceded the final Broker dependency rebuild and remains historical. Cargo
then rebuilt the final auth-first Broker and consumer. That Local active-Memory
execution failed in 150.58 seconds when the fixture's direct subscriber received
`Remote(Deadline)` during its initial snapshot loop. The fixture canceled Source,
which returned `Stopped` with owned cleanup completed. The borrowed subscriber
ends on any notice error and has no reconnect loop. This result does not
establish a publisher exit, two successful initial views or completed Memory
generations. That result did not qualify Local active Memory; Native15 and
Local owned-shutdown proofs are reused.

After the final Local authorization read-only and Core lookup producer changes,
the unchanged Local active-Memory selector ran once on App25. It failed in
79.67 seconds: the direct subscriber returned `Remote(Deadline)` in the initial
snapshot loop. After the helper canceled the service, joining Source returned
`Remote(OutcomeUncertain)`. The helper took the retained failure owner and
completed its additional shutdown successfully; the final diagnostic's
`retained_owner: false` is after that cleanup. This log does not establish which
error preceded cancellation, two expected instance views or completed Memory
generations. Its Local verifier baseline passed in 3 ms; Knowledge commits are
not Memory proof or a causal explanation of the prior failure. M01–M05 remain
recorded, without retry expansion or a new deadline.
The diagnostic used by M05 omitted `ForegroundFailure.service`. When shutdown
fails, Source preserves the prior operational error there, while `error` reports
the shutdown failure. Thus the printed `OutcomeUncertain` does not identify the
first Source failure and may describe cleanup; M05 does not expose that preserved
cause. The current diagnostic includes that field.

After the final Core local-file/global-EOF, App concurrent-startup, Knowledge
context and Broker read-commit changes, App26 rebuilt the integration target.
The unchanged Local active-Memory selector passed once in M06 (86.36 seconds).
It checked both expected instance views, empty Task/session corpus fields,
both actual Memory generations, and terminal presence revocation with owned
shutdown and cleanup. The exact private fixture container was absent after
completion, without external deletion or signals. M01–M05 remain historical;
this combined pass does not identify which producer change resolved them.
Native15, owned-shutdown and classifier evidence are reused. This Linux journey
does not prove deployed Source integration or native Windows/macOS behavior.
