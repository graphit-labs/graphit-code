---
references:
  - target: {type: task, id: tsk-1750}
    relation: implements
  - target: {type: task, id: tsk-7a35}
    relation: specified_by
  - target: {type: task, id: tsk-aeda}
    relation: relates_to
---

# Typed private authentication storage

`graphit_auth::AuthStore` decodes and publishes the typed provider/profile
content of the configured private `auth.json`. It receives a Core
`AuthDocumentStore` and `ProfileLimits`; storage data does not authenticate a
caller or grant permission. [Authentication profiles](auth_profiles.md)
describes the schema and account lifecycle. Core owns the private I/O and
publication in [Private authentication document](private_auth_document.md).

The implementation is in `crates/graphit-auth/src/store.rs`, its fixtures in
`store/tests.rs`, and the consumed Core API in
`crates/graphit-core/src/auth_document.rs` and `auth_document/publication.rs`.

## Reading current content

`AuthStore::new` validates the supplied profile limits with a temporary empty
typed document. It does not create a file, provider, profile, credential or
login. `load(context)` asks Core to read the configured private document using
that operation's original snapshot. Core checks the original cancellation and
deadline, verifies current private directory/file access and bounds the bytes
read. Auth then decodes the present body once under the typed limits. An absent
file produces an empty revision-1 document; malformed or inaccessible data
returns an error.

`AuthSnapshot` owns the typed document and Core's optional expected bytes. Its
public getters borrow `document()` and report `was_present()`. It has no public
constructor, Clone, Debug or deserialization implementation. The expected bytes
are data for a later content comparison; they carry no directory identity or
request grant. A separately authorized fresh operation can publish those
expected bytes under its own live context. Replacing a configured file with
identical private bytes does not by itself invalidate a content comparison.

Crate-private `load_locked` retains the shared publisher lock while its caller
inspects the current document. `snapshot_parts` borrows disjoint typed data and
an `AuthSnapshotRecheck`; that rechecker reads current private bytes with the
supplied original snapshot and returns Conflict if they differ from the saved
expected bytes. It does not retain a root file descriptor or inspect ancestor
identity. Consumers choose their recheck boundaries according to the authority
and credential they actually use.

## Typed mutation and byte CAS

`mutate(snapshot, context, closure)` checks the live context, applies the typed
closure, encodes bounded canonical JSON and drops the typed document before
publication. Profile operations check the expected logical revisions and
prospective document capacity. Closure or encoding errors return before Core
publication. The closure runs without the publisher lock; remote credential
production must finish before entering this local mutation.

The prepared body uses `Zeroizing<Vec<u8>>` and remains privately bound to its
actual `AuthStore` and context. Commit refuses a prepared body passed to another
store. Core acquires the short publisher lock and compares the current private
bytes with the supplied expected bytes. A conflicting current document returns
`AuthStoreError::Core(Conflict)` and preserves the winner. This final comparison
can fail after the typed closure has already run, so callers must not treat the
closure as an external effect boundary.

The publication result is `Result<AuthPublicationEffect, AuthStoreError>`.
`Unchanged` means the current expected bytes already equal the encoded body.
`Applied` follows successful atomic replacement. Core creates private temporary
content, writes and syncs it, checks expected bytes again and atomically
publishes through the existing filesystem library. Native I/O errors propagate;
there is no automatic retry, permission repair or manufactured success.

`publish(snapshot, context)` uses the same path with a no-op typed closure. An
explicit empty publication can create `auth.json`; an explicit
`ensure_default_local` mutation creates only the Local CPU provider, without a
profile, credential, active account or login. Runtime filesystem views must
exclude Core's shared `publication::AUTH_PRIVATE_NAMES` rather than maintain a
second list.

Crate-private `prepare_latest`/`commit_latest` instead decode and mutate the
latest document while retaining its publisher lock. Consumers check their
original target expectations inside that transaction. It must not span an HTTP
request or other asynchronous credential production.

## Authority and credential consumers

[NativeProfileManager](native_profile_manager.md) derives the store from its
RoleIssuer. It requires an admitted finite administrator or verified role with
ManageAuthProfiles, prepares the typed publication, rechecks that authority and
then commits. Possessing a store, document, expected bytes, configured path or
request context does not replace that check.

RoleIssuer validates logical installation/brand selections and exact trusted
PROJECT roots. Runtime authentication revalidates its actual runtime-key source.
Local authentication reads the current profile document, derives the selected
identity/key binding, drops decoded secrets and retains its binding and original
request. Subsequent checks read the current source and compare that binding.
[PROJECT record admissions](project_record_admission.md) retain their real
caller/grant and recheck the exact module operation and original request.

[Service selections](service_credentials.md) retain the typed credential data
they consume. Their existing gates check current provider/profile content or
saved expected bytes as required by the selected authority and credential mode.
An unrelated publication need not invalidate a consumer that compares only its
selected binding; changing or removing its actual credential/provider source
must fail its recheck. Restoring content requires a separate authorized
operation; storage does not refresh credentials, perform login or create
permission.

The [Broker profile refresh](native_broker_profile_refresh.md) consumer borrows
selected secrets and the private content rechecker, then uses the existing
prepared publication path. These borrows do not add a public proof type,
directory binding, observation quota or publication authority.

## Fixtures and verification boundaries

The actual private-store fixtures cover absence and explicit publication,
malformed data, private access, cancellation/deadline, configured content limits,
stale byte CAS preserving the winning publication, logical revision conflicts,
prospective capacity, lifecycle changes, same-content file replacement and
configured links. `fresh_request_publishes_expected_bytes_without_a_snapshot_request_proof`
and `prepared_publication_keeps_actual_store_binding_without_effects_on_swap`
distinguish expected content from private prepared-body ownership.

Issuer fixtures exercise source revocation by removing the actual runtime-key
or provider document, then checking retained admissions, leases and getters.
The installation-isolation cases consume physical PROJECT roots from their
`ProjectPaths`/`PublicationScope` producers and preserve sibling admission after
another PROJECT is retired. They do not require renaming a live installation or
changing filesystem permissions.

Focused storage commands use the canonical runner and configured offline cache:

```sh
CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true cargo xtask cargo test --locked --offline -p graphit-auth --lib store::tests::
CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true cargo xtask cargo test --locked --offline -p graphit-auth --doc store::
```

Record selected names, executed counts, platform, executable and timings with
each qualification. Source inspection and a fixture test establish their
recorded boundary; they do not qualify an installed service, live OIDC flow,
three-platform migration or whole-product performance. Historical execution
receipts remain in Task evidence rather than defining current storage APIs.
