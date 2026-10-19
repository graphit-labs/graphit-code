---
title: Native Auth CLI command component
description: Provider, Local profile and native Broker OAuth commands with caller-owned administrative authority.
type: specification
status: implemented
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-5fb0}
    relation: implements
  - target: {type: task, id: tsk-5ea2}
    relation: implements
  - target: {type: task, id: tsk-8f98}
    relation: implements
  - target: {type: task, id: tsk-dc33}
    relation: implements
  - target: {type: task, id: tsk-5060}
    relation: supports
---

# Native Auth CLI command component

`graphit_app::auth_cli` implements typed Clap commands and executes provider and
Local profile operations and native Broker OAuth through the [native profile manager](native_profile_manager.md).
Its caller supplies an actual `ProfileManagerAuthority`, backed by a current
`AdminParent` or a permitted verified role. Neither arguments nor profile identity
create authority. The manager retains the same original request and checks physical
installation, issuer lineage, cancellation, deadline and `ManageAuthProfiles` before
publication.

Native Broker login/refresh is implemented by `tsk-8f98`. The central executable
uses the [active administrative transport](rust_auth_active_admin.md) when the
installation's daemon is running, and the installation-owned Auth entry when
native discovery is absent or the installation singleton has been released.
The standalone entry acquires that singleton again before creating authority;
a transport or authorization error from an owned runtime never selects this path.
Both entries select Local or Native Broker login from
the requested provider's actual kind; an active Local profile does not route a
login requested for a Broker provider to the Local executor. The retained daemon's Runtime origin explicitly
permits profile management; Local and PROJECT origins do not acquire that permission.
This document does not certify browser PROJECT authorization or a builtin role factory. `tsk-e849` completed
profile-contract refinement; `tsk-8b55` removed the cumulative login connection quota.

The Auth SDK also provides [Native installation authority](native_installation_authority.md)
by qualifying the existing Runtime caller against its effective signed Native profile.
This separate policy admits only Configure, ManageCache and PublishedFilesystem;
profile management and ordinary Source/role permissions are unchanged. The active
installation ingress is a separate consumer delivery, not a new Auth CLI command.

## Commands

The component exposes the same `Command: clap::Subcommand` to the central CLI
and the private administrative JSON request. It parses without a project, Git
checkout or configured agent.

| Command | Behavior |
|---|---|
| `provider list` / `provider show NAME` | Read provider metadata and safe service configuration |
| `provider add NAME --type local\|broker` | Add current SDK configuration |
| `provider update NAME [options]` | Apply explicit options while retaining omitted settings; invalidate dependent login through the SDK |
| `provider remove NAME --yes [--cascade]` | Confirm removal; a referenced provider requires cascade |
| `login --provider NAME --profile NAME --username USER` | Store supplied Local credentials and activate the profile in one publication |
| `login ... --anonymous` | Store the explicit SDK anonymous assertion when the Local provider permits it |
| `login --provider NAME --profile NAME [--s3-disabled]` | Use native Broker PKCE OAuth for a Broker provider, publish the signed profile and activate it |
| `refresh [--expected-revision N]` | Rotate credentials of the actual active Broker profile using provider/token CAS |
| `account list` / `account show [NAME]` | Read profile identity assertions and metadata, without credentials |
| `account use NAME` | Select the profile atomically |
| `logout [--profile NAME]` | Delete selected profile credentials; omission selects the active profile |

Mutations accept `--expected-revision` to reject a script's stale document revision.
The component also passes the current captured provider/profile revisions to native
CAS APIs. It does not retry an outdated write. A provider update invalidates
the old profile credentials through their captured provider revision; the selected
profile name remains available as metadata.

Provider options map directly to `ProviderConfig`: Local/Broker identity, Broker
endpoint/resource/anonymous configuration, embedding and rerank modes, local GGUF
device/device ID, direct protocol/endpoint/model/revision/recipe/dimensions and protocol-specific
request dimensions/batch endpoint, S3 location and
credential source. `--clear-broker` and `--clear-s3` remove their configuration;
clear-S3 cannot be combined with replacement S3 fields in the same command. Local
execution device options require Local AI mode; direct options require Direct AI mode. SDK validation
enforces compatible topology instead of silently correcting it.

On creation, the S3 credential source defaults to `broker` for `--type broker`
and `login` for `--type local`. Explicit `--s3-credential-source` options retain
their meaning and must satisfy SDK topology validation. Embedding and rerank
defaults remain Local CPU unless selected explicitly. Adding a provider does not
activate a profile or replace credentials of an existing Local profile.

Updates retain the captured S3 credential source when that option is omitted,
including a change of provider type. A type change that makes the retained source
incompatible fails before publication; select a compatible source explicitly in
the same update. Existing stored settings are not repaired or rewritten by defaults.

Direct protocol names map exactly to the [profile producer](auth_profiles.md):
OpenAI and compatible, explicit `google-gemini-1`/`google-gemini-2`, and existing
Cohere/Voyage embedding protocols. Direct rerank accepts only native Cohere/v2,
Voyage/v1 and Jina/v1 protocols with `--rerank-dimensions 0`.
`--embedding-request-dimensions` requests OpenAI output-dimension reduction;
it does not truncate input text. Google embeddings use
`--embedding-batch-endpoint` plus a literal `models/name` model and single endpoint.

Direct configuration requires `--embedding-model-revision` and `--embedding-recipe`,
or the corresponding `--rerank-model-revision` and `--rerank-recipe` options.
Declare the provider's pinned model/deployment revision and reproducible processing
recipe explicitly. Updates preserve omitted values already stored; a new Direct
service cannot infer them from the endpoint or model name. Provider show/list retain
these non-secret identity fields. No alias, synthesized revision or provider wire
parameter substitutes for them.

Local `--embedding-device metal` and `--rerank-device metal` are explicit macOS
GGUF selections with device ID zero. `coreml` and ONNX-specific device flags are rejected;
local execution uses the canonical `execution` field in the Auth document. CPU remains the
default, and the backend validates whether the selected device applies. Provider
configuration declares capabilities without activating a profile or enabling
reranking.
Omitted update options retain the captured value; the respective
`--embedding-clear-request-dimensions` and `--embedding-clear-batch-endpoint`
clear it explicitly. Incompatible/missing fields fail before publication, including clearing
a required Google route. Retired protocol names are rejected without aliases.

Local credential inputs are `--mcp-key`, `--broker-key`, `--embedding-api-key`,
`--rerank-api-key`, paired `--s3-access-key`/`--s3-secret-key`, optional
`--s3-session-token`, `--s3-expires-at` (UTC seconds) and `--aws-profile`. Secret inputs become protected SDK
credentials. Anonymous login cannot carry username, organization, team or Broker
key assertions. The shared parser does not require a username because native Broker
login obtains identity from the signed token pair. The Local executor requires
`--username` or explicit `--anonymous`; it rejects the Broker-only `--s3-disabled`
flag. The Broker executor rejects all Local identity and credential flags.

`--s3-disabled` prevents native HubRead/HubWrite credential use for that selected
Broker profile before remote validation or storage requests. It leaves embedding
and inventory subject to their usual authorization.

## Caller and results

```text
execute(Command, &NativeProfileManager, ProfileManagerAuthority, now)
    -> Result<CommandResult, CommandError>
```

`now` is the caller's actual UTC timestamp. Read results are owned JSON projections
of borrowed SDK views. They never include keys, tokens or private auth-document
content. Displayed endpoint/resource URLs remove userinfo, query and fragment;
this redaction does not modify the stored configuration or OAuth audience.

`CommandResult::Publication` returns the actual `AuthDocumentPublication` intact.
The product caller must inspect effect, failure, cleanup failure and pending cleanup
before reporting success or exiting. Applied/unknown effects are not converted
into an unqualified error or retried mutation. `CommandError::Profile` retains the
native manager error; missing targets, incompatible arguments and explicit revision
conflicts remain clear errors without exposing input secrets.

## Native Broker caller

The product caller chooses the executor using the actual provider kind. Native
Broker operations use the same caller-owned manager and administrative authority:

```text
broker::login(&LoginArguments, &NativeProfileManager, ProfileManagerAuthority,
              &NativeOAuth, InteractiveLoginConfig, announce, now)
    -> Result<AuthDocumentPublication, broker::Error>
broker::refresh(&RefreshArguments, &NativeProfileManager, ProfileManagerAuthority,
                &NativeOAuth, now)
    -> Result<AuthDocumentPublication, broker::Error>
```

The caller constructs the real verifier from the captured current provider and
supplies the native OAuth SDK. The login helper passes the checked authorization
URL to a narrow announcement/opening callback. It retains the SDK's owned pending
flow and loopback listener while receiving the callback, then obtains the actual UTC
publication timestamp. PKCE, state, nonce, signed ID/access claims, provider revision
and token CAS remain SDK checks. Local JWT validation through expiry uses cached
trusted JWKS; UserInfo is not a mandatory admission call.

Announcement failure, cancellation and dropping the login future release the owned
listener and pending flow. A remote exchange may succeed while a newer provider
update or logout prevents publication. The helper preserves the SDK error's
`remote_effect()` and returns the real publication outcome, including owned pending
cleanup. It neither retries nor overwrites the newer document.

The synchronous executor returns a causal error for native Broker login or refresh
routed to it. The command types derive Clap and serde. Their seven protected
credential fields serialize only as explicit inputs to the private native HTTP
request; the types do not implement Debug. Read and publication responses never
serialize commands, credentials or private Auth document contents. The active
consumer dispatches native Broker operations through the asynchronous helpers.
Before mutable commands it reuses authorized native scratch recovery without
interpreting or promoting previous-process content; reads need no extra recovery.

The component creates no issuer, runtime key, admin parent, service session, legacy
auth envelope decoder or quota controller. The actual integration must reuse the
daemon's issuer/key while it runs; an installation-owned standalone startup may
acquire the real singleton and must clean up only its own runtime resources.

When discovery is absent or the installation singleton is free, `auth_entry::run` acquires that same installation-owned
Auth startup without starting a Source listener, project, model service or service
manager. Its Runtime authentication and current administrative parent authorize
the existing profile manager. Broker login uses the existing OAuth and owned
loopback callback flow, including `--s3-disabled`; Device login retains its own
SDK flow. The caller supplies the authorization URL callback, so the central CLI
uses the same announcement and `--non-interactive` behavior with or without Source.
Local login continues through the Local executor. Standalone refresh selects the
active profile's current Broker provider and calls the same asynchronous
`broker::refresh` wrapper as active administration. A stale `--expected-revision`
is rejected before the token request. Refresh does not start Source, require an
unexpired access token for local administration, or change Broker token duration.
Uncertain remote effects remain explicit and are never retried automatically.

The standalone entry retains recovery, original cancellation/deadline, provider
and profile CAS, publication effects and pending cleanup. It removes only its own
temporary runtime key before returning; a failed cleanup retains ownership for an
explicit continuation. Connection, authentication or protocol failure from an
advertised Source whose singleton is still held remains causal and does not enter
the standalone path. Stale discovery is preserved; ownership and runtime-key
recovery belong to the exclusive native startup, never the dispatch probe.

## Verification

Focused component tests use temporary installations, actual `DaemonAuth` startup, runtime authentication and genuine `AdminParent`, with explicit runtime-only administrative permission. They cover provider CRUD and cascade, Local login, profile switching/logout and redaction, omitted-setting preservation and login invalidation, stale revision rejection, denied authority, cancellation/deadline and parser conflicts. They exercise actual private Auth publication and owned cleanup. Native Broker fixtures use actual HTTP metadata/JWKS/token endpoints and signed native token pairs. They validate PKCE and refresh requests, malformed-then-valid callbacks, invalid state/nonce/audience, redacted profile views, owned listener cleanup on announcement failure/cancellation/Drop, and provider update/logout CAS during remote exchanges. Current Broker proofs use local signed JWT validation through expiry, not mandatory per-request UserInfo.

The active-daemon and compiled-executable receipts are recorded separately in
[Active Auth administration](rust_auth_active_admin.md); the component receipts
above retain their original scope.

The focused `installation_only_native_login_uses_selected_broker_and_cleans_runtime`
regression exercises the standalone entry in a separate temporary installation:
an active Local profile, explicit Broker selection, real signed HTTP OIDC exchange,
announcement/callback, profile claims and revisions, and absence of Source discovery
and the temporary runtime key after cleanup. It passed on App10b: one test in
0.67 seconds of libtest execution, with a 0.693-second process wrapper.
The standalone callback peer runs on a separate runtime; listener refusal is checked
after the entry has returned and the peer has joined, rather than treating the
callback response's EOF as completion of the login owner.

The `auth_entry` integration target adds `compiled_standalone_native_refresh_preserves_cas_rotation_redaction_and_cleanup`, using the actual CLI and the existing signed Broker fixture. Stale document revision preserves Auth bytes and sends no token request; success advances the token revision once while retaining the active profile, provider revision, signed identity, redaction and owned runtime cleanup.

The smaller `auth_entry` integration target passed all nine cases on Linux:
eight existing component consumers and the compiled Native CLI regression.
Execution took 3.117888 seconds (libtest reported 3.11 seconds); its separate
emission and build-lock wait took 1132.415221 seconds after cache removal.
The same Cargo emission produced the normal `target/debug/graphit` executable.
The receipt preserves the two earlier linker failures and the separately executed
technical-profile refresh that returned `Applied`; no previous JWT or Native Now
suite was repeated to replace a removed cache.

The eight `tests/auth_entry.rs` consumer tests also passed in 1.10 seconds. Their
929.40-second command duration includes Cargo's reported 15 minutes 28 seconds of
compilation and build-lock wait, which were not timed separately. The earlier
0.655-second standalone focal failure is retained: its helper checked listener
closure prematurely on callback EOF. The correction moves that check after the
completed entry and peer join and requires `ConnectionRefused`, without sleeps or
polling.

## Device access-only profile

The [Native Broker Device login contract](native_broker_device_login.md) defines the separate `--device` journey, typed `broker_device` profile, optional metadata, actual-expiry service use and local logout. Its implementation is under qualification under tsk-5fb0. Existing Local username and Code ID/refresh requirements apply to their complete `ProfileData` body; `ProfileRecord::data()` returns that optional body, while `name()`/`provider()` select either body. No Device username, ID token or refresh token is fabricated.
