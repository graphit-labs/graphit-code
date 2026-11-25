# Installation-owned Auth entry component

`graphit_app::auth_entry::run` accepts a typed Auth command, `InstallationPaths`,
a resolved `HubResourcePolicy`, the original `RequestContext` and output writers.
It returns `io::Result<ExitCode>` and operates on the configured installation
without selecting a project, agent or Git checkout.

The entry starts `DaemonAuth` with no trusted project roots. Runtime has only
administrative `ManageAuthProfiles`; ordinary and Local administrative permissions
are empty. The owned current runtime key authenticates the caller. Its genuine
`bootstrap_admin` session and `admin_parent` authorize `NativeProfileManager` under
the original cancellation and deadline. Paths use the normal filesystem APIs;
private credentials retain the SDK's actual access checks.

The executable first tries [active daemon administration](rust_auth_active_admin.md).
Absent discovery or a released installation singleton permits standalone startup.
The standalone entry must acquire the singleton itself, so a concurrent daemon
still prevents acquisition. Transport or authorization failure with a live owner
remains an error. Discovery from an ended owner is preserved for Source to replace.

Provider CRUD, Local login, account selection and logout use `auth_cli::execute`.
Broker login selects the explicitly requested provider. Refresh selects the actual
active profile and its current provider. They use the real `BrokerVerifier`,
`NativeOAuth` and profile-manager helpers. `--expected-revision` is checked before
the token request, and actual provider/profile/token CAS remains required.
PKCE, state, nonce, signed token checks and callback ownership are unchanged.
Device login uses its separate SDK flow. An uncertain remote effect is reported
and never triggers an automatic token-request retry.

Content limits come from Hub policy. Native role TTL is thirty minutes with a
24-hour maximum; administrative sessions last fifteen minutes with a one-hour
maximum. These issuer values are separate from Broker token policy.

Reads emit redacted JSON. Successful mutations emit `Applied` or `Unchanged` from
`AuthPublicationEffect`. Command failures emit causal diagnostics and return a
failure exit code. Startup, output and runtime-removal errors return `io::Error`.
An output failure after publication leaves the completed publication applied.
Credentials and private document bodies never enter output or diagnostics.

After the command, the entry explicitly removes its matching runtime key with a
fresh finite request from the same policy. This allows removal after the command's
original is cancelled. Removal failures are returned; dropping the entry releases
its native owner and singleton. A separate explicit invocation acquires ownership
and regenerates the derived runtime key. No command or Auth CAS is replayed.

## Verification

`tests/auth_entry.rs` covers actual private temporary installations, typed commands,
CAS, redaction, cancellation/deadline, failing output and runtime removal. A key
replacement is preserved on removal conflict; a separate command verifies released
ownership. Compiled CLI cases exercise standalone signed Broker login and refresh,
Device polling and ended-owner discovery. `auth_entry.rs` also checks exclusion by
a live daemon. These Linux component/executable checks do not qualify other hosts
or physical browser launch.

Run the affected targets with the repository's coordinated build workflow:

```text
cargo xtask cargo test -p graphit-app --test integration --locked --offline auth_entry::
```

Implementation: [auth_entry.rs](../../crates/graphit-app/src/auth_entry.rs).
Tests: [auth_entry.rs](../../crates/graphit-app/tests/auth_entry.rs).
Producer: [Local Auth CLI component](native_auth_cli.md).
