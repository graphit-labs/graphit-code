---
title: Active daemon Auth administration
description: Native typed Auth commands on the retained daemon issuer and listener.
type: specification
status: implemented
references:
  - target: {type: task, id: tsk-dc33}
    relation: implements
  - target: {type: task, id: tsk-f416}
    relation: refines
---

# Active daemon Auth administration

The existing Source listener also serves `POST /admin/auth`. It retains the
same `DaemonAuth`, runtime key and singleton as Source. Foreground startup
explicitly permits `ManageAuthProfiles` for Runtime administration; Local and
PROJECT origins do not obtain that permission. This route authenticates the
current physical Runtime key, obtains the existing `AdminParent`, and executes
`NativeProfileManager` under one original finite request. Stored identity and
command JSON do not create authority.

Before mutable commands, the same authorized native manager discards reserved
private scratch left by a prior process through its existing locked recovery API.
It does not interpret or promote that content; recovery effects/failures remain
causal and do not replay a command or CAS. Reads skip this extra recovery I/O.

The request uses the same serde-enabled `auth_cli::Command` as Clap. Seven
protected credential inputs have explicit private request codecs; commands do
not implement Debug. The native Runtime header is marked sensitive. Input,
output, URL and JSON limits come from the existing Hub content policy. Tokens,
private documents and commands never enter response events or diagnostics.

The response is `application/x-ndjson` with `Cache-Control: no-store`:
checked `AuthorizationUrl`, redacted `Read`, actual `Publication`, or causal
`Failure`. Publication records its original physical effect, failure, cleanup
effect/failure and pending status. An applied or unknown publication is not
retried or described as rolled back. Failed SDK cleanup retains its actual
owned proof for a fresh authenticated cleanup request and shutdown.

The client API `auth_active::run_if_active` reads native discovery through the
existing SDK and probes the installation singleton with `FileLock::is_locked`.
Absent discovery or a released singleton returns `None` before reading the
private runtime key or sending an administrative request. Discovery can survive
an interrupted process; its recorded PID and endpoint do not prove ownership.
The probe does not create, truncate or remove the lock file. The standalone
[Auth entry](rust_auth_entrypoint.md) must still acquire the singleton itself,
validate installation identity and recover its own runtime through `DaemonAuth`.
A concurrent owner therefore prevents standalone acquisition.

With the singleton held, the client reads the current private key and uses the
active administrative route. Discovery, transport, authentication, cancellation
and output errors remain errors; connection refusal and HTTP 401/403 do not
trigger standalone execution or rotate a live key. Native Broker login/refresh
also work through the standalone entry after the previous owner has ended.
The stale discovery record remains intact: its removal belongs to its Source
owner, and a future Source publication replaces it under exclusive ownership.

Native Broker dispatch uses the actual captured provider kind/configuration and
revision. Existing `BrokerVerifier`, `NativeOAuth` and native command helpers
validate metadata, trusted JWKS, PKCE, state, nonce, signed ID/access claims and
refresh provider/token CAS. The loopback callback listener remains server-owned
through exchange and publication. Login sends its checked URL to the CLI, which
prints it and invokes the mature platform browser launcher locally. With
`--non-interactive`, it prints the URL without launching a browser; external
completion must still arrive before the original deadline.

Broker access JWTs remain valid locally until their signed expiry; signature,
issuer/audience, purpose/client/resource and time checks remain required.
UserInfo is not mandatory on each request. Broker access TTL defaults to ten
minutes, with configured one-minute to one-hour bounds; this consumer does not
change it. Native issuer defaults remain separate: roles thirty minutes and
administrative sessions fifteen minutes. Profile identity is not PROJECT
permission, and this native administrative transport grants no browser access.

Response EOF and service cancellation cancel the original, await the native
operation, release the callback and drain actual jobs before removing runtime
resources. Failed cleanup keeps its owner instead of unlinking foreign files.
The queue stores only real spawned work and real pending cleanup proofs; no
predictive reservation, quota ledger or flow framework is introduced.

## Verification

Current Main source passes locked offline App library/executable checks and
scoped strict Clippy for the App library, executable and affected Auth/Source
test targets. Six native integration cases pass, covering retained daemon
sockets, Local commands and CAS/redaction/denial, private previous-process scratch
recovery, compiled CLI outside a project, signed Broker callback and refresh
through SDK and compiled CLI, discovery absence and cancelled originals, response
EOF and service shutdown of pending callback work. Existing foreground six,
Source HTTP seven and Source cancellation/join two affected cases also pass;
standalone Auth's five-case guard passed. The intentionally unserved active-owner
fixture passes its final focal case with an explicit short request deadline
(.35s), preserving actual key/document bytes. Task receipts retain the commands
and earlier failed fixture/lint diagnostics. These are Linux native/component/socket/executable
receipts, not physical browser launch, PROJECT authorization, or other platforms.

Implementation: [admin_auth.rs](../../crates/graphit-app/src/admin_auth.rs),
[auth_active.rs](../../crates/graphit-app/src/auth_active.rs).
Fixtures: [auth_active.rs](../../crates/graphit-app/tests/auth_active.rs).
