# Rust browser OAuth sessions

Current path contract: PROJECT roots and relative paths cross JSON/API/CLI as literal Unicode strings. Serde preserves spaces, accents, escapes and separators without lossy conversion. Non-Unicode input fails explicitly; `{os,units}` is rejected. Historical evidence below describes the code that was exercised then and does not qualify the removed path codec.

BrowserAuth composes configured BrokerVerifier instances and the Rust WebOAuth protocol. The process owner supplies trusted provider names, a canonical HTTPS origin and actual timeouts. Explicit literal-loopback HTTP may be enabled for development. Config changes drain and replace the component; providers remain fixed for its lifetime. Root owns executable and route composition.

GET /auth/providers lists only sorted configured provider names. POST /auth/login selects one of those names and requires the exact configured Origin. It returns HTTP200 JSON `{"authorization_url":"…"}` and the opaque flow cookie. The browser navigates to that URL; the login fetch does not follow a cross-origin HTTP redirect. Dynamic Web registration and Authorization Code with state, nonce and PKCE run inside an owned pending-login future. GET /auth/callback delivers the configured callback once through a flow cookie. A successful ID/access signed JWT exchange issues a separate unpredictable opaque session cookie and redirects to `/`. No token, daemon credential, raw callback or borrowed authority is returned or persisted.

GET /auth/session creates a new original per request and validates the signed access JWT locally using trusted cached JWKS. Issuer, subject, registered client, audiences/resource, expiry and provider binding remain exact. Cache misses or real key rotation may require SDK key discovery; cached-key valid requests require no UserInfo or introspection. Revocation, account blocking and changed claims take effect at access expiry or a subsequent genuine refresh. This delay is accepted. No cached UserInfo identity substitutes for signed claims.

Login-start SDK failures preserve their existing HTTP status and `error` message,
and add finite `phase` and `cause` fields. `register` identifies the actual SDK
registration call, including its discovery/key-fetch prerequisites; `authorize`
identifies preparation of the authorization request. A register error alone does
not prove that a DCR request was sent. Causes retain the concrete finite SDK
category, such as `broker_invalid_metadata`, `broker_connection`,
`broker_transport`, `http_rejected` or `invalid_response`. If the same Original
has actually expired or been cancelled, `original_deadline` or
`original_cancelled` takes precedence in this diagnostic. Stop/deadline before
the authorization URL is delivered retains the existing interrupted response.

The same failure path writes one journal line containing only those finite
labels, even when the HTTP caller has disconnected. It never formats the SDK
error or includes endpoint URLs, headers, provider bodies, credentials, state,
nonce or authorization URLs. Successful start responses and cookies are unchanged.
These diagnostics introduce no retry, timeout extension or alternate OAuth flow;
they do not establish the cause of an earlier unrecorded failure. The focused
HTTP consumers are in `crates/graphit-app/tests/browser_auth_start.rs`.

The six focused HTTP tests passed: invalid discovery, rejected registration,
invalid authorization preparation, original deadline, cancellation with owned
cleanup, and successful start with the existing cookie and PKCE/state/nonce.
Their five failure journal lines contained only the expected finite labels;
the private-data canary was absent. This qualifies the start component, not a
completed human login in a running Source. Earlier unrecorded HTTP502 responses
remain unattributed until the current consumer produces a diagnostic.
Clippy also passed for the App library and this integration target with
`--no-deps -- -D warnings`; the unrelated Auth and JWT suites were not repeated.

The `/auth/session` response contains the provider, signed subject, optional signed username and access expiry. A missing username is JSON null, never the subject copied as a name. UserInfo may only enrich genuinely missing profile fields when a consumer needs them; it is not an authentication or permission requirement. This status consumer needs no UserInfo call.

`GET /api/daemon/status` is composed with the Browser when Source is present. It uses the same active cookie, refresh and signed-access verification as `/auth/session`; an explicitly supplied Origin must match the configured origin. Its no-store response is the existing daemon control `Reply`, limited for this read to `{ "pid": <u32>, "state": "running" | "stopping" }`. The state comes from the current Source stop token. It exposes no runtime credential, credential path, logs or invented scheduler/uptime data. It does not admit daemon administration: `POST /admin/daemon` retains its separate Runtime `Configure` requirement, and the Browser status route accepts no commands.

Refresh is automatic when required and serialized under the session mutex. A rotating refresh failure with a possible remote effect invalidates the session. Dropping the request while refresh is in flight also marks its session unusable; the prior refresh token is never retried. Completed SDK failures retain their actual remote-effect classification.

POST /auth/logout requires the configured Origin, invalidates the local session before remote revocation and clears its cookie even if revocation fails. Providers without revocation still lose the local session. Cookies use the CookieJar SDK, HttpOnly, Path /, HTTPS Secure and __Host names. Flow cookies are SameSite=Lax for OIDC callback; session cookies are Strict. All component responses are no-store.

Server memory owns zeroizing credential data. Session lifetime is independent of access expiry. Owned Tokio TTL timers release unused credentials without another request; dropping a session cancels its timer. Login expiry and cancellation clean pending flows. Shutdown cancels work, joins the same owned jobs and destroys sessions.

Authentication does not grant PROJECT access. Browser Task, Memory, Knowledge and AST reads use separate real current Broker PROJECT decisions and module-specific admissions on every operation. No permissive policy is supplied. This component does not certify the executable, React journey or local enrollment.

Linux, Windows and macOS compose the same native AST, Now, Live and Hub routers. Their existing producers retain Web sessions, PROJECT authorization, original requests, cancellation and owned cleanup. Hub routes require the actual Source service; an absent service returns unavailable rather than substituting another source. Platform composition alone does not certify a native Windows or macOS execution.

The application owner composes the router and drains the component before replacing its providers:

```no_run
use graphit_app::browser_auth::{BrowserAuth, BrowserPolicy};
use graphit_auth::broker::BrokerVerifier;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn compose(providers: Vec<(String, Arc<BrokerVerifier>)>, policy: BrowserPolicy, stop: CancellationToken) -> std::io::Result<axum::Router> {
    let sessions = BrowserAuth::new(providers, "https://workspace.example", policy, stop)?;
    Ok(sessions.router())
}
async fn stop(sessions: &BrowserAuth) -> std::io::Result<()> { sessions.shutdown().await }
```

## Browser Task reads

BrowserAuth::task_router(TaskServer, Arc<BrokerProjectAccess>) composes the existing auth routes with POST /api/tasks/catalogue, /get, /session, /view and /session-heads. The routes require the exact configured Origin and an active opaque session cookie. They accept the same typed selection/input envelope as TaskServer; no runtime headers, active CLI profile or incoming grant DTO substitutes for the session. The process owner provides TaskServer from the same DaemonAuth/installation and configured provider revision as BrowserAuth. Selectors choose only retained physical project/version tuples.

Each request creates one original Hub RequestContext from the Task service policy.
Refresh is serialized by the existing session credential mutex. The handler copies
the access token into a zeroizing request value and releases that mutex before
local access-JWT verification, fresh PROJECT authorization and Records read/delivery.
A full valid JWT works with UserInfo unavailable; these reads do not request
profile enrichment. Invalid, expired or wrong-audience credentials return 401.
A missing or removed PROJECT grant/provider returns 403; malformed envelopes
return 400 and the actual request-body limit returns 413, without creating a
project or store. Backend failures retain secret-safe network/response/store categories.

The handler captures the selected physical ProjectPaths/ProjectId/VersionContext and passes the borrowed AuthenticatedWebCaller to ProjectRecordAuthority::admit_broker_read for ProjectReadModule::Task. The configured backend makes a fresh PROJECT request before execute_read reaches the persisted NativeTaskStore. It is not a cached permission lease. Dates, dependencies, histories and cursor pins are the Records producer's data. Catalogue cursor objects and the other native read results remain unchanged; no participant claim fence or OAuth secret is delivered.

A disconnected handler cancels its original Core token. Work remains in the existing BrowserAuth-owned JoinSet until native cleanup completes; shutdown cancels admission, joins work and releases session credentials. TaskServer must also be drained before DaemonAuth teardown. A rotating refresh with an uncertain remote effect invalidates the session, using the same B1 helper and SDK classification. Local live/expiry and physical-admission checks run before delivery. Remote account/grant changes have the existing JWT-expiry and fresh-PROJECT semantics, without polling or a new resource ledger.

POST /api/tasks/context accepts the same selection envelope without an input payload. It authenticates and obtains the same fresh PROJECT admission, then returns only the selected captured NativePath, project ID and resolved version. These are selector data, not an authority or grant. The browser echoes this tuple into subsequent catalogue/get requests and sends an explicit selected version in the JSON envelope. Paths are literal Unicode JSON strings; invalid Unicode or old object envelopes are rejected. Unknown or ambiguous other worktrees remain selection errors until a real native context is supplied; this endpoint does not promise a global project catalogue. It does not materialize or read a Task corpus.

POST /api/tasks/contexts accepts an empty selection and no input payload. It returns `{"contexts":[…],"current":…}` using exactly those same context tuples. Only the projects and versions already retained by the TaskServer owner are considered. Every tuple requires its own fresh PROJECT admission and physical rechecks in the same original request. A denied or failed candidate fails the entire response with its existing error category; no partial or fabricated empty inventory is returned. Collection and aggregate response-byte limits come from the Task content policy, and the original request deadline covers the whole operation. This is not a global Workspace/Hub catalogue and does not discover Git branches or checkouts. The NativePath must remain in the selection and in stale-response keys: ID and version alone can be ambiguous for two retained directories of the same project/version. Listing grants no authority to the next read, which authorizes again.

The native socket fixtures persist/reopen Task and Session data and exercise all five reads, PROJECT denial/removal, selection, malformed/body-limit errors, JWT failure, serialized rotation and shutdown. The TCP EOF case joins the read before shutdown while keeping the legitimate session-expiry timer alive, then completes a sibling read. A separate explicit public Broker fixture test exercises its real signed Web access JWT and PostgreSQL PROJECT endpoint; its OAuth registration/callback counterpart is the local signed TCP test issuer. This is component integration, not a production full OIDC or React/executable qualification.

The React native Task calendar and record views are described in [Native Task calendar and record views](rust_task_gantt.md). They echo the authenticated context JSON for catalogue/detail/Session reads; context is selector data and grants no authority.

The login JSON response and retained-context listing are implemented by tsk-744f. Its five signed-TCP workspace tests passed with three genuinely trusted native roots, including two directories sharing project ID/version, exact-path selection, persisted/reopened calendar data, second-PROJECT denial without partial output, input/body/collection limits and deadline/stop joins. Fourteen affected local Browser tests and one compiled Foreground consumer also passed. Earlier public Broker and Source receipts remain separate antecedents; these twenty tests do not qualify hosted React, which belongs to tsk-17a7. Receipts and earlier failures are retained in `.build/rust-foreground-react/`. The final aggregate checks Original and Task.stop after encoding limits; the five workspace tests passed again after that change, and the affected App library/Foreground test passed Clippy with `--no-deps -- -D warnings` (`api-strict02.log`).

## Browser AST reads

The foreground composes its actual shared GraphServer into the existing Web session owner. POST `/api/ast/contexts` accepts `{}` and returns `{contexts:[{project_dir,project_id,project_version}],current:selection|null}`. Discovery checks the live physical tuple, current AST module setting and ReadAst PROJECT permission. It omits denied or disabled projects and leaves current null when the operational selection is not admitted. Task settings and Task grants do not admit or block AST.

POST `/api/ast/query`, `/schema`, `/sample`, `/neighborhood` accepts the exact typed PROJECT envelope. `selection` contains optional NativePath `project_dir` and actual `project_version`; query adds `{query,parameters?}`, and neighborhood adds `{anchor_label,anchor_key:{type,value},relationship,other_label,direction,limit,cursor?}`. Schema and sample contain only selection. Unknown fields, unavailable or ambiguous selections and incorrect operation inputs fail rather than selecting a fallback.

The exact canonical Origin and active Web cookie are required. WebOAuth verifies the registered caller and locally validates its access JWT; the separate Broker ReadAst decision does not use a runtime credential or UserInfo freshness. The async owner retains this admission and shares one genuine PreparedRead with its native Data job using the same Original. After reading, module/config, physical publication, live selection, admission, cancellation, deadline and session state are checked before delivery. Confirmed Copy, native jobs and shutdown retain the existing Core/Data ownership contracts.

AST handlers copy only the access token into a zeroizing request value and
release the session credential mutex before local JWT verification, PROJECT
authorization or graph reads. Refresh remains serialized when required; a
pending AST read does not block the same session's identity or other readers.

PROJECT sample nodes provide the actual Parquet primary key as `primary_key:{name,value:{type:"string"|"int64",value:string}}`. String values remain literal and Int64 values remain exact decimal text; clients can pass the typed value directly as a neighborhood anchor. Imported samples omit this field and keep their existing shape. Neither selector nor key DTO grants access.

### PROJECT AST Source

`POST /api/ast/source` uses the same cookie, canonical Origin, verified Web caller,
separate Broker PROJECT ReadAst admission and Original as the graph readers.
File Source does not require AST indexing to
be enabled; graph discovery, graph queries and entity Source retain their AST
module gate. Disabling indexing does not revoke a separately admitted file read.
The request is `{selection:{project_dir?,project_version?},source:{path,entity?,entity_type?,head?,tail?,start_line?,end_line?,pattern?,regex?,before?,after?,line_numbers?}}`.
Unknown Source fields and explicit null Source selectors are rejected. The Source parameter
DTO is shared by the existing flat CLI/MCP arguments and the nested Web request.

The response is `{origin:"indexed"|"physical",selection:{source,total_lines,start_line?,end_line?,matches?}}`.
Ordinary excerpts retain global file coordinates. Entity selection applies the
requested range relative to the observed entity window; returned coordinates
still refer to the full file. Pattern results retain Core's line prefixes and
individual match coordinates, including disjoint contexts.

File inspection does not require a graph or parser. Indexed text, including an
empty stored document, takes precedence; an explicit missing indexed Source may
read the exact physical project/version path with the native Source capture.
There is no Git lookup or extension filter. Invalid UTF-8 and acquisition or
integrity errors are errors rather than reasons to fall back.

Entity inspection requires the confirmed native PROJECT Copy graph. It resolves
name/path/type without interpolating input values, rejects ambiguous matches,
and requires available exact start/end coordinates. Its observed Raw generation
and physical/inherited branch are checked against the acquired Raw index when
present. The actual Source bytes must match the graph's Source SHA before an
entity window is applied. Physical fallback cannot apply old coordinates to
unrelated bytes. Graph, scope, module, grant and Original checks bracket delivery.

The React file consumer must abort superseded fetches, preserve the returned
origin, and use returned global excerpt coordinates for numbering/highlighting.
An entity UI caller is not implied by the file consumer. Installed Source remains
a separate existing CLI/MCP path; this PROJECT endpoint does not silently select
an installed context or grant imported access.

### Installed AST and PROJECT semantic retrieval

The Browser uses `POST /api/ast/imports` with `{}` to discover the installation's
actual GlobalNative/Native AST entries. Results contain `id`, `version`, exact
`context` (`id@version`) and `backend`. Catalogue identifiers confer no authority.
The server explicitly enables `with_web_imported_ast_read(provider)` for configured
Browser providers; otherwise imported AST is denied even for a valid Web token or
PROJECT/Knowledge grant. The non-clone admission retains its issuer, verifier,
provider, original request and captured installation and rechecks at I/O and output.

`POST /api/ast/imported/schema` and `/sample` accept `{source:{id,version}}`;
`/query` additionally accepts `query:{query,parameters?}`, and `/neighborhood`
accepts the same typed neighborhood input as PROJECT. `/api/ast/imported/source`
accepts `{source:{id,version},input:SourceParameters}`. Installed Source has no
implicit fallback to a project. Entity extents are resolved from that exact installed
graph and applied to the indexed document by the existing Source selectors. Browser
import samples expose the genuine observed primary key; existing native installed
sample fields are unchanged. All routes use the existing cookie and Origin checks.
CLI `ast imports` and MCP `graphit_ast_imports` list the same catalogue with native
ReadAst authorization and no PROJECT selector.

`POST /api/ast/search` accepts the closed body
`{selection:{project_dir?,project_version?},search:{query,mode?,k?,metric?}}`.
The current local producer supports `mode:semantic|vector` and
`metric:l2|cosine|dot`. Web uses fresh ReadAst PROJECT admission and the same
SourceServer model, index, client and original request. A separate Web embedding
service admission obtains the local lease; the Web caller is never coerced into
Runtime. Missing, disabled and unavailable producers retain the Source SDK's
actual states. FTS, hybrid and imported semantic retrieval require their own active
producers and remain unfinished obligations; this endpoint does not simulate them
with Cypher or silently reinterpret their modes.
