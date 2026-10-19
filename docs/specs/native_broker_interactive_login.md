---
title: Native Broker interactive login
type: specification
status: implemented-library
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-4345}
    relation: implements
  - target: {type: task, id: tsk-a1ba}
    relation: derived_from
  - target: {type: task, id: tsk-8b55}
    relation: implements
---

# Native Broker interactive login library

The Rust library adds a Tokio TCP loopback callback listener to the existing
[profile login](native_broker_profile_login.md),
[profile manager](native_profile_manager.md) and
[native OAuth protocol](broker_native_oauth.md). Those producers retain authority
checks, PKCE/state/nonce and signed token verification, and profile publication.

## Use

1. Obtain the manager's actual managed snapshot under the original request and
   administrative authority. Supply its captured NativeOAuth and profile,
   provider and S3 selection.
2. Supply `InteractiveLoginConfig`: a literal loopback address, exact callback
   path and positive `InteractiveLoginLimits`. Port zero chooses an ephemeral
   port; a fixed port and IPv4 or IPv6 loopback are supported.
3. Call `begin_interactive_broker_login`. It validates configuration and uses
   the shared admission checks before allocating transport buffers, binding and
   discovering the provider. The existing producer checks again around discovery.
4. Borrow `authorization_url` and open it using the caller's browser integration.
   The URL is sensitive and must not be logged.
5. Consume the attempt with `receive`. It accepts sequential connections until
   a structurally valid callback, cancellation, the original deadline or an I/O
   error. Its listener and stream close before the protected callback is returned.
6. Consume the result with `publish(now)`. It calls the existing managed finish
   once with the same pending login, service, request and callback.

A structural HTTP callback does not authenticate the user. NativeOAuth validates
the callback and tokens, checks fresh authority and the physical snapshot before
the sole token POST, then the existing manager creates or replaces and activates
the selected profile with one Core publication. Wrong state ends the attempt.
The caller cannot replace its request, service, tokens or CAS expectation.

## HTTP and resource limits

Tokio provides asynchronous sockets and borrowed httparse handles HTTP syntax.
The listener requires HTTP/1.1 GET origin form, the exact path and exactly one
Host matching its actual bound authority. IPv6 authorities use brackets; port80
uses the canonical Host without its default port. The callback preserves the raw
query and explicit nonzero port required by NativeOAuth.

| Limit | Meaning |
| --- | --- |
| `head_bytes` | Maximum request head, with one extra byte available to detect oversize input. |
| `header_entries` | Maximum borrowed HTTP header slots. |
| `target_bytes` | Maximum raw request target before copying. |
| `callback_bytes` | Maximum assembled callback; origin plus target must fit before binding. |

All input limits must be positive and each actual buffer allocation representable.
Invalid method, route, Host, absolute form, fragment, Transfer-Encoding, Expect,
duplicate or nonzero Content-Length, excess head bytes and configured input limits
reject the request before a token POST. Rejected HTTP receives 400 and closes that
stream; another connection may deliver the callback under the same original
cancellation and deadline. There is no cumulative connection quota or replacement
request. NativeOAuth separately enforces its captured callback and token limits.

Responses are empty 204 for a structural callback or 400 for rejected HTTP, with
Connection close and no input echo. Transport errors use static categories and
do not expose callback, code, state, claims or tokens. Broker errors and the
existing RemoteEffect/publication result retain their original cause.

Accept, read and write share the original permanent cancellation signal and
absolute deadline. Head and callback buffers have Zeroizing ownership; parser
borrows end before another read. One attempt owns at most one stream. Dropping an
attempt or a pending receive closes its sockets through normal Rust ownership;
there is no detached server or background task. During discovery and publication,
the existing OAuth producer retains ownership of its own internal pending state.

## Implementation and validation

Production lives in
`crates/graphit-auth/src/profile_manager/broker_login/interactive_login.rs`.
The parent shares its existing `admit_login` checks with the ordinary begin flow.
Fixtures in the nested broker_login test module reuse the real issuer, AuthStore,
signed Broker tokens and profile manager.

The affected Linux group has 12 functional tests: real IPv4/IPv6 and fixed-port
publication, input and framing boundaries, rejected requests followed by a valid callback, wrong
state, changes to authority or the physical snapshot, and cancellation/deadline
with pending socket operations and actual response bytes. The existing 20 login
regressions remain required. Rustdoc checks the consuming API and opaque types;
scoped formatting and strict Auth Clippy check the affected implementation.
Automatic browser launching and full CLI command wiring are separate callers.
