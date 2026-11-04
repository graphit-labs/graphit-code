//! Real Broker OAuth sessions for the installed RemoteNative HTTP consumers.
//! The Broker Ready channel and browser cookies never enter logs or receipts.
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use zeroize::Zeroizing;

#[path = "hub_authored_resources.rs"]
mod hub_authored_resources;
#[path = "hub_file_mutations.rs"]
mod hub_file_mutations;
#[path = "hub_file_publication.rs"]
mod hub_file_publication;
#[path = "hub_file_publication_probe.rs"]
mod hub_file_publication_probe;
#[path = "hub_file_reads.rs"]
mod hub_file_reads;
#[path = "hub_remote_catalog.rs"]
mod hub_remote_catalog;
#[path = "hub_unpublish.rs"]
mod hub_unpublish;
#[path = "hub_upload.rs"]
mod hub_upload;

fn checked<T, E>(result: Result<T, E>, label: &str) -> T {
    result.unwrap_or_else(|_| panic!("{label}"))
}
fn wire<'a>(value: &'a Value, name: &str) -> &'a str {
    value[name]
        .as_str()
        .unwrap_or_else(|| panic!("required private fixture field"))
}
pub(crate) struct BrokerProcess {
    child: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
    pub(crate) ready: Value,
}
impl BrokerProcess {
    async fn join_child(child: &mut Child) -> bool {
        match tokio::time::timeout(Duration::from_secs(300), child.wait()).await {
            Ok(Ok(status)) => status.success(),
            _ => {
                let _ = child.start_kill();
                let _ = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
                false
            }
        }
    }
    pub(crate) async fn start(binary: &std::path::Path, project: &str) -> Self {
        Self::start_with_now(binary, project, false).await
    }
    async fn start_with_now(binary: &std::path::Path, project: &str, now_read: bool) -> Self {
        Self::start_web_projects(binary, project, None, now_read, false).await
    }
    async fn start_web_projects(
        binary: &std::path::Path,
        project: &str,
        second: Option<&str>,
        now_read: bool,
        standalone_catalog: bool,
    ) -> Self {
        Self::start_web_selected(binary, project, second, now_read, standalone_catalog, false).await
    }
    async fn start_web_publication(binary: &std::path::Path, project: &str) -> Self {
        Self::start_web_selected(binary, project, None, false, false, true).await
    }
    async fn start_web_selected(
        binary: &std::path::Path,
        project: &str,
        second: Option<&str>,
        now_read: bool,
        standalone_catalog: bool,
        hub_storage_write: bool,
    ) -> Self {
        let mut child = checked(
            tokio::process::Command::new(binary)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn(),
            "owned real Broker process",
        );
        let mut input = child.stdin.take().expect("private Broker stdin");
        let mut output =
            BufReader::new(child.stdout.take().expect("private Broker stdout")).lines();
        let result = futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(async {
            let mut start = json!({"command":"start_web","project_id":project,
                "authorization_ttl_seconds":60,"local_challenge_ttl_seconds":600,
                "access_ttl_seconds":600,"refresh_ttl_seconds":3600});
            if hub_storage_write {
                start["hub_storage_write"] = json!(true);
            }
            if let Some(second) = second {
                start["second_project_id"] = json!(second);
            }
            if now_read {
                start["now_read"] = json!(true);
            }
            if standalone_catalog {
                start["standalone_catalog"] = json!(true);
            }
            checked(
                input.write_all(format!("{start}\n").as_bytes()).await,
                "private Broker Start",
            );
            checked(input.flush().await, "Broker Start flush");
            let line = checked(
                checked(
                    tokio::time::timeout(Duration::from_secs(300), output.next_line()).await,
                    "Broker Ready guard",
                ),
                "Broker Ready read",
            )
            .unwrap_or_else(|| {
                let status = match child.try_wait() {
                    Ok(Some(status)) => status.code().map_or_else(
                        || "terminated_by_signal".to_owned(),
                        |code| format!("exit_code={code}"),
                    ),
                    Ok(None) => "still_running".to_owned(),
                    Err(_) => "status_unavailable".to_owned(),
                };
                panic!("Broker Ready line missing; child_status={status}");
            });
            let line = Zeroizing::new(line);
            let ready: Value = checked(serde_json::from_str(&line), "private Broker Ready decode");
            assert!(ready["event"] == "ready", "Broker Ready event");
            assert!(ready.get("web").is_some(), "explicit Web fixture profile");
            ready
        }))
        .await;
        match result {
            Ok(ready) => Self {
                child,
                input,
                output,
                ready,
            },
            Err(failure) => {
                drop(input);
                let _ = Self::join_child(&mut child).await;
                std::panic::resume_unwind(failure)
            }
        }
    }
    async fn sts_command(&mut self, command: &'static str) {
        assert!(matches!(command, "hold_sts" | "await_sts" | "release_sts"));
        let bytes = serde_json::to_vec(&json!({"command":command})).expect("private STS command");
        checked(self.input.write_all(&bytes).await, "STS command write");
        checked(self.input.write_all(b"\n").await, "STS command framing");
        checked(self.input.flush().await, "STS command flush");
        let line = checked(
            checked(
                tokio::time::timeout(Duration::from_secs(35), self.output.next_line()).await,
                "finite STS command ack",
            ),
            "STS command ack read",
        )
        .expect("STS command ack line");
        let line = Zeroizing::new(line);
        assert_eq!(
            checked::<Value, _>(serde_json::from_str(&line), "STS command ack decode"),
            json!({"event":"ack","command":command,"ok":true})
        );
    }
    pub(crate) async fn close(mut self) {
        let result = futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(async {
            checked(
                self.input.write_all(b"{\"command\":\"shutdown\"}\n").await,
                "private Broker Shutdown",
            );
            checked(self.input.flush().await, "Broker Shutdown flush");
            let line = checked(
                checked(
                    tokio::time::timeout(Duration::from_secs(300), self.output.next_line()).await,
                    "Broker Ack guard",
                ),
                "Broker Ack read",
            )
            .expect("Broker Ack line");
            let line = Zeroizing::new(line);
            assert!(
                checked::<Value, _>(serde_json::from_str(&line), "Broker Ack decode")
                    == json!({"event":"ack","command":"shutdown","ok":true}),
                "confirmed Broker resource cleanup"
            );
        }))
        .await;
        drop(self.input);
        let joined = Self::join_child(&mut self.child).await;
        if let Err(failure) = result {
            std::panic::resume_unwind(failure);
        }
        assert!(joined, "Broker child exited after cleanup");
    }
}
struct WebHost {
    service: BrowserAuth,
    origin: String,
    client: reqwest::Client,
    stop: CancellationToken,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl WebHost {
    async fn start(
        sources: crate::source_server::SourceServer,
        tasks: Option<crate::task_server::TaskServer>,
        broker: &Value,
        provider_revision: u64,
    ) -> Self {
        let endpoint = wire(broker, "endpoint");
        let verifier = Arc::new(checked(
            BrokerVerifier::new(
                BrokerPolicy {
                    endpoint,
                    mcp_resource: None,
                    provider_revision,
                    network: ClientPolicy {
                        origin: endpoint,
                        allow_loopback_http: true,
                        proxy: None,
                        additional_roots: &[],
                    },
                },
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
                },
                VerifierLimits {
                    token_bytes: 4096,
                    json: JsonLimits {
                        bytes: 32768,
                        nodes: 256,
                        depth: 16,
                        scalar_bytes: 8192,
                        collection_entries: 64,
                    },
                    max_keys: 8,
                    metadata_ttl: Duration::from_secs(60),
                    key_ttl: Duration::from_secs(60),
                    unknown_key_cooldown: Duration::from_secs(1),
                },
            ),
            "real Broker verifier",
        ));
        let listener = checked(TcpListener::bind("127.0.0.1:0").await, "Web host listener");
        let origin = format!(
            "http://{}",
            checked(listener.local_addr(), "Web host address")
        );
        let stop = CancellationToken::new();
        let service = checked(
            BrowserAuth::new(
                vec![("broker".into(), verifier)],
                &origin,
                BrowserPolicy {
                    request_timeout: Duration::from_secs(30),
                    login_timeout: Duration::from_secs(30),
                    session_lifetime: Duration::from_secs(600),
                    ..policy()
                },
                stop.clone(),
            ),
            "real BrowserAuth host",
        );
        let backend = Arc::new(task::backend(&format!(
            "{endpoint}/v1/project/access/authorize"
        )));
        let router = match tasks {
            Some(tasks) => service.task_router(tasks, backend.clone(), Some(sources.clone())),
            None => service.router(),
        }
        .merge(service.now_router(sources.clone()))
        .merge(service.ast_router(sources, backend))
        .merge(crate::foreground_ui::router());
        let stopped = stop.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stopped.cancelled_owned())
                .await
        });
        let client = checked(
            reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build(),
            "private browser HTTP client",
        );
        Self {
            service,
            origin,
            client,
            stop,
            server,
        }
    }
    async fn login(&self, broker: &Value) -> Zeroizing<String> {
        let response = checked(
            self.client
                .post(format!("{}/auth/login", self.origin))
                .header(header::ORIGIN, &self.origin)
                .json(&json!({"provider":"broker"}))
                .send()
                .await,
            "real Web login start",
        );
        assert_eq!(response.status(), StatusCode::OK);
        let login_cookie = Zeroizing::new(cookie_header(&response));
        let body: Value = checked(response.json().await, "private authorization URL");
        let authorization = checked(
            Url::parse(wire(&body, "authorization_url")),
            "actual authorization URL",
        );
        assert!(
            authorization.origin()
                == checked(Url::parse(wire(broker, "issuer")), "Broker origin").origin(),
            "authorization belongs to actual Broker"
        );
        let response = checked(
            self.client.get(authorization).send().await,
            "Broker authorization admission",
        );
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let login = checked(
            checked(Url::parse(wire(broker, "issuer")), "Broker URL").join(checked(
                response.headers()[header::LOCATION].to_str(),
                "Broker login redirect",
            )),
            "actual Broker login URL",
        );
        let id = login
            .query_pairs()
            .find(|(name, _)| name == "id")
            .expect("real pending authorization")
            .1
            .into_owned();
        let identity = &broker["web"];
        let response = checked(
            self.client
                .post(login)
                .form(&[
                    ("login_method", "local"),
                    ("username", wire(identity, "username")),
                    ("password", wire(identity, "password")),
                ])
                .send()
                .await,
            "productive LOCAL login",
        );
        assert!(
            !id.is_empty(),
            "pending authorization ID retained in login URL"
        );
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let callback = checked(
            Url::parse(checked(
                response.headers()[header::LOCATION].to_str(),
                "real callback redirect",
            )),
            "actual callback URL",
        );
        assert!(
            callback.origin() == checked(Url::parse(&self.origin), "Web origin").origin(),
            "callback belongs to registered Web host"
        );
        assert!(
            callback
                .query_pairs()
                .any(|(name, value)| name == "code" && !value.is_empty()),
            "real authorization code received"
        );
        let response = checked(
            self.client
                .get(callback)
                .header(header::COOKIE, login_cookie.as_str())
                .send()
                .await,
            "real registered callback exchange",
        );
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert!(
            response.headers()[header::LOCATION] == "/",
            "completed Web callback redirect"
        );
        let cookie = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with("graphit_session="))
            .expect("real opaque Web session cookie");
        assert!(
            cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
            "private session cookie attributes"
        );
        Zeroizing::new(
            cookie
                .split(';')
                .next()
                .expect("session cookie pair")
                .to_owned(),
        )
    }
    async fn post(&self, cookie: &str, path: &str, body: Value) -> reqwest::Response {
        self.client
            .post(format!("{}{path}", self.origin))
            .header(header::ORIGIN, &self.origin)
            .header(header::COOKIE, cookie)
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|error| {
                let class = if error.is_timeout() {
                    "timeout"
                } else if error.is_connect() {
                    "connect"
                } else {
                    "transport"
                };
                // Route is a static product path. Never format error/URL/body/cookies.
                panic!("real protected Web {path}: {class}")
            })
    }
    async fn close(self) {
        self.stop.cancel();
        let shutdown = self.service.shutdown().await;
        checked(
            checked(
                checked(
                    tokio::time::timeout(Duration::from_secs(30), self.server).await,
                    "Web host join guard",
                ),
                "Web host join",
            ),
            "Web host serve",
        );
        checked(shutdown, "BrowserAuth owned shutdown");
    }
}

// Only test payload setup uses the fixture's Native publisher bearer. The Web
// reader below goes through the productive Auth/lease/STSv3 consumer transport.
async fn seed_store(
    broker: &Value,
    producer: graphit_core::project_identity::ProjectId,
) -> Arc<dyn object_store::ObjectStore> {
    use object_store::aws::{AmazonS3Builder, S3ConditionalPut, S3CopyIfNotExists};
    let client = checked(
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build(),
        "owned Native seed HTTP client",
    );
    let response = checked(
        client
            .post(format!("{}/v1/s3/credentials", wire(broker, "endpoint")))
            .bearer_auth(wire(broker, "access_bearer"))
            .json(&json!({"scope":"project","project_id":producer,"module":"hub"}))
            .send()
            .await,
        "actual Native seed STSv3",
    );
    assert_eq!(response.status(), StatusCode::OK);
    let credentials: Value = checked(response.json().await, "private seed STSv3 wire");
    assert!(
        credentials["scope"] == "project"
            && credentials["module"] == "hub"
            && credentials["project_id"] == producer.to_string(),
        "actual STS scope"
    );
    assert!(
        credentials["bucket"] == broker["bucket"]
            && credentials["region"] == broker["region"]
            && credentials["endpoint"] == broker["storage_endpoint"]
            && credentials["prefixes"] == json!([wire(broker, "prefix")]),
        "actual STS topology"
    );
    let copy_if_not_exists = match wire(&credentials, "copy_if_not_exists") {
        "multipart" => S3CopyIfNotExists::Multipart,
        "if_none_match" => S3CopyIfNotExists::Header("if-none-match".into(), "*".into()),
        _ => panic!("unsupported issued Native seed copy strategy"),
    };
    let store = checked(
        AmazonS3Builder::new()
            .with_bucket_name(wire(&credentials, "bucket"))
            .with_region(wire(&credentials, "region"))
            .with_endpoint(wire(&credentials, "endpoint"))
            .with_virtual_hosted_style_request(false)
            .with_access_key_id(wire(&credentials, "access_key_id"))
            .with_secret_access_key(wire(&credentials, "secret_access_key"))
            .with_token(wire(&credentials, "session_token"))
            .with_conditional_put(S3ConditionalPut::ETagMatch)
            .with_copy_if_not_exists(copy_if_not_exists)
            .with_client_options(
                object_store::ClientOptions::new()
                    .with_allow_http(true)
                    .with_timeout(Duration::from_secs(30)),
            )
            .build(),
        "real issued Native seed S3 SDK",
    );
    Arc::new(store)
}
async fn publish_ast_sdk(
    source: &crate::source_server::SourceServer,
    store: &Arc<dyn object_store::ObjectStore>,
    broker: &Value,
    producer: graphit_core::project_identity::ProjectId,
    version: &str,
    name: &str,
    body: &str,
) -> crate::native_publication::PublicationReceipt {
    use arrow_array::{ArrayRef, RecordBatch, StringArray};
    use graphit_core::{artifact_store::CapturedSourceReader, file_lock::Cancellation};
    use graphit_data::{
        graph_writer::{GraphEdge, GraphNode, GraphValue, IcebugWriter},
        lance_module_store::LanceModuleStore,
    };
    use graphit_hub::{
        ArtifactType,
        native_root::{NativeComponent, NativeRole, NativeSnapshot},
        policy::RequestContext,
    };
    use std::collections::BTreeMap;
    let cancel = Cancellation::default();
    let request = checked(
        RequestContext::new(
            source.policy().content,
            &cancel,
            std::time::Instant::now() + Duration::from_secs(120),
        ),
        "finite SDK publication request",
    );
    let graph = checked(
        checked(
            IcebugWriter::new(Arc::clone(request.snapshot())),
            "actual Icebug writer",
        )
        .push_file(
            [
                GraphNode {
                    label: "File".into(),
                    key: "main.rs".into(),
                    properties: BTreeMap::from([
                        ("path".into(), GraphValue::String("main.rs".into())),
                        ("name".into(), GraphValue::String("main.rs".into())),
                    ]),
                },
                GraphNode {
                    label: "Function".into(),
                    key: "hello".into(),
                    properties: BTreeMap::from([
                        ("uid".into(), GraphValue::String("hello".into())),
                        ("name".into(), GraphValue::String(name.into())),
                        ("path".into(), GraphValue::String("main.rs".into())),
                        ("line_number".into(), GraphValue::Int64(2)),
                        ("end_line".into(), GraphValue::Int64(3)),
                    ]),
                },
            ],
            [GraphEdge {
                kind: "CONTAINS".into(),
                source_key: "main.rs".into(),
                target_key: "hello".into(),
                uid: "contains-hello".into(),
            }],
        ),
        "real graph nodes and relationship",
    );
    let graph = checked(graph.finish(), "completed real Icebug image");
    let source_dir = graph.directory().join("source.lance");
    let schema = graphit_ast::indexed_source::source_schema();
    let batch = checked(
        RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec!["main.rs"])) as ArrayRef,
                Arc::new(StringArray::from(vec![Some(body)])) as ArrayRef,
            ],
        ),
        "actual Source schema batch",
    );
    let module = checked(
        LanceModuleStore::create(
            source_dir.to_str().expect("owned Source path"),
            schema,
            vec![batch],
        )
        .await,
        "real Source Lance dataset",
    );
    let snapshot = module.snapshot();
    let metadata = checked(
        snapshot.native_metadata(request.snapshot()).await,
        "actual Source SDK metadata",
    );
    let base = checked(
        object_store::path::Path::from_filesystem_path(&source_dir),
        "owned SDK base path",
    );
    let manifest_path = metadata
        .manifest_location()
        .path
        .prefix_match(&base)
        .expect("SDK manifest belongs to owned Source")
        .map(|value| value.as_ref().to_owned())
        .collect::<Vec<_>>()
        .join("/");
    let manifest_bytes = checked(
        std::fs::read(source_dir.join(&manifest_path)),
        "actual physical Lance manifest",
    );
    let graph_bytes = checked(
        std::fs::read(graph.directory().join("icebug.json")),
        "actual physical Icebug manifest",
    );
    let graph_metadata: Value = checked(
        serde_json::from_slice(&graph_bytes),
        "actual Icebug metadata",
    );
    let components = [
        NativeComponent {
            role: NativeRole::Graph,
            path: "graph".into(),
            snapshot: NativeSnapshot::Icebug {
                manifest_path: "icebug.json".into(),
                generation: wire(&graph_metadata, "generation").into(),
                manifest_sha256: Sha256::digest(&graph_bytes).into(),
            },
        },
        NativeComponent {
            role: NativeRole::Source,
            path: "source.lance".into(),
            snapshot: NativeSnapshot::Lance {
                dataset_version: snapshot.version(),
                manifest_path,
                manifest_sha256: Sha256::digest(&manifest_bytes).into(),
            },
        },
    ];
    let reader = checked(
        CapturedSourceReader::capture(
            &graph.directory(),
            graphit_fs::NodeKind::Directory,
            &source.policy().file,
            source.policy().read,
            Arc::clone(request.snapshot()),
        ),
        "capture completed real SDK files",
    );
    let publication = crate::native_publication::PublicationMetadata {
        provider: "broker".into(),
        artifact_type: ArtifactType::Ast,
        id: "fixture/web-reader".into(),
        version: version.into(),
        name: "WebReaderFixture".into(),
        description: None,
        tags: vec![],
        author: None,
        dependencies: None,
    };
    let limits = crate::native_publication::metadata_limits(
        source.policy().content,
        source.policy().registry.limits().projection(),
    );
    let receipt = checked(
        crate::native_publication::publish(
            crate::native_publication::PublishInput {
                store: store.as_ref(),
                storage_prefix: wire(broker, "prefix"),
                producer,
                metadata: &publication,
                reader: &reader,
                components: &components,
                limits,
            },
            &request,
            || request.check(),
        )
        .await,
        "productive immutable publication and conditional catalog",
    );
    assert_eq!(receipt.catalog, "published");
    receipt
}

async fn fixture(broker: &Value) -> crate::source_server::test_fixture::Fixture {
    fixture_with_web_admissions(broker, (Some("broker"), None), &[]).await
}
async fn fixture_with_web_admissions(
    broker: &Value,
    providers: (Option<&str>, Option<&str>),
    extra_rules: &[graphit_auth::ServiceRuleInput<'_>],
) -> crate::source_server::test_fixture::Fixture {
    use graphit_auth::{
        CredentialSource, RemoteServiceBindingInput, ServiceOrigin, ServicePurpose,
        ServiceRuleInput,
    };
    let mut rules = vec![ServiceRuleInput::Remote {
        origin: ServiceOrigin::Web,
        binding: RemoteServiceBindingInput::Broker {
            provider: "broker",
            purpose: ServicePurpose::HubRead,
            endpoint: wire(broker, "endpoint"),
            resource: None,
        },
        credential: CredentialSource::IncomingBearer,
        incoming: None,
    }];
    rules.extend_from_slice(extra_rules);
    use graphit_auth::Permission;
    use graphit_core::{brand::Brand, module_settings::Module};
    let fixture = crate::source_server::test_fixture::fixture_with_web_read_providers(
        &[Permission::Read(Module::Ast)],
        &[Permission::ManageAuthProfiles],
        Brand::default(),
        graphit_hub::NativeBinding::KiroV3,
        &["consumer"],
        &rules,
        providers,
    );
    configure_hub_fixture(fixture).await
}
async fn configure_hub_fixture(
    mut fixture: crate::source_server::test_fixture::Fixture,
) -> crate::source_server::test_fixture::Fixture {
    use futures_util::FutureExt;
    use graphit_core::config::{ConfigKey, ConfigSnapshot};
    let configured = std::panic::AssertUnwindSafe(async {
        // Role tools require Source and Task to share the captured Hub envelope.
        // Reuse that actual policy before constructing or sharing the Source owner.
        let mut source_policy = fixture.server.policy().clone();
        source_policy.content = source_policy.native_install.hub.content();
        checked(
            fixture.server.shutdown().await,
            "join original Source fixture before configured replacement",
        );
        fixture.server = checked(
            crate::source_server::SourceServer::new(
                Arc::clone(&fixture.auth),
                Arc::clone(&fixture.projects),
                0,
                source_policy,
                CancellationToken::new(),
            ),
            "Source owner with the captured Hub envelope",
        );
        let brand = fixture.paths.brand();
        let key = checked(
            ConfigKey::new("broker.allow_loopback_http"),
            "explicit loopback fixture key",
        );
        let environment = checked(
            ConfigSnapshot::capture_environment(
                brand,
                std::slice::from_ref(&key),
                [(key.environment_variable(brand).into(), "true".into())],
            ),
            "explicit fixture config layer",
        );
        let config = checked(
            ConfigSnapshot::new(
                brand,
                [
                    checked(
                        graphit_hub::policy::HubResourcePolicy::distribution_defaults(brand),
                        "maintained Hub defaults",
                    ),
                    environment,
                ],
            ),
            "captured fixture configuration",
        );
        checked(
            fixture.server.attach_role_tools(config),
            "attach actual Source configuration before sharing",
        );
    })
    .catch_unwind()
    .await;
    if let Err(failure) = configured {
        let _ = std::panic::AssertUnwindSafe(fixture.server.shutdown())
            .catch_unwind()
            .await;
        std::panic::resume_unwind(failure);
    }
    fixture
}
fn install_record(
    source: &crate::source_server::SourceServer,
    broker: &Value,
    producer: graphit_core::project_identity::ProjectId,
    receipt: &crate::native_publication::PublicationReceipt,
) {
    use graphit_core::file_lock::Cancellation;
    use graphit_hub::{
        ArtifactId, ConcreteVersion, InstallationTarget, SourceRef,
        registry::{Installation, InstalledArtifact, InstalledBackend, InstalledPayload},
        remote_native::RemoteNativeRecord,
    };
    let registry = &source.policy().registry;
    let limits = registry.limits().projection();
    let id = checked(
        ArtifactId::new(&receipt.id, limits),
        "exact published artifact ID",
    );
    let version = checked(
        ConcreteVersion::new(&receipt.version, limits),
        "exact published version",
    );
    assert_eq!(receipt.root_sha256.len(), 64);
    let mut digest = [0u8; 32];
    for (i, byte) in digest.iter_mut().enumerate() {
        *byte = checked(
            u8::from_str_radix(&receipt.root_sha256[i * 2..i * 2 + 2], 16),
            "actual root digest",
        );
    }
    let original_source = checked(
        SourceRef::hub_cache(
            &source.policy().installation,
            producer,
            receipt.artifact_type,
            id.clone(),
            None,
            version,
            limits,
        ),
        "owned Hub cache reference",
    );
    let record = checked(
        RemoteNativeRecord::new(
            "broker",
            wire(broker, "bucket"),
            wire(broker, "region"),
            Some(wire(broker, "storage_endpoint")),
            wire(broker, "prefix"),
            digest,
            limits,
        ),
        "explicit remote topology and digest",
    );
    // Installation metadata is data only. The real Web request still obtains its
    // imported admission, current service selection/lease, Broker STS and S3 IO.
    checked(
        checked(
            registry.writer(Duration::from_secs(5), &Cancellation::default()),
            "owned installation writer",
        )
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: receipt.artifact_type,
            id,
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source,
                display: None,
                alias: None,
                entry_name: None,
                backend: InstalledBackend::RemoteNative(record),
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        }),
        "publish actual GlobalNative RemoteNative tuple",
    );
}
fn remove_current_provider(fixture: &crate::source_server::test_fixture::Fixture) {
    use graphit_auth::profile_manager::{NativeProfileManager, ProfileManagerAuthority};
    use graphit_core::file_lock::Cancellation;
    use graphit_hub::policy::RequestContext;
    let cancel = Cancellation::default();
    let original = checked(
        RequestContext::new(
            fixture.server.policy().content,
            &cancel,
            std::time::Instant::now() + Duration::from_secs(30),
        ),
        "current provider mutation Original",
    );
    let issuer = fixture.auth.issuer();
    let caller = checked(
        issuer.authenticate_runtime(&fixture.raw, &original),
        "real Native manager caller",
    );
    let session = checked(
        issuer.bootstrap_admin(&caller),
        "current administrative session",
    );
    let parent = checked(
        issuer.admin_parent(&caller, &session),
        "current provider manager authority",
    );
    let manager = NativeProfileManager::new(issuer);
    let snapshot = checked(
        manager.begin(ProfileManagerAuthority::Admin(&parent)),
        "actual provider snapshot",
    );
    let revision = snapshot.revision();
    let publication = checked(
        manager.remove_provider(snapshot, "broker", false, revision),
        "remove provider through actual manager",
    );
    assert!(
        publication.failure().is_none() && publication.cleanup_failure().is_none(),
        "provider mutation completed"
    );
    assert!(
        publication.into_pending_cleanup().is_none(),
        "provider mutation has no pending cleanup"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built genuine Broker Web/STSv3/MinIO fixture; Root owns the Code compilation window"]
async fn real_broker_web_reads_installed_remote_ast_without_project_authority() {
    use futures_util::FutureExt;
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit built real Broker fixture executable");
    // This producer is absent from the consumer's actual project inventory.
    let producer = checked(
        "01D39ZY06FGSCTVN4T2V9PKHFZ".parse::<graphit_core::project_identity::ProjectId>(),
        "canonical independent producer",
    );
    let broker = BrokerProcess::start(std::path::Path::new(&binary), &producer.to_string()).await;
    let mut fixture_owner = None;
    let mut host = None;
    let observed = std::panic::AssertUnwindSafe(async {
        fixture_owner = Some(fixture(&broker.ready).await);
        let fixture = fixture_owner.as_ref().expect("owned admitted Source fixture");
        assert!(fixture.projects.iter().all(|(_, id, _)| *id != producer));
        let store=seed_store(&broker.ready,producer).await;
        let one=publish_ast_sdk(&fixture.server,&store,&broker.ready,producer,"release/one","web-real-one","Web SDK source one\npub fn one() {}\n").await;
        let two=publish_ast_sdk(&fixture.server,&store,&broker.ready,producer,"release/two","web-real-two","Web SDK source two\npub fn two() {}\n").await;
        assert_ne!(one.root_sha256,two.root_sha256,"immutable release roots differ");
        install_record(&fixture.server,&broker.ready,producer,&one);
        install_record(&fixture.server,&broker.ready,producer,&two);
        let revision=crate::graph_server::tests::browser_provider(&fixture.server.graph_server(),&fixture.raw,wire(&broker.ready,"endpoint"));
        host=Some(WebHost::start(fixture.server.clone(), None,&broker.ready,revision).await);
        let web=host.as_ref().expect("owned Web host");
        let cookie=web.login(&broker.ready).await;
        let response=web.post(&cookie,"/api/ast/imports",json!({})).await;
        assert_eq!(response.status(),StatusCode::OK);
        let imports:Value=checked(response.json().await,"actual installed import catalog");
        let entries=imports["imports"].as_array().expect("actual import entries");
        for version in ["release/one","release/two"] {
            assert!(entries.iter().any(|item|item["id"]=="fixture/web-reader" && item["version"]==version && item["backend"]=="remote_native"),"both exact RemoteNative releases visible without PROJECT");
        }
        for (version,name,body) in [("release/one","web-real-one","Web SDK source one"),("release/two","web-real-two","Web SDK source two")] {
            let selected=json!({"id":"fixture/web-reader","version":version});
            let response=web.post(&cookie,"/api/ast/imported/schema",json!({"source":selected})).await;
            if response.status() != StatusCode::OK {
                let status = response.status();
                let body = checked(response.text().await, "finite remote schema error body");
                let reason = match body.as_str() {
                    "authentication_or_permission_denied" => "remote_authority_denied",
                    "remote_native_read_failed" => "remote_read_failed",
                    "storage_credentials_expired" => "storage_credentials_expired",
                    "graph authentication or permission denied" => "imported_admission_denied",
                    "request_cancelled_or_expired" => "request_cancelled_or_expired",
                    _ => "unclassified_finite_failure",
                };
                panic!("remote schema HTTP {status}; {reason}");
            }
            let schema:Value=checked(response.json().await,"actual remote schema");
            let response=web.post(&cookie,"/api/ast/imported/sample",json!({"source":selected})).await;
            assert_eq!(response.status(),StatusCode::OK);
            let sample:Value=checked(response.json().await,"actual remote sample");
            assert_eq!(sample["generation"],schema["generation"]);
            assert!(sample["nodes"].as_array().expect("sample nodes").iter().any(|node|node["label"]=="File"), "actual File sample node");
            // Remote sample intentionally omits PK. Resolve the actual key via schema/Cypher.
            let file_schema = schema["nodes"].as_array().expect("actual schema nodes").iter().find(|node| node["label"] == "File").expect("actual File schema");
            let primary_key = wire(file_schema, "primary_key");
            assert!(primary_key.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') && !primary_key.is_empty(), "fixture PK is a Cypher identifier");
            let key_column = file_schema["columns"].as_array().expect("actual File columns").iter().find(|column| column[0] == primary_key).expect("actual PK column");
            let key_type = key_column[1].as_str().expect("native PK type");
            assert!(key_type.eq_ignore_ascii_case("STRING"), "actual fixture File string PK");
            let key_response = web.post(&cookie, "/api/ast/imported/query", json!({"source":selected,"query":{"query":format!("MATCH (f:File) RETURN f.{primary_key} LIMIT 1"),"parameters":{}}})).await;
            assert_eq!(key_response.status(), StatusCode::OK);
            let keys: Value = checked(key_response.json().await, "actual File PK query");
            assert_eq!(keys["columns"].as_array().expect("PK query columns").len(), 1);
            let rows = keys["rows"].as_array().expect("PK query rows"); assert_eq!(rows.len(), 1);
            let row = rows[0].as_array().expect("PK query row"); assert_eq!(row.len(), 1);
            let cell = &row[0]; assert!(wire(cell, "type").eq_ignore_ascii_case(key_type));
            let key = wire(cell, "value"); assert_eq!(key, "main.rs", "PK came from actual remote query");
            let anchor = json!({"type":"string","value":key});
            let response=web.post(&cookie,"/api/ast/imported/query",json!({"source":selected,
                "query":{"query":"MATCH (f:Function) RETURN f.name","parameters":{}}})).await;
            assert_eq!(response.status(),StatusCode::OK);
            let query:Value=checked(response.json().await,"actual remote Cypher result");
            assert!(query.to_string().contains(name),"exact release data returned by actual query engine");
            let response=web.post(&cookie,"/api/ast/imported/neighborhood",json!({"source":selected,
                "neighborhood":{"anchor_label":"File","anchor_key":anchor,"relationship":"CONTAINS","other_label":"Function","direction":"outgoing","limit":1}})).await;
            assert_eq!(response.status(),StatusCode::OK);
            let neighborhood:Value=checked(response.json().await,"actual remote neighborhood");
            assert_eq!(neighborhood["edges"][0]["uid"],"contains-hello");
            let response=web.post(&cookie,"/api/ast/imported/source",json!({"source":selected,"input":{"path":"main.rs","head":1}})).await;
            assert_eq!(response.status(),StatusCode::OK);
            let inspected:Value=checked(response.json().await,"actual pinned remote Source");
            assert_eq!(inspected["origin"],"indexed");
            assert!(inspected.to_string().contains(body),"exact version Source bytes delivered");
        }
        // A valid registered session cannot import arbitrary absent releases.
        assert_eq!(web.post(&cookie,"/api/ast/imported/schema",json!({"source":{"id":"fixture/web-reader","version":"absent"}})).await.status(),StatusCode::NOT_FOUND);
        // Revoke only local current provider authority. Cached signed Web tokens
        // and installed data do not preserve permission or inherit Native state.
        remove_current_provider(fixture);
        let response=web.post(&cookie,"/api/ast/imported/schema",json!({"source":{"id":"fixture/web-reader","version":"release/one"}})).await;
        assert_eq!(response.status(),StatusCode::FORBIDDEN);
    }).catch_unwind().await;
    finish_remote_test_owners(
        observed,
        host,
        fixture_owner.as_ref().map(|fixture| &fixture.server),
        broker,
    )
    .await;
}

fn published_object_path(
    source: &crate::source_server::SourceServer,
    broker: &Value,
    receipt: &crate::native_publication::PublicationReceipt,
    relative: &str,
) -> object_store::path::Path {
    let limits = source.policy().registry.limits().projection();
    let id = checked(
        graphit_hub::ArtifactId::new(&receipt.id, limits),
        "published ID",
    );
    let version = checked(
        graphit_hub::ConcreteVersion::new(&receipt.version, limits),
        "published version",
    );
    assert_eq!(receipt.root_sha256.len(), 64);
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = checked(
            u8::from_str_radix(&receipt.root_sha256[index * 2..index * 2 + 2], 16),
            "published digest",
        );
    }
    let release = checked(
        graphit_hub::remote_storage::release_prefix(
            receipt.producer,
            receipt.artifact_type,
            &id,
            &version,
            &digest,
        ),
        "canonical published release",
    );
    checked(
        object_store::path::Path::parse(format!(
            "{}/{}/{relative}",
            wire(broker, "prefix"),
            release.trim_end_matches('/')
        )),
        "owned remote object key",
    )
}
async fn append_to_published_object(
    store: &Arc<dyn object_store::ObjectStore>,
    path: &object_store::path::Path,
) {
    use object_store::ObjectStoreExt as _;
    let object = checked(store.get(path).await, "read owned published object");
    let marker = object_store::UpdateVersion {
        e_tag: object.meta.e_tag.clone(),
        version: object.meta.version.clone(),
    };
    assert!(
        marker.e_tag.is_some() || marker.version.is_some(),
        "real conditional marker"
    );
    let mut bytes = checked(object.bytes().await, "complete original object").to_vec();
    bytes.push(b'\n');
    checked(
        store
            .put_opts(
                path,
                bytes.into(),
                object_store::PutMode::Update(marker).into(),
            )
            .await,
        "conditional corruption of owned fixture object",
    );
}
async fn finish_remote_test_owners(
    observed: std::thread::Result<()>,
    host: Option<WebHost>,
    source: Option<&crate::source_server::SourceServer>,
    broker: BrokerProcess,
) {
    use futures_util::FutureExt;
    let host_result = std::panic::AssertUnwindSafe(async {
        if let Some(host) = host {
            host.close().await;
        }
    })
    .catch_unwind()
    .await;
    let source_result = std::panic::AssertUnwindSafe(async {
        if let Some(source) = source {
            checked(source.shutdown().await, "owned Source and graph shutdown");
        }
    })
    .catch_unwind()
    .await;
    let broker_result = std::panic::AssertUnwindSafe(broker.close())
        .catch_unwind()
        .await;
    for result in [observed, host_result, source_result, broker_result] {
        if let Err(failure) = result {
            std::panic::resume_unwind(failure);
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web/STSv3/MinIO fixture and coordinated App emission"]
async fn real_broker_web_rejects_corrupt_root_and_source_manifest_with_healthy_sibling() {
    use futures_util::FutureExt;
    use graphit_core::file_lock::Cancellation;
    use graphit_hub::{
        native_root::{NativeRole, NativeRoot, NativeSnapshot},
        policy::RequestContext,
    };
    use object_store::ObjectStoreExt as _;
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    let producer = checked(
        "01D39ZY06FGSCTVN4T2V9PKHFZ".parse::<graphit_core::project_identity::ProjectId>(),
        "independent producer",
    );
    let broker = BrokerProcess::start(std::path::Path::new(&binary), &producer.to_string()).await;
    let mut fixture_owner = None;
    let mut host = None;
    let observed = std::panic::AssertUnwindSafe(async {
        fixture_owner = Some(fixture(&broker.ready).await);
        let fixture = fixture_owner.as_ref().expect("owned admitted Source fixture");
        assert!(fixture.projects.iter().all(|(_, id, _)| *id != producer));
        let store = seed_store(&broker.ready, producer).await;
        let root_case = publish_ast_sdk(&fixture.server, &store, &broker.ready, producer,
            "integrity/root", "root-before-corruption", "root original source\n").await;
        let manifest_case = publish_ast_sdk(&fixture.server, &store, &broker.ready, producer,
            "integrity/manifest", "manifest-before-corruption", "manifest original source\n").await;
        let healthy = publish_ast_sdk(&fixture.server, &store, &broker.ready, producer,
            "integrity/healthy", "healthy-sibling", "healthy sibling source\n").await;
        for receipt in [&root_case, &manifest_case, &healthy] {
            install_record(&fixture.server, &broker.ready, producer, receipt);
        }
        let root_key = published_object_path(&fixture.server, &broker.ready, &root_case, "native.json");
        // Valid trailing JSON whitespace must still change the full raw descriptor SHA.
        append_to_published_object(&store, &root_key).await;
        let manifest_root_key = published_object_path(&fixture.server, &broker.ready, &manifest_case, "native.json");
        let descriptor = checked(checked(store.get(&manifest_root_key).await, "owned source descriptor").bytes().await, "complete descriptor");
        let cancel = Cancellation::default();
        let request = checked(RequestContext::new(fixture.server.policy().content, &cancel,
            std::time::Instant::now() + Duration::from_secs(30)), "descriptor observation Original");
        let limits = crate::native_publication::metadata_limits(fixture.server.policy().content,
            fixture.server.policy().registry.limits().projection());
        let decoded = checked(NativeRoot::decode(&descriptor, limits, &request), "actual native descriptor");
        let source_component = decoded.components().iter().find(|component| component.role == NativeRole::Source).expect("declared actual Source component");
        let NativeSnapshot::Lance { manifest_path, dataset_version: original_version, manifest_sha256: original_digest } = &source_component.snapshot else { panic!("Source SDK Lance pin"); };
        let manifest_key = published_object_path(&fixture.server, &broker.ready, &manifest_case,
            &format!("{}/{manifest_path}", source_component.path));
        // Replace with another physical SDK manifest of the same real schema/version.
        // Appending bytes invalidates the Lance footer before its hash guard can run.
        let healthy_root_key = published_object_path(&fixture.server, &broker.ready, &healthy, "native.json");
        let healthy_descriptor = checked(checked(store.get(&healthy_root_key).await, "healthy descriptor").bytes().await, "complete healthy descriptor");
        let healthy_root = checked(NativeRoot::decode(&healthy_descriptor, limits, &request), "healthy NativeRoot");
        let healthy_source = healthy_root.components().iter().find(|component| component.role == NativeRole::Source).expect("healthy Source component");
        let NativeSnapshot::Lance { manifest_path: healthy_manifest_path, dataset_version: healthy_version, .. } = &healthy_source.snapshot else { panic!("healthy Source Lance pin"); };
        assert_eq!(healthy_version, original_version, "same actual SDK dataset version");
        let healthy_manifest_key = published_object_path(&fixture.server, &broker.ready, &healthy, &format!("{}/{healthy_manifest_path}", healthy_source.path));
        let healthy_manifest = checked(checked(store.get(&healthy_manifest_key).await, "healthy physical SDK manifest").bytes().await, "complete valid SDK manifest");
        assert_ne!(<[u8; 32]>::from(Sha256::digest(&healthy_manifest)), *original_digest, "physical manifest bytes differ from selected pin");
        let target = checked(store.get(&manifest_key).await, "selected manifest mutation marker");
        let marker = object_store::UpdateVersion { e_tag: target.meta.e_tag.clone(), version: target.meta.version.clone() };
        assert!(marker.e_tag.is_some() || marker.version.is_some());
        drop(target);
        checked(store.put_opts(&manifest_key, healthy_manifest.into(), object_store::PutMode::Update(marker).into()).await, "conditional substitution with real valid SDK manifest");
        let revision = crate::graph_server::tests::browser_provider(&fixture.server.graph_server(), &fixture.raw, wire(&broker.ready, "endpoint"));
        host = Some(WebHost::start(fixture.server.clone(), None, &broker.ready, revision).await);
        let web = host.as_ref().expect("owned Web host");
        let cookie = web.login(&broker.ready).await;
        let response = web.post(&cookie, "/api/ast/imports", json!({})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let before: Value = checked(response.json().await, "registered pinned imports");
        let response = web.post(&cookie, "/api/ast/imported/schema", json!({"source":{"id":"fixture/web-reader","version":"integrity/root"}})).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(checked(response.text().await, "finite root integrity code"), "remote_native_digest_mismatch");
        let response = web.post(&cookie, "/api/ast/imported/source", json!({"source":{"id":"fixture/web-reader","version":"integrity/manifest"},"input":{"path":"main.rs","head":1}})).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(checked(response.text().await, "finite manifest integrity code"),
            "remote_native_digest_mismatch", "integrity code replaces all partial Source output");
        let response = web.post(&cookie, "/api/ast/imported/source", json!({"source":{"id":"fixture/web-reader","version":"integrity/healthy"},"input":{"path":"main.rs","head":1}})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let healthy_read: Value = checked(response.json().await, "healthy sibling Source");
        assert_eq!(healthy_read["origin"], "indexed");
        assert!(healthy_read.to_string().contains("healthy sibling source"));
        let response = web.post(&cookie, "/api/ast/imported/schema", json!({"source":{"id":"fixture/web-reader","version":"integrity/healthy"}})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = web.post(&cookie, "/api/ast/imports", json!({})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let after: Value = checked(response.json().await, "imports after failed reads");
        assert_eq!(after, before, "read failures preserve installed selection catalogue");
    }).catch_unwind().await;
    finish_remote_test_owners(
        observed,
        host,
        fixture_owner.as_ref().map(|fixture| &fixture.server),
        broker,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web/STSv3/MinIO and coordinated App emission"]
async fn real_broker_web_checks_origin_cancels_one_job_and_joins_owned_shutdown() {
    use futures_util::FutureExt;
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    let producer = checked(
        "01D39ZY06FGSCTVN4T2V9PKHFZ".parse::<graphit_core::project_identity::ProjectId>(),
        "independent producer",
    );
    let broker = BrokerProcess::start(std::path::Path::new(&binary), &producer.to_string()).await;
    let mut fixture_owner = None;
    let mut host = None;
    let mut release_job = None;
    let mut pending = None;
    let mut closing = None;
    let observed = std::panic::AssertUnwindSafe(async {
        fixture_owner = Some(fixture(&broker.ready).await);
        let fixture = fixture_owner
            .as_ref()
            .expect("owned admitted Source fixture");
        assert!(fixture.projects.iter().all(|(_, id, _)| *id != producer));
        let store = seed_store(&broker.ready, producer).await;
        let receipt = publish_ast_sdk(
            &fixture.server,
            &store,
            &broker.ready,
            producer,
            "lifecycle/pinned",
            "lifecycle-function",
            "lifecycle source\n",
        )
        .await;
        install_record(&fixture.server, &broker.ready, producer, &receipt);
        let graph = fixture.server.graph_server();
        let revision = crate::graph_server::tests::browser_provider(
            &graph,
            &fixture.raw,
            wire(&broker.ready, "endpoint"),
        );
        host = Some(WebHost::start(fixture.server.clone(), None, &broker.ready, revision).await);
        let web = host.as_ref().expect("owned Web host");
        let cookie = web.login(&broker.ready).await;
        let selected = json!({"id":"fixture/web-reader","version":"lifecycle/pinned"});
        let denied = checked(
            web.client
                .post(format!("{}/api/ast/imported/schema", web.origin))
                .header(header::ORIGIN, "https://foreign.invalid")
                .header(header::COOKIE, cookie.as_str())
                .json(&json!({"source":selected}))
                .send()
                .await,
            "current foreign-origin request",
        );
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        // The session/JWT is still valid: the current origin guard denied this request.
        assert_eq!(
            web.post(
                &cookie,
                "/api/ast/imported/schema",
                json!({"source":selected})
            )
            .await
            .status(),
            StatusCode::OK
        );
        // Credentials/refresh are deliberately serialized within one Web session.
        // Use another genuine session to test independent service/job health.
        let sibling_cookie = web.login(&broker.ready).await;
        let (started, release) = crate::graph_server::tests::gate_next_native_job(&graph);
        release_job = Some(release);
        let client = web.client.clone();
        let origin = web.origin.clone();
        let request_cookie = cookie.clone();
        let body = json!({"source":selected});
        pending = Some(tokio::spawn(async move {
            client
                .post(format!("{origin}/api/ast/imported/sample"))
                .header(header::ORIGIN, origin)
                .header(header::COOKIE, request_cookie.as_str())
                .json(&body)
                .send()
                .await
        }));
        let cancellation = checked(
            checked(
                tokio::task::spawn_blocking(move || started.recv_timeout(Duration::from_secs(30)))
                    .await,
                "join job observation",
            ),
            "productive remote job reached its test barrier",
        );
        let request = pending.take().expect("owned pending request");
        request.abort();
        assert!(
            request.await.is_err_and(|error| error.is_cancelled()),
            "HTTP subscriber aborted"
        );
        checked(
            tokio::time::timeout(Duration::from_secs(5), async {
                while !cancellation.is_cancelled() {
                    tokio::task::yield_now().await;
                }
            })
            .await,
            "disconnected HTTP subscriber cancels its original",
        );
        // Gate is per-job and consumed once; an independent admitted request remains healthy.
        let sibling = web
            .post(
                &sibling_cookie,
                "/api/ast/imported/source",
                json!({"source":selected,"input":{"path":"main.rs","head":1}}),
            )
            .await;
        assert_eq!(sibling.status(), StatusCode::OK);
        let sibling: Value = checked(sibling.json().await, "healthy sibling source");
        assert!(sibling.to_string().contains("lifecycle source"));
        let shutdown_graph = graph.clone();
        closing = Some(tokio::spawn(async move { shutdown_graph.shutdown().await }));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), closing.as_mut().unwrap())
                .await
                .is_err(),
            "shutdown waits for its retained blocking job"
        );
        let waiter = closing.take().unwrap();
        waiter.abort();
        assert!(waiter.await.is_err_and(|error| error.is_cancelled()));
        let shutdown_graph = graph.clone();
        closing = Some(tokio::spawn(async move { shutdown_graph.shutdown().await }));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), closing.as_mut().unwrap())
                .await
                .is_err(),
            "a dropped shutdown subscriber did not detach the owned job"
        );
        checked(
            release_job.take().unwrap().send(()),
            "release owned graph job",
        );
        checked(
            checked(
                checked(
                    tokio::time::timeout(Duration::from_secs(30), closing.take().unwrap()).await,
                    "graph owned-shutdown guard",
                ),
                "graph shutdown join",
            ),
            "graph shutdown result",
        );
    })
    .catch_unwind()
    .await;
    if let Some(request) = pending {
        request.abort();
        let _ = request.await;
    }
    if let Some(release) = release_job {
        let _ = release.send(());
    }
    if let Some(waiter) = closing {
        waiter.abort();
        let _ = waiter.await;
    }
    finish_remote_test_owners(
        observed,
        host,
        fixture_owner.as_ref().map(|fixture| &fixture.server),
        broker,
    )
    .await;
}

type CpuFixture = (
    crate::foreground_source::foreground_memory_jobs_tests::Fixture,
    crate::foreground_source::ForegroundSource,
    crate::task_server::TaskServer,
    crate::source_server::SourceServer,
    String,
    (
        graphit_core::project_paths::ProjectPaths,
        graphit_core::project_identity::ProjectId,
        graphit_core::version_context::VersionContext,
    ),
);

async fn finish_cpu_test_owners(
    observed: std::thread::Result<()>,
    host: Option<WebHost>,
    owners: [Option<CpuFixture>; 2],
    broker: Option<BrokerProcess>,
) {
    use futures_util::FutureExt;
    let mut outcomes = vec![("original", observed)];
    outcomes.push((
        "Web host",
        std::panic::AssertUnwindSafe(async {
            if let Some(host) = host {
                host.close().await;
            }
        })
        .catch_unwind()
        .await,
    ));
    for (owner_index, owner) in owners.into_iter().enumerate() {
        let Some(owner) = owner else {
            continue;
        };
        let labels = if owner_index == 0 {
            [
                "producer Source",
                "producer Task",
                "producer foreground/model",
            ]
        } else {
            [
                "consumer Source",
                "consumer Task",
                "consumer foreground/model",
            ]
        };
        // Keep the installation/tempdir alive until all of its actual owners join.
        let (_fixture, foreground, tasks, source, _raw, _selected) = owner;
        outcomes.push((
            labels[0],
            std::panic::AssertUnwindSafe(async {
                checked(source.shutdown().await, "CPU Source shutdown");
            })
            .catch_unwind()
            .await,
        ));
        outcomes.push((
            labels[1],
            std::panic::AssertUnwindSafe(async {
                checked(tasks.shutdown().await, "CPU Task shutdown");
            })
            .catch_unwind()
            .await,
        ));
        outcomes.push((
            labels[2],
            std::panic::AssertUnwindSafe(async {
                checked(foreground.shutdown().await, "CPU foreground/model shutdown");
            })
            .catch_unwind()
            .await,
        ));
    }
    outcomes.push((
        "Broker",
        std::panic::AssertUnwindSafe(async {
            if let Some(broker) = broker {
                broker.close().await;
            }
        })
        .catch_unwind()
        .await,
    ));
    if outcomes.iter().any(|(_, outcome)| outcome.is_err()) {
        for (label, outcome) in &outcomes {
            eprintln!(
                "WebCPU outcome {label}: {}",
                if outcome.is_ok() { "ok" } else { "failed" }
            );
        }
    }
    for (_, outcome) in outcomes {
        if let Err(failure) = outcome {
            std::panic::resume_unwind(failure);
        }
    }
}

fn cpu_publication_metadata(
    kind: graphit_hub::ArtifactType,
    version: &str,
) -> crate::native_publication::PublicationMetadata {
    crate::native_publication::PublicationMetadata {
        provider: "broker".into(),
        artifact_type: kind,
        id: match kind {
            graphit_hub::ArtifactType::Ast => "fixture/web-cpu-ast",
            _ => "fixture/web-cpu-knowledge",
        }
        .into(),
        version: version.into(),
        name: "ActualCPUWebSnapshot".into(),
        description: None,
        tags: vec![],
        author: None,
        dependencies: None,
    }
}

async fn cpu_index_ast(producer: &CpuFixture) {
    use graphit_core::file_lock::Cancellation;
    let (producer_fixture, _, _, producer_source, _, producer_selected) = producer;
    let resources = checked(producer_source.semantic_model(), "producer CPU resources")
        .expect("actual CPU resources");
    use crate::foreground_ast_sources::{ForegroundAstSourceConfig, ForegroundAstSources};
    use graphit_core::{config::ConfigStore, module_settings::Module};
    let cancellation = Cancellation::default();
    checked(
        checked(
            ConfigStore::project(&producer_selected.0),
            "producer config store",
        )
        .set(
            Module::Ast.key(),
            json!(true),
            Duration::from_secs(5),
            &cancellation,
        ),
        "enable actual AST producer",
    );
    checked(
        std::fs::write(
            producer_selected.0.root().join("browser_search.rs"),
            "pub fn browser_search_sum(left: i32, right: i32) -> i32 { left + right }\n",
        ),
        "real indexed AST source",
    );
    let request = checked(
        resources
            .policy
            .request_context(std::time::Instant::now(), &cancellation),
        "AST indexing Original",
    );
    let ast = Arc::new(checked(
        ForegroundAstSources::new(
            Arc::clone(&resources.auth),
            ForegroundAstSourceConfig {
                live: producer_source.graph_server().live_ast(),
                installation: producer_source.policy().installation.clone(),
                base: producer_fixture.config().clone(),
                policy: resources.policy.clone(),
                scope: resources.scope,
                native: resources.native,
                binding: resources.binding,
                environment: Vec::new(),
            },
            request.snapshot(),
        ),
        "actual AST producer",
    ));
    let context = graphit_daemon::supervisor::ProjectContext {
        id: producer_selected.1,
        version: producer_selected.2.clone(),
    };
    assert!(
        checked(
            ast.index(
                &producer_selected.0,
                &context,
                graphit_daemon::watcher::ChangeBatch {
                    rescan: true,
                    ..Default::default()
                },
                &cancellation
            )
            .await,
            "real Raw and EntitySearch index"
        )
        .is_some()
    );
    assert!(
        checked(
            ast.index_semantic(
                producer_selected.0.clone(),
                context,
                Some(Arc::clone(&resources)),
                None,
                std::collections::BTreeSet::new(),
                false,
                cancellation.clone(),
                cancellation.clone()
            )
            .await,
            "real AST CPU NN index"
        )
        .version
        .is_some()
    );
}

async fn cpu_publish_ast(
    producer: &CpuFixture,
    consumer_source: &crate::source_server::SourceServer,
    broker: &Value,
    store: &Arc<dyn object_store::ObjectStore>,
) {
    use graphit_auth::{
        ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture, ScopeInput,
    };
    use graphit_core::{artifact_store::CapturedPublicationScope, file_lock::Cancellation};
    use graphit_hub::{ArtifactType, native_root::NativeRole};
    let (producer_fixture, _, _, producer_source, producer_raw, producer_selected) = producer;
    let resources = checked(producer_source.semantic_model(), "producer CPU resources")
        .expect("actual CPU resources");
    let cancellation = Cancellation::default();
    let request = checked(
        resources
            .policy
            .request_context(std::time::Instant::now(), &cancellation),
        "AST publication Original",
    );
    let issuer = resources.auth.issuer();
    let caller = checked(
        issuer.authenticate_runtime(producer_raw, &request),
        "actual AST producer caller",
    );
    let scope = checked(
        CapturedPublicationScope::project(
            &producer_selected.0,
            producer_selected.1,
            resources.scope,
            request.snapshot(),
        ),
        "AST captured scope",
    );
    let binding = checked(scope.binding(), "AST current project binding");
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = checked(
        authority.admit_authenticated_read(
            &caller,
            &request,
            ScopeInput {
                installation: resources.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                    version: &producer_selected.2,
                }),
                providers: &[],
                binding: resources.binding,
                global_version: "",
            },
            ProjectReadModule::Ast,
        ),
        "actual AST publication admission",
    );
    let live = producer_source.live_projects();
    let capture = checked(
        crate::project_graph::PreparedRead::capture(
            producer_source.policy(),
            &live,
            (
                &producer_selected.0,
                producer_selected.1,
                &producer_selected.2,
            ),
            Arc::clone(request.snapshot()),
        ),
        "capture actual PROJECT graph",
    );
    let (_staged, graph) = checked(
        producer_source
            .graph_server()
            .capture_current_export(
                &capture,
                &producer_selected.0,
                producer_fixture.config(),
                &live,
                request.snapshot(),
            )
            .await,
        "export actual current AST graph",
    );
    let components = checked(
        crate::native_publication::ast_components(&graph, &request).await,
        "actual SDK AST component pins",
    );
    for role in [
        NativeRole::Graph,
        NativeRole::Source,
        NativeRole::EntitySearch,
    ] {
        assert!(
            components.iter().any(|component| component.role == role),
            "all active AST roles captured"
        );
    }
    let reader = checked(
        graph.source_reader(request.snapshot()),
        "actual AST source reader",
    );
    for component in &components {
        let role = if component.role == NativeRole::Graph {
            "graph"
        } else if component.role == NativeRole::Source {
            "source"
        } else if component.role == NativeRole::EntitySearch {
            "entity_search"
        } else {
            "unexpected"
        };
        match &component.snapshot {
            graphit_hub::native_root::NativeSnapshot::Lance {
                dataset_version,
                manifest_path,
                manifest_sha256,
            } => eprintln!(
                "WebCPU AST Lance pin: role={role}; path={}; dataset_version={dataset_version}; manifest={manifest_path}; sha256={manifest_sha256:02x?}",
                component.path
            ),
            graphit_hub::native_root::NativeSnapshot::Icebug {
                generation,
                manifest_path,
                manifest_sha256,
            } => eprintln!(
                "WebCPU AST graph pin: role={role}; path={}; generation={generation}; manifest={manifest_path}; sha256={manifest_sha256:02x?}",
                component.path
            ),
        }
    }
    for version in ["release/one", "release/two"] {
        let publication = cpu_publication_metadata(ArtifactType::Ast, version);
        let receipt = checked(
            crate::native_publication::publish(
                crate::native_publication::PublishInput {
                    store: store.as_ref(),
                    storage_prefix: wire(broker, "prefix"),
                    producer: producer_selected.1,
                    metadata: &publication,
                    reader,
                    components: &components,
                    limits: crate::native_publication::metadata_limits(
                        resources.policy.content(),
                        resources.policy.projection(),
                    ),
                },
                &request,
                || {
                    request.check()?;
                    admission.recheck().map_err(std::io::Error::other)?;
                    capture
                        .check(producer_source.policy(), producer_fixture.config(), &live)
                        .map_err(std::io::Error::other)?;
                    graph
                        .revalidate(request.snapshot())
                        .map_err(std::io::Error::other)
                },
            )
            .await,
            "publish real AST SDK snapshot through Broker STS/S3",
        );
        install_record(consumer_source, broker, producer_selected.1, &receipt);
    }
}

async fn cpu_publish_knowledge(
    producer: &CpuFixture,
    consumer_source: &crate::source_server::SourceServer,
    broker: &Value,
    store: &Arc<dyn object_store::ObjectStore>,
) -> u64 {
    use graphit_auth::{
        ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture, ScopeInput,
    };
    use graphit_core::{artifact_store::CapturedPublicationScope, file_lock::Cancellation};
    use graphit_hub::ArtifactType;
    let (_producer_fixture, _, _, producer_source, producer_raw, producer_selected) = producer;
    let resources = checked(producer_source.semantic_model(), "producer CPU resources")
        .expect("actual CPU resources");
    let cancellation = Cancellation::default();
    let request = checked(
        resources
            .policy
            .request_context(std::time::Instant::now(), &cancellation),
        "Knowledge publication Original",
    );
    let issuer = resources.auth.issuer();
    let caller = checked(
        issuer.authenticate_runtime(producer_raw, &request),
        "actual Knowledge producer caller",
    );
    let scope = checked(
        CapturedPublicationScope::project(
            &producer_selected.0,
            producer_selected.1,
            resources.scope,
            request.snapshot(),
        ),
        "Knowledge captured scope",
    );
    let binding = checked(scope.binding(), "Knowledge current project binding");
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = checked(
        authority.admit_authenticated_read(
            &caller,
            &request,
            ScopeInput {
                installation: resources.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                    version: &producer_selected.2,
                }),
                providers: &[],
                binding: resources.binding,
                global_version: "",
            },
            ProjectReadModule::Knowledge,
        ),
        "actual Knowledge publication admission",
    );
    let task_policy = checked(
        crate::foreground_tasks::task_policy(
            &resources.policy,
            resources.scope,
            resources.native,
            resources.binding,
        ),
        "real Knowledge task policy",
    );
    let frozen = checked(
        crate::knowledge_package_export::capture_native_knowledge(
            &request,
            &admission,
            &caller,
            checked(
                crate::foreground_knowledge::limits(&task_policy),
                "actual Knowledge read limits",
            ),
        )
        .await,
        "capture actual Source/NN pair",
    )
    .expect("published Knowledge corpus");
    assert!(frozen.generation.version > 0);
    assert_eq!(frozen.components.len(), 1);
    let clone_version = match &frozen.components[0].snapshot {
        graphit_hub::native_root::NativeSnapshot::Lance {
            dataset_version, ..
        } => *dataset_version,
        _ => panic!("Knowledge Wiki is a Lance snapshot"),
    };
    for version in ["release/one", "release/two"] {
        let publication = cpu_publication_metadata(ArtifactType::Knowledge, version);
        let receipt = checked(
            crate::native_publication::publish(
                crate::native_publication::PublishInput {
                    store: store.as_ref(),
                    storage_prefix: wire(broker, "prefix"),
                    producer: producer_selected.1,
                    metadata: &publication,
                    reader: &frozen.reader,
                    components: &frozen.components,
                    limits: crate::native_publication::metadata_limits(
                        resources.policy.content(),
                        resources.policy.projection(),
                    ),
                },
                &request,
                || {
                    request.check()?;
                    admission.recheck().map_err(std::io::Error::other)
                },
            )
            .await,
            "publish real Knowledge Pair through Broker STS/S3",
        );
        install_record(consumer_source, broker, producer_selected.1, &receipt);
    }
    clone_version
}

async fn cpu_read_ast(web: &WebHost, cookie: &str) {
    let ast_line = "pub fn browser_search_sum(left: i32, right: i32) -> i32 { left + right }";
    for version in ["release/one", "release/two"] {
        let source = json!({"id":"fixture/web-cpu-ast","version":version});
        let response = web.post(cookie, "/api/ast/imported/source", json!({"source":source,
            "input":{"path":"browser_search.rs","entity":"browser_search_sum","entity_type":"Function"}})).await;
        let status = response.status();
        let body = checked(response.text().await, "remote entity response");
        let error_code = match body.as_str() {
            "request_cancelled_or_expired" => "request_cancelled_or_expired",
            "authentication_store_failure" => "authentication_store_failure",
            "authentication_or_permission_denied" => "authentication_or_permission_denied",
            "storage_credentials_expired" => "storage_credentials_expired",
            "remote_native_binding_changed" => "remote_native_binding_changed",
            "remote_native_digest_mismatch" => "remote_native_digest_mismatch",
            "remote_native_component_unavailable" => "remote_native_component_unavailable",
            "remote_native_read_failed" => "remote_native_read_failed",
            _ => "other",
        };
        assert_eq!(
            status,
            StatusCode::OK,
            "actual remote AST entity: {error_code}"
        );
        let value: Value = checked(serde_json::from_str(&body), "remote entity result");
        assert_eq!(value["origin"], "indexed");
        assert_eq!(value["selection"]["source"], ast_line);
        assert_eq!(value["selection"]["start_line"], 1);
        assert_eq!(value["selection"]["end_line"], 1);
    }
    let source = json!({"id":"fixture/web-cpu-ast","version":"release/one"});
    for selector in [
        json!({"head":1}),
        json!({"tail":1}),
        json!({"start_line":1,"end_line":1,"line_numbers":true}),
        json!({"pattern":"browser_search_sum","before":0,"after":0}),
        json!({"pattern":"^pub fn browser_search_[a-z]+","regex":true,"before":0,"after":0}),
    ] {
        let mut input = json!({"path":"browser_search.rs"});
        input
            .as_object_mut()
            .unwrap()
            .extend(selector.as_object().unwrap().clone());
        let response = web
            .post(
                cookie,
                "/api/ast/imported/source",
                json!({"source":source,"input":input}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK, "remote Source selector");
        let value: Value = checked(response.json().await, "remote Source selection");
        assert_eq!(value["origin"], "indexed");
        assert_eq!(value["selection"]["total_lines"], 2);
        let selection = &value["selection"];
        if selector.get("pattern").is_some() {
            assert_eq!(selection["source"], format!(">    1: {ast_line}"));
            assert_eq!(
                selection["matches"],
                json!([{"line_number":1,"line":ast_line,"is_match":true}])
            );
            assert!(selection.get("start_line").is_none());
            assert!(selection.get("end_line").is_none());
        } else {
            let (text, line_number) = if selector.get("tail").is_some() {
                (String::new(), 2)
            } else if selector.get("line_numbers").is_some() {
                (format!("   1: {ast_line}"), 1)
            } else {
                (ast_line.to_owned(), 1)
            };
            assert_eq!(selection["source"], text);
            assert_eq!(selection["start_line"], line_number);
            assert_eq!(selection["end_line"], line_number);
            assert!(selection.get("matches").is_none());
        }
    }
    for mode in ["fts", "semantic", "vector", "hybrid"] {
        let response = web
            .post(
                cookie,
                "/api/ast/imported/search",
                json!({"source":source,
            "search":{"query":"browser_search_sum","mode":mode,"k":8,"metric":"cosine"}}),
            )
            .await;
        let status = response.status();
        let body = checked(response.text().await, "real AST CPU search body");
        let finite_code = if body == "invalid graph request" {
            "InvalidGraphRequest"
        } else if body == std::io::Error::from(std::io::ErrorKind::InvalidData).to_string() {
            "QueryInvalidData"
        } else if body == std::io::Error::from(std::io::ErrorKind::InvalidInput).to_string() {
            "QueryInvalidInput"
        } else {
            "Other"
        };
        assert_eq!(
            status,
            StatusCode::OK,
            "remote AST search mode={mode} code={finite_code}"
        );
        let value: Value = checked(serde_json::from_str(&body), "real AST CPU search result");
        assert_eq!(value["status"], "available");
        assert_eq!(value["mode"], mode);
        assert!(value.get("search_generation").is_none());
        let hit = value["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|hit| hit["name"] == "browser_search_sum")
            .expect("real remote AST hit");
        assert_eq!(hit["path"], "browser_search.rs");
        assert!(
            hit["entity_key"]
                .as_str()
                .is_some_and(|key| !key.is_empty())
        );
        assert!(hit.get("content").is_none());
        if matches!(mode, "fts" | "hybrid") {
            assert!(hit["score"].as_f64().is_some_and(f64::is_finite));
        }
    }
}

async fn cpu_read_knowledge(web: &WebHost, cookie: &str, pair: &u64) -> (Value, Value) {
    let response = web
        .post(
            cookie,
            "/api/knowledge/imported/contexts",
            json!({"input":{}}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let contexts: Value = checked(response.json().await, "remote Knowledge contexts");
    assert!(contexts.get("selection").is_none());
    assert!(contexts.get("native").is_none());
    for version in ["release/one", "release/two"] {
        let source = json!({"kind":"imported_knowledge","artifact":{"id":"fixture/web-cpu-knowledge","version":version}});
        assert!(contexts["contexts"].as_array().unwrap().contains(&source));
        let response = web
            .post(
                cookie,
                "/api/knowledge/imported/heads",
                json!({"input":{"source":source}}),
            )
            .await;
        let status = response.status();
        let body = checked(response.text().await, "remote Knowledge heads body");
        let finite_code = match body.as_str() {
            "remote_native_read_failed" => "remote_native_read_failed",
            "remote_native_binding_changed" => "remote_native_binding_changed",
            "remote_native_digest_mismatch" => "remote_native_digest_mismatch",
            "remote_native_component_unavailable" => "remote_native_component_unavailable",
            "import_selection_changed" => "import_selection_changed",
            _ => "Other",
        };
        assert_eq!(
            status,
            StatusCode::OK,
            "remote Knowledge heads code={finite_code}"
        );
        let heads: Value = checked(serde_json::from_str(&body), "remote Knowledge heads");
        assert_eq!(heads["source"], source);
        assert_eq!(heads["native"]["state"], "published");
        assert_eq!(heads["native"]["generation"]["version"], *pair);
        let heads = heads["native"]["heads"].as_array().unwrap();
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0]["slug"], "knowledge-cpu");
        let response = web
            .post(
                cookie,
                "/api/knowledge/imported/page",
                json!({"input":{"source":source,"slug":"knowledge-cpu"}}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let page: Value = checked(response.json().await, "remote Knowledge FULL18 page");
        assert_eq!(page["source"], source);
        assert!(page.get("selection").is_none());
        assert_eq!(page["native"]["state"], "published");
        assert_eq!(page["native"]["generation"]["version"], *pair);
        assert_eq!(page["native"]["page"]["state"], "found");
        let document = &page["native"]["page"]["document"];
        assert_eq!(document["slug"], "knowledge-cpu");
        assert_eq!(document["title"], "Knowledge CPU search");
        assert_eq!(
            document["body"],
            "Knowledge CPU search explains a real local embedding query and same snapshot lexical results."
        );
    }
    let source = json!({"kind":"imported_knowledge","artifact":{"id":"fixture/web-cpu-knowledge","version":"release/one"}});
    let mut expected_generations = None;
    for mode in ["fts", "semantic", "hybrid"] {
        let response = web.post(cookie, "/api/knowledge/imported/search", json!({"input":{"source":source,
            "search":{"query":"Knowledge CPU search","mode":mode,"top_k":2,"page_size":1,"preview":true}}})).await;
        assert_eq!(response.status(), StatusCode::OK, "remote Knowledge search");
        let value: Value = checked(response.json().await, "remote Knowledge search result");
        assert_eq!(value["source"], source);
        assert!(value.get("selection").is_none_or(Value::is_null));
        let native = &value["native"];
        assert_eq!(native["state"], "hits");
        assert_eq!(native["total"], 1);
        assert_eq!(native["generation"]["version"], *pair);
        if mode != "fts" {
            assert_eq!(native["semantic_generation"]["version"], *pair);
            let generations = (
                native["generation"].clone(),
                native["semantic_generation"].clone(),
            );
            if let Some(expected) = &expected_generations {
                assert_eq!(&generations, expected);
            } else {
                expected_generations = Some(generations);
            }
        }
        let hits = native["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        assert_eq!(hit["focused_source"]["slug"], "knowledge-cpu");
        assert_eq!(hit["preview"]["text"], "Knowledge CPU search");
        assert_eq!(hit["preview"]["source"], "summary");
        assert!(hit.get("body").is_none());
        if mode != "fts" {
            assert!(
                hit["semantic_distance"]
                    .as_f64()
                    .is_some_and(f64::is_finite)
            );
        }
        if mode != "semantic" {
            assert!(hit["lexical_score"].as_f64().is_some_and(f64::is_finite));
        }
        if mode == "hybrid" {
            assert!(hit["rrf_score"].as_f64().is_some_and(f64::is_finite));
        }
    }
    expected_generations.expect("observed real Pair generations")
}

async fn cpu_read_degraded(web: &WebHost, cookie: &str, generations: (Value, Value)) {
    let source = json!({"kind":"imported_knowledge","artifact":{"id":"fixture/web-cpu-knowledge","version":"release/one"}});
    let (generation, semantic_generation) = generations;
    for (mode, state) in [("semantic", "unavailable"), ("hybrid", "partial")] {
        let response = web
            .post(
                cookie,
                "/api/knowledge/imported/search",
                json!({"input":{"source":source,
            "search":{"query":"Knowledge CPU search","mode":mode,"top_k":2,"page_size":1}}}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value = checked(response.json().await, "remote degraded Knowledge search");
        assert_eq!(value["source"], source);
        let native = &value["native"];
        assert_eq!(native["state"], state);
        assert_eq!(native["reason"], "model_unavailable");
        assert_eq!(native["generation"], generation);
        assert_eq!(native["semantic_generation"], semantic_generation);
        let hits = native["hits"].as_array().unwrap();
        if mode == "semantic" {
            assert!(hits.is_empty());
        } else {
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0]["focused_source"]["slug"], "knowledge-cpu");
            assert!(
                hits[0]["lexical_score"]
                    .as_f64()
                    .is_some_and(f64::is_finite)
            );
            assert!(hits[0].get("semantic_distance").is_none());
            assert!(hits[0].get("rrf_score").is_none());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web/STSv3/MinIO and the qualified local CPU model executable"]
async fn real_broker_web_reads_ast_published_pins_and_knowledge_corpus_with_cpu_and_degraded_search()
 {
    use crate::foreground_source::foreground_memory_jobs_tests::{
        knowledge_native_search_fixture, stop_browser_model,
    };
    use futures_util::FutureExt;
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit genuine Broker fixture executable");
    std::env::var_os("GRAPHIT_TEST_MODEL_SERVER").expect("explicit qualified CPU model executable");
    let mut producer_owner = None;
    let mut consumer_owner = None;
    let mut broker_owner = None;
    let mut host = None;
    eprintln!("WebCPU phase: caller entered");
    let observed = Box::pin(
        std::panic::AssertUnwindSafe(async {
            eprintln!("WebCPU phase: producer fixture");
            producer_owner = Some(Box::pin(knowledge_native_search_fixture(false, None)).await);
            let producer = producer_owner.as_ref().unwrap();
            eprintln!("WebCPU phase: Broker fixture");
            broker_owner = Some(
                BrokerProcess::start(std::path::Path::new(&binary), &producer.5.1.to_string())
                    .await,
            );
            let broker = broker_owner.as_ref().unwrap();
            eprintln!("WebCPU phase: consumer fixture");
            consumer_owner = Some(
                Box::pin(knowledge_native_search_fixture(
                    true,
                    Some(wire(&broker.ready, "endpoint")),
                ))
                .await,
            );
            let (_, _, consumer_tasks, consumer_source, consumer_raw, consumer_selected) =
                consumer_owner.as_ref().unwrap();
            assert_ne!(
                producer.5.1, consumer_selected.1,
                "producer and consumer projects are distinct"
            );
            assert!(
                !checked(
                    consumer_source.live_projects().is_active(
                        &producer.5.0,
                        producer.5.1,
                        &producer.5.2
                    ),
                    "consumer inventory membership"
                ),
                "producer absent from consumer inventory"
            );
            {
                let producer_model = checked(producer.3.semantic_model(), "producer CPU model")
                    .expect("actual producer model resources");
                let consumer_model = checked(consumer_source.semantic_model(), "consumer CPU model")
                    .expect("actual consumer model resources");
                let producer_local = producer_model
                    .local
                    .as_ref()
                    .expect("actual producer local CPU model");
                let consumer_local = consumer_model
                    .local
                    .as_ref()
                    .expect("actual consumer local CPU model");
                eprintln!(
                    "WebCPU model identity: config_id_equal={}; model_id_equal={}; revision_equal={}; dimensions_equal={}",
                    producer_local.expected.config_id().0 == consumer_local.expected.config_id().0,
                    producer_local.expected.model() == consumer_local.expected.model(),
                    producer_local.expected.revision() == consumer_local.expected.revision(),
                    producer_local.expected.dimensions() == consumer_local.expected.dimensions(),
                );
            }
            eprintln!("WebCPU phase: AST index");
            Box::pin(cpu_index_ast(producer)).await;
            let store = seed_store(&broker.ready, producer.5.1).await;
            eprintln!("WebCPU phase: AST publication");
            Box::pin(cpu_publish_ast(
                producer,
                consumer_source,
                &broker.ready,
                &store,
            ))
            .await;
            eprintln!("WebCPU phase: Knowledge publication");
            let pair = Box::pin(cpu_publish_knowledge(
                producer,
                consumer_source,
                &broker.ready,
                &store,
            ))
            .await;
            let revision = crate::graph_server::tests::browser_provider(
                &consumer_source.graph_server(),
                consumer_raw,
                wire(&broker.ready, "endpoint"),
            );
            host = Some(
                WebHost::start(
                    consumer_source.clone(),
                    Some(consumer_tasks.clone()),
                    &broker.ready,
                    revision,
                )
                .await,
            );
            let web = host.as_ref().unwrap();
            let cookie = web.login(&broker.ready).await;
            eprintln!("WebCPU phase: AST HTTP");
            Box::pin(cpu_read_ast(web, &cookie)).await;
            eprintln!("WebCPU phase: Knowledge HTTP");
            let generations = Box::pin(cpu_read_knowledge(web, &cookie, &pair)).await;
            let (consumer_storage, consumer_foreground, consumer_tasks, _, _, _) =
                consumer_owner.as_mut().unwrap();
            stop_browser_model(consumer_storage, consumer_foreground).await;
            checked(
                consumer_tasks.attach_memory_model(None),
                "detach stopped consumer model",
            );
            eprintln!("WebCPU phase: degraded Knowledge HTTP");
            Box::pin(cpu_read_degraded(
                host.as_ref().unwrap(),
                &cookie,
                generations,
            ))
            .await;
        })
        .catch_unwind(),
    )
    .await;
    Box::pin(finish_cpu_test_owners(
        observed,
        host,
        [producer_owner, consumer_owner],
        broker_owner,
    ))
    .await;
}

/// Create the large Native fixture future in this bounded phase, so the caller's
/// coroutine stores its pointer instead of retaining every generic setup frame.
#[inline(never)]
fn knowledge_phase<F: std::future::Future>(create: impl FnOnce() -> F) -> std::pin::Pin<Box<F>> {
    Box::pin(create())
}

/// Consumer proof: the broker owns Web/STS/storage; the foreground owns both
/// model lifecycles. No test-issued grant or alternate model client is involved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker storage fixture, pinned Gemma/BGE and model server"]
async fn real_broker_web_imported_knowledge_local_bge_rerank_preserves_candidates_and_cursor() {
    use crate::foreground_source::foreground_memory_jobs_tests::{
        knowledge_cpu_search_fixture_with_rerank, knowledge_native_search_fixture,
    };
    use futures_util::FutureExt;
    use graphit_core::file_lock::Cancellation;
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit genuine Broker fixture executable");
    std::env::var_os("GRAPHIT_TEST_MODEL_SERVER").expect("explicit model server");
    std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL").expect("pinned Gemma artifact");
    std::env::var_os("GRAPHIT_BGE_RERANKER_V2_M3_MODEL").expect("pinned BGE artifact");
    let mut producer_owner = None;
    let mut consumer_owner = None;
    let mut broker_owner = None;
    let mut host = None;
    let mut consumer_fixture = None;
    let mut consumer_services = None;
    let mut consumer_running = None;
    let consumer_stop = Cancellation::default();
    let observed = Box::pin(std::panic::AssertUnwindSafe(async {
        producer_owner = Some(knowledge_phase(|| knowledge_native_search_fixture(true, None)).await);
        let producer = producer_owner.as_ref().unwrap();
        broker_owner = Some(BrokerProcess::start(std::path::Path::new(&binary), &producer.5.1.to_string()).await);
        let broker = broker_owner.as_ref().unwrap();
        consumer_owner = Some(knowledge_phase(|| knowledge_cpu_search_fixture_with_rerank(
            true, Some(wire(&broker.ready, "endpoint")), false,
        )).await);
        let (_, _, tasks, source_server, raw, selected) = consumer_owner.as_ref().unwrap();
        assert_ne!(producer.5.1, selected.1, "the imported producer is a distinct namespace");
        assert!(!checked(source_server.live_projects().is_active(&producer.5.0, producer.5.1, &producer.5.2), "consumer inventory"));
        let store = seed_store(&broker.ready, producer.5.1).await;
        let pair = knowledge_phase(|| cpu_publish_knowledge(producer, source_server, &broker.ready, &store)).await;
        let revision = crate::graph_server::tests::browser_provider(&source_server.graph_server(), raw, wire(&broker.ready, "endpoint"));
        let tasks = tasks.clone();
        let source_server = source_server.clone();
        let (fixture, foreground, _, _, _, _) = consumer_owner.take().unwrap();
        consumer_fixture = Some(fixture);
        consumer_services = Some((tasks.clone(), source_server.clone()));
        let signal = consumer_stop.clone();
        consumer_running = Some(tokio::spawn(async move { foreground.run(&signal).await }));
        host = Some(WebHost::start(source_server.clone(), Some(tasks.clone()), &broker.ready, revision).await);
        let web = host.as_ref().unwrap();
        let cookie = web.login(&broker.ready).await;
        let source = json!({"kind":"imported_knowledge","artifact":{"id":"fixture/web-cpu-knowledge","version":"release/one"}});
        let search = async |page_size: usize, rerank: bool, cursor: Option<Value>| {
            let mut query = json!({"query":"Knowledge CPU search", "mode":"fts", "top_k":2, "page_size":page_size, "preview":true, "rerank":rerank});
            if let Some(cursor) = cursor { query["cursor"] = cursor; }
            let response = web.post(&cookie, "/api/knowledge/imported/search", json!({"input":{"source":source,"search":query}})).await;
            assert_eq!(response.status(), StatusCode::OK, "real imported rerank consumer");
            checked::<Value,_>(response.json().await, "imported rerank response")
        };
        let mut first_cursor = None;
        let mut first_slug = None;
        for page_size in [2, 1] {
            let baseline = search(page_size, false, None).await;
            assert_eq!(baseline["native"]["total"], 2);
            assert_eq!(baseline["native"]["generation"]["version"], pair);
            assert_eq!(baseline["native"]["hits"].as_array().unwrap().len(), page_size);
            assert!(baseline["native"]["hits"].as_array().unwrap().iter().all(|hit| hit.get("rerank_score").is_none()));
            let mut actual = search(page_size, true, None).await;
            let hits = actual["native"]["hits"].as_array_mut().unwrap();
            let scores: Vec<_> = hits.iter().map(|hit| hit["rerank_score"].as_f64().filter(|score| score.is_finite()).expect("real local BGE score")).collect();
            assert!(scores.windows(2).all(|scores| scores[0] >= scores[1]));
            for hit in hits.iter_mut() { hit.as_object_mut().unwrap().remove("rerank_score"); }
            hits.sort_by_key(|hit| hit["focused_source"]["slug"].as_str().unwrap().to_owned());
            let mut expected = baseline.clone();
            expected["native"]["hits"].as_array_mut().unwrap().sort_by_key(|hit| hit["focused_source"]["slug"].as_str().unwrap().to_owned());
            assert_eq!(actual, expected, "rerank preserves retrieval fields, namespace, pin and cursor of the same page");
            if page_size == 1 {
                assert!(baseline["native"]["next_cursor"].is_object());
                first_cursor = Some(baseline["native"]["next_cursor"].clone());
                first_slug = Some(baseline["native"]["hits"][0]["focused_source"]["slug"].clone());
            }
        }
        let cursor = first_cursor.unwrap();
        let next = search(1, true, Some(cursor.clone())).await;
        assert_eq!(next["native"]["total"], 2);
        assert_eq!(next["native"]["generation"]["version"], pair);
        assert_eq!(next["native"]["hits"].as_array().unwrap().len(), 1);
        assert_ne!(next["native"]["hits"][0]["focused_source"]["slug"], first_slug.unwrap());
        assert!(next["native"]["hits"][0]["rerank_score"].as_f64().is_some_and(f64::is_finite));
        assert!(next["native"]["next_cursor"].is_null());
        let wrong_source = json!({"kind":"imported_knowledge","artifact":{"id":"fixture/web-cpu-knowledge","version":"release/two"}});
        let rejected = web.post(&cookie, "/api/knowledge/imported/search", json!({"input":{"source":wrong_source,"search":{
            "query":"Knowledge CPU search", "mode":"fts", "top_k":2, "page_size":1, "rerank":true, "cursor":cursor,
        }}})).await;
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST, "cursor grants no access to another installed version");
    }).catch_unwind()).await;
    let mut outcomes = vec![("original", observed)];
    outcomes.push((
        "Web host",
        std::panic::AssertUnwindSafe(async {
            if let Some(host) = host {
                host.close().await;
            }
        })
        .catch_unwind()
        .await,
    ));
    if let Some((tasks, source)) = consumer_services {
        outcomes.push((
            "consumer Source",
            std::panic::AssertUnwindSafe(async {
                checked(source.shutdown().await, "consumer Source shutdown");
            })
            .catch_unwind()
            .await,
        ));
        outcomes.push((
            "consumer Task",
            std::panic::AssertUnwindSafe(async {
                checked(tasks.shutdown().await, "consumer Task shutdown");
            })
            .catch_unwind()
            .await,
        ));
    }
    consumer_stop.cancel();
    outcomes.push((
        "consumer foreground/model",
        knowledge_phase(|| {
            std::panic::AssertUnwindSafe(async {
                if let Some(running) = consumer_running {
                    match checked(running.await, "consumer foreground join") {
                        Ok(()) => (),
                        Err(mut failure) => {
                            if let Some(owner) = failure.owner.take() {
                                checked(
                                    owner.shutdown().await,
                                    "retained consumer foreground/model shutdown",
                                );
                            }
                            panic!("consumer foreground failed: {failure:?}");
                        }
                    }
                }
            })
            .catch_unwind()
        })
        .await,
    ));
    for (label, outcome) in &outcomes {
        if outcome.is_err() {
            eprintln!("Web Knowledge outcome {label}: failed");
        }
    }
    let observed = outcomes
        .into_iter()
        .find_map(|(_, result)| result.err())
        .map_or(Ok(()), Err);
    knowledge_phase(|| {
        finish_cpu_test_owners(
            observed,
            None,
            [producer_owner, consumer_owner],
            broker_owner,
        )
    })
    .await;
    // Keep the consumed foreground's installation alive until the real runner,
    // external Source/Task owners and the Broker have all joined.
    drop(consumer_fixture);
}

mod now;
