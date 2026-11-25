---
title: Native process principal observation
references:
  - target: {type: task, id: tsk-8bee}
    relation: implements
  - target: {type: task, id: tsk-f2f5}
    relation: corrects
  - target: {type: task, id: tsk-e849}
    relation: derived_from
  - target: {type: task, id: tsk-0070}
    relation: supports
---

# Native process principal observation

`graphit_core::native_principal::NativeProcessPrincipal` captures the current
effective native principal through the filesystem SDK facade. This is a library
observation for future native command admission; it creates no authentication
state, login, permission, admin session, Role or grant. It requires no RuntimeKey,
authentication document or profile. A kernel principal does not distinguish a
human from another program running with that same principal.

## Request and lifetime

`capture(&SnapshotContext)` checks original cancellation and
absolute deadline before and after the SDK call. `recheck(&SnapshotContext)` also
requires the original request identity before another SDK observation. Shared
references to the original request remain valid. Matching clocks or cancellation
objects on a fresh request do not establish identity. Cancellation remains final;
a new context cannot renew the observation.

The owned proof retains no native handles or pointers. It is automatically
`Send + Sync`, with no unsafe implementation, cloning, debugging, serialization,
identity getters, construction from data or restoration. A thread transfer is
valid only while the effective principal observed on the receiving thread agrees
with the original. A recheck is observational: it cannot make a later effect
atomic. Consumers must perform their final check directly before the synchronous
effect, without an intervening await.

## Native SDK behavior

Linux and macOS use the pinned safe rustix 1.1.5 process APIs. Two consecutive
PID, effective UID and effective GID observations must agree. Instability fails
without retry. Real UID equality, groups, names, environment and terminal state
are not additional admission rules.

Windows uses pinned winsafe 0.0.29 safe token APIs. It opens the effective thread
token with QUERY/OpenAsSelf and falls back to the process token only for
ERROR_NO_TOKEN. Access denied, anonymous token and other errors fail closed.
Temporary RAII token handles are dropped within the observation.

The SDK validates the User SID before its revision, authority and subauthorities
are normalized into 68 fixed bytes. Counts above 15, invalid revision, inconsistent
SDK length or malformed iterators fail. Token statistics before and after User
inspection must agree. Reopening the effective token must reproduce the original
process/thread origin, process ID, TokenId, AuthenticationId, ModifiedId, token
type and applicable impersonation level. Recheck repeats that complete sequence.
No rule forbids impersonation; a changed effective context is rejected.

Existing [private installation bootstrap](private_installation_bootstrap.md)
and Windows private-file checks qualify physical files under their separate
primary-process-token policy. This observation does not assert that the effective
token owns those files. Composition of effective identity and physical ownership
belongs to the future native admission producer.

## Costs and errors

The observation owns its fixed native facts and original request identity, without
a retained-size admission parameter or counter. It performs no filesystem reads
or writes. Kernel syscalls are a separate cost. Windows GetTokenInformation
allocates kernel-sized temporary buffers inside the mature SDK; its safe API
exposes no caller allocation ceiling. This module makes no hard RSS or
preallocation guarantee.

Errors expose only static variants: Cancelled, Deadline, WrongRequest, Unavailable,
InvalidObservation, Changed and Unsupported. Native identity, token values, SDK
payloads and paths are never part of those diagnostics. Unsupported platforms
fail closed. Recheck does not rebase an observation after a mismatch.

## Verification and integration status

The focused executable checks are `cargo test --locked -p graphit-fs`,
`cargo test --locked -p graphit-core --lib native_principal`,
`cargo test --locked -p graphit-core --doc`, strict Clippy for both crates,
and the existing native safety guard. The guard permits only the exact facade
module export and preserves existing qualified unsafe sites.

The current Main correction in tsk-f2f5 passed 12 focused Linux functional tests,
including actual SDK capture/recheck, thread transfer and the native child test
binary, plus eight compile-fail doctests, strict Core all-targets Clippy and
rustfmt for the changed Rust sources. The former fixed-layout meter test alone
was retired. The filesystem SDK facade is unchanged; its previous qualification
is separate from this Core API correction. These results do not establish native
Windows/macOS execution or NativeCLI admission.
