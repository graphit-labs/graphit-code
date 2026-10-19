use arrow_array::{ArrayRef, RecordBatch, StringArray};
use axum::http::{HeaderName, HeaderValue, header::AUTHORIZATION};
use graphit_app::source_server::{SourcePolicy, SourceServer};
use graphit_ast::indexed_source::source_schema;
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ScopeCapture,
    ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
        ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet,
        ScopeLimits, SnapshotPolicy, SourceReadLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    config::ConfigSnapshot,
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::BootstrapLimits,
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectId, ProjectIdentityStore, ProjectMetadata},
    project_paths::InstallationPaths,
    project_paths::ProjectPaths,
    runtime_key::RuntimeKeyLimits,
    tcp_listener::{self, Options, PortSelection},
    version_context::VersionContext,
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupPolicy};
use graphit_data::lance_module_store::LanceModuleStore;
use graphit_hub::{
    NativeBinding,
    native::ContentLimits,
    policy::{HubResourcePolicy, RequestContext},
    registry::HubRegistry,
};
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult},
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
type TestResult = Result<(), Box<dyn std::error::Error>>;

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_until(cancel, Instant::now() + Duration::from_secs(30))
}
fn request_until(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
fn scopes() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 128).unwrap()
}
fn documents() -> AuthDocumentLimits {
    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn profiles() -> ProfileLimits {
    ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn policy(read: bool) -> StartupPolicy {
    let permissions = if read {
        vec![Permission::Read(Module::Ast)]
    } else {
        vec![]
    };
    let source = || SourceAuthorityPolicy {
        ordinary: &permissions,
        administrative: &[],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope: scopes(),
        document: documents(),
        profiles: profiles(),
        authority: IssuerAuthorityPolicy::new(
            source(),
            source(),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 1024,
            },
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 8192,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap(),
        ttl: TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
    }
}

fn publish_local(auth: &DaemonAuth, paths: &InstallationPaths, ctx: &RequestContext<'_>) {
    let id = InstallationIdentityStore::new(paths)
        .unwrap()
        .load()
        .unwrap()
        .unit_id()
        .unwrap()
        .clone();
    let scope = CapturedPublicationScope::installation(
        paths,
        id,
        [paths.root().to_owned()],
        scopes(),
        ctx.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(scope, documents(), ctx.snapshot()).unwrap(),
        profiles(),
        ctx,
    )
    .unwrap();
    let before = store.load(ctx).unwrap();
    let result = store
        .mutate(before, ctx, |doc, ctx| {
            doc.add_provider(
                "local".into(),
                ProviderConfig {
                    kind: ProviderKind::Local {
                        allow_aws_credential_chain: false,
                        broker: None,
                    },
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: S3Source::LocalCredentials,
                    },
                },
                doc.revision(),
                1,
                ctx,
            )?;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("local").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(
                ProfileData {
                    name: "actual-local".into(),
                    provider: "local".into(),
                    username: "tester".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new("test-local-key".into())),
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expected,
                1,
                ctx,
            )?;
            doc.activate("actual-local", doc.revision(), ctx)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert!(result.failure().is_none());
    let ScopeCapture { live, record } = auth.installation();
    assert!(live.binding().unwrap() == *record);
}

struct Fixture {
    auth: Arc<DaemonAuth>,
    paths: InstallationPaths,
    projects: Arc<Vec<(ProjectPaths, ProjectId, VersionContext)>>,
    raw: String,
    _directory: tempfile::TempDir,
}
impl Fixture {
    async fn imported(
        &self,
        id: &str,
        producer: usize,
        rows: Option<&[(&str, Option<&str>)]>,
    ) -> graphit_hub::SourceRef {
        use graphit_core::native_path::NativePath;
        use graphit_hub::{
            ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
            registry::{Installation, InstalledArtifact, InstalledBackend, InstalledPayload},
        };
        let registry = self.registry();
        let limits = registry.limits().projection();
        let root = self._directory.path().join("imported").join(id);
        let source = SourceRef::local_link(
            NativePath::new(root).unwrap(),
            self.projects[producer].1,
            ArtifactType::Ast,
            ArtifactId::new(id, limits).unwrap(),
            ConcreteVersion::new("release-1", limits).unwrap(),
            limits,
        )
        .unwrap();
        fs::create_dir_all(source.directory().as_path()).unwrap();
        if let Some(rows) = rows {
            let columns: Vec<ArrayRef> = vec![
                Arc::new(StringArray::from(
                    rows.iter().map(|r| r.0).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter().map(|r| r.1).collect::<Vec<_>>(),
                )),
            ];
            let batch = RecordBatch::try_new(source_schema(), columns).unwrap();
            LanceModuleStore::create(
                source
                    .directory()
                    .as_path()
                    .join("source.lance")
                    .to_str()
                    .unwrap(),
                source_schema(),
                vec![batch],
            )
            .await
            .unwrap();
        }
        registry
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .put_metadata(InstalledArtifact {
                consumer: graphit_core::artifact_store::PublicationSlot::Scope,
                artifact_type: ArtifactType::Ast,
                id: source.id().clone(),
                installations: vec![Installation {
                    target: InstallationTarget::GlobalNative,
                    original_source: source.clone(),
                    alias: None,
                    entry_name: None,
                    display: None,
                    backend: InstalledBackend::Native,
                    payload: InstalledPayload::Original {
                        selection_binding: None,
                    },
                }],
            })
            .unwrap();
        source
    }
    fn new(local: bool) -> Self {
        Self::with_permission(local, true)
    }
    fn with_permission(local: bool, read: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let trusted = ["first", "second"].map(|name| {
            let root = directory.path().join(name);
            fs::create_dir(&root).unwrap();
            ProjectPaths::at(&root, &paths).unwrap()
        });
        let auth = Arc::new(DaemonAuth::start(&paths, &trusted, policy(read), &ctx).unwrap());
        let raw = {
            let checked = auth.runtime().check(ctx.snapshot()).unwrap();
            String::from_utf8(checked.secret(ctx.snapshot()).unwrap().to_vec()).unwrap()
        };
        if local {
            publish_local(&auth, &paths, &ctx);
        }
        let mut projects = Vec::new();
        for (name, project) in ["first", "second"].into_iter().zip(trusted) {
            let root = project.root();
            let identity = ProjectIdentityStore::at(&project)
                .writer(Duration::from_secs(5), &cancel)
                .unwrap()
                .ensure(&ProjectMetadata::new(name).unwrap())
                .unwrap();
            let checkout = GitCheckout::inspect(root, Duration::from_secs(5), &cancel).unwrap();
            // Fixture-only setup observes actual checkout once. Source reads use retained facts.
            for explicit in ["", "feature"] {
                let version =
                    VersionContext::select(&project, checkout.as_checkout(), explicit, None, "")
                        .unwrap();
                projects.push((project.clone(), identity.id(), version));
            }
        }
        Self {
            auth,
            paths,
            projects: Arc::new(projects),
            raw,
            _directory: directory,
        }
    }
    fn registry(&self) -> HubRegistry {
        let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
            self.auth.installation().live.owner()
        else {
            unreachable!()
        };
        let config = ConfigSnapshot::new(self.auth.installation().live.brand(), []).unwrap();
        HubRegistry::global(
            &self.paths,
            installation_id,
            HubResourcePolicy::resolve(&config).unwrap().hub(),
        )
        .unwrap()
    }
    fn source_policy(&self, timeout: Duration) -> SourcePolicy {
        let limits = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
        let config = ConfigSnapshot::new(self.paths.brand(), []).unwrap();
        let original = graphit_core::file_io::SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        SourcePolicy {
            graph_query: graphit_ast::icebug_query::QueryOptions {
                buffer_pool_bytes: 128 * 1024 * 1024,
                query_threads: 2,
                max_rows: 64,
                max_bytes: 65536,
                max_depth: 32,
            },
            installation: self.paths.clone(),
            native_install: graphit_app::native_install_http::NativeInstallPolicy::capture(
                &config, &original,
            )
            .unwrap(),
            content: ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 32,
                max_nodes: 4096,
                max_scalar_bytes: 4096,
                max_collection_entries: 64,
            },
            timeout,
            scope: scopes(),
            file: SnapshotPolicy::new(
                CopyPolicy::RejectFileLinks,
                CopyLimits::new(1048576, 128, 65536, 8).unwrap(),
                RetainedFileSet::new([], limits).unwrap(),
                limits,
            )
            .unwrap(),
            read: SourceReadLimits::new(65536, 4096, 1048576).unwrap(),
            path: limits,
            index: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
            binding: NativeBinding::KiroV3,
            registry: self.registry(),
        }
    }
    fn corpus(&self, project: usize) -> PathBuf {
        self.paths
            .root()
            .join("ast")
            .join(self.projects[project].1.to_string())
            .join("source.lance")
    }
    async fn publish(&self, project: usize, rows: &[(&str, Option<&str>)]) -> LanceModuleStore {
        let path = self.corpus(project);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(
                rows.iter().map(|r| r.0).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                rows.iter().map(|r| r.1).collect::<Vec<_>>(),
            )),
        ];
        let batch = RecordBatch::try_new(source_schema(), columns).unwrap();
        LanceModuleStore::create(path.to_str().unwrap(), source_schema(), vec![batch])
            .await
            .unwrap()
    }
    fn file(&self, project: usize, path: &str, content: &str) {
        fs::write(self.projects[project].0.root().join(path), content).unwrap();
    }
    fn server(&self, timeout: Duration) -> SourceServer {
        SourceServer::new(
            Arc::clone(&self.auth),
            Arc::clone(&self.projects),
            0,
            self.source_policy(timeout),
            CancellationToken::new(),
        )
        .unwrap()
    }
    async fn start(&self) -> Endpoint {
        let listener = tcp_listener::reserve(
            Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &Cancellation::default(),
        )
        .unwrap();
        let uri = format!("http://{}/mcp", listener.address());
        let handler = self.server(Duration::from_secs(30));
        let running = handler.clone();
        let task = tokio::spawn(async move { SourceServer::serve(&running, listener).await });
        Endpoint { handler, task, uri }
    }
}
struct Endpoint {
    handler: SourceServer,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
    uri: String,
}
impl Endpoint {
    async fn client(
        &self,
        credential: Option<&str>,
        origin: Option<&str>,
    ) -> RunningService<RoleClient, ()> {
        let mut headers = HashMap::new();
        if let Some(raw) = credential {
            let mut value = HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        if let Some(origin) = origin {
            headers.insert(
                HeaderName::from_static("x-graphit-auth-origin"),
                HeaderValue::from_str(origin).unwrap(),
            );
        }
        let config =
            StreamableHttpClientTransportConfig::with_uri(self.uri.clone()).custom_headers(headers);
        ().serve(StreamableHttpClientTransport::with_client(
            reqwest::Client::new(),
            config,
        ))
        .await
        .unwrap()
    }
    async fn stop(self) {
        self.handler.shutdown().await.unwrap();
        self.task.await.unwrap().unwrap();
    }
}
async fn call(client: &RunningService<RoleClient, ()>, args: Value) -> CallToolResult {
    client
        .call_tool(
            CallToolRequestParams::new("graphit_ast_source")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap()
}
fn text(result: &CallToolResult) -> &str {
    result.content[0].as_text().unwrap().text.as_str()
}
fn success(result: CallToolResult, expected: &str) {
    assert_eq!(result.is_error, Some(false));
    assert_eq!(text(&result), expected);
}
fn error(result: CallToolResult, category: &str) {
    assert_eq!(result.is_error, Some(true));
    assert!(text(&result).contains(category), "{}", text(&result));
}

#[tokio::test]
async fn real_sdk_lists_schema_and_reads_raw_files_with_exact_selectors() -> TestResult {
    let f = Fixture::new(true);
    let raw = "\u{feff}α\r\nβ\0\nlast";
    f.file(0, "unindexed.unknown", raw);
    f.file(0, "empty", "");
    let before = fs::read(f.projects[0].0.identity_file())?;
    let e = f.start().await;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    let tools = client.list_tools(Default::default()).await?;
    assert_eq!(tools.tools.len(), 1);
    assert_eq!(tools.tools[0].name, "graphit_ast_source");
    assert_eq!(
        tools.tools[0].input_schema.get("required"),
        Some(&json!(["path"]))
    );
    success(
        call(&client, json!({"path":"unindexed.unknown"})).await,
        raw,
    );
    success(call(&client, json!({"path":"empty"})).await, "");
    let expected = graphit_core::text_slice::apply(
        raw,
        &graphit_core::text_slice::Request {
            head: 2,
            line_numbers: true,
            ..Default::default()
        },
    )
    .unwrap()
    .source;
    success(
        call(
            &client,
            json!({"path":"unindexed.unknown","head":2,"line_numbers":true}),
        )
        .await,
        &expected,
    );
    let expected = graphit_core::text_slice::apply(
        raw,
        &graphit_core::text_slice::Request {
            pattern: "β".into(),
            before: 1,
            after: 1,
            ..Default::default()
        },
    )
    .unwrap()
    .source;
    success(
        call(
            &client,
            json!({"path":"unindexed.unknown","pattern":"β","before":1,"after":1}),
        )
        .await,
        &expected,
    );
    success(
        call(
            &client,
            json!({"path":"unindexed.unknown","pattern":"no matches"}),
        )
        .await,
        "",
    );
    assert_eq!(fs::read(f.projects[0].0.identity_file())?, before);
    assert!(!f.corpus(0).exists());
    client.cancel().await?;
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn actual_lance_available_empty_and_raw_win_null_and_missing_fall_back() -> TestResult {
    let f = Fixture::new(true);
    let raw = "\u{feff}α\r\n\0🦀";
    fs::create_dir(f.projects[0].0.root().join("empty"))?;
    f.file(0, "raw", "physical differs");
    f.file(0, "null", "physical null");
    f.file(0, "missing", "physical missing");
    let _store = f
        .publish(
            0,
            &[("empty", Some("")), ("raw", Some(raw)), ("null", None)],
        )
        .await;
    let e = f.start().await;
    for (credential, origin) in [(&*f.raw, "runtime"), ("test-local-key", "local")] {
        let client = e.client(Some(credential), Some(origin)).await;
        for (path, expected) in [
            ("empty", ""),
            ("raw", raw),
            ("null", "physical null"),
            ("missing", "physical missing"),
        ] {
            success(call(&client, json!({"path":path})).await, expected);
        }
        client.cancel().await?;
    }
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn selected_real_project_and_named_version_are_independent_of_server_cwd() -> TestResult {
    let f = Fixture::new(false);
    f.file(0, "same", "first physical");
    f.file(2, "same", "second physical");
    let _store = f.publish(0, &[("same", Some("first indexed"))]).await;
    // The actual producer creates the retained named branch once in setup.
    let branch = graphit_data::lance_branch_store::LanceBranchStore::open(
        f.corpus(0).to_str().unwrap(),
        source_schema(),
        None,
    )
    .await?;
    let encoded = f.projects[1].2.lance_branch();
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(vec!["same"])),
        Arc::new(StringArray::from(vec![Some("named indexed")])),
    ];
    let batch = RecordBatch::try_new(source_schema(), columns)?;
    branch
        .capture_target(Some(&encoded))
        .await?
        .materialize()
        .await?
        .prepare_merge(vec![batch], vec!["path".into()])
        .await?
        .commit()
        .await?;
    let e = f.start().await;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    success(call(&client, json!({"path":"same"})).await, "first indexed");
    success(call(&client,json!({"path":"same","project_dir":f.projects[2].0.root(),"project_version":f.projects[2].2.version()})).await,"second physical");
    success(
        call(&client, json!({"path":"same","project_version":"feature"})).await,
        "named indexed",
    );
    success(
        call(
            &client,
            json!({"path":"same","project_dir":f.projects[0].0.root()}),
        )
        .await,
        "first indexed",
    );
    error(
        call(&client, json!({"path":"same","project_version":"unknown"})).await,
        "unavailable",
    );
    client.cancel().await?;
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn per_call_credentials_fail_closed_and_current_local_replacement_is_observed() -> TestResult
{
    let f = Fixture::new(true);
    f.file(0, "secret", "payload must not leak");
    let e = f.start().await;
    for (credential, origin, category) in [
        (None, Some("runtime"), "credential"),
        (
            Some("wrong-sensitive-token"),
            Some("runtime"),
            "authentication",
        ),
        (Some(&*f.raw), None, "origin"),
        (Some(&*f.raw), Some("managed"), "origin"),
    ] {
        let client = e.client(credential, origin).await;
        let result = call(&client, json!({"path":"secret"})).await;
        assert!(!text(&result).contains("payload must not leak"));
        assert!(!text(&result).contains("wrong-sensitive-token"));
        error(result, category);
        client.cancel().await?;
    }
    let client = e.client(Some("test-local-key"), Some("local")).await;
    success(
        call(&client, json!({"path":"secret"})).await,
        "payload must not leak",
    );
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    // Replace actual Local profile data in the native document; no fixture grant or cached caller.
    let id = InstallationIdentityStore::new(&f.paths)?
        .load()?
        .unit_id()
        .expect("private installation has an identity")
        .clone();
    let scope = CapturedPublicationScope::installation(
        &f.paths,
        id,
        [f.paths.root().to_owned()],
        scopes(),
        ctx.snapshot(),
    )?;
    let store = AuthStore::new(
        AuthDocumentStore::capture(scope, documents(), ctx.snapshot())?,
        profiles(),
        &ctx,
    )?;
    let before = store.load(&ctx)?;
    let result = store.mutate(before, &ctx, |doc, ctx| {
        let profile = doc.profile("actual-local").unwrap();
        let expected = ProfileExpectation {
            document_revision: doc.revision(),
            provider_revision: doc.provider("local").unwrap().revision(),
            profile_revision: Some(profile.revision()),
        };
        doc.replace_profile_data(
            ProfileData {
                name: "actual-local".into(),
                provider: "local".into(),
                username: "tester".into(),
                organization: None,
                teams: vec![],
                credentials: ProfileCredentials::Local(LocalCredentials {
                    anonymous: false,
                    mcp_key: Some(Secret::new("replacement-local-key".into())),
                    broker_key: None,
                    embedding_api_key: None,
                    rerank_api_key: None,
                    s3: None,
                }),
            },
            expected,
            1,
            ctx,
        )?;
        Ok(())
    })?;
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    error(
        call(&client, json!({"path":"secret"})).await,
        "authentication",
    );
    client.cancel().await?;
    let replacement = e.client(Some("replacement-local-key"), Some("local")).await;
    success(
        call(&replacement, json!({"path":"secret"})).await,
        "payload must not leak",
    );
    replacement.cancel().await?;
    let runtime = e.client(Some(&f.raw), Some("runtime")).await;
    let removed = f.auth.remove_runtime(ctx.snapshot());
    assert!(removed.failure().is_none());
    error(
        call(&runtime, json!({"path":"secret"})).await,
        "authentication",
    );
    runtime.cancel().await?;
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn strict_schema_paths_and_unavailable_routes_produce_typed_errors() -> TestResult {
    let f = Fixture::new(false);
    f.file(0, "raw", "real");
    let e = f.start().await;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    for args in [
        json!({}),
        json!({"path":null}),
        json!({"path":"raw","head":null}),
        json!({"path":"raw","head":true}),
        json!({"path":"raw","head":1.5}),
        json!({"path":"raw","extra":1}),
    ] {
        error(call(&client, args).await, "arguments");
    }
    for path in ["", "../raw", "/raw", "raw/../raw", "./raw", "raw\0"] {
        error(call(&client, json!({"path":path})).await, "relative");
    }
    error(
        call(&client, json!({"path":"raw","entity":"x"})).await,
        "entity",
    );
    error(
        call(&client, json!({"path":"raw","context":"foreign"})).await,
        "context",
    );
    error(
        call(&client, json!({"path":"raw","project_version":""})).await,
        "invalid",
    );
    error(
        call(&client, json!({"path":"raw","head":-1})).await,
        "inspection",
    );
    let unknown = client
        .call_tool(CallToolRequestParams::new("unknown"))
        .await
        .unwrap_err();
    assert!(
        unknown.to_string().contains("unknown tool") || unknown.to_string().contains("not found")
    );
    client.cancel().await?;
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn index_or_physical_failures_never_turn_into_successful_fallback() -> TestResult {
    let f = Fixture::new(false);
    f.file(0, "raw", "disk content must not be fallback");
    let path = f.corpus(0);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, "invalid Lance")?;
    let e = f.start().await;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    let result = call(&client, json!({"path":"raw"})).await;
    assert!(!text(&result).contains("disk content"));
    error(result, "index");
    client.cancel().await?;
    e.stop().await;
    let f = Fixture::new(false);
    #[cfg(unix)]
    std::os::unix::fs::symlink("outside", f.projects[0].0.root().join("linked"))?;
    let e = f.start().await;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    #[cfg(unix)]
    error(call(&client, json!({"path":"linked"})).await, "operation");
    error(call(&client, json!({"path":"missing"})).await, "operation");
    client.cancel().await?;
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn missing_read_ast_permission_and_wrong_index_schema_refuse_physical_payload() -> TestResult
{
    let f = Fixture::with_permission(false, false);
    f.file(0, "raw", "forbidden physical payload");
    let e = f.start().await;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    error(call(&client, json!({"path":"raw"})).await, "permission");
    client.cancel().await?;
    e.stop().await;
    let f = Fixture::new(false);
    f.file(0, "raw", "forbidden fallback payload");
    let column: ArrayRef = Arc::new(StringArray::from(vec!["raw"]));
    let batch = RecordBatch::try_from_iter([("path", column)])?;
    let path = f.corpus(0);
    fs::create_dir_all(path.parent().unwrap())?;
    let _store =
        LanceModuleStore::create(path.to_str().unwrap(), batch.schema(), vec![batch]).await?;
    let e = f.start().await;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    let result = call(&client, json!({"path":"raw"})).await;
    assert!(!text(&result).contains("forbidden fallback payload"));
    error(result, "index");
    client.cancel().await?;
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn imported_context_namespaces_raw_and_current_checkout_independence() -> TestResult {
    let f = Fixture::new(true);
    for (id, text) in [("vendorA/context", "A raw"), ("vendorB/context", "B raw")] {
        f.imported(id, 0, Some(&[("same", Some(text)), ("empty", Some(""))]))
            .await;
    }
    let e = f.start().await;
    // Only after DaemonAuth and Runtime tuples are retained: no project capture is needed for raw.
    fs::remove_dir_all(f.projects[0].0.root())?;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    for (id, text) in [("vendorA/context", "A raw"), ("vendorB/context", "B raw")] {
        success(
            call(
                &client,
                json!({"path":"same","context":format!("{id}@release-1")}),
            )
            .await,
            text,
        );
        success(
            call(
                &client,
                json!({"path":"empty","context":format!("{id}@release-1")}),
            )
            .await,
            "",
        );
    }
    client.cancel().await?;
    let local = e.client(Some("test-local-key"), Some("local")).await;
    success(
        call(
            &local,
            json!({"path":"same","context":"vendorA/context@release-1"}),
        )
        .await,
        "A raw",
    );
    local.cancel().await?;
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn imported_context_misses_require_explicit_unambiguous_runtime_fallback() -> TestResult {
    let f = Fixture::new(false);
    f.file(0, "same", "actual producer physical");
    f.imported(
        "vendor/context",
        0,
        Some(&[("same", None), ("empty", Some(""))]),
    )
    .await;
    f.imported("vendor/absent", 0, None).await;
    f.imported("vendor/foreign", 2, None).await;
    let e = f.start().await;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    let root = f.projects[0].0.root().to_str().unwrap();
    for context in ["vendor/context@release-1", "vendor/absent@release-1"] {
        error(
            call(&client, json!({"path":"same","context":context})).await,
            "text unavailable",
        );
        error(
            call(
                &client,
                json!({"path":"same","context":context,"project_dir":root}),
            )
            .await,
            "ambiguous",
        );
        error(
            call(
                &client,
                json!({"path":"same","context":context,"project_version":"feature"}),
            )
            .await,
            "explicit project_dir",
        );
        success(call(&client,json!({"path":"same","context":context,"project_dir":root,"project_version":"feature"})).await,"actual producer physical");
    }
    success(call(&client,json!({"path":"empty","context":"vendor/context@release-1","project_dir":root,"project_version":"feature"})).await,"");
    error(call(&client,json!({"path":"same","context":"vendor/foreign@release-1","project_dir":root,"project_version":"feature"})).await,"project or path invalid");
    for context in [
        "",
        "vendor/context",
        "@release-1",
        "vendor/context@",
        "vendor/context@wrong",
    ] {
        error(
            call(&client, json!({"path":"same","context":context})).await,
            "imported",
        );
    }
    let bad = f.imported("vendor/broken", 0, None).await;
    fs::create_dir(bad.directory().as_path().join("source.lance"))?;
    error(call(&client,json!({"path":"same","context":"vendor/broken@release-1","project_dir":root,"project_version":"feature"})).await,"storage unavailable or invalid");
    let wrong = f.imported("vendor/schema", 0, None).await;
    let schema = Arc::new(source_schema().project(&[0])?);
    LanceModuleStore::create(
        wrong
            .directory()
            .as_path()
            .join("source.lance")
            .to_str()
            .unwrap(),
        Arc::clone(&schema),
        vec![RecordBatch::new_empty(schema)],
    )
    .await?;
    error(call(&client, json!({"path":"same","context":"vendor/schema@release-1","project_dir":root,"project_version":"feature"})).await,"storage unavailable or invalid");
    assert!(
        !f._directory
            .path()
            .join("imported/vendor/absent/source.lance")
            .exists()
    );
    client.cancel().await?;
    fs::write(f.registry().path(), "malformed registry")?;
    let malformed = e.client(Some(&f.raw), Some("runtime")).await;
    error(
        call(
            &malformed,
            json!({"path":"same","context":"vendor/context@release-1"}),
        )
        .await,
        "imported operation failed",
    );
    malformed.cancel().await?;
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn imported_auth_precedes_malformed_registry_and_corpus() -> TestResult {
    let f = Fixture::with_permission(true, false);
    f.imported("vendor/context", 0, None).await;
    fs::write(f.registry().path(), "malformed")?;
    let e = f.start().await;
    for (credential, origin) in [
        (&*f.raw, "runtime"),
        ("invalid-runtime", "runtime"),
        ("test-local-key", "local"),
        ("invalid-local", "local"),
    ] {
        let client = e.client(Some(credential), Some(origin)).await;
        error(
            call(
                &client,
                json!({"path":"same","context":"vendor/context@release-1"}),
            )
            .await,
            "authentication or permission",
        );
        client.cancel().await?;
    }
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn imported_fallback_omitted_version_uses_only_a_unique_retained_root() -> TestResult {
    let mut f = Fixture::new(false);
    f.projects = Arc::new(vec![f.projects[0].clone()]);
    f.file(0, "same", "unique retained producer");
    f.imported("vendor/unique", 0, None).await;
    let e = f.start().await;
    let client = e.client(Some(&f.raw), Some("runtime")).await;
    success(call(&client,json!({"path":"same","context":"vendor/unique@release-1","project_dir":f.projects[0].0.root()})).await,"unique retained producer");
    client.cancel().await?;
    e.stop().await;
    Ok(())
}

#[tokio::test]
async fn source_client_requires_deadline_before_credentials_or_network() {
    use graphit_app::ast_source::{SourceArguments, call_source_result};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("absent-installation");
    let installation =
        InstallationPaths::resolve(Brand::default(), temp.path(), None, Some(&root)).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let arguments: SourceArguments = serde_json::from_value(json!({"path":"README.md"})).unwrap();
    let limits = NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap();
    for cancelled in [false, true] {
        let cancellation = Cancellation::default();
        let original = RequestContext::service(
            ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 32,
                max_nodes: 4096,
                max_scalar_bytes: 4096,
                max_collection_entries: 64,
            },
            &cancellation,
        )
        .unwrap();
        if cancelled {
            cancellation.cancel();
        }
        let error = call_source_result(
            &installation,
            listener.local_addr().unwrap(),
            &reqwest::Client::new(),
            &arguments,
            limits,
            &original,
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.kind(),
            if cancelled {
                std::io::ErrorKind::Interrupted
            } else {
                std::io::ErrorKind::InvalidInput
            }
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(!root.exists());
    }
}
