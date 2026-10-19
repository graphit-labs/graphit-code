# S3 STS storage and UI network configuration

## Storage credentials follow the provider

Graphit mounts native LanceDB and Icebug data directly on S3. Authentication and storage are
configured together on the active provider:

- `local` without an S3 bucket uses filesystem paths. With a bucket, it uses login keys, an AWS
  shared-config profile, or the AWS credential chain when explicitly enabled.
- `broker` uses filesystem paths when valid discovery omits S3 because Broker storage is disabled.
  When discovery advertises `graphit-s3-credentials-v3`, Graphit calls `POST /v1/s3/credentials`
  with its current Broker bearer and the framework-selected `project`, `user`, or `hub` scope plus
  the physical `task`, `memory`, `knowledge`, `ast`, or `hub` module. The Broker derives a session policy from current grants and
  returns a temporary access key, secret, session token, expiry, bucket, region, endpoint,
  prefixes, and authorization revision.

An in-process manager reuses each valid Broker scope and renews it according to its issued expiry
and the existing service refresh margin. Object requests and their bodies travel
directly between Graphit, LanceDB or LadybugDB and S3; the Broker is contacted only to issue or renew
the session.

Task and Memory background embedding jobs keep a continuous, cancellable corpus
request. If their Native access token needs renewal, OAuth uses the finite storage
operation deadline already selected by the resolver, capped by any caller deadline.
Renewal does not give the corpus job an artificial expiry. Valid credentials do not
trigger OAuth again; rejection, identity changes, cancellation and expired operation
deadlines remain errors and never select local storage as a fallback.

Storage selection is independent of AI configuration. An AI service configured as `broker` uses
its advertised Broker capability; a disabled service does not run inference. In particular,
`search.rerank=false` prevents rerank calls without changing the storage backend.

The Broker may define several named storage routes, but every effective S3 grant for one requested
scope must resolve to one route. Different projects may receive different topology. Conflicting
matching routes fail closed. The client sends only the scope, module and, for project scope, the immutable
project ULID; it cannot select a route, bucket, prefix, operation set, role, or duration. Graphit
rejects any response that does not echo scope/project/module exactly. The Broker
keeps its permanent S3 identity private and uses it only to call STS.

Temporary credentials are bearer secrets. For a Broker provider, credential sessions
remain in process memory, keyed by authenticated identity and bearer fingerprint, provider revision, storage scope and module;
`auth.json` never receives them and a restarted process requests fresh grants. Concurrent requests
for one key share one exchange, while projects and user/Hub scopes never share a credential. A change
to the S3 session policy is reflected at the next renewal; PROJECT admission is rechecked independently.
A credential already issued remains usable until its STS expiry or an
object-store-side revocation, so deployments should choose a duration that matches their revocation
requirements.

Tasks and Memory use authoritative LanceDB tables directly on S3 whenever S3 is configured.
Knowledge and AST from the Hub read their exact installed native datasets directly.
Locally indexed Knowledge and AST use their canonical global project corpora and native
version branches; general sync does not hydrate them by Git ancestry or clone a repository.
Local Icebug Parquets and authorized remote Icebug Parquets use the same in-memory
Ladybug query engine, differing in local-file or S3 range transport. File artifacts that agents require as ordinary files may
still be downloaded to their managed local installation.

## Memory and Task tables and versions

Memory and Task operations resolve storage through their live, typed PROJECT or USER admission.
The Source daemon attaches the same resolver to its issuer, so CLI, MCP, HTTP, Browser readers,
agent tools, references, lexical indexing and Memory embedding jobs open the same authoritative
Lance table. A remote operation does not acquire a local corpus writer or create a local mirror.
A denied or failed discovery, credential exchange or S3 request is an error, including on writes.
A valid Broker discovery explicitly advertising storage disabled selects the local backend.

For PROJECT, STSv3 requests contain `scope: "project"`, the immutable `project_id` and physical
`module: "memory"` or `"task"`. The returned module prefix owns `index.lance` directly; Graphit
cannot choose another bucket or prefix. Project versions remain Lance branches inside that table:
the default version uses the default native branch, and another version follows the existing inherited
branch semantics until its first materialized write. The same version rules apply locally.
No version is inferred from a checkout's absolute path.

USER Memory requests contain `scope: "user"`, `module: "memory"` and the canonical
`installation_id`; `project_id` is absent. The response echoes the installation and uses an empty
`project_id`. USER Memory retains its existing verified issuer/subject **and installation** owner;
independent clients of that installation share its table, while another installation or user is
isolated by the Broker contract. USER Memory has no PROJECT/Git branch.

With standalone S3 selected on a local provider, the configured prefix is extended with
`projects/<project_id>/<module>/index.lance`. USER uses
`users/<installation_id>/<user namespace key>/memory/index.lance` below that prefix. A local
provider's optional Broker for AI does not change its own storage selection.

Each new store operation captures the SDK's current head for the selected branch; an existing
reader retains its captured snapshot. The credential cache contains the authorized AWS credential
provider and transport options, not
corpus snapshots. Continuations and deferred embedding writes bind to a stable fingerprint of the
authorized endpoint and table URI, so a topology change cannot reuse another table’s revision.
Credential renewal for the same destination preserves that fingerprint. Revision checks, conditional SDK commits, idempotency, Task claims/leases and
immutable history still apply across clients. Credential expiry or a failed renewal never creates
a local replacement table. Graphit does not persist STS responses or temporary credentials in
`auth.json` or a credential file. Lance may retain ordinary dataset URIs in its own manifests;
the in-memory credential provider is not serialized. A continuous background job retains
its corpus cancellation context; each discovery/STS resolution uses the configured request timeout
(and never extends a finite caller deadline).
A captured snapshot does not expire merely because its original STS session expired. Continuous
Memory embedding readers renew expired transport between batches and reopen that exact SDK version
and physical branch, preserving inherited-branch identity, document position and emitted vectors.
They do not recapture latest or rediscover the corpus. A calculated batch uses a newly authorized
writer; a changed destination is rejected rather than mixing tables. Foreground operations still
respect their original deadline and report real S3 credential failures explicitly.

The native Lance AWS transport receives the granted credentials, region, endpoint and conditional
copy policy. Both SDK endpoint keys and signed requests are fixed explicitly, so an ambient AWS
endpoint, profile or unsigned-request option cannot replace the Broker grant. If the Broker omits
an endpoint, Graphit pins the SDK's regional AWS default (`https://s3.<region>.amazonaws.com`).
The SDK's unconditional environment overrides for HTTP and retry duration are checked when opening
the transport: a conflicting override is an explicit configuration error, never another destination
or a longer retry policy. Remove the conflicting setting or configure the intended Graphit provider.


## Configuration examples

Direct local credentials:

```bash
graphit provider add workstation --type local \
  --s3-bucket graphit --s3-region us-east-1 \
  --s3-endpoint http://127.0.0.1:9000 --s3-prefix tenant/alice \
  --s3-credential-source login
graphit login --profile alice --provider workstation \
  --s3-access-key "$S3_ACCESS_KEY" --s3-secret-key "$S3_SECRET_KEY"
```

Broker-issued STS:

```bash
graphit provider add company --type broker \
  --broker-endpoint https://broker.example.com
graphit login --profile alice --provider company
```

A first-class Broker provider requires an authenticated user. It uses Broker-issued S3 sessions
only when discovery exposes storage; otherwise its storage remains local. It does not issue
anonymous S3 sessions. A local provider can still use an anonymous Broker identity for AI/Hub ACL
calls, but it must configure its own S3 access if it needs remote storage.

## UI and MCP listeners

The shared listener uses `mcp.host`/`mcp.port`; trusted `task.browser` configures Broker
browser identity, PROJECT authorization and optional exact public origin. Absent Browser
configuration is headless, not anonymous data access. CORS is not authorization. Keep an
exposed listener behind TLS and the intended network boundary.

`mcp.host` and `mcp.port` configure the daemon listener. Its generated runtime key rotates at each
start. A local provider may use a static MCP key. With a Broker provider, each remote HTTP caller
sends a Broker access JWT whose `aud` includes this endpoint's configured `--mcp-resource` URI.
Graphit verifies the JWT and forwards the same bearer to Broker APIs, which require their own
audience in that token. `graphit mcp --stdio` targets the local daemon using its runtime key or a
scoped capability token. The Broker provider discovers issuer and signing keys; its MCP resource
URI is configured locally and must appear in Broker discovery.

## Diagnostics

```bash
graphit provider show company
graphit account show alice
graphit account list
graphit mcp
```

Inspect `~/.graphit/auth.json` permissions, but never publish its contents. It contains identity
and service credentials. Broker-issued temporary S3 credentials are never
stored there; local profiles may contain their configured S3 credentials.

## Focused storage integration tests

The Rust App tests use signed Native Broker tokens and a local STSv3 contract server, with
independent client states against one S3-compatible service. The maintained local runner starts
isolated Moto S3/STS, obtains temporary credentials, and removes its temporary files and server
on exit. It does not use a cloud account or a provider agent CLI. Moto covers client/storage
contracts; it does not certify a deployment's IAM policies or hardware/platform support.

After Cargo emits the `graphit-app` unit-test executable (`--lib --no-run`), use a test Python
environment and pass the executable path returned in Cargo's `compiler-artifact` output:

```bash
python -m pip install -r scripts/record-storage-test-requirements.txt
# Set GRAPHIT_APP_TEST_BINARY to the actual compiled test executable first.
python scripts/test-record-storage-s3.py --test-binary "$GRAPHIT_APP_TEST_BINARY"
```

The runner explicitly selects the ignored `task_server::record_storage_tests` focals. Without the
required local fixture, those tests are not a passing qualification. The tests exercise actual
Task operations, Memory consumers, Source MCP index maintenance, background embedding orchestration,
revision conflicts, versions, USER isolation, renewal and explicit storage failures. The small
embedding callback validates persistence/reuse, not model inference quality.

The selected Task and Session regressions use the existing DaemonAuth, service rules and
production `RecordStorageResolver`. Current reads leave `history` and Task `comments` null;
explicit pages traverse to EOF at the captured generation, including after a newer write.
The Web scenario uses MCP writes and the existing `POST /api/tasks/session` route for current
Session state, replaceable history pages, cursor rejection across projects and fresh PROJECT
denial before storage. It does not require a UserInfo refresh.

Two additional focals compose an actually verified `BrokerProjectCaller::Incoming` with fresh
PROJECT authorization and the native Task read protocol. Their service rules retain the incoming
bearer, exact subject and MCP resource. They cover authoritative S3 and valid Discovery disabling
storage, without changing the Broker provider to a local provider. For S3, independent
installations share the same granted PROJECT table; disabled storage uses independent consumers
of the same local corpus. They check current state, pinned history/comment pages, EOF and
project/version/section cursor rejection. These are direct App/Auth/protocol composition tests;
they do not add a BrokerIncoming HTTP origin to TaskServer or qualify the public Broker
PostgreSQL ACL fixture.

To select only these two Incoming cases from the emitted executable:

```bash
# Set GRAPHIT_APP_TEST_BINARY to the actual compiled test executable first.
python scripts/test-record-storage-s3.py --test-binary "$GRAPHIT_APP_TEST_BINARY" \
  --filter task_server::record_storage_tests::transports::broker_incoming_
```

Test source and an emitted executable alone do not establish a passing qualification. Record the
exact executable and completed focal results separately; historical storage receipts do not
qualify the selected current/page behavior.
