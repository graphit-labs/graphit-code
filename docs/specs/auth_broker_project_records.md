---
title: Broker PROJECT authorization for native Records reads
description: Locally verified Broker callers and fresh PROJECT decisions bound to the original request and actual local captures.
type: specification
status: implemented
references:
  - target: {type: task, id: tsk-01eb}
    relation: supports
  - target: {type: task, id: tsk-4559}
    relation: implements
  - target: {type: task, id: tsk-438e}
    relation: implements
  - target: {type: task, id: tsk-e52a}
    relation: uses
  - target: {type: task, id: tsk-35d6}
    relation: uses
  - target: {type: task, id: tsk-2025}
    relation: supports
---

# Broker PROJECT authorization for Records reads

`ProjectRecordAuthority::admit_broker_read` admits a Task, Memory, Knowledge, AST or Hub
read only after two separate checks: an actual locally verified Broker caller
and a fresh PROJECT authorization response. It returns the existing
`ProjectReadAdmission` used by Records readers. It does not issue a role, admit
writes or turn an identity, profile or response DTO into reusable authority.

## Caller and local binding

`BrokerRecordRead` supplies `BrokerProjectCaller::Web` or `Incoming`, its exact
`BrokerVerifier`, the original `RequestContext`, a `BrokerProjectScope`, the
configured provider name, the PROJECT client and `ProjectReadModule`.
The caller must already exist through the actual JWT/OAuth SDK. Native login
identity alone cannot construct this admission.

The scope contains actual installation, project and optional provider captures,
including the project's `VersionContext`. Validation checks installation, brand,
captured records, exact configured roots, project ownership and selected version
root under the same original cancellation and deadline. It requires no agent
`NativeBinding`, Git lookup or active CLI profile.

The provider must exist in the issuer's physical Auth store and have Broker kind,
the verified provider revision and the verifier's exact issuer endpoint. Incoming
callers also retain the configured MCP resource binding; Web callers retain their
registered client and resource through the Web access proof. The active personal
CLI profile does not select or authorize the browser principal.
The Auth document is parsed once for admission, then discarded. The admission
keeps its physical observation for local revalidation rather than retaining the
parsed profiles and credentials.

## Configured PROJECT endpoint

Create `BrokerProjectAccess` once from `ProjectAccessPolicy` and the existing HTTP
and JSON input limits. Its endpoint is explicitly configured and independent of
the OAuth issuer, API audience and resource. Its own captured origin/TLS policy
applies; it never derives a URL from issuer metadata or follows redirects.
The path is `/v1/project/access/authorize`, with no URL credentials, query or
fragment. Literal loopback HTTP requires the existing explicit transport setting.

Each admission sends one POST with the verified caller's sensitive Bearer header:

```json
{"project_id":"01KSH1CRFFG8Z74B5ZS78WW808","module":"task","action":"read"}
```

The project ID comes from the real project capture. Local project version is not
sent to the Broker and is not an ACL selector. The current Broker deployment uses
`required_scopes=[]`; JWT signature, issuer, API audience, resource, time, token
use, client binding and the typed scope claim remain validated.

A 200 response must be an object with exactly `subject` (an object containing
`issuer` and `subject`), `authorization_revision` (i64), `project_id`, `module` and
`action`. The returned literal principal and operation must match the request.
The current SQL revision is an observation, not a permission lease or a provider
revision. Duplicate or unknown fields, arrays in place of objects, trailing JSON,
wrong types, excessive input or mismatched principal/operation are rejected.
A PROJECT 403 denies the read. Other status codes, network, authentication,
physical-state and cancellation/deadline errors retain typed causes without
including the Bearer token or response body in diagnostics.

## PROJECT Hub reads

`ProjectReadModule::Hub` requires `Permission::Read(Module::Hub)`. A Web
admission sends the literal `module: "hub", action: "read"` through the same
`BrokerRecordRead` and `ProjectRecordAuthority::admit_broker_read` path. A
Knowledge decision, another project, a write decision or a valid JWT without a
matching PROJECT Hub decision cannot supply this admission.

Reading the global installed registry uses its separate installation admission.
An original Agent, Skill or Rule descriptor inside a maintained PROJECT requires
its own real PROJECT Hub/read admission and physical source capture. Artifact
version is not inferred as a project version. Hub/read grants neither Hub writes
nor S3/STS access, and a native rendered payload is not an original Markdown
descriptor. The caller keeps both admissions through its actual protected reads.
`InstalledHubReadAdmission::check_project_source` checks that they belong to
the same issuer, Original snapshot, registered Web caller, verifier and exact
provider selection. It accepts only the existing PROJECT Hub/read proof,
rejecting Knowledge and non-Web proofs. It locally rechecks both admissions
without another PROJECT call or authority conversion.

The affected Auth HTTP fixture uses a registered Web client and signed JWTs to
exercise Hub/read success, denied PROJECT access, Knowledge/other-project/write
response mismatches, invalid JWTs and unavailable UserInfo. This validates the
Auth boundary; Broker SQL capability policy and the Live descriptor consumer
retain their own product qualification.

## Escrita Memory pelo principal Web

`ProjectRecordAuthority::admit_broker_write(BrokerRecordWrite)` retorna a
`ProjectWriteAdmission` existente para `Memory` ou `FencedMemory`. O input recebe
o `AuthenticatedWebCaller` real, verificador, backend, mesmo `RequestContext`,
capturas físicas e provider. Não recebe Actor, username ou identidade livre,
nem transforma Web em Runtime ou Incoming. Outros tipos de escrita PROJECT
são recusados por este produtor.

A requisição usa `module: memory` e `action: write`. A resposta deve corresponder
exatamente ao principal, projeto, módulo e escrita solicitados. Uma decisão
de leitura não autoriza escrita. A prova guarda a permissão específica e
revalida as capturas, provider, validade assinada e Original em seus getters.
Cada nova operação pede decisão SQL atual; os rechecks locais não repetem a
requisição. `FencedMemory` retorna `MemoryFenceRequired`, que o store deve cumprir.

O ator Web é atribuição emitida pelo servidor a partir de installation e
issuer/sub verificados. É estável entre pedidos do mesmo principal e não muda
com username/display name. O mesmo helper serve PROJECT e USER Web; esse ator
não concede acesso ou substitui a admissão. As APIs não alteram o TTL nem o
comportamento OAuth descrito abaixo.

## Expiration and later operations

JWT authentication trusts the signed access token until its expiration, with
trusted cached discovery/JWKS and key rotation. Warm requests require neither
UserInfo nor introspection. UserInfo remains optional for missing profile data.
Remote token revocation, account blocking and claim changes can take until the
access token expires to be observed. The Broker's existing default access-token
duration is 10 minutes, configurable from 1 minute to 1 hour. The separate native
issuer's 30-minute default is unchanged. PKCE, state, nonce, ID-token validation
and refresh retain their existing OAuth behavior.

Every new Records operation requests a fresh PROJECT decision from the Broker's
SQL ACL. Removing a permission denies the next operation even while the JWT stays
valid. An existing admission rechecks only its original request, signed expiry,
provider observation and physical captures; it performs no repeated remote ACL
calls. This does not promise remote ACL revocation between each instruction or
atomicity between the HTTP decision and a local read.

## Qualification boundary

Focused tests use the real issuer/capture fixture and existing OAuth HTTP fixture
for current provider state, warm JWT authentication, strict response handling,
PROJECT denial and local rechecks. The ignored native test additionally uses the
public Broker PROJECT fixture with its actual signed token, matching trusted JWKS,
ordinary PostgreSQL TLS ACL and finite cleanup. Supply its qualified executable
through `GRAPHIT_BROKER_PROJECT_FIXTURE_BIN`; credentials stay in transient IPC.

```text
cargo test --locked -p graphit-auth --lib issuer::tests::broker_records
cargo test --locked -p graphit-auth --lib broker_records_native_public_project_fixture -- --ignored
cargo test --locked -p graphit-records --test broker_records -- --ignored
```

Linux qualification executed two focused Broker Records cases, five Web OAuth
cases and 25 existing Records admission cases. The focused cases reject expired,
malformed and wrong-service JWTs while UserInfo is unavailable, and reject
malformed, oversized and mismatched PROJECT responses. The initial native Auth case
verified Task, Memory and Knowledge read admissions against the actual Broker
fixture and denied the next operation after ACL removal. The Memory write
consumer extends that same fixture with Memory/FencedMemory and server-derived
actor checks. Independent read/write SQL capability denial reuses the unchanged
server qualification tsk-5df1; no additional fixture protocol is introduced.

The separate Records integration seeds a Task session with a legitimate role,
then uses the incoming Broker proof to read it through `NativeTaskStore`.
It rejects a different request or module and denies a removed PROJECT permission
before touching an invalid storage path. Both native tests completed fixture
shutdown and process reaping. Their setup deadlines are finite test settings;
they do not change access-token duration or product request deadlines.

This SDK qualification does not complete browser routing or the Broker's full OIDC
server. See [Web OAuth](rust_web_oauth.md), [role credentials](role_credentials.md)
and [the native Auth CLI component](native_auth_cli.md) for their separate callers.
