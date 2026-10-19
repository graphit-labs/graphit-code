# Installation-owned Auth entry component

`graphit_app::auth_entry::run` accepts the existing typed Auth command,
`InstallationPaths`, a resolved `HubResourcePolicy`, the caller's original
`RequestContext`, and stdout/stderr writers. It operates on that private
installation without discovering a project, selecting an agent, reading Git,
or consulting project inventory. It contains no argument parser.

The entry starts the existing `DaemonAuth` with no trusted project roots.
Its Runtime origin has only administrative `ManageAuthProfiles`; ordinary
permissions and Local administrative permissions are empty. A checked owned
runtime key admits the original caller, then `bootstrap_admin`/`admin_parent`
authorize `NativeProfileManager` and the existing `auth_cli::execute` command.
Before executing, the entry invokes `recover_stage` under the same original
administrative parent to discard a leftover native Auth scratch file. It never
interprets or promotes scratch contents, nor repeats a CAS.
This is the existing installation administrative path, not a project grant.

Profile updates use the current Unix time. Local login, provider CRUD, account
selection and logout use the synchronous executor. Native Broker login and refresh
compose the existing verifier and `NativeOAuth` with the profile-manager wrappers.
Login selects the explicitly requested provider; refresh selects the actual active
profile and its current provider. `--expected-revision` is checked before the token
request, and provider/profile/token CAS remains in the existing SDK. Access-token
expiry does not require starting Source before an administrative refresh.
The central executable first uses [active daemon administration](rust_auth_active_admin.md).
It calls this standalone entry when native discovery is absent or the installation
singleton has been released. The dispatch probe grants no ownership; this entry
still acquires the singleton exclusively, including when another owner starts
between the probe and acquisition. Browser PROJECT authorization is separate. An existing daemon
singleton rejects this standalone entry; the entry does not adopt, replace,
or rotate its credential. Broker operations use their asynchronous native executor;
they are not sent to the synchronous Local executor.

Native document limits come from the Hub input policy. Native role TTL defaults
are 30 minutes with a 24-hour maximum; administrative sessions last 15 minutes
with a one-hour maximum. These values govern the native issuer and are separate
from Broker access-token policy. No process/accounting quota or reservation is
added here.

Read output is the executor's redacted JSON projection. Publication output
records the native physical effect. Failure diagnostics preserve the actual
publication failure, cleanup effect/failure and pending status; an applied
publication is never described as rolled back. This standalone entry neither logs nor serializes raw command credentials.
The active HTTP consumer separately serializes explicit protected request inputs. A writer failure after publication remains an
output failure while the physical publication remains applied.

Native errors preserve `remote_effect`, including `MayHaveApplied`; the entry does
not retry an uncertain token request or a failed CAS. Login retains its existing
PKCE, state, nonce, signed ID/access-token checks and callback ownership. No new
UserInfo admission or token-duration policy is introduced.

With Source stopped, an operator can read `account show`
for the current document revision, then run
`graphit --global-dir INSTALLATION refresh --expected-revision REVISION`.
`Applied` reports a completed native publication; the operator retains control of
starting Source. A stale revision leaves the document unchanged before a token
request. An uncertain remote effect is not an instruction to repeat the old
refresh credential.
An interrupted Source can leave discovery behind. A released singleton allows
the same standalone acquisition without manually deleting discovery or credentials.
The command preserves that old discovery record and cleans only its own runtime
key; a future Source owns its discovery publication.

After the command ends, the entry explicitly cleans its runtime key and
releases the issuer and singleton. Cleanup uses a fresh finite request from the
same Hub policy, so a cancelled original does not prevent owned cleanup.
Pending document scratch cleanup obtains a genuine administrative parent from
that same owned issuer. Command/CAS publication is not replayed.

`AuthEntryFailure` exposes the cause, cleanup failure and whether native
resources remain. Its `cleanup` continuation only finishes those owned
resources. Failed cleanup retains the actual daemon/runtime/recovery owner,
pending proof and singleton where necessary; no foreign key or lock pathname
is unlinked as a fallback. Startup errors also use the SDK's retained
`StartupResources` and explicit recovery.

The eight component tests use real private scratch installations, native
publication, the existing typed command parser, cancellation/deadline and failing
output writers. The compiled CLI tests use the existing signed HTTP
Broker fixture to exercise standalone login, stale refresh CAS, rotation,
redaction and cleanup. The ended-owner case publishes discovery through the SDK,
preserves live-owner connection refusal and HTTP 401/403 without OAuth or file
changes, then releases the owner without cleanup. The CLI recovers the runtime
through exclusive startup, preserves stale CAS and discovery, and removes its own
key after a successful refresh. Its scope is installation-owned Auth; active-daemon HTTP,
browser PROJECT authorization and other native hosts have separate qualification.

Implementation: [auth_entry.rs](../../crates/graphit-app/src/auth_entry.rs).
Fixtures: [auth_entry.rs](../../crates/graphit-app/tests/auth_entry.rs) contains the
component consumers and compiled standalone Native refresh test, using the existing
signed Broker fixture. The historical standalone Login-only focal remains in
[broker_tests.rs](../../crates/graphit-app/src/auth_cli/broker_tests.rs).
Producer: [Local Auth CLI component](native_auth_cli.md).
