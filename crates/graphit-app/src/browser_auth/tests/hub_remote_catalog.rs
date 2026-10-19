//! Reader qualification: genuine Web OAuth, HubRead and S3; no fabricated leases.
//! Codec-authored catalog documents below qualify readers, not publication.
use super::*;
use graphit_core::project_identity::ProjectId;
use graphit_hub::{
    ArtifactId, ArtifactType,
    policy::RequestContext,
    project_metadata::{HubProjectMetadata, project_metadata_key},
    remote_metadata::{ManifestInput, RemoteArtifactManifest, VersionInput},
    remote_storage::registry_key,
};
use object_store::ObjectStoreExt as _;
use std::sync::atomic::{AtomicBool, Ordering};

const PROJECTS: &str = "/api/hub/remote/projects";
const SEARCH: &str = "/api/hub/remote/search";
const STORAGE: &str = "catalog-storage";
const KEYS: &str = "catalog-keys";

// Relay genuine, signed MinIO reads. A single metadata response can be retained
// after real backend I/O, so lifecycle assertions cannot pass on an empty job.
pub(super) struct StorageRelay {
    pub(super) endpoint: String,
    state: Arc<RelayState>,
    stop: CancellationToken,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}
struct RelayState {
    backend: String,
    client: reqwest::Client,
    hold: AtomicBool,
    hold_file: AtomicBool,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
impl StorageRelay {
    pub(super) async fn start(backend: &str) -> Self {
        let listener = checked(
            TcpListener::bind("127.0.0.1:0").await,
            "storage relay listener",
        );
        let endpoint = format!("http://{}", checked(listener.local_addr(), "relay address"));
        let state = Arc::new(RelayState {
            backend: backend.trim_end_matches('/').into(),
            client: checked(
                reqwest::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .build(),
                "owned relay client",
            ),
            hold: AtomicBool::new(false),
            hold_file: AtomicBool::new(false),
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        let router = Router::new()
            .fallback(relay_storage)
            .with_state(Arc::clone(&state));
        let stop = CancellationToken::new();
        let stopped = stop.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stopped.cancelled_owned())
                .await
        });
        Self {
            endpoint,
            state,
            stop,
            server,
        }
    }
    pub(super) fn hold_next_file(&self) {
        self.state.hold_file.store(true, Ordering::SeqCst);
    }
    pub(super) fn release(&self) {
        self.state.release.notify_waiters();
    }
    fn hold_next_metadata(&self) {
        self.state.hold.store(true, Ordering::SeqCst);
    }
    pub(super) async fn entered(&self) {
        checked(
            tokio::time::timeout(Duration::from_secs(15), self.state.entered.notified()).await,
            "real metadata request reached relay",
        );
    }
    pub(super) async fn close(mut self) {
        self.state.release.notify_waiters();
        self.stop.cancel();
        match tokio::time::timeout(Duration::from_secs(30), &mut self.server).await {
            Ok(result) => checked(checked(result, "relay join"), "relay serve"),
            Err(_) => {
                self.server.abort();
                let _ = self.server.await;
                panic!("relay shutdown deadline after abort and join");
            }
        }
    }
}
async fn relay_storage(
    State(state): State<Arc<RelayState>>,
    request: axum::extract::Request,
) -> Response {
    let hold = request.uri().path().ends_with("/hub/metadata/project.json")
        && state.hold.swap(false, Ordering::SeqCst)
        || request.method() == axum::http::Method::GET
            && request.uri().path().ends_with("/payload.zip")
            && state.hold_file.swap(false, Ordering::SeqCst);
    // Register before backend I/O so an early close also releases this hold.
    let released = hold.then(|| state.release.notified());
    let uri = request
        .uri()
        .path_and_query()
        .expect("S3 path and query")
        .as_str();
    let response = checked(
        state
            .client
            .request(request.method().clone(), format!("{}{uri}", state.backend))
            .headers(request.headers().clone())
            .send()
            .await,
        "genuine signed backend response",
    );
    let status = response.status();
    let headers = response.headers().clone();
    let body = checked(response.bytes().await, "genuine storage body");
    if let Some(released) = released {
        state.entered.notify_one();
        released.await;
    }
    let mut result = Response::new(axum::body::Body::from(body));
    *result.status_mut() = status;
    *result.headers_mut() = headers;
    result
}

pub(super) async fn join_catalog_read_job(host: &WebHost) {
    let guard = Instant::now() + Duration::from_secs(15);
    assert!(
        !host.service.0.stop.is_cancelled(),
        "session owner remains live"
    );
    assert!(
        host.service
            .0
            .sessions
            .lock()
            .await
            .values()
            .all(|session| !session.cancelled.is_cancelled() && session.expires > guard),
        "session timers cannot complete during the read join guard"
    );
    checked(
        tokio::time::timeout_at(guard.into(), async {
            let mut jobs = host.service.0.jobs.lock().await;
            let result = jobs
                .join_next()
                .await
                .expect("owned catalog read job retained");
            checked(result, "owned catalog read job joined");
        })
        .await,
        "catalog jobs join guard",
    );
}
#[track_caller]
pub(super) fn assert_catalog_job_pending(
    host: &WebHost,
) -> impl std::future::Future<Output = ()> + '_ {
    let caller = std::panic::Location::caller();
    async move {
        let mut jobs = host.service.0.jobs.lock().await;
        match tokio::time::timeout(Duration::from_millis(100), jobs.join_next()).await {
            Err(_) => {}
            Ok(Some(Ok(()))) => {
                panic!("{caller}: a completed job was observed before releasing held I/O")
            }
            Ok(Some(Err(_))) => panic!("{caller}: an owned job failed before releasing held I/O"),
            Ok(None) => panic!("{caller}: no owned job remains during held I/O"),
        }
    }
}

fn spawn_catalog_read(
    host: &WebHost,
    cookie: &str,
    body: Value,
) -> tokio::task::JoinHandle<reqwest::Response> {
    let client = checked(
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(45))
            .build(),
        "lifecycle reader client",
    );
    let request = client
        .post(format!("{}{PROJECTS}", host.origin))
        .header(header::ORIGIN, &host.origin)
        .header(header::COOKIE, cookie)
        .header("X-Graphit-Request", "ui")
        .json(&body);
    tokio::spawn(async move { checked(request.send().await, "real in-flight catalog response") })
}

fn object_path(prefix: &str, key: &str) -> object_store::path::Path {
    object_store::path::Path::from(format!("{}/{key}", prefix.trim_end_matches('/')))
}
pub(super) async fn put_json(
    store: &dyn object_store::ObjectStore,
    prefix: &str,
    key: &str,
    value: Value,
) {
    checked(
        store
            .put(
                &object_path(prefix, key),
                checked(serde_json::to_vec(&value), "fixture JSON").into(),
            )
            .await,
        "owned reader fixture object",
    );
}
pub(super) async fn seed_project_metadata(
    source: &crate::source_server::SourceServer,
    store: &dyn object_store::ObjectStore,
    prefix: &str,
    producer: ProjectId,
) {
    let policy = &source.policy().native_install.hub;
    let project = HubProjectMetadata {
        id: producer,
        name: "Web catalog producer".into(),
    };
    checked(
        store
            .put(
                &object_path(prefix, &project_metadata_key(producer)),
                checked(
                    project.encode(policy.content().max_input_bytes),
                    "canonical project metadata",
                )
                .into(),
            )
            .await,
        "real metadata object",
    );
}

async fn seed_catalog(
    source: &crate::source_server::SourceServer,
    store: &dyn object_store::ObjectStore,
    prefix: &str,
    producer: ProjectId,
) {
    seed_project_metadata(source, store, prefix, producer).await;
    let policy = &source.policy().native_install.hub;
    let cancellation = Cancellation::default();
    let original = checked(
        policy.request_context(Instant::now(), &cancellation),
        "fixture Original",
    );
    for (id, name) in [
        ("fixture/catalog-a", "Catalog A"),
        ("fixture/catalog-b", "Catalog B"),
    ] {
        let id_value = checked(
            ArtifactId::new(id, policy.projection()),
            "canonical artifact ID",
        );
        let versions = [
            VersionInput {
                version: "release/one",
                hash: &"01".repeat(32),
            },
            VersionInput {
                version: "release/two",
                hash: &"02".repeat(32),
            },
        ];
        let input = ManifestInput {
            producer,
            artifact_type: ArtifactType::Skill,
            id,
            name,
            description: Some("HTTP reader fixture"),
            tags: &["catalog"],
            author: None,
            latest: Some("release/two"),
            versions: &versions,
            dependencies: &[],
        };
        let limits =
            crate::native_publication::metadata_limits(policy.content(), policy.projection());
        let bytes = RemoteArtifactManifest::encode(&input, limits, &original)
            .unwrap_or_else(|cause| panic!("strict catalog manifest: {cause:?}"));
        checked(
            store
                .put(
                    &object_path(
                        prefix,
                        &checked(
                            registry_key(producer, ArtifactType::Skill, &id_value),
                            "registry key",
                        ),
                    ),
                    bytes.into(),
                )
                .await,
            "real manifest object",
        );
    }
}
pub(super) async fn request(
    host: &WebHost,
    cookie: Option<&str>,
    path: &str,
    body: &str,
) -> reqwest::Response {
    let mut request = host
        .client
        .post(format!("{}{path}", host.origin))
        .header(header::ORIGIN, &host.origin)
        .header("X-Graphit-Request", "ui")
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_owned());
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    checked(request.send().await, "real Web catalog response")
}
pub(super) async fn page(host: &WebHost, cookie: &str, path: &str, input: Value) -> Value {
    let response = request(host, Some(cookie), path, &input.to_string()).await;
    let status = response.status();
    assert!(
        response.headers()[header::CACHE_CONTROL]
            .to_str()
            .expect("cache policy")
            .contains("no-store")
    );
    let body = checked(response.bytes().await, "bounded catalog body");
    let decoded = serde_json::from_slice::<Value>(&body);
    let code = decoded
        .as_ref()
        .ok()
        .and_then(|value| value["error"]["code"].as_str())
        .filter(|code| {
            [
                "invalid_request",
                "permission_denied",
                "file_not_found",
                "state_conflict",
                "limit",
                "cancelled",
                "deadline",
                "unavailable",
                "file_read_unavailable",
                "hub_catalog_unavailable",
                "catalog_not_found",
                "catalog_restart_required",
                "invalid_arguments",
                "authentication_or_permission_denied",
                "artifact_not_found",
                "installation_conflict",
                "remote_file_too_large",
                "unsupported",
                "invalid_remote_file",
                "file_installation_failed",
                "file_job_failed",
                "file_mutation_unavailable",
            ]
            .contains(code)
        })
        .unwrap_or("unclassified_public_error");
    let cache_phase = decoded
        .as_ref()
        .ok()
        .and_then(|value| value["outcome"]["cache"].as_str())
        .filter(|phase| {
            matches!(
                *phase,
                "not_attempted" | "in_progress" | "Created" | "Updated" | "Unchanged" | "unknown"
            )
        })
        .unwrap_or("unclassified");
    let committed = decoded
        .as_ref()
        .ok()
        .and_then(|value| value["outcome"]["publication"]["committed"].as_bool());
    let hub_pending = decoded
        .as_ref()
        .ok()
        .and_then(|value| value["outcome"]["publication"]["hub_pending"].as_bool());
    assert_eq!(
        status,
        StatusCode::OK,
        "productive Web catalog status; public_code={code} cache_phase={cache_phase} committed={committed:?} hub_pending={hub_pending:?} path={path}"
    );
    let value: Value = checked(decoded, "public catalog DTO");
    assert!(value["status"] == "ok", "successful catalog DTO");
    for private in [
        "access_token",
        "refresh_token",
        "secret_access_key",
        "session_token",
        "authorization_revision",
        "subject",
        "endpoint",
        "hash",
    ] {
        assert!(
            !String::from_utf8_lossy(&body).contains(&format!("\"{private}\"")),
            "no private transport fields"
        );
    }
    value
}
pub(super) async fn rejected(
    host: &WebHost,
    cookie: Option<&str>,
    path: &str,
    body: &str,
    status: StatusCode,
    code: &str,
) {
    let response = request(host, cookie, path, body).await;
    assert_eq!(response.status(), status, "finite Web catalog rejection");
    let value: Value = checked(response.json().await, "public rejection DTO");
    if code == "unauthorized" {
        assert!(
            value["error"] == "session unavailable",
            "existing session failure contract"
        );
    } else {
        assert!(value["error"]["code"] == code, "finite public cause");
    }
    assert!(
        value.as_object().is_some_and(|v| v.len() == 1),
        "no protected response on denial"
    );
}

pub(super) fn standalone_store(ready: &Value) -> Arc<dyn object_store::ObjectStore> {
    standalone_storage_store(&ready["standalone_storage"])
}

pub(super) fn standalone_publisher_store(ready: &Value) -> Arc<dyn object_store::ObjectStore> {
    standalone_storage_store(&ready["standalone_publisher_storage"])
}

fn standalone_storage_store(keys: &Value) -> Arc<dyn object_store::ObjectStore> {
    use object_store::aws::{AmazonS3Builder, S3ConditionalPut, S3CopyIfNotExists};
    assert!(
        keys.is_object(),
        "required separate LocalS3 fixture capability"
    );
    let mut builder = AmazonS3Builder::new()
        .with_bucket_name(wire(keys, "bucket"))
        .with_region(wire(keys, "region"))
        .with_endpoint(wire(keys, "endpoint"))
        .with_virtual_hosted_style_request(false)
        .with_access_key_id(wire(keys, "access_key_id"))
        .with_secret_access_key(wire(keys, "secret_access_key"))
        .with_conditional_put(S3ConditionalPut::ETagMatch)
        .with_copy_if_not_exists(S3CopyIfNotExists::Multipart)
        .with_client_options(
            object_store::ClientOptions::new()
                .with_allow_http(true)
                .with_timeout(Duration::from_secs(30)),
        );
    if let Some(token) = keys["session_token"].as_str() {
        builder = builder.with_token(token);
    }
    Arc::new(checked(builder.build(), "actual separate LocalS3 SDK"))
}

// Productive profile manager publications only: no direct AuthStore editing.
pub(super) async fn configure_standalone(
    fixture: &crate::source_server::test_fixture::Fixture,
    ready: &Value,
    endpoint: &str,
) {
    configure_standalone_named(fixture, ready, endpoint, STORAGE, KEYS).await;
}

pub(super) async fn configure_standalone_named(
    fixture: &crate::source_server::test_fixture::Fixture,
    ready: &Value,
    endpoint: &str,
    provider: &str,
    profile_name: &str,
) {
    use graphit_auth::{
        profile::{
            AiService, LocalCredentials, LocalS3, ProfileExpectation, ProviderConfig, ProviderKind,
            S3Location, S3Source, Secret,
        },
        profile_manager::{LocalProfileInput, NativeProfileManager, ProfileManagerAuthority},
    };
    let keys = &ready["standalone_storage"];
    assert!(
        keys.is_object(),
        "required genuine standalone fixture capability"
    );
    let cancellation = Cancellation::default();
    let original = checked(
        RequestContext::new(
            fixture.server.policy().content,
            &cancellation,
            Instant::now() + Duration::from_secs(30),
        ),
        "manager Original",
    );
    let issuer = fixture.auth.issuer();
    let caller = checked(
        issuer.authenticate_runtime(&fixture.raw, &original),
        "manager caller",
    );
    let session = checked(issuer.bootstrap_admin(&caller), "manager admin");
    let parent = checked(issuer.admin_parent(&caller, &session), "manager authority");
    let manager = NativeProfileManager::new(issuer);
    let snapshot = checked(
        manager.begin(ProfileManagerAuthority::Admin(&parent)),
        "provider snapshot",
    );
    let revision = snapshot.revision();
    let config = ProviderConfig {
        kind: ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: None,
        },
        embedding: AiService::Disabled,
        rerank: AiService::Disabled,
        s3: S3Location {
            bucket: Some(wire(keys, "bucket").into()),
            region: Some(wire(keys, "region").into()),
            endpoint: Some(endpoint.into()),
            prefix: Some(wire(keys, "prefix").into()),
            source: S3Source::LocalCredentials,
        },
    };
    let publication = checked(
        manager.add_provider(snapshot, provider.into(), config, revision, 1),
        "add actual Local provider",
    );
    assert!(
        publication.failure().is_none() && publication.cleanup_failure().is_none(),
        "provider publication"
    );
    assert!(
        publication.into_pending_cleanup().is_none(),
        "provider cleanup"
    );
    let snapshot = checked(
        manager.begin(ProfileManagerAuthority::Admin(&parent)),
        "profile snapshot",
    );
    let expected = ProfileExpectation {
        document_revision: snapshot.revision(),
        provider_revision: snapshot
            .providers()
            .find(|v| v.name() == provider)
            .expect("current storage provider")
            .revision(),
        profile_revision: None,
    };
    let input = LocalProfileInput {
        name: profile_name.into(),
        provider: provider.into(),
        username: "untrusted-storage-username".into(),
        organization: None,
        teams: vec!["untrusted-storage-team".into()],
        credentials: LocalCredentials {
            anonymous: false,
            mcp_key: None,
            broker_key: None,
            embedding_api_key: None,
            rerank_api_key: None,
            s3: Some(LocalS3 {
                access_key_id: Some(Secret::new(wire(keys, "access_key_id").to_owned().into())),
                secret_access_key: Some(Secret::new(
                    wire(keys, "secret_access_key").to_owned().into(),
                )),
                session_token: keys["session_token"]
                    .as_str()
                    .map(|token| Secret::new(token.to_owned().into())),
                expires_at: keys["expires_at"].as_i64(),
                aws_profile: None,
            }),
        },
    };
    let publication = checked(
        manager.install_local(snapshot, input, expected, 1),
        "install actual transport profile",
    );
    assert!(
        publication.failure().is_none() && publication.cleanup_failure().is_none(),
        "profile publication"
    );
    assert!(
        publication.into_pending_cleanup().is_none(),
        "profile cleanup"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web/STSv3/MinIO fixture and coordinated App emission"]
async fn real_web_hub_catalog_broker_and_standalone_keep_subject_paging_and_denials() {
    use futures_util::FutureExt;
    use graphit_auth::{ServiceOrigin, ServiceRuleInput};
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    let producer = checked(
        "01D39ZY06FGSCTVN4T2V9PKHFZ".parse::<ProjectId>(),
        "remote producer",
    );
    let second = checked(
        "01D39ZY06FGSCTVN4T2V9PKHG0".parse::<ProjectId>(),
        "second real granted producer",
    );
    let broker = BrokerProcess::start_web_projects(
        std::path::Path::new(&binary),
        &producer.to_string(),
        Some(&second.to_string()),
        false,
        true,
    )
    .await;
    let mut fixture_owner = None;
    let mut host = None;
    let mut relay_owner = None;
    let mut readers = Vec::new();
    let observed = std::panic::AssertUnwindSafe(async {
        let extra = [ServiceRuleInput::StandaloneS3 { purpose: graphit_auth::ServicePurpose::HubRead, origin: ServiceOrigin::Web, provider: STORAGE, profile: Some(KEYS) }];
        fixture_owner = Some(fixture_with_web_admissions(&broker.ready, (None, Some("broker")), &extra).await);
        let fixture = fixture_owner.as_ref().expect("owned admitted Source");
        assert!(fixture.projects.iter().all(|(_,id,_)|*id!=producer), "producer has no consumer PROJECT authority");
        relay_owner=Some(StorageRelay::start(wire(&broker.ready,"storage_endpoint")).await);
        let relay=relay_owner.as_ref().expect("owned real storage relay");
        configure_standalone(fixture, &broker.ready, &relay.endpoint).await;
        let store = seed_store(&broker.ready, producer).await;
        let prefix = wire(&broker.ready, "prefix");
        seed_catalog(&fixture.server, store.as_ref(), prefix, producer).await;
        let second_store=seed_store(&broker.ready,second).await;
        seed_catalog(&fixture.server,second_store.as_ref(),prefix,second).await;
        let provider_revision = crate::graph_server::tests::browser_provider(&fixture.server.graph_server(), &fixture.raw, wire(&broker.ready,"endpoint"));
        let (_task_directory, tasks, _) = crate::task_server::tests::fixture_with_reference_permissions(&[], &[], None, false);
        host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, provider_revision).await);
        let host = host.as_ref().expect("owned Web host");
        let cookie = host.login(&broker.ready).await;
        let sibling = host.login(&broker.ready).await;
        let identity_response=checked(host.client.get(format!("{}/auth/session",host.origin)).header(header::COOKIE,cookie.as_str()).send().await,"actual Web session identity");
        assert_eq!(identity_response.status(),StatusCode::OK);
        let web_identity:Value=checked(identity_response.json().await,"actual verified Web subject");
        let mut second_ready=broker.ready.clone();
        assert!(second_ready["web_second"].is_object(), "required second genuine Web fixture identity");
        second_ready["web"]=broker.ready["web_second"].clone();
        let second_cookie=host.login(&second_ready).await;
        let second_identity_response=checked(host.client.get(format!("{}/auth/session",host.origin)).header(header::COOKIE,second_cookie.as_str()).send().await,"second actual Web session identity");
        assert_eq!(second_identity_response.status(),StatusCode::OK);
        let second_identity:Value=checked(second_identity_response.json().await,"second verified Web subject");
        assert!(wire(&second_identity,"subject")!=wire(&web_identity,"subject") && wire(&second_identity,"subject")==wire(&broker.ready["web_second"],"subject"), "two independently authenticated subjects");
        let projects = json!({"transport":"broker","provider":"broker","limit":1,"id":producer});
        rejected(host,None,PROJECTS,&projects.to_string(),StatusCode::UNAUTHORIZED,"unauthorized").await;
        let project_page = page(host,&cookie,PROJECTS,projects.clone()).await;
        assert!(project_page["projects"].as_array().is_some_and(|v|v.len()==1), "actual discoverable producer");
        assert!(project_page["projects"][0]["id"]==producer.to_string() && project_page["projects"][0]["name"]=="Web catalog producer", "canonical project metadata");
        assert!(project_page["next_cursor"].is_null(), "filtered project EOF");
        let mut named=projects.clone();named.as_object_mut().expect("projects input").remove("id");named["name_prefix"]=json!("Web catalog");
        assert!(page(host,&cookie,PROJECTS,named.clone()).await["projects"][0]["id"]==producer.to_string(),"literal project name filter");
        named["name_prefix"]=json!("absent literal prefix");
        assert!(page(host,&cookie,PROJECTS,named).await["projects"].as_array().is_some_and(Vec::is_empty),"honest project no matches");
        for body in [
            r#"{"transport":"broker","provider":"broker","limit":1,"unknown":true}"#,
            r#"{"transport":"broker","provider":"broker","limit":1,"limit":2}"#,
            r#"{"transport":"broker","provider":"broker","limit":1,"cursor":null}"#,
            r#"{"transport":"broker","provider":"broker","limit":1,"identity_provider":"broker"}"#,
            r#"{"transport":"broker","provider":"broker","limit":0}"#,
        ] { rejected(host,Some(&cookie),PROJECTS,body,StatusCode::BAD_REQUEST,"invalid_request").await; }
        let oversized=request(host,Some(&cookie),PROJECTS,&" ".repeat(65537)).await;
        assert_eq!(oversized.status(),StatusCode::PAYLOAD_TOO_LARGE,"existing Task/Hub body ceiling");
        let foreign = json!({"transport":"broker","provider":"missing","limit":1});
        rejected(host,Some(&cookie),PROJECTS,&foreign.to_string(),StatusCode::FORBIDDEN,"permission_denied").await;
        let response = checked(host.client.post(format!("{}{PROJECTS}",host.origin)).header(header::ORIGIN,"https://foreign.invalid")
            .header("X-Graphit-Request","ui").header(header::COOKIE,cookie.as_str()).json(&projects).send().await,"cross-origin request");
        assert_eq!(response.status(),StatusCode::FORBIDDEN,"origin rejects before protected reads");
        let search = json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","limit":1});
        assert!(page(host,&second_cookie,PROJECTS,projects.clone()).await["projects"].as_array().is_some_and(Vec::is_empty), "second subject cannot discover first producer");
        let mut second_projects=projects.clone();second_projects["id"]=json!(second);
        assert!(page(host,&second_cookie,PROJECTS,second_projects.clone()).await["projects"][0]["id"]==second.to_string(), "second subject discovers its real grant");
        rejected(host,Some(&second_cookie),SEARCH,&search.to_string(),StatusCode::FORBIDDEN,"permission_denied").await;
        let mut second_search=search.clone();second_search["producer"]=json!(second);
        assert!(page(host,&second_cookie,SEARCH,second_search.clone()).await["artifacts"].as_array().is_some_and(|v|v.len()==1), "second subject actual STSv3 read for second producer");
        rejected(host,Some(&cookie),SEARCH,r#"{"transport":"broker","provider":"broker","limit":1}"#,StatusCode::BAD_REQUEST,"invalid_request").await;
        let mut exact_version=search.clone();exact_version["id"]=json!("fixture/catalog-a");exact_version["version"]=json!("release/one");
        let filtered=page(host,&cookie,SEARCH,exact_version).await;
        assert!(filtered["artifacts"][0]["versions"]==json!(["release/one","release/two"]) && filtered["next_cursor"].is_null(),"version is a literal manifest filter");
        let first = page(host,&cookie,SEARCH,search.clone()).await;
        assert!(first["artifacts"].as_array().is_some_and(|v|v.len()==1), "bounded artifact page");
        assert!(first["artifacts"][0]["versions"]==json!(["release/one","release/two"]) && first["artifacts"][0]["latest"]=="release/two", "all versions and declared latest");
        let cursor = first["next_cursor"].as_str().expect("real artifact cursor");
        let mut next = search.clone(); next["cursor"]=json!(cursor);
        let last = page(host,&cookie,SEARCH,next.clone()).await;
        assert!(last["artifacts"].as_array().is_some_and(|v|v.len()==1) && last["next_cursor"].is_null(), "second page EOF");
        assert!(first["artifacts"][0]["id"] != last["artifacts"][0]["id"], "advancing page position");
        next["limit"]=json!(2);
        rejected(host,Some(&cookie),SEARCH,&next.to_string(),StatusCode::CONFLICT,"catalog_restart_required").await;
        let mut empty=search.clone();empty["id"]=json!("fixture/absent");
        assert!(page(host,&cookie,SEARCH,empty).await["artifacts"].as_array().is_some_and(Vec::is_empty), "honest no matches");

        // Standalone has a separate issued namespace and LocalS3 capability.
        let broker_store = Arc::clone(&store);
        let broker_prefix = prefix.to_owned();
        let standalone_store = standalone_store(&broker.ready);
        let store = standalone_store;
        let prefix = wire(&broker.ready["standalone_storage"], "prefix");
        seed_catalog(&fixture.server, store.as_ref(), prefix, producer).await;
        seed_catalog(&fixture.server, store.as_ref(), prefix, second).await;
        // The Local profile's username/team grant nothing. Only the actual Web
        // subject's ACL can reveal this producer through Standalone.
        for key in ["v1/global/projects.json","v1/anonymous/projects.json","v1/authenticated/projects.json"] {
            put_json(store.as_ref(),prefix,key,json!({"v":1,"projects":[]})).await;
        }
        let standalone=json!({"transport":"standalone","provider":STORAGE,"profile":KEYS,"limit":1});
        assert!(page(host,&cookie,PROJECTS,standalone.clone()).await["projects"].as_array().is_some_and(Vec::is_empty), "transport profile is not an ACL subject");
        let user_key=format!("v1/users/{}/{}/projects.json",URL_SAFE_NO_PAD.encode(wire(&broker.ready,"issuer")),URL_SAFE_NO_PAD.encode(wire(&web_identity,"subject")));
        put_json(store.as_ref(),prefix,&user_key,json!({"v":1,"projects":[{"id":producer}]})).await;
        assert!(page(host,&cookie,PROJECTS,standalone.clone()).await["projects"][0]["id"]==producer.to_string(), "actual Web subject retained across target providers");

        assert!(page(host,&second_cookie,PROJECTS,standalone.clone()).await["projects"].as_array().is_some_and(Vec::is_empty), "second subject does not inherit first user ACL or transport claims");
        let second_user_key=format!("v1/users/{}/{}/projects.json",URL_SAFE_NO_PAD.encode(wire(&broker.ready,"issuer")),URL_SAFE_NO_PAD.encode(wire(&second_identity,"subject")));
        put_json(store.as_ref(),prefix,&second_user_key,json!({"v":1,"projects":[{"id":second}]})).await;
        assert!(page(host,&second_cookie,PROJECTS,standalone.clone()).await["projects"][0]["id"]==second.to_string(), "Standalone captures the second actual Web subject");
        let metadata=HubProjectMetadata {id:second,name:"Web second producer".into()};
        checked(store.put(&object_path(prefix,&project_metadata_key(second)),checked(metadata.encode(fixture.server.policy().content.max_input_bytes),"second canonical metadata").into()).await,"second reader metadata object");
        put_json(store.as_ref(),prefix,&user_key,json!({"v":1,"projects":[{"id":producer},{"id":second}]})).await;
        let project_first=page(host,&cookie,PROJECTS,standalone.clone()).await;
        let mut project_next=standalone.clone();project_next["cursor"]=project_first["next_cursor"].clone();
        assert!(project_next["cursor"].is_string(), "real projects continuation");
        let project_last=page(host,&cookie,PROJECTS,project_next.clone()).await;
        assert!(project_first["projects"][0]["id"]!=project_last["projects"][0]["id"] && project_last["next_cursor"].is_null(), "projects advance to actual EOF");
        rejected(host,Some(&second_cookie),PROJECTS,&project_next.to_string(),StatusCode::CONFLICT,"catalog_restart_required").await;
        project_next["limit"]=json!(2);
        rejected(host,Some(&cookie),PROJECTS,&project_next.to_string(),StatusCode::CONFLICT,"catalog_restart_required").await;
        let mut standalone_search=search.clone();standalone_search["transport"]=json!("standalone");standalone_search["provider"]=json!(STORAGE);standalone_search["profile"]=json!(KEYS);
        rejected(host,Some(&second_cookie),SEARCH,&standalone_search.to_string(),StatusCode::FORBIDDEN,"permission_denied").await;
        let mut second_standalone_search=standalone_search.clone();second_standalone_search["producer"]=json!(second);
        assert!(page(host,&second_cookie,SEARCH,second_standalone_search).await["artifacts"].as_array().is_some_and(|v|v.len()==1), "second subject independent Standalone artifact grant");
        let standalone_first=page(host,&cookie,SEARCH,standalone_search.clone()).await;
        standalone_search["cursor"]=standalone_first["next_cursor"].clone();
        assert!(page(host,&cookie,SEARCH,standalone_search.clone()).await["next_cursor"].is_null(), "Standalone search page EOF");
        let mut exact=standalone.clone();exact["id"]=json!(producer);
        // Keep one genuine read in I/O until the HTTP operation reaches its
        // existing 30-second deadline. The worker remains owned until released.
        // Session expiry timers share this owner and remain live for the sibling.
        // Observe completed work without waiting for those legitimate timers.
        {
            let mut jobs = host.service.0.jobs.lock().await;
            while let Some(result) = jobs.try_join_next() {
                checked(result, "completed catalog job joined");
            }
        }
        relay.hold_next_metadata();
        readers.push(spawn_catalog_read(host,&cookie,exact.clone()));
        relay.entered().await;
        assert_catalog_job_pending(host).await;
        let outcome=tokio::time::timeout(Duration::from_secs(40),readers.last_mut().expect("owned pending reader")).await;
        if outcome.is_ok() { readers.pop(); }
        let response=checked(checked(outcome,"deadline response guard"),"deadline reader join");
        assert_eq!(response.status(),StatusCode::GATEWAY_TIMEOUT,"original operation deadline");
        // A transport may already have quiesced at its own deadline. In either
        // case, observe the held read's actual JoinHandle. Its enqueue drained
        // older completions; only this read and the live session timers remain.
        relay.state.release.notify_waiters();
        join_catalog_read_job(host).await;
        let _=page(host,&sibling,PROJECTS,projects.clone()).await;
        // Disconnect cancels the request, without detaching its pending work.
        relay.hold_next_metadata();
        readers.push(spawn_catalog_read(host,&cookie,exact.clone()));
        relay.entered().await;assert_catalog_job_pending(host).await;
        let disconnected=readers.last_mut().expect("owned disconnect reader");disconnected.abort();
        let outcome=disconnected.await;readers.pop();
        assert!(outcome.is_err_and(|error|error.is_cancelled()),"client future disconnected");
        // Cancellation may quiesce transport promptly; completion is observed
        // by the same owner, never inferred from a JoinSet entry count.
        relay.state.release.notify_waiters();join_catalog_read_job(host).await;
        let _=page(host,&sibling,PROJECTS,projects.clone()).await;
        put_json(store.as_ref(),prefix,&user_key,json!({"v":1,"projects":[]})).await;
        let broken=checked(ArtifactId::new("fixture/catalog-a",fixture.server.policy().native_install.hub.projection()),"protected manifest ID");
        let broken_key=checked(registry_key(producer,ArtifactType::Skill,&broken),"protected manifest key");
        checked(store.put(&object_path(prefix,&broken_key),b"{invalid protected manifest".to_vec().into()).await,"owned malformed metadata fixture");
        rejected(host,Some(&cookie),SEARCH,&standalone_search.to_string(),StatusCode::FORBIDDEN,"permission_denied").await;
        checked(broker_store.put(&object_path(&broker_prefix,&broken_key),b"{invalid protected manifest".to_vec().into()).await,"owned malformed Broker metadata fixture");
        let mut corrupt_search=search.clone();corrupt_search["id"]=json!("fixture/catalog-a");
        rejected(host,Some(&cookie),SEARCH,&corrupt_search.to_string(),StatusCode::INTERNAL_SERVER_ERROR,"hub_catalog_unavailable").await;
        seed_catalog(&fixture.server,store.as_ref(),prefix,producer).await;
        // A denied request did not destroy another live browser session.
        assert!(page(host,&sibling,PROJECTS,projects.clone()).await["projects"][0]["id"]==producer.to_string(), "healthy sibling after rejection");
        let logout=checked(host.client.post(format!("{}/auth/logout",host.origin)).header(header::ORIGIN,&host.origin)
            .header(header::COOKIE,cookie.as_str()).send().await,"logout");
        assert_eq!(logout.status(),StatusCode::NO_CONTENT);
        rejected(host,Some(&cookie),PROJECTS,&projects.to_string(),StatusCode::UNAUTHORIZED,"unauthorized").await;
        let _=page(host,&sibling,PROJECTS,projects.clone()).await;
        remove_current_provider(fixture);
        rejected(host,Some(&sibling),PROJECTS,&projects.to_string(),StatusCode::FORBIDDEN,"permission_denied").await;
    }).catch_unwind().await;
    for reader in readers {
        reader.abort();
        let _ = reader.await;
    }
    let relay_result = std::panic::AssertUnwindSafe(async {
        if let Some(relay) = relay_owner {
            relay.close().await;
        }
    })
    .catch_unwind()
    .await;
    let observed = observed.and(relay_result);
    finish_remote_test_owners(
        observed,
        host,
        fixture_owner.as_ref().map(|f| &f.server),
        broker,
    )
    .await;
}
