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

Source serves `POST /admin/auth` on its existing listener with the same `DaemonAuth`,
runtime key and singleton. Runtime explicitly permits `ManageAuthProfiles`.
Local and PROJECT origins do not acquire that permission. The route authenticates
the current key and obtains a genuine administrative session and parent before
executing `NativeProfileManager` under one finite original request.

The request uses the serde-enabled `auth_cli::Command` shared with Clap. Protected
credential fields have explicit private request codecs; commands do not implement
Debug. The Runtime header is sensitive. Hub policy bounds input/output, URLs and
JSON. Tokens, private documents and command credentials never enter response events
or diagnostics.

The response is `application/x-ndjson` with `Cache-Control: no-store`. Events contain
checked `AuthorizationUrl` or `DeviceAuthorization`, redacted `Read`, successful
`Publication { effect }`, or causal `Failure { message, remote_effect }`.
Publication reports `Applied` or `Unchanged`. Errors before commit leave the previous
Auth document in place; output failure does not roll back a completed publication.
Uncertain OAuth effects remain explicit and are not retried automatically.

`auth_active::run_if_active` accepts a content byte bound, reads discovery with the
real SDK and probes `FileLock::is_locked`. Absent discovery or a released singleton
returns `None` before reading the private key or sending HTTP. The probe does not
mutate the lock file or grant ownership. The [standalone entry](rust_auth_entrypoint.md)
must acquire the singleton before creating its runtime key and authority.
A stale discovery record is preserved for its Source owner to replace.

With the singleton held, the client reads the current private key and calls the
active listener. Transport, authentication, cancellation, deadline and output
errors remain errors. Connection refusal and HTTP 401/403 do not select standalone
execution or rotate a live owner's key.

Broker operations use the selected provider's actual kind, configuration and
revision. Real `BrokerVerifier`, `NativeOAuth` and command helpers retain PKCE,
state, nonce, trusted JWKS, signed ID/access claims and refresh CAS checks. The
callback listener remains owned through exchange and publication. The CLI announces
the checked URL and uses its existing browser launcher unless `--non-interactive`
is selected. Device login uses the separate SDK polling flow.

Broker JWTs retain their signed validity and existing claim checks; UserInfo is
not required for each request. This consumer does not alter token durations,
PROJECT permissions or browser authority.

Response EOF and service cancellation cancel the original and await the native
operation. Shutdown drains actual spawned jobs before runtime removal. Callback
listeners and real process resources retain their ordinary lifecycle ownership.

## Verification

`tests/auth_active.rs` exercises the actual listener, issuer, CLI transport and
signed loopback Broker. Cases cover Local CRUD/CAS/redaction, authentication denial,
compiled CLI, Broker login/refresh, Device polling, response EOF and shutdown of
pending work. `tests/daemon_control.rs` verifies authenticated status/stop and actual
child completion. These tests use private temporary installations. They do not
qualify a physical browser launch or another native operating system.

```text
cargo xtask cargo test -p graphit-app --test integration --locked --offline auth_active::
cargo xtask cargo test -p graphit-app --test integration --locked --offline daemon_control::
```

Implementation: [admin_auth.rs](../../crates/graphit-app/src/admin_auth.rs),
[auth_active.rs](../../crates/graphit-app/src/auth_active.rs).
Tests: [auth_active.rs](../../crates/graphit-app/tests/auth_active.rs).
