---
title: Runtime Now subscription
content-type: reference
audience: developers
status: current-source-requalification
updated: 2026-10-12
references:
  - target: {type: task, id: tsk-d1da}
    relation: implements
  - target: {type: task, id: tsk-60d0}
    relation: implemented_by
  - target: {type: task, id: tsk-4970}
    relation: uses
  - target: {type: task, id: tsk-ff63}
    relation: uses
  - target: {type: task, id: tsk-e381}
    relation: uses
  - target: {type: task, id: tsk-c52b}
    relation: uses
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: task, id: tsk-8923}
    relation: relates_to
---

# Runtime Now subscription

`graphit_daemon::now_subscription` consumes the Broker's current authorized Now
view through the real [captured WebSocket transport](rust_http_websocket.md).
Its snapshots contain small PROJECT heads and counts. It reads no local corpus,
USER body, history, profile setting or credential file on its own. It does not
create local grants from remote DATA, cache an old view or recalculate a corpus
on heartbeat.

## Admission and caller ownership

The same configured issuer captures the caller's
[PresenceRead selection](presence_service_credentials.md). Runtime and Local
callers use `capture_presence_read_service`; a current Role uses
`capture_role_presence_read_service`. A Role is not converted to a Runtime caller.
An absent RemoteRequest permission stays a denial.

For Native mode, the caller retains the
[warm verifier owner](rust_daemon_broker_verifiers.md), calls `get` with its actual
selection, then obtains `selection.native_credential(&verifier).await`. For the
explicit Local Broker key mode it obtains `selection.credential()`. There is no
JWT format detection, fallback credential, second AuthStore, token refresh or
provider default. Both paths produce a real borrowed `ServiceCredentialLease`.

The caller owns selection, verifier and lease throughout the subscription. The
subscription borrows the lease and Original; it cannot restore them into a
static task or own a self-referential lease. `ServiceBindingView::Broker` must
have the actual PresenceRead purpose. The lease's captured snapshot identity must
match the supplied Original before any native connection. Endpoint and Bearer
derive from that checked binding and lease. Credentials are sent only in a
unique sensitive Authorization header; no query string, diagnostic or snapshot
contains them.

`start_now_subscription(&WebSocketClient, &ServiceCredentialLease,
&RequestContext, NowSubscriptionConfig)` preserves that context. Config receives
a canonical `ProjectId`, independent optional literal instance/version filters,
and positive command/snapshot byte caps. The transport captures the existing
network policy and its own frame/message/send limits; request steps and
heartbeats never renew the Original's deadline or cancellation signal.

The actual bcf2 context can carry an ordinary finite deadline or a caller-owned
cancelable service Original with no clock deadline. The adapter preserves this
optional deadline through the real transport; initial welcome uses receive(),
and negotiated heartbeat ticks use receive_until(Instant). No deadline is
fabricated or renewed, and signed token expiry remains independent.

## Shared DATA decoder for HTTP and NDJSON

`decode_now_snapshot(bytes: &[u8], expected_project: ProjectId,
instance_filter: Option<&str>, version_filter: Option<&str>,
snapshot_bytes: NonZeroUsize) -> Result<RemoteNowSnapshot, NowSubscriptionError>`
validates one complete snapshot envelope. HTTP and NDJSON consumers pass the
actual frame bytes and their expected project and independent literal filters.
The whole byte cap is enforced before parsing. The existing object-only parser
uses `serde_json::from_slice`, rejects invalid UTF-8, unknown or duplicate fields,
and consumes the real Broker `NowSnapshotData` validator. The helper accepts
only a snapshot; welcome and error notices are not snapshots.

The helper and subscriber share the same project, nested-project, strict pair
ordering, duplicate and filter validation. Invalid caller filters return
InvalidInput; over-cap bytes return Limit. Protocol/shape errors retain the
existing private causal diagnostics. No bytes-to-String copy, Value prepass or
irrelevant command config is introduced. The returned DATA has private
fields and borrowed getters. Transport admission remains owned by its caller;
decoding does not create a caller, lease, Original or authority from serialized
content.

The focused decoder fixtures exercise DATA validation only. They do not qualify
native Auth, network transport, daemon ownership or an installed host. Current
subscriber service-context qualification remains tracked by the publisher's
native journey independently of this helper.

## Protocol and complete replacement

The subscriber sends the first bounded Text command:

```json
{"op":"subscribe","project_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}
```

Each absent filter is omitted, independently. Null, empty, wildcard or unknown
fields are not substitutes for omission. The typed outbound config cannot
represent duplicate or unknown keys. The serializer bounds escaped UTF-8 bytes
against physical vector length before reserving or appending, checks Original
and preserves its first causal error. There is no quota or counting pass.

Only after actual PROJECT/channel admission does the Broker send welcome. Its
`heartbeat_timeout_ns` is a canonical unsigned decimal string. Runtime parses
the exact value, checks conversion to Duration, and uses its positive half,
strictly below the advertised timeout. A nonrepresentable or zero half is a
finite protocol failure, not a new interval knob or TTL.

Subsequent heartbeat Text is exactly `{"op":"heartbeat"}`. Runtime checks the
real lease around native awaits and output. The transport's elapsed wakeup has
priority over queued messages, preserving pending fragmented-read state while
the consumer checks admission and sends heartbeat. Transport Ping/Pong does not
renew Presence. The Broker performs fresh PROJECT authorization and replay;
this consumer does not manufacture a new grant or refresh unchanged local heads.

`next()` returns an owned `RemoteNowSnapshot`. Serialization has this allowlist:

```json
{"type":"snapshot","project_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","instances":[]}
```

Each populated instance contains only instance ID, literal version and the
producer's validated `NowSnapshotData`. The consumer reuses its full strict
Deserialize implementation. Private protocol and instance envelopes require
objects, reject unknown/duplicate fields and apply the whole-message cap before
decoding. Project identity must match the request and every nested Now object;
each optional filter is restrictive. Instances are strictly sorted by the
lexicographic `(instance_id, version)` pair; duplicates are rejected. Counts and
decimal timestamps preserve the producer's precision, including forbidden
sections with empty rows and null counts.

Every successful snapshot replaces the whole previous view. An authorized
`instances: []` is a current empty view, not NoCorpus. The API retains no old
snapshot or history. Its read-only getters and Serialize implementation are not
local admissions or proof of an active principal.

An authorization change cannot retract an already queued snapshot. Under the
same finite Original, the consumer continues reading until it receives the
Broker's current replacement or terminal Notice. Integration checks therefore
observe the actual four section masks and Notice rather than assume that the
immediately next frame reflects a concurrent ACL change.

## Errors, explicit recovery and shutdown

The Broker's Notice precedes Close. Its current finite codes are `invalid_data`,
`too_large`, `unauthenticated`, `forbidden`, `deadline`,
`authorization_unavailable`, `closed`, `superseded`, `backpressure`,
`outcome_uncertain`, `not_registered` and `owner_conflict`. The list includes the
server's actual error conversions as well as its nine constants.
No aliases or obsolete extra codes are accepted.

Notice backpressure or Close 1013 returns `RecoveryRequired` with that actual
cause and ends the socket. The caller removes its previous view, ends the old
borrow, captures current Auth and explicitly starts again under the same
Original to obtain full replay. There is no automatic infinite retry, retry
quota, delta catch-up or guessed sequence. A denied request is not retried with
another credential mode. Pre-upgrade HTTP errors retain the actual SDK numeric
status, including 401, 403, 503 and 504; response strings are not used to infer it.

Any error, cancellation, invalid frame, deadline, Close or abandoned `next`
future ends the native socket. The caller must withdraw its displayed view
before handling that result. `shutdown(self)` consumes and closes an active
socket within the same Original; a terminal owner has no remaining socket.
Dropping it drops the native owner, including an interrupted send's pending SDK
buffer. It never flushes an extra error/Close after an incomplete send and does
not roll back bytes already sent.

The subscription spawns no jobs and has no Arc cycle. Source/CLI consumers that
spawn their own stream jobs must cancel and join those jobs before releasing
Auth owners. Internal Hyper upgrade/resolver tasks belong to the mature vendor
transport; this API does not claim a join for them. Actual admission, request,
Serde and transport causes stay behind finite redacted wrappers. Diagnostics
and exposed error-source chains do not reveal credentials, private response
strings or source paths.

## Current verification boundary

Source is [now_subscription.rs](../../crates/graphit-daemon/src/now_subscription.rs).
Three private protocol/cadence/writer checks passed, including all twelve actual
Notice-to-cause mappings. Strict Clippy passed for the current Daemon library and
native test target. The Linux integration uses real DaemonAuth/AuthStore service
captures, Ed25519 JWT/JWKS, opaque credentials, the Broker TCP WebSocket routers
and a PostgreSQL TLS store whose application role is NOSUPERUSER.

The native journey passed with three Native/Local publisher channels, complete
and empty replacements, independent literal filters, decimal timestamp precision,
channel withdrawal and registered-version removal (`not_registered`/1008), and
terminal abandoned reads. It also passed current four-section forbidden masks,
PresenceRead revocation, profile revision invalidation, cancellation and Original
deadline. A real PostgreSQL lock on Local authentication plus Native publications
caused broadcast lag/1013; a fresh actual capture under the same Original obtained
full replay. Remote opaque expiry returned HTTP401, a signed Native JWT lacking
the required scope returned HTTP403, the closed runtime returned HTTP503, and a real SQL lock
exhausted the existing Broker operation deadline and returned HTTP504.
Different Original, invalid filter and snapshot byte-cap cases also passed.

Runtime shutdown joined the Broker connection jobs; TCP server tasks, sockets,
SQL connections/pool and the owned Docker fixture were explicitly closed. The
main fixture's advertised 60s interval keeps publishers active during its finite
Original 40s; the isolated lag runtime advertises 2s, and the separate Original
expiry case remains 500ms. The main interval alone is not a heartbeat/expiry proof.
Fixture ACL replacement is one PostgreSQL transaction, so its module-only change
does not create a transient interval with no PresenceRead grant. Queued snapshots
are consumed until the actual current replacement or terminal Notice; denials
remain terminal, with no automatic retry.

The focused qualification before the bcf2 optional-deadline adapter is recorded
in `tsk-60d0` and
`.build/rust-runtime-now-subscription-60d0/`: `native-12.log` passed one native
journey (34.64s); `unit-03.log` passed three private checks; `strict-06.log` passed
current scoped Clippy (1.10s). Earlier failed logs remain historical. The
transport's own HTTP/WS checks qualify its separate library. The subscriber's
native journey covers the actual pre-upgrade HTTP401/403/503/504 classifications.
CLI/Source/instances consumers are separate deliveries `tsk-7f8c`/`tsk-9e52` using
this same API. No Windows/macOS, installed Kiro or complete migration result is
inferred from a protocol fixture or these focused Linux checks.

Current adapter requalification belongs to tsk-8923. The previous Native12 and
unit03 evidence remains historical until the affected finite/service cancellation
journeys are executed against the current Source.
