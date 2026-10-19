//! Wire regressions for the same authoritative tables, including the Web bearer route.
use super::*;
use axum::{extract::RawQuery, http::header, response::Redirect};
use graphit_auth::broker::{
    BrokerRequestContext, BrokerVerifier,
    project_access::{
        BrokerProjectAccess, BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead,
        ProjectAccessPolicy,
    },
};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct WebState {
    nonce: String,
    challenge: String,
    issued: String,
    incoming_issued: String,
}
pub(super) fn signed(state: &BrokerState, claims: Value) -> String {
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    format!(
        "{message}.{}",
        URL_SAFE_NO_PAD.encode(state.key.sign(message.as_bytes()).as_ref())
    )
}
pub(super) async fn register(Json(input): Json<Value>) -> (StatusCode, Json<Value>) {
    assert_eq!(input["application_type"], "native");
    assert_eq!(input["token_endpoint_auth_method"], "none");
    (
        StatusCode::CREATED,
        Json(
            json!({"client_id":"web-client","application_type":input["application_type"],
        "redirect_uris":input["redirect_uris"]}),
        ),
    )
}
pub(super) async fn authorize(
    AxumState(state): AxumState<Arc<BrokerState>>,
    RawQuery(query): RawQuery,
) -> Redirect {
    let fields: HashMap<String, String> =
        url::form_urlencoded::parse(query.as_deref().unwrap_or("").as_bytes())
            .into_owned()
            .collect();
    assert_eq!(fields["client_id"], "web-client");
    assert_eq!(fields["response_type"], "code");
    assert_eq!(fields["code_challenge_method"], "S256");
    assert_eq!(fields["resource"], format!("{}/v1", state.origin));
    let mut callback = url::Url::parse(&fields["redirect_uri"]).unwrap();
    assert_eq!(callback.host_str(), Some("127.0.0.1"));
    assert_eq!(callback.path(), "/auth/callback");
    {
        let mut web = state.web.lock().unwrap();
        web.nonce = fields["nonce"].clone();
        web.challenge = fields["code_challenge"].clone();
    }
    callback
        .query_pairs_mut()
        .append_pair("code", "fixture-code")
        .append_pair("state", &fields["state"]);
    Redirect::to(callback.as_str())
}
pub(super) async fn tokens(
    AxumState(state): AxumState<Arc<BrokerState>>,
    body: String,
) -> Json<Value> {
    let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    assert_eq!(form["grant_type"], "authorization_code");
    assert_eq!(form["client_id"], "web-client");
    assert_eq!(form["resource"], format!("{}/v1", state.origin));
    let mut web = state.web.lock().unwrap();
    assert_eq!(
        URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes())),
        web.challenge
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let access = signed(
        &state,
        json!({"iss":state.origin,"sub":"web-subject","client_id":"web-client","token_use":"access",
        "jti":"web-storage","aud":["broker-api",format!("{}/v1",state.origin)],"iat":now,"exp":now+600,
        "preferred_username":"web-user"}),
    );
    web.issued = access.clone();
    let identity = signed(
        &state,
        json!({"iss":state.origin,"sub":"web-subject","aud":"web-client","iat":now,"exp":now+600,
        "nonce":web.nonce,"preferred_username":"web-user"}),
    );
    Json(json!({"access_token":access,"id_token":identity,"token_type":"Bearer","expires_in":600}))
}
pub(super) async fn project_access(
    AxumState(state): AxumState<Arc<BrokerState>>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let bearer = headers[header::AUTHORIZATION].to_str().unwrap();
    let subject = {
        let identities = state.web.lock().unwrap();
        if !identities.issued.is_empty() && bearer == format!("Bearer {}", identities.issued) {
            "web-subject"
        } else {
            assert!(
                !identities.incoming_issued.is_empty()
                    && bearer == format!("Bearer {}", identities.incoming_issued)
            );
            state.incoming_project_calls.fetch_add(1, Ordering::SeqCst);
            "incoming-subject"
        }
    };
    assert_eq!(input.as_object().unwrap().len(), 3);
    assert!(matches!(input["module"].as_str(), Some("memory" | "task")));
    assert!(matches!(input["action"].as_str(), Some("read" | "write")));
    if !state.project_grant.load(Ordering::SeqCst) {
        return (StatusCode::FORBIDDEN, Json(json!({})));
    }
    (
        StatusCode::OK,
        Json(
            json!({"subject":{"issuer":state.origin,"subject":subject},"authorization_revision":1,
        "project_id":input["project_id"],"module":input["module"],"action":input["action"]}),
        ),
    )
}

struct Host {
    origin: String,
    serving: tokio::task::JoinHandle<io::Result<()>>,
}
impl Drop for Host {
    fn drop(&mut self) {
        self.serving.abort();
    }
}
impl Host {
    fn native(client: &Client) -> Self {
        let socket = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &Cancellation::default(),
        )
        .unwrap();
        let origin = format!("http://{}", socket.address());
        let server = client.server.clone();
        Self {
            origin,
            serving: tokio::spawn(async move { server.serve_listener(socket).await }),
        }
    }
    async fn join(&mut self) {
        (&mut self.serving).await.unwrap().unwrap();
    }
}
async fn connect(host: &Host, raw: &str) -> rmcp::service::RunningService<RoleClient, ()> {
    let mut headers = HashMap::new();
    let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    bearer.set_sensitive(true);
    headers.insert(header::AUTHORIZATION, bearer);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_static("runtime"),
    );
    ().serve(StreamableHttpClientTransport::with_client(
        reqwest::Client::new(),
        StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", host.origin))
            .custom_headers(headers),
    ))
    .await
    .unwrap()
}
async fn mcp(
    client: &rmcp::service::RunningService<RoleClient, ()>,
    operation: &str,
    input: Value,
) -> Value {
    let response = client
        .call_tool(
            CallToolRequestParams::new(operation.to_owned()).with_arguments(
                json!({"ai_optimized":false,"input":input})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let value: Value =
        serde_json::from_str(&response.content[0].as_text().expect("MCP text result").text)
            .expect("MCP JSON result");
    assert_ne!(
        response.is_error,
        Some(true),
        "{operation}: {}",
        value["code"]
    );
    value
}
fn selection(client: &Client) -> Value {
    browser::memory_fixture::selection(&client.server, 0)
}
async fn native_http(client: &Client, host: &Host, operation: &str, input: Value) -> Value {
    let response = reqwest::Client::new()
        .post(format!("{}/api/tasks/native/{operation}", host.origin))
        .bearer_auth(&client.raw)
        .header("x-graphit-auth-origin", "runtime")
        .json(&json!({"selection":selection(client),"input":input}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "native HTTP record operation failed"
    );
    response.json().await.unwrap()
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn task_mcp_and_native_http_observe_each_others_remote_commits() {
    let broker = Broker::start(S3Fixture::load()).await;
    let a = Client::new(&broker, None);
    let b = Client::new(&broker, Some(&a));
    let mut host_a = Host::native(&a);
    let mut host_b = Host::native(&b);
    let caller = connect(&host_a, &a.raw).await;
    let session = mcp(
        &caller,
        "graphit_task_session_create",
        json!({"idempotency_key":"wire-session","title":"Wire session",
        "description":"Shared S3 via actual transports","strategy":"MCP writes, HTTP reads"}),
    )
    .await;
    let task=mcp(&caller,"graphit_task_create",json!({"idempotency_key":"wire-task","session_id":session["result"]["id"],
        "title":"Wire Task","description":"Authoritative remote record","task_type":"TASK","priority":1,
        "checks":[{"kind":"acceptance","text":"Read the shared record"},{"kind":"test","text":"Verify MCP and HTTP consumers"}]})).await;
    let id = task["result"]["specification"]["id"].clone();
    assert_eq!(
        native_http(&b, &host_b, "get_task", json!({"id":id})).await["result"]["task"]["specification"]
            ["title"],
        "Wire Task"
    );
    let claim = native_http(
        &b,
        &host_b,
        "claim_task",
        json!({"id":id,"lease_ms":120000}),
    )
    .await;
    let revised = native_http(&b,&host_b,"revise_task",json!({"target":{"id":id,"claim":{"token":claim["result"]["token"],
        "expected_revision":claim["result"]["record"]["specification"]["revision"]}},"lease_ms":120000,
        "reason":"HTTP revision","title":"HTTP revised Task"})).await;
    assert_eq!(
        mcp(&caller, "graphit_task_get", json!({"id":id})).await["result"]["task"]["specification"]
            ["title"],
        "HTTP revised Task"
    );
    let checked = native_http(&b,&host_b,"verify_task_check",json!({"target":{"id":id,
        "claim":{"token":claim["result"]["token"],"expected_revision":revised["result"]["specification"]["revision"]}},
        "lease_ms":120000,"check_id":revised["result"]["specification"]["checks"][0]["id"],"passed":true,
        "evidence":"Actual independent MCP writer and HTTP reader"})).await;
    let comment = native_http(&b,&host_b,"add_task_comment",json!({"target":{"id":id,
        "claim":{"token":claim["result"]["token"],"expected_revision":checked["result"]["specification"]["revision"]}},
        "lease_ms":120000,"idempotency_key":"wire-comment","kind":"decision","body":"Shared comment via HTTP"})).await;
    assert_eq!(comment["result"]["kind"], "created");
    let observed = mcp(&caller, "graphit_task_get", json!({"id":id})).await;
    assert_eq!(
        observed["result"]["task"]["specification"]["checks"][0]["status"],
        "passed"
    );
    assert!(observed["result"]["history"].is_null());
    assert!(observed["result"]["comments"].is_null());
    assert!(observed["result"].get("events").is_none());
    let comments = mcp(
        &caller,
        "graphit_task_get",
        json!({"id":id,"comments":{"page_size":1,
        "cursor":observed["result"]["comments_start_cursor"]}}),
    )
    .await;
    assert_eq!(
        comments["result"]["generation"],
        observed["result"]["generation"]
    );
    assert_eq!(
        comments["result"]["comments"]["comments"][0]["body"],
        "Shared comment via HTTP"
    );
    assert_eq!(
        comments["result"]["comments"]["comments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(comments["result"]["comments"]["next_cursor"].is_null());
    let referenced = mcp(&caller,"graphit_task_create",json!({"idempotency_key":"wire-reference-task",
        "session_id":session["result"]["id"],"title":"Referenced Task","description":"Persists an explicit relation",
        "task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Persist the authorized task"},{"kind":"test","text":"Read the shared remote task"}],"references":[{"target":{"type":"task","id":id},"relation":"supports"}]})).await;
    let related = native_http(
        &b,
        &host_b,
        "get_task",
        json!({"id":referenced["result"]["specification"]["id"]}),
    )
    .await;
    assert_eq!(related["result"]["task"]["references"][0]["id"], id);
    let first = native_http(
        &b,
        &host_b,
        "search",
        json!({"text":"Task","filter":{},"page_size":1,"top_k":4}),
    )
    .await;
    let cursor = first["result"]["Published"]["next_cursor"].clone();
    assert!(
        cursor.is_object(),
        "remote search must expose a usable continuation"
    );
    assert!(cursor["binding"]["corpus_key"].is_string());
    let next = native_http(
        &b,
        &host_b,
        "search",
        json!({"text":"Task","filter":{},"page_size":1,"top_k":4,"cursor":cursor}),
    )
    .await;
    assert_eq!(
        next["result"]["Published"]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut foreign = cursor;
    foreign["binding"]["corpus_key"] = json!("another-storage-destination");
    let rejected = b
        .task(
            "search",
            json!({"text":"Task","filter":{},"page_size":1,"top_k":4,"cursor":foreign}),
            false,
        )
        .await;
    assert_eq!(
        rejected
            .expect_err("another corpus cannot continue this snapshot")
            .code,
        "invalid_cursor"
    );
    caller.cancel().await.unwrap();
    for client in [&a, &b] {
        client.assert_no_local_tables();
        client.server.shutdown().await.unwrap();
    }
    host_a.join().await;
    host_b.join().await;
}

fn network() -> NetworkLimits {
    NetworkLimits {
        url_bytes: 8192,
        request_body_bytes: 8192,
        response_body_bytes: 32768,
        max_in_flight: 8,
        header_bytes: 16384,
        header_count: 32,
        max_idle_per_host: 2,
        idle_timeout: Duration::from_secs(30),
        max_certificates: 2,
        max_certificate_bytes: 8192,
    }
}
fn web_verifier(client: &Client) -> Arc<BrokerVerifier> {
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        client.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let caller = client
        .server
        .0
        .auth
        .issuer()
        .authenticate_runtime(&client.raw, &request)
        .unwrap();
    let (binding, revision) = caller.native_provider().unwrap();
    verifier_cache().get_native(&binding, revision).unwrap()
}

fn project_backend(broker: &Broker) -> BrokerProjectAccess {
    BrokerProjectAccess::new(
        ProjectAccessPolicy {
            endpoint: &format!("{}/v1/project/access/authorize", broker.state.origin),
            network: ClientPolicy {
                origin: &broker.state.origin,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
        },
        network(),
        JsonLimits {
            bytes: 32768,
            nodes: 256,
            depth: 16,
            scalar_bytes: 8192,
            collection_entries: 64,
        },
    )
    .unwrap()
}

// Compose the existing verified Incoming admission and native protocol under
// the App's actual issuer/resolver. TaskServer's Runtime/Local/Role wire origins
// do not manufacture a BrokerIncoming credential.
async fn incoming_read(
    client: &Client,
    broker: &Broker,
    bearer: &str,
    operation: &str,
    input: Value,
    selected: usize,
) -> Result<Value, TaskStoreError> {
    let cancellation = Cancellation::default();
    let policy = &client.server.0.policy;
    let original = RequestContext::new(
        policy.content,
        &cancellation,
        Instant::now() + policy.timeout,
    )
    .unwrap();
    let snapshot = client
        .server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live Incoming storage test selection"));
    let (paths, project_id, version) = &snapshot.projects[selected];
    let scope =
        CapturedPublicationScope::project(paths, *project_id, policy.scope, original.snapshot())?;
    let binding = scope.binding()?;
    let verifier = Arc::clone(&client.incoming_verifier);
    let caller = verifier
        .verify(bearer, BrokerRequestContext::new(original.snapshot()))
        .await
        .map_err(io::Error::other)?;
    let backend = project_backend(broker);
    let authority = ProjectRecordAuthority::new(client.server.0.auth.issuer());
    let admission = authority
        .admit_broker_read(BrokerRecordRead {
            caller: BrokerProjectCaller::Incoming(&caller),
            verifier: &verifier,
            backend: &backend,
            original: &original,
            scope: BrokerProjectScope {
                installation: client.server.0.auth.installation(),
                project: ProjectCapture {
                    scope: ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                    version,
                },
                providers: &[],
            },
            provider: "incoming",
            module: ProjectReadModule::Task,
        })
        .await
        .map_err(io::Error::other)?;
    assert!(Arc::ptr_eq(
        original.snapshot(),
        admission.request()?.snapshot()
    ));
    let mut service = graphit_auth::RecordStorageAdmission::ProjectRead(&admission)
        .service()
        .map_err(io::Error::other)?
        .expect("Broker RecordStorage selection");
    assert_eq!(
        service.mode().map_err(io::Error::other)?,
        graphit_auth::ServiceMode::BrokerIncoming
    );
    let request = serde_json::from_value(json!({"operation":operation,"input":input}))?;
    let response = execute_read(
        request,
        original.snapshot(),
        &admission,
        policy.read,
        std::num::NonZeroUsize::new(policy.content.max_input_bytes).unwrap(),
    )
    .await?;
    Ok(serde_json::to_value(response)?)
}
fn web_policy() -> crate::browser_auth::BrowserPolicy {
    crate::browser_auth::BrowserPolicy {
        oauth: graphit_auth::broker::oauth::OAuthLimits {
            authorization_url_bytes: 8192,
            callback_bytes: 8192,
            callback_query_entries: 8,
            form_body_bytes: 8192,
            token_bytes: 4096,
            response_nodes: 32,
            response_scalar_bytes: 8192,
        },
        request_timeout: Duration::from_secs(30),
        login_timeout: Duration::from_secs(30),
        session_lifetime: Duration::from_secs(600),
        refresh_margin: Duration::ZERO,
        allow_loopback_http: true,
    }
}
async fn login(http: &reqwest::Client, origin: &str) -> String {
    let response = http
        .post(format!("{origin}/auth/login"))
        .header(header::ORIGIN, origin)
        .json(&json!({"provider":"broker"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let flow = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let authorization = response.json::<Value>().await.unwrap()["authorization_url"]
        .as_str()
        .unwrap()
        .to_owned();
    let authorized = http.get(authorization).send().await.unwrap();
    assert_eq!(authorized.status(), StatusCode::SEE_OTHER);
    let callback = authorized.headers()[header::LOCATION].to_str().unwrap();
    let response = http
        .get(callback)
        .header(header::COOKIE, flow)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            value
                .to_str()
                .ok()
                .filter(|v| v.starts_with("graphit_session="))
                .map(|v| v.split(';').next().unwrap().to_owned())
        })
        .expect("server-owned Web session cookie")
}
async fn web(
    http: &reqwest::Client,
    host: &Host,
    cookie: &str,
    path: &str,
    selected: Value,
    input: Value,
) -> reqwest::Response {
    http.post(format!("{}{path}", host.origin))
        .header(header::ORIGIN, &host.origin)
        .header(header::COOKIE, cookie)
        .json(&json!({"selection":selected,"input":input}))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn authenticated_browser_web_bearer_reads_and_writes_authoritative_s3_with_fresh_project_grants()
 {
    let broker = Broker::start(S3Fixture::load()).await;
    let a = Client::new(&broker, None);
    let b = Client::new(&broker, Some(&a));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let stop = CancellationToken::new();
    let service = crate::browser_auth::BrowserAuth::new(
        vec![("broker".into(), web_verifier(&b))],
        &origin,
        web_policy(),
        stop.clone(),
    )
    .unwrap();
    let backend = Arc::new(project_backend(&broker));
    let router = service.task_router(b.server.clone(), backend, None);
    let stopped = stop.clone();
    let mut host = Host {
        origin,
        serving: tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stopped.cancelled_owned())
                .await
        }),
    };
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .unwrap();
    let cookie = login(&http, &host.origin).await;
    let response = web(
        &http,
        &host,
        &cookie,
        "/api/memory/create",
        selection(&b),
        memory_input("web-create"),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Web Memory writes must obtain their admitted STS lease"
    );
    let created: Value = response.json().await.unwrap();
    let id = created["native"]["id"].clone();
    assert_eq!(
        a.memory("memory_get", json!({"id":id}), 0).await.unwrap()["native"]["memory"]["revision"],
        1
    );
    let response = web(
        &http,
        &host,
        &cookie,
        "/api/memory/update",
        selection(&b),
        json!({"id":id,"expected_revision":1,
        "idempotency_key":"web-update","reason":"Web revision","body":"Web revised shared memory"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        a.memory("memory_get", json!({"id":id}), 0).await.unwrap()["native"]["memory"]["revision"],
        2
    );
    assert_eq!(
        a.memory("memory_history", json!({"id":id}), 0)
            .await
            .unwrap()["native"]["revisions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let response = web(
        &http,
        &host,
        &cookie,
        "/api/memory/list",
        selection(&b),
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let listed: Value = response.json().await.unwrap();
    assert_eq!(listed["native"]["memories"].as_array().unwrap().len(), 1);
    assert_eq!(listed["native"]["memories"][0]["id"], id);
    assert_eq!(listed["native"]["memories"][0]["revision"], 2);
    let response = web(&http, &host, &cookie, "/api/memory/delete", selection(&b),
        json!({"id":id,"expected_revision":2,"idempotency_key":"web-delete","reason":"Delete remote record"})).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(a.memory("memory_get", json!({"id":id}), 0).await.unwrap()["native"].is_null());
    let deleted = a
        .memory("memory_history", json!({"id":id}), 0)
        .await
        .unwrap();
    assert_eq!(deleted["native"]["deleted"], true);
    assert_eq!(deleted["native"]["revisions"].as_array().unwrap().len(), 2);
    let mut native = Host::native(&a);
    let caller = connect(&native, &a.raw).await;
    let session = mcp(
        &caller,
        "graphit_task_session_create",
        json!({"idempotency_key":"web-task-session","title":"Web read",
        "description":"Native writer, Web reader","strategy":"independent transports"}),
    )
    .await;
    let session_id = session["result"]["id"].clone();
    let session_claim = mcp(
        &caller,
        "graphit_task_session_claim",
        json!({"id":session_id,"lease_ms":120000}),
    )
    .await;
    mcp(
        &caller,
        "graphit_task_session_revise",
        json!({
            "target":{"id":session_id,"claim":{"token":session_claim["result"]["token"],
                "expected_revision":session_claim["result"]["record"]["revision"]}},
            "lease_ms":120000,"reason":"Before Web capture","title":"Pinned Web Session"
        }),
    )
    .await;
    let response = web(
        &http,
        &host,
        &cookie,
        "/api/tasks/session",
        selection(&b),
        json!({"id":session_id}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let selected_session: Value = response.json().await.unwrap();
    assert_eq!(
        selected_session["session"]["input"]["title"],
        "Pinned Web Session"
    );
    assert_eq!(selected_session["session"]["revision"], 3);
    assert!(selected_session["history"].is_null());
    assert!(selected_session.get("events").is_none());
    mcp(&caller, "graphit_task_session_revise", json!({
        "target":{"id":session_id,"claim":{"token":session_claim["result"]["token"],"expected_revision":3}},
        "lease_ms":120000,"reason":"After Web capture","title":"Latest Web Session"
    })).await;
    let mut cursor = selected_session["history_start_cursor"].clone();
    let mut previous = selected_session["creation"]["record"].clone();
    for revision in 2..=3 {
        let response = web(
            &http,
            &host,
            &cookie,
            "/api/tasks/session",
            selection(&b),
            json!({"id":session_id,"history":{"page_size":1,"cursor":cursor}}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let page: Value = response.json().await.unwrap();
        assert_eq!(page["generation"], selected_session["generation"]);
        assert_eq!(page["session"], selected_session["session"]);
        assert_eq!(page["history"]["before"]["revision"], revision - 1);
        assert_eq!(page["history"]["before"], previous);
        assert_eq!(page["history"]["events"].as_array().unwrap().len(), 1);
        assert_eq!(page["history"]["events"][0]["record"]["revision"], revision);
        previous = page["history"]["events"][0]["record"].clone();
        cursor = page["history"]["next_cursor"].clone();
        assert_eq!(cursor.is_null(), revision == 3);
    }
    assert_eq!(previous, selected_session["session"]);
    let response = web(
        &http,
        &host,
        &cookie,
        "/api/tasks/session",
        selection(&b),
        json!({"id":session_id}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let latest: Value = response.json().await.unwrap();
    assert_eq!(latest["session"]["revision"], 4);
    assert_eq!(latest["session"]["input"]["title"], "Latest Web Session");
    assert!(latest["history"].is_null());
    assert_ne!(latest["generation"], selected_session["generation"]);
    let response = web(&http, &host, &cookie, "/api/tasks/session",
        browser::memory_fixture::selection(&b.server, 1),
        json!({"id":session_id,"history":{"page_size":1,"cursor":selected_session["history_start_cursor"]}})).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let rejected: Value = response.json().await.unwrap();
    assert_eq!(rejected["error"]["code"], "invalid_cursor");
    let task=mcp(&caller,"graphit_task_create",json!({"idempotency_key":"web-task","session_id":session["result"]["id"],
        "title":"Browser readable Task","description":"Remote Task table","task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Persist the authorized task"},{"kind":"test","text":"Read the shared remote task"}]})).await;
    let task_id = task["result"]["specification"]["id"].clone();
    let response = web(
        &http,
        &host,
        &cookie,
        "/api/tasks/get",
        selection(&b),
        json!({"id":task_id}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let task: Value = response.json().await.unwrap();
    assert_eq!(
        task["task"]["specification"]["title"],
        "Browser readable Task"
    );
    assert!(
        broker.state.web_sts_calls.load(Ordering::SeqCst) >= 2,
        "Memory and Task use the Web incoming bearer for STS"
    );
    let issued = broker.state.calls.load(Ordering::SeqCst);
    broker.state.project_grant.store(false, Ordering::SeqCst);
    let response = web(
        &http,
        &host,
        &cookie,
        "/api/memory/get",
        selection(&b),
        json!({"id":id}),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "valid JWT does not grant PROJECT access"
    );
    assert_eq!(
        broker.state.calls.load(Ordering::SeqCst),
        issued,
        "denied PROJECT admission must precede storage access"
    );
    let response = web(&http, &host, &cookie, "/api/tasks/session", selection(&b),
        json!({"id":session_id,"history":{"page_size":1,"cursor":selected_session["history_start_cursor"]}})).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        broker.state.calls.load(Ordering::SeqCst),
        issued,
        "a retained history cursor grants no PROJECT or storage authority"
    );
    caller.cancel().await.unwrap();
    service.shutdown().await.unwrap();
    host.join().await;
    for client in [&a, &b] {
        client.assert_no_local_tables();
        client.server.shutdown().await.unwrap();
    }
    native.join().await;
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn remote_record_storage_requires_explicit_external_permission() {
    let broker = Broker::start(S3Fixture::load()).await;
    let client = Client::new_with_remote(&broker, None, false);
    assert!(
        client
            .memory("memory_create", memory_input("no-remote-permission"), 0)
            .await
            .is_err()
    );
    assert!(
        client
            .task(
                "session_create",
                json!({"idempotency_key":"no-remote-task","title":"denied",
        "description":"No external effect permission","strategy":"no network grant"}),
                true
            )
            .await
            .is_err()
    );
    assert_eq!(
        broker.state.calls.load(Ordering::SeqCst),
        0,
        "denial must happen before STS issuance"
    );
    client.assert_no_local_tables();
    client.server.shutdown().await.unwrap();
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn valid_broker_discovery_disabling_s3_uses_the_selected_local_contract() {
    let broker = Broker::start(S3Fixture::load()).await;
    broker.state.storage_enabled.store(false, Ordering::SeqCst);
    let client = Client::new(&broker, None);
    let memory = client
        .memory("memory_create", memory_input("disabled-s3-memory"), 0)
        .await
        .unwrap();
    let id = memory["native"]["id"].clone();
    assert_eq!(
        client
            .memory("memory_get", json!({"id":id}), 0)
            .await
            .unwrap()["native"]["memory"]["revision"],
        1
    );
    task_value(
        client
            .task(
                "session_create",
                json!({"idempotency_key":"disabled-s3-session","title":"local",
        "description":"Valid disabled remote discovery","strategy":"selected local backend"}),
                true,
            )
            .await,
    );
    let snapshot = client
        .server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live local selection"));
    for module in ["memory", "task"] {
        assert!(
            snapshot.projects[0]
                .0
                .project_runtime(snapshot.projects[0].1, &[module, "index.lance"])
                .unwrap()
                .is_dir()
        );
    }
    assert_eq!(
        broker.state.calls.load(Ordering::SeqCst),
        0,
        "disabled discovery must not issue STS credentials"
    );
    client.server.shutdown().await.unwrap();
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn broker_incoming_s3_reads_current_and_pinned_pages() {
    broker_incoming_selected_reads(true).await;
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn broker_incoming_disabled_storage_reads_current_and_pinned_pages() {
    broker_incoming_selected_reads(false).await;
}

async fn broker_incoming_selected_reads(storage_enabled: bool) {
    let broker = Broker::start(S3Fixture::load()).await;
    broker
        .state
        .storage_enabled
        .store(storage_enabled, Ordering::SeqCst);
    let a = Client::new(&broker, None);
    // S3 clients have distinct installations and the same immutable PROJECT.
    // Disabled storage instead needs two consumers of the same local corpus.
    let mut b = if storage_enabled {
        Client::new(&broker, Some(&a))
    } else {
        a.reopen(&broker)
    };
    let bearer = zeroize::Zeroizing::new(broker.token_for(
        "incoming-subject",
        Some(&format!("{}/v1", broker.state.origin)),
    ));
    broker.state.web.lock().unwrap().incoming_issued = bearer.to_string();
    let session = task_value(
        a.task(
            "session_create",
            json!({
                "idempotency_key":"incoming-session", "title":"Incoming Session",
                "description":"The admitted reader selects the writer's actual corpus",
                "strategy":"Read current and traverse explicit pinned history"
            }),
            true,
        )
        .await,
    );
    let session_id = session["result"]["id"].clone();
    let claim = task_value(
        a.task(
            "claim_session",
            json!({"id":session_id,"lease_ms":120000}),
            true,
        )
        .await,
    );
    let revised = task_value(
        a.task(
            "revise_session",
            json!({
                "target":{"id":session_id,"claim":{"token":claim["result"]["token"],
                    "expected_revision":claim["result"]["record"]["revision"]}},
                "lease_ms":120000,"reason":"Before Incoming capture","title":"Pinned Session"
            }),
            true,
        )
        .await,
    );
    assert_eq!(revised["result"]["revision"], 3);
    let task = task_value(
        a.task(
            "create",
            json!({
                "idempotency_key":"incoming-task","session_id":session_id,
                "title":"Incoming Task","description":"Same table and selected generation",
                "task_type":"TASK","priority":1,
                "checks":[{"kind":"acceptance","text":"Read the actual selected corpus"},
                    {"kind":"test","text":"Traverse explicit history to EOF"}]
            }),
            true,
        )
        .await,
    );
    let task_id = task["result"]["specification"]["id"].clone();
    let task_claim = task_value(
        a.task("claim", json!({"id":task_id,"lease_ms":120000}), true)
            .await,
    );
    task_value(a.task("revise", json!({
        "target":{"id":task_id,"claim":{"token":task_claim["result"]["token"],
            "expected_revision":task_claim["result"]["record"]["specification"]["revision"]}},
        "lease_ms":120000,"reason":"Before Incoming capture","title":"Pinned Task"
    }), true).await);
    let current_session = incoming_read(
        &b,
        &broker,
        &bearer,
        "get_session",
        json!({"id":session_id}),
        0,
    )
    .await
    .unwrap();
    let current_task = incoming_read(&b, &broker, &bearer, "get_task", json!({"id":task_id}), 0)
        .await
        .unwrap();
    for current in [&current_session, &current_task] {
        assert!(current["result"].get("events").is_none());
        assert!(current["result"]["history"].is_null());
        assert!(current["result"]["history_start_cursor"].is_string());
    }
    assert!(current_task["result"]["comments"].is_null());
    assert_eq!(current_session["result"]["session"], revised["result"]);
    assert_eq!(
        current_task["result"]["task"]["specification"]["title"],
        "Pinned Task"
    );
    assert_eq!(
        current_session["result"]["generation"],
        current_task["result"]["generation"]
    );

    // Publish after the Incoming reader has captured its two start cursors.
    task_value(a.task("revise_session", json!({
        "target":{"id":session_id,"claim":{"token":claim["result"]["token"],"expected_revision":3}},
        "lease_ms":120000,"reason":"After Incoming capture","title":"Latest Session"
    }), true).await);
    task_value(a.task("add_task_comment", json!({
        "target":{"id":task_id,"claim":{"token":task_claim["result"]["token"],"expected_revision":3}},
        "lease_ms":120000,"idempotency_key":"incoming-comment","kind":"decision",
        "body":"Only the latest generation includes this comment"
    }), true).await);
    for (operation, id, captured, field) in [
        ("get_session", &session_id, &current_session, "session"),
        ("get_task", &task_id, &current_task, "task"),
    ] {
        let mut cursor = captured["result"]["history_start_cursor"].clone();
        let mut previous = captured["result"]["creation"]["record"].clone();
        for revision in 2..=3 {
            let page = incoming_read(
                &b,
                &broker,
                &bearer,
                operation,
                json!({"id":id,"history":{"page_size":1,"cursor":cursor}}),
                0,
            )
            .await
            .unwrap();
            assert_eq!(
                page["result"]["generation"],
                captured["result"]["generation"]
            );
            assert_eq!(page["result"][field], captured["result"][field]);
            let history = &page["result"]["history"];
            assert_eq!(history["events"].as_array().unwrap().len(), 1);
            let before = &history["before"];
            let event = &history["events"][0]["record"];
            assert_eq!(before, &previous);
            if field == "task" {
                assert_eq!(before["specification"]["revision"], revision - 1);
                assert_eq!(event["specification"]["revision"], revision);
                assert!(page["result"]["comments"].is_null());
            } else {
                assert_eq!(before["revision"], revision - 1);
                assert_eq!(event["revision"], revision);
            }
            previous = event.clone();
            cursor = history["next_cursor"].clone();
            assert_eq!(cursor.is_null(), revision == 3);
        }
        assert_eq!(previous, captured["result"][field]);
    }
    let pinned_comments = incoming_read(
        &b,
        &broker,
        &bearer,
        "get_task",
        json!({"id":task_id,
        "comments":{"page_size":1,"cursor":current_task["result"]["comments_start_cursor"]}}),
        0,
    )
    .await
    .unwrap();
    assert_eq!(
        pinned_comments["result"]["generation"],
        current_task["result"]["generation"]
    );
    assert!(
        pinned_comments["result"]["comments"]["comments"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(pinned_comments["result"]["comments"]["next_cursor"].is_null());
    let latest = incoming_read(
        &b,
        &broker,
        &bearer,
        "get_task",
        json!({"id":task_id,
        "history":{"page_size":4},"comments":{"page_size":1}}),
        0,
    )
    .await
    .unwrap();
    assert_ne!(
        latest["result"]["generation"],
        current_task["result"]["generation"]
    );
    assert_eq!(latest["result"]["task"]["specification"]["revision"], 4);
    assert_eq!(
        latest["result"]["history"]["events"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(latest["result"]["history"]["next_cursor"].is_null());
    assert_eq!(
        latest["result"]["comments"]["comments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(latest["result"]["comments"]["next_cursor"].is_null());
    let latest_session = incoming_read(
        &b,
        &broker,
        &bearer,
        "get_session",
        json!({"id":session_id}),
        0,
    )
    .await
    .unwrap();
    assert_eq!(
        latest_session["result"]["session"]["input"]["title"],
        "Latest Session"
    );
    assert_eq!(latest_session["result"]["session"]["revision"], 4);
    assert!(latest_session["result"]["history"].is_null());

    // Neither another PROJECT nor another selected version can reuse a cursor.
    assert!(matches!(
        incoming_read(
            &b,
            &broker,
            &bearer,
            "get_session",
            json!({"id":session_id,
        "history":{"page_size":1,"cursor":current_session["result"]["history_start_cursor"]}}),
            1
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
    b.branch("feature/incoming-pages");
    assert!(matches!(
        incoming_read(
            &b,
            &broker,
            &bearer,
            "get_session",
            json!({"id":session_id,
        "history":{"page_size":1,"cursor":current_session["result"]["history_start_cursor"]}}),
            2
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
    assert!(matches!(
        incoming_read(
            &b,
            &broker,
            &bearer,
            "get_task",
            json!({"id":task_id,
        "history":{"page_size":1,"cursor":current_task["result"]["comments_start_cursor"]}}),
            0
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let issued = broker.state.calls.load(Ordering::SeqCst);
    broker.state.project_grant.store(false, Ordering::SeqCst);
    let denied = incoming_read(
        &b,
        &broker,
        &bearer,
        "get_session",
        json!({"id":session_id,
        "history":{"page_size":1,"cursor":current_session["result"]["history_start_cursor"]}}),
        0,
    )
    .await
    .unwrap_err();
    let TaskStoreError::Request(error) = denied else {
        panic!("PROJECT denial must precede storage")
    };
    assert!(matches!(
        error.get_ref().and_then(
            |e| e.downcast_ref::<graphit_auth::broker::project_access::BrokerProjectError>()
        ),
        Some(graphit_auth::broker::project_access::BrokerProjectError::Denied)
    ));
    assert_eq!(broker.state.calls.load(Ordering::SeqCst), issued);
    assert!(broker.state.incoming_project_calls.load(Ordering::SeqCst) > 0);
    if storage_enabled {
        assert!(broker.state.incoming_sts_calls.load(Ordering::SeqCst) > 0);
        a.assert_no_local_tables();
        b.assert_no_local_tables();
    } else {
        assert_eq!(issued, 0, "valid disabled Discovery never calls STS");
        let snapshot = b
            .server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live disabled-storage test selection"));
        assert!(
            snapshot.projects[0]
                .0
                .project_runtime(snapshot.projects[0].1, &["task", "index.lance"])
                .unwrap()
                .is_dir()
        );
    }
    b.server.shutdown().await.unwrap();
    a.server.shutdown().await.unwrap();
}
