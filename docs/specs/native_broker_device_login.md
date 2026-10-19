---
title: Native Broker Device login
type: specification
status: implemented
references:
  - target: {type: task, id: tsk-5fb0}
    relation: implements
  - target: {type: task, id: tsk-e672}
    relation: uses
  - target: {type: task, id: tsk-2080}
    relation: derived_from
---

# Native Broker Device login

This contract covers the Rust CLI and Auth library Device consumer. The implementation and fixtures are applied under tsk-5fb0. Current Device and active Code focals, productive Broker HTTP/CLI T10, strict production Auth/App/CLI and strict Broker integration-test checks have passed. The active Device transport case has also passed, and final Broker T10 producer review reported NoFault. Strict checks of the affected Auth/App test targets have also passed. The existing [Code login](native_broker_interactive_login.md) retains its PKCE, state, nonce, ID token and rotating-refresh contract.

## Authorize from another browser

Configure a Broker provider with its actual endpoint and no MCP resource. Its stored S3 credential source must be Broker, including when AI execution is disabled and no bucket is configured. This selects a credential source and grants no storage or PROJECT permission. For example:

~~~text
graphit provider add work --type broker --broker-endpoint https://broker.example --s3-credential-source broker --embedding-mode disabled --rerank-mode disabled
~~~

Obtain current installation profile-management authority. Run:

~~~text
graphit login --device --provider work --profile laptop
graphit account show
graphit logout --profile laptop
~~~

The CLI prints a public verification URI and user code. Open that URI in a browser and enter the code. Complete the Broker's real LOCAL authentication, any required password-change/MFA/CAPTCHA steps, and explicit consent. The CLI waits for the Broker to grant or reject the request. Non-interactive mode still prints the instructions and uses no callback listener. The active daemon path may open the complete verification URI when interaction is enabled; the installation-only path prints instructions.

A successful result publishes and activates the selected profile in one physical auth.json publication. An existing target may be replaced with the newly verified issuer/subject. Use account show/list to inspect the selected profile; username, organization and teams may be absent. Display metadata does not confer a role, PROJECT access or permission.

Local identity and credential flags are rejected for Device login. A Local provider, provider with an MCP resource, missing discovery endpoint or unsupported Device grant denies before a Device authorization POST. An expected-revision mismatch denies before protected I/O. Permission, provider revision and the original physical authentication snapshot remain subject to fresh checks.

## Access-only protocol

The configured issuer's validated discovery must advertise device_authorization_endpoint and the Device grant URN. Endpoints use the existing same-origin URL policy. The static native CLI client ID and deployment access audience come from the existing Broker metadata. No endpoint, client, scope, audience or resource is inferred from untrusted token data.

The OAuth2 5 client creates the authorization and token forms. Authorization sends client_id and no scope. Token polling sends client_id, device_code and grant_type=urn:ietf:params:oauth:grant-type:device_code, without scope, resource, secret, Basic credentials or redirect. The granted scope is absent or empty. This version has no scoped capabilities, ID token, refresh token, family or renewal.

The private pending owns device_code; only user_code, verification_uri and optional verification_uri_complete can cross the event/CLI boundary. The complete URI must carry the same user code at the same validated verification path. The existing BoundedClient limits response bytes under the Original before the SDK deserializes them. Device consumes the SDK token result directly and ignores unrecognized response parameters as required by [RFC 6749 §5.1](https://www.rfc-editor.org/rfc/rfc6749#section-5.1) and [RFC 8628 §3.5](https://www.rfc-editor.org/rfc/rfc8628#section-3.5). Known-field decoding remains the pinned SDK contract. Effectively nonempty scopes, an actual refresh token, invalid Bearer/lifetime/access data or a failed signed proof reject publication. The typed consumer checks the values of SDK scopes, including empty elements retained by its space split; whitespace-only entries and refresh null grant no capability. Extra ID/metadata fields are ignored and never become proof or persisted profile data. The existing Code/refresh codec is unchanged.

Polling uses oauth2::exchange_device_access_token().request_async. The library handles authorization_pending and persistent slow_down increments of five seconds. The absolute grant expiry is anchored before authorization I/O and cannot be extended by a delayed prompt or a new poll. The original cancellation/deadline bounds every network request and SDK sleep. The SDK retries generic AsyncHttpClient errors; a terminal oneshot/select adapter ends the owned SDK future at the first transport/gate failure, preventing an ambiguous token POST from being replayed. It fabricates no HTTP response and creates no detached polling task.

Broker errors access_denied, expired_token, invalid_request, invalid_scope, invalid_grant, invalid_client, unsupported_grant_type, temporarily_unavailable and server_error terminate the attempt. Failures expose finite categories and actual NotSent/MayHaveApplied effects; HTTP diagnostics do not include token bodies or secrets. Retry a failed login as a new user-driven authorization, never as a replay of a consumed grant.

The Broker owns its existing device_code_ttl (default 10 minutes, 5–30 minutes) and device_poll_interval (default 5 seconds, 1–30 seconds). These differ from its existing access-token TTL. The client uses actual expires_in/interval and the Original deadline, without a new TTL or scope policy.

## Verified profile and lifetime

BrokerVerifier::verify_device_login validates the access JWT locally with the existing algorithm/signature/JWKS cache, issuer, nonempty subject, client_id, access audience, token_use, identifier and temporal checks. It uses no invented expected subject, ID token or nonce. The persisted subject is the opaque `sub` from this verified JWT, not a raw LOCAL identifier or a value derived by the client. Optional display attributes come only from the signed access claims. No UserInfo request is required for validity or absent metadata.

ProfileRecord holds either the complete existing ProfileData body or BrokerDeviceProfileData. data() returns an optional complete Local/Code body; broker_device_data() selects the Device body. name()/provider() are common selectors. The Device JSON credentials discriminant is broker_device; its tokens contain exactly access_token, token_type Bearer and expires_at in Unix seconds. The data-level username, organization and teams are optional, omitted when absent and non-null when present. Absent teams remain distinguishable from a signed empty list. Local username and Code ID-token requirements remain intact.

The stored expiry is the minimum of signed exp and the token response lifetime, anchored to the real response-request clock. A Device service lease uses the actual expiry with zero refresh margin; a renewable Code profile retains the existing refresh margin. Expired Device credentials report that a new login is required. A reader receives neither profile-management permission nor background renewal. A valid JWT alone grants no PROJECT/RBAC access.

Logout removes the selected local profile and active selection through the existing CAS. It supplies no fabricated id_token_hint or EndSession request and promises no remote revocation. A copied access JWT can remain valid until its actual expiry.

## Owned SDK phases and publication

NativeProfileManager::begin_device_broker_login consumes a genuine ManagedAuthSnapshot, its matching NativeOAuth service and explicit BrokerLoginSelection. ManagedDeviceBrokerLogin::instructions borrows only public instructions after fresh checks. receive consumes that pending and returns ReceivedDeviceBrokerLogin; publish(now) consumes the verified response and returns the actual AuthDocumentPublication.

Every phase retains the same manager, authority, issuer/provider revision, Original and physical observation. No network await holds an issuer or writer lock. Profile scalar/team/backing limits precede owned profile copies. Publication replaces and activates using one prepare/commit over the original snapshot, with final tokens → authority → tokens checks. A competing change, including active-selection ABA, denies rather than reloading or adopting its state. After token consumption, local mapping/prepare/authority failure remains MayHaveApplied. The exact Core publication effect and pending cleanup proof remain owned by the caller; cleanup never replays authorization.

## Evidence and remaining qualification

Protocol: broker/oauth/device.rs and broker/oauth/device/tests.rs cover forms, signed identity, optional attributes, SDK cadence, terminal errors/transport, cancellation and input bounds. Managed: profile_manager/broker_login/device_login.rs and tests/broker_login/device_login.rs cover publication, physical CAS and cancellation. profile/tests.rs covers the closed Device codec and presence rules. issuer/tests/service_credentials.rs covers zero refresh margin and relogin expiry. auth_cli/device_tests.rs exercises the CLI SDK consumer, account/logout, public events and no-publication errors.
