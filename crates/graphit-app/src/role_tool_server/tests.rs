use super::*;
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, RoleDefinition,
    ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::{
        CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotPolicy,
        SourceReadLimits, StoreOwner,
    },
    auth_document::AuthDocumentLimits,
    brand::Brand,
    git_checkout::GitCheckout,
    installation_bootstrap::BootstrapLimits,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::InstallationPaths,
    runtime_key::RuntimeKeyLimits,
};
use graphit_daemon::auth_startup::StartupPolicy;
use graphit_data::knowledge_index::{KnowledgeIndexContentLimits, KnowledgeIndexWriteLimits};
use graphit_hub::{
    NativeBinding,
    native::{ContentLimits, Role},
    registry::HubRegistry,
};
use graphit_records::task_store::TaskStoreReadLimits;
use rmcp::{
    RoleClient,
    service::{ClientServiceExt, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::{
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    time::Duration,
};
use zeroize::Zeroizing;
fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn request(cancel: &Cancellation, content: ContentLimits) -> RequestContext<'_> {
    RequestContext::new(content, cancel, Instant::now() + Duration::from_secs(90)).unwrap()
}
fn fixture() -> (tempfile::TempDir, RoleToolServer, String) {
    fixture_with_launch(NativeBinding::KiroV3, false)
}
fn fixture_with_launch(
    binding: NativeBinding,
    launch: bool,
) -> (tempfile::TempDir, RoleToolServer, String) {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let content = ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    };
    let ctx =
        RequestContext::new(content, &cancel, Instant::now() + Duration::from_secs(120)).unwrap();
    let scope = ScopeLimits::new(16, 4096, 256).unwrap();
    let mut permissions: Vec<_> = [
        Permission::Read(Module::Task),
        Permission::WriteTask,
        Permission::WriteOwnTask,
        Permission::CoordinateTask,
        Permission::ManageAccessGrants,
        Permission::Delegate,
        Permission::InspectGit,
    ]
    .iter()
    .chain(&[
        Permission::Read(Module::Memory),
        Permission::Read(Module::Knowledge),
        Permission::Read(Module::Ast),
        Permission::Read(Module::Hub),
        Permission::WriteMemory,
        Permission::MutateModule(Module::Knowledge),
        Permission::MutateModule(Module::Ast),
    ])
    .copied()
    .collect();
    if launch {
        permissions.extend([
            Permission::Configure,
            Permission::External(graphit_auth::ExternalEffect::PublishedFilesystem),
            Permission::External(graphit_auth::ExternalEffect::NativeLaunch),
        ]);
    }
    let source = || SourceAuthorityPolicy {
        ordinary: &permissions,
        administrative: &permissions,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    let authority = IssuerAuthorityPolicy::new(
        source(),
        source(),
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
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
    .unwrap();
    let policy = StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope,
        document: AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
        profiles: ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        authority,
        ttl: TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
    };
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("raw"), "actual native payload").unwrap();
    let project = ProjectPaths::at(&root, &paths).unwrap();
    let peer_root = temp.path().join("peer-project");
    std::fs::create_dir(&peer_root).unwrap();
    let peer_paths = ProjectPaths::at(&peer_root, &paths).unwrap();
    let trusted_projects = vec![project.clone(), peer_paths.clone()];
    let auth = Arc::new(DaemonAuth::start(&paths, &trusted_projects, policy, &ctx).unwrap());
    let raw = {
        let key = auth.runtime().check(ctx.snapshot()).unwrap();
        String::from_utf8(key.secret(ctx.snapshot()).unwrap().to_vec()).unwrap()
    };
    let id = ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .ensure(&ProjectMetadata::new("project").unwrap())
        .unwrap()
        .id();
    let checkout = GitCheckout::inspect(&root, Duration::from_secs(5), &cancel).unwrap();
    let version = VersionContext::select(&project, checkout.as_checkout(), "", None, "").unwrap();
    let copy = CopyLimits::new(1048576, 128, 65536, 16).unwrap();
    let read = TaskStoreReadLimits {
        native: NativeDocumentFileLimits::new(1048576, 65536, 256, 8388608).unwrap(),
        batch_rows: NonZeroU32::new(16).unwrap(),
    };

    let config = ConfigSnapshot::new(paths.brand(), []).unwrap();
    let native_original = graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    );
    let native_install =
        crate::native_install_http::NativeInstallPolicy::capture(&config, &native_original)
            .unwrap();
    let hub = &native_install.hub;
    let retained = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
    let StoreOwner::Installation { installation_id } = auth.installation().live.owner() else {
        unreachable!()
    };
    let source = SourcePolicy {
        graph_query: graphit_ast::icebug_query::QueryOptions {
            buffer_pool_bytes: 128 * 1024 * 1024,
            query_threads: 2,
            max_rows: (hub.content()).max_collection_entries,
            max_bytes: (hub.content()).max_output_bytes,
            max_depth: (hub.content()).max_depth,
        },
        installation: paths.clone(),
        content: hub.content(),
        timeout: native_install.timeout,
        scope,
        file: SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            copy,
            RetainedFileSet::new([], retained).unwrap(),
            retained,
        )
        .unwrap(),
        read: SourceReadLimits::new(65536, 4096, 1048576).unwrap(),
        path: retained,
        index: read.native,
        binding,
        registry: HubRegistry::global(&paths, installation_id, hub.hub()).unwrap(),
        native_install,
    };
    let content = source.content;
    let artifacts = source.native_install.hub.artifacts();
    let memory_read = MemoryStoreReadLimits {
        max_query_bytes: n(4 * 1048576),
        batch_rows: NonZeroU32::new(1).unwrap(),
        native: read.native,
    };
    let knowledge = KnowledgeIndexReadLimits {
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: n(256),
        },
        native: read.native,
        batch_rows: NonZeroU32::new(1).unwrap(),
    };
    let policy = RoleToolPolicy {
        source,
        config,
        memory_read,
        memory_write: MemoryStoreWriteLimits {
            read: memory_read,
            artifacts,
            max_input_bytes: n(1048576),
            max_row_bytes: n(2 * 1048576),
        },
        task_max_query_bytes: n(4 * 1048576),
        knowledge,
        knowledge_heads: KnowledgeHeadReadLimits {
            max_slug_bytes: n(256),
            native: read.native,
            batch_rows: NonZeroU32::new(1).unwrap(),
        },
        document_source: DocumentSourceLimits {
            max_input_bytes: n(4 * 1048576),
            max_targets: n(128),
            max_virtual_bytes: n(4 * 1048576),
            max_virtual_lines: n(65536),
            max_output_bytes: n(content.max_output_bytes),
            max_matches: n(65536),
        },
        instructions: InstructionLimits {
            max_templates: n(128),
            max_template_bytes: n(4 * 1048576),
            max_values: n(1024),
            max_variable_bytes: n(4 * 1048576),
            max_output_bytes: n(content.max_output_bytes),
            fuel: NonZeroU64::new(100000).unwrap(),
            recursion: n(64),
        },
    };
    let peer_id = ProjectIdentityStore::at(&peer_paths)
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .ensure(&ProjectMetadata::new("peer-project").unwrap())
        .unwrap()
        .id();
    let peer_version = VersionContext::select(
        &peer_paths,
        graphit_core::version_context::Checkout::NonGit,
        "main",
        None,
        "",
    )
    .unwrap();
    let server = RoleToolServer::new(
        auth,
        Arc::new(vec![
            (project, id, version),
            (peer_paths, peer_id, peer_version),
        ]),
        0,
        policy,
        CancellationToken::new(),
    )
    .unwrap();
    (temp, server, raw)
}
fn project_snapshot(
    server: &RoleToolServer,
) -> Arc<crate::live_project_selections::LiveProjectSnapshot> {
    server.0.live_projects.snapshot().unwrap()
}

fn captured_module_settings(
    server: &RoleToolServer,
    policy: &crate::source_server::project_hook::HookPolicy,
    original: &RequestContext<'_>,
) -> Arc<[graphit_core::module_settings::ModuleSettings]> {
    let snapshot = project_snapshot(server);
    snapshot
        .projects
        .iter()
        .map(|(paths, _, _)| {
            policy
                .selected_modules(
                    &server.0.policy.source.installation,
                    paths,
                    original.snapshot(),
                )
                .unwrap()
        })
        .collect::<Vec<_>>()
        .into()
}

fn scope<'a>(
    server: &'a RoleToolServer,
    project: &'a CapturedPublicationScope,
    record: &'a graphit_core::artifact_store::ScopeBindingRecord,
    version: &'a VersionContext,
) -> ScopeInput<'a> {
    ScopeInput {
        installation: server.0.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: project,
                record,
            },
            version,
        }),
        providers: &[],
        binding: server.0.task.binding,
        global_version: "",
    }
}
fn credential(
    server: &RoleToolServer,
    raw: &str,
    role: Role,
    permissions: &[Permission],
    lifetime: Duration,
) -> (
    graphit_auth::AdminSessionHandle,
    graphit_auth::RunHandle,
    RoleCredentialReference,
    graphit_auth::RoleBearer,
) {
    credential_at(server, raw, 0, role, permissions, lifetime)
}
fn credential_at(
    server: &RoleToolServer,
    raw: &str,
    index: usize,
    role: Role,
    permissions: &[Permission],
    lifetime: Duration,
) -> (
    graphit_auth::AdminSessionHandle,
    graphit_auth::RunHandle,
    RoleCredentialReference,
    graphit_auth::RoleBearer,
) {
    let definition = RoleDefinition::tool_owned(
        role,
        project_snapshot(server).projects[index].0.brand(),
        server.0.task.binding,
        permissions,
        [37; 32],
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
    )
    .unwrap();
    credential_with_definition(server, raw, index, definition, permissions, lifetime)
}
fn credential_with_definition(
    server: &RoleToolServer,
    raw: &str,
    index: usize,
    definition: RoleDefinition,
    permissions: &[Permission],
    lifetime: Duration,
) -> (
    graphit_auth::AdminSessionHandle,
    graphit_auth::RunHandle,
    RoleCredentialReference,
    graphit_auth::RoleBearer,
) {
    credential_with_definition_at_binding(
        server,
        raw,
        index,
        definition,
        permissions,
        lifetime,
        server.0.task.binding,
    )
}
fn credential_with_definition_at_binding(
    server: &RoleToolServer,
    raw: &str,
    index: usize,
    definition: RoleDefinition,
    permissions: &[Permission],
    lifetime: Duration,
    binding: NativeBinding,
) -> (
    graphit_auth::AdminSessionHandle,
    graphit_auth::RunHandle,
    RoleCredentialReference,
    graphit_auth::RoleBearer,
) {
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let reference = definition.reference().clone();
    match reference.source() {
        graphit_hub::native::RoleSource::ToolOwned(_) => {
            issuer
                .configure_tool_owned_definition(&parent, definition)
                .unwrap();
        }
        graphit_hub::native::RoleSource::HubArtifact(_) => {
            issuer.register_definition(&parent, definition).unwrap();
        }
    }
    let (paths, id, _) = &project_snapshot(server).projects[index];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.task.scope, original.snapshot())
            .unwrap();
    let record = project.binding().unwrap();
    let input = ScopeInput {
        installation: server.0.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &record,
            },
            version: &project_snapshot(server).projects[index].2,
        }),
        providers: &[],
        binding,
        global_version: "",
    };
    let run = issuer.start_run(&parent, &input, lifetime).unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, permissions, &input)
        .unwrap();
    (session, run, reference, bearer)
}
fn role_credential(
    reference: &RoleCredentialReference,
    bearer: &graphit_auth::RoleBearer,
    content: ContentLimits,
) -> Credential {
    let cancel = Cancellation::default();
    let original = request(&cancel, content);
    Credential {
        role_binding: None,
        bearer: Zeroizing::new(bearer.expose_for_transport().to_owned()),
        origin: Origin::Role(Zeroizing::new(
            String::from_utf8(
                reference
                    .encode(content.max_input_bytes, &original)
                    .unwrap(),
            )
            .unwrap(),
        )),
    }
}
async fn connect(uri: &str, credential: Credential) -> RunningService<RoleClient, ()> {
    connect_at(uri, credential, None).await
}
async fn connect_at(
    uri: &str,
    credential: Credential,
    selection: Option<Value>,
) -> RunningService<RoleClient, ()> {
    connect_at_with_binding(uri, credential, selection, None).await
}
async fn connect_at_with_binding(
    uri: &str,
    credential: Credential,
    selection: Option<Value>,
    binding: Option<NativeBinding>,
) -> RunningService<RoleClient, ()> {
    let Origin::Role(reference) = credential.origin else {
        panic!("real role required");
    };
    let mut headers = std::collections::HashMap::new();
    let mut bearer =
        axum::http::HeaderValue::from_str(&format!("Bearer {}", credential.bearer.as_str()))
            .unwrap();
    bearer.set_sensitive(true);
    headers.insert(axum::http::header::AUTHORIZATION, bearer);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_static("role"),
    );
    let mut reference_header = axum::http::HeaderValue::from_str(&reference).unwrap();
    reference_header.set_sensitive(true);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-role-reference"),
        reference_header,
    );
    if let Some(binding) = binding {
        headers.insert(
            axum::http::HeaderName::from_static("x-graphit-role-binding"),
            axum::http::HeaderValue::from_static(binding.as_str()),
        );
    }
    if let Some(selection) = selection {
        headers.insert(
            axum::http::HeaderName::from_static(ROLE_SELECTION_HEADER),
            axum::http::HeaderValue::from_str(&serde_json::to_string(&selection).unwrap()).unwrap(),
        );
    }
    tokio::time::timeout(
        Duration::from_secs(10),
        ().serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::builder()
                    .no_proxy()
                    .connect_timeout(Duration::from_secs(5))
                    .read_timeout(Duration::from_secs(30))
                    .build()
                    .unwrap(),
                StreamableHttpClientTransportConfig::with_uri(uri.to_owned())
                    .custom_headers(headers),
            ),
            rmcp::service::ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
            },
        ),
    )
    .await
    .unwrap()
    .unwrap()
}
fn loopback_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(30))
        .build()
        .unwrap()
}
fn typed_value(result: &CallToolResult) -> Value {
    result
        .structured_content
        .clone()
        .expect("typed internal or explicit JSON response")
}
async fn call(
    client: &RunningService<RoleClient, ()>,
    name: &str,
    args: Value,
    expected_error: bool,
) -> Value {
    let mut arguments = args.as_object().unwrap().clone();
    arguments.insert("ai_optimized".into(), Value::Bool(false));
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        client.call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments)),
    )
    .await
    .expect("Role MCP tool response deadline")
    .unwrap();
    let value: Value = typed_value(&result);
    assert_eq!(
        result.is_error,
        Some(expected_error),
        "{name}: {}",
        value.get("code").unwrap_or(&Value::Null)
    );
    value
}
async fn serve(server: &RoleToolServer) -> (String, tokio::task::JoinHandle<io::Result<()>>) {
    let socket = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let uri = format!("http://{}/mcp", socket.address());
    let socket = socket.into_listener();
    socket.set_nonblocking(true).unwrap();
    let socket = tokio::net::TcpListener::from_std(socket).unwrap();
    let service = server.clone();
    let serving = tokio::spawn(async move {
        axum::serve(socket, service.router())
            .with_graceful_shutdown(service.0.stop.clone().cancelled_owned())
            .await?;
        service.shutdown().await
    });
    (uri, serving)
}
async fn seed_tasks(server: &RoleToolServer, raw: &str) -> Value {
    let runtime = || Credential {
        role_binding: None,
        bearer: Zeroizing::new(raw.to_owned()),
        origin: Origin::Runtime,
    };
    let args = |v: Value| serde_json::from_value::<Arguments>(v).unwrap();
    let session=server.0.tasks.call(args(json!({"input":{"idempotency_key":"role-session","title":"Role current corpus","description":"Native records","strategy":"SDK"}})),"create_session",true,runtime(),CancellationToken::new()).await.unwrap_or_else(|error_value|panic!("native seed failed: {}",serde_json::to_value(error_value).unwrap()["code"]));
    let session_id = session["result"]["id"].clone();
    let task=server.0.tasks.call(args(json!({"input":{"idempotency_key":"role-task","session_id":session_id,"title":"Nebula semantic delivery","description":"literal native lexical payload","task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"preserve same Original"},{"kind":"test","text":"exercise the actual native SDK"}]}})),"create_task",true,runtime(),CancellationToken::new()).await.unwrap_or_else(|error_value|panic!("native seed failed: {}",serde_json::to_value(error_value).unwrap()["code"]));
    crate::task_entry::tests::assert_known_result_delivery(
        &rmcp::model::CallToolResult::structured(task.clone()),
    );
    task["result"]["specification"]["id"].clone()
}
async fn seed_knowledge(server: &RoleToolServer, raw: &str) {
    use graphit_auth::ProjectWriteOperation;
    use graphit_data::{
        knowledge_document_projection as h, knowledge_index::publish_native_authored,
    };
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let (paths, id, _) = &project_snapshot(server).projects[0];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.task.scope, original.snapshot())
            .unwrap();
    let record = project.binding().unwrap();
    let selected_snapshot = project_snapshot(server);
    let scope = scope(server, &project, &record, &selected_snapshot.projects[0].2);
    let authority = ProjectRecordAuthority::new(issuer);
    let knowledge = authority
        .admit_authenticated_write(&caller, &original, scope, ProjectWriteOperation::Knowledge)
        .unwrap();
    let pages = [h::AuthoredPageFields {
        slug: Some("native-guide"),
        title: Some("Native guide"),
        body: Some(
            "# Original

Real retained page",
        ),
        kind: Some("page"),
        summary: Some("SDK page"),
        description: None,
        summary_truncated: false,
        source_path: "docs/native.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([1; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(256),
            }),
        },
        updated_at: None,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: &[],
    }];
    let prepared = h::prepare(
        original.snapshot(),
        &pages,
        &[],
        h::KnowledgeDocumentProjectionLimits {
            max_documents: n(128),
            max_observations: n(128),
            max_rows: n(1024),
            max_field_bytes: n(65536),
            max_text_bytes: n(4 * 1048576),
        },
    )
    .unwrap();
    let limits = KnowledgeIndexWriteLimits {
        content: server.0.policy.knowledge.content,
        native: server.0.policy.knowledge.native,
        artifacts: server.0.task.write.artifacts,
    };
    publish_native_authored(original.snapshot(), &knowledge, &prepared, limits)
        .await
        .unwrap();
}
#[tokio::test]
async fn actual_role_socket_catalogue_native_reads_writes_and_lexical_search() {
    let (_temp, server, raw) = fixture();
    let task_id = seed_tasks(&server, &raw).await;
    let (scout_session, scout_run, scout_ref, scout_bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[
            Permission::Read(Module::Task),
            Permission::Read(Module::Memory),
            Permission::Read(Module::Knowledge),
            Permission::Read(Module::Ast),
            Permission::Read(Module::Hub),
        ],
        Duration::from_secs(90),
    );
    let (_tracker_session, _tracker_run, tracker_ref, tracker_bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[
            Permission::Read(Module::Task),
            Permission::Read(Module::Knowledge),
            Permission::Read(Module::Ast),
        ],
        Duration::from_secs(90),
    );
    let (_scribe_session, _scribe_run, scribe_ref, scribe_bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[
            Permission::Read(Module::Task),
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
            Permission::WriteOwnTask,
        ],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let scout = connect(
        &uri,
        role_credential(&scout_ref, &scout_bearer, server.0.task.content),
    )
    .await;
    let tracker = connect(
        &uri,
        role_credential(&tracker_ref, &tracker_bearer, server.0.task.content),
    )
    .await;
    let scribe = connect(
        &uri,
        role_credential(&scribe_ref, &scribe_bearer, server.0.task.content),
    )
    .await;
    let scout_tools = scout.list_all_tools().await.unwrap();
    assert!(scout_tools.iter().any(|t| t.name == "graphit_module_skill"));
    assert!(
        !scout_tools
            .iter()
            .any(|t| t.name == "graphit_memory_insert")
    );
    let tracker_tools = tracker.list_all_tools().await.unwrap();
    assert!(
        !tracker_tools
            .iter()
            .any(|t| t.name.starts_with("graphit_memory_"))
    );
    let scribe_tools = scribe.list_all_tools().await.unwrap();
    assert!(
        scribe_tools
            .iter()
            .any(|t| t.name == "graphit_memory_insert")
    );
    assert!(!scribe_tools.iter().any(|t| t.name == "graphit_task_claim"));
    assert!(!scribe_tools.iter().any(|t| t.name == "graphit_ast_source"));
    let insert = json!({"input":{"idempotency_key":"role-memory","title":"Nebula decision","body":"Nebula literal
second line","classification":"decision","mandatory":true,"important":false,"tags":["native"]}});
    let denied = call(&scout, "graphit_memory_insert", insert.clone(), true).await;
    assert_eq!(denied["code"], "authentication_or_permission_denied");
    let memory = call(&scribe, "graphit_memory_insert", insert, false).await;
    let memory_id = memory["result"]["id"].clone();
    let read = json!({"input":{"id":memory_id}});
    let (one, two) = tokio::join!(
        call(&scout, "graphit_memory_get", read.clone(), false),
        call(&scribe, "graphit_memory_get", read.clone(), false)
    );
    assert_eq!(one["memory"]["id"], two["memory"]["id"]);
    assert_eq!(
        call(&tracker, "graphit_memory_get", read, true).await["code"],
        "authentication_or_permission_denied"
    );
    let source = call(
        &scout,
        "graphit_memory_source",
        json!({"input":{"id":memory_id,"selectors":{"head":1,"line_numbers":true}}}),
        false,
    )
    .await;
    assert!(
        source["selection"]["source"]
            .as_str()
            .unwrap()
            .contains("Nebula literal")
    );
    let changed=call(&scribe,"graphit_memory_update",json!({"input":{"id":memory_id,"expected_revision":1,"idempotency_key":"role-update","reason":"decided","body":"Nebula updated
line2"}}),false).await;
    assert_eq!(changed["result"]["revision"], 2);
    let history = call(
        &scout,
        "graphit_memory_history",
        json!({"input":{"id":memory_id}}),
        false,
    )
    .await;
    assert_eq!(history["revisions"].as_array().unwrap().len(), 1);
    let stale=call(&scribe,"graphit_memory_update",json!({"input":{"id":memory_id,"expected_revision":1,"idempotency_key":"role-stale","reason":"must fail","body":"should not publish"}}),true).await;
    assert_eq!(stale["code"], "revision_conflict");
    seed_knowledge(&server, &raw).await;
    let task_search = call(
        &scout,
        "graphit_task_search",
        json!({"input":{"text":"Nebula","filter":{},"page_size":8,"top_k":8,"cursor":null}}),
        false,
    )
    .await;
    assert_eq!(
        task_search["Published"]["results"][0]["task"]["id"],
        task_id
    );
    let memory_search = call(
        &scout,
        "graphit_memory_search",
        json!({"input":{"text":"Nebula","top_k":8}}),
        false,
    )
    .await;
    assert_eq!(memory_search["kind"], "published");
    assert_eq!(memory_search["hits"][0]["current"]["id"], memory_id);
    let page = call(
        &tracker,
        "graphit_wiki_source",
        json!({"input":{"slug":"native-guide"}}),
        false,
    )
    .await;
    assert_eq!(page["kind"], "found");
    assert!(
        page["selection"]["source"]
            .as_str()
            .unwrap()
            .contains("Real retained page")
    );
    let absent = call(
        &scout,
        "graphit_wiki_source",
        json!({"input":{"slug":"not-present"}}),
        false,
    )
    .await;
    assert_eq!(absent["kind"], "page_absent");
    let heads = call(&scout, "graphit_knowledge_list", json!({"input":{}}), false).await;
    assert_eq!(heads["heads"][0]["slug"], "native-guide");
    let schema = call(
        &tracker,
        "graphit_knowledge_schema",
        json!({"input":{}}),
        false,
    )
    .await;
    let columns = schema["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 18);
    assert_eq!(columns[16]["name"], "document_state");
    assert_eq!(columns[16]["nullable"], false);
    assert_eq!(columns[17]["name"], "reference_projection");
    assert_eq!(columns[17]["nullable"], true);
    let ast = call(
        &scout,
        "graphit_ast_source",
        json!({"input":{"path":"raw"}}),
        false,
    )
    .await;
    assert_eq!(ast["result"]["source"], "actual native payload");
    assert!(
        call(
            &scout,
            "graphit_task_claim",
            json!({"input":{"id":task_id,"lease_ms":1000}}),
            true
        )
        .await["code"]
            .as_str()
            .is_some()
    );
    // The coordinator's real fence never transfers to a delegated Role actor.
    let claimed = server
        .0
        .tasks
        .call(
            serde_json::from_value(json!({"input":{"id":task_id,"lease_ms":120000}})).unwrap(),
            "claim_task",
            true,
            Credential {
                role_binding: None,
                bearer: Zeroizing::new(raw.clone()),
                origin: Origin::Runtime,
            },
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|_| panic!("native coordinator claim failed"));
    let refused = call(&scribe, "graphit_task_comment_add", json!({"input":{
        "target":{"id":task_id,"claim":{"token":claimed["result"]["token"],"expected_revision":claimed["result"]["record"]["specification"]["revision"]}},
        "lease_ms":120000,"idempotency_key":"foreign-coordinator-fence","kind":"decision","body":"must not publish"
    }}), true).await;
    assert_ne!(refused["code"], "invalid_arguments");
    let unchanged = call(
        &scout,
        "graphit_task_get",
        json!({"input":{"id":task_id,"comments":{"page_size":100}}}),
        false,
    )
    .await;
    assert_eq!(
        unchanged["result"]["task"]["specification"]["revision"],
        claimed["result"]["record"]["specification"]["revision"]
    );
    assert_eq!(unchanged["result"]["comments"]["comments"], json!([]));
    assert!(unchanged["result"]["comments"]["next_cursor"].is_null());
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let caller = server
        .0
        .auth
        .issuer()
        .authenticate_runtime(&raw, &original)
        .unwrap();
    let parent = server
        .0
        .auth
        .issuer()
        .admin_parent(&caller, &scout_session)
        .unwrap();
    server
        .0
        .auth
        .issuer()
        .close_run(&parent, &scout_run)
        .unwrap();
    assert!(scout.list_all_tools().await.is_err());
    assert_eq!(
        call(&scout, "graphit_memory_list", json!({"input":{}}), true).await["code"],
        "authentication_or_permission_denied"
    );
    assert_eq!(
        call(
            &scribe,
            "graphit_memory_get",
            json!({"input":{"id":memory_id}}),
            false
        )
        .await["kind"],
        "found"
    );
    scout.cancel().await.unwrap();
    tracker.cancel().await.unwrap();
    scribe.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn module_skill_captured_precedence_exact_reference_and_denied_module() {
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[
            Permission::Read(Module::Task),
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
        ],
        Duration::from_secs(60),
    );
    let run = |v: Value| {
        server.call(
            "graphit_module_skill".into(),
            serde_json::from_value(v).unwrap(),
            role_credential(&reference, &bearer, server.0.task.content),
            CancellationToken::new(),
        )
    };
    let default = run(json!({"input":{"module":"task"}})).await;
    assert_eq!(default.is_error, Some(false));
    let value: Value = typed_value(&default);
    assert_eq!(value["origin"], "CompiledDefault");
    assert!(
        value["source"]
            .as_str()
            .unwrap()
            .contains("graphit_task_get")
    );
    let companion =
        run(json!({"input":{"module":"task","reference":"references/execution.md"}})).await;
    assert_eq!(companion.is_error, Some(false));
    assert_eq!(
        run(json!({"input":{"module":"task","reference":"../private"}}))
            .await
            .is_error,
        Some(true)
    );
    assert_eq!(
        run(json!({"input":{"module":"ast"}})).await.is_error,
        Some(true)
    );
    assert_eq!(
        run(json!({"input":{"module":"unknown"}})).await.is_error,
        Some(true)
    );
    assert_eq!(
        run(json!({"input":{"module":"task","hub":{"id":"vendor/task","version":"1"}}}))
            .await
            .is_error,
        Some(true)
    ); // Scribe has no ReadHub.
    let root = &project_snapshot(&server).projects[0].0;
    let project = root
        .root()
        .join(root.brand().directory())
        .join("rules/skills/task");
    let global = server
        .0
        .policy
        .source
        .installation
        .root()
        .join("rules/skills/task");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&global).unwrap();
    std::fs::write(
        global.join("entry.md"),
        "global {{ brand_display }} {{ mcp_tool_task_get }}",
    )
    .unwrap();
    std::fs::write(
        project.join("entry.md"),
        "project {{ brand_display }} {{ mcp_tool_task_get }}",
    )
    .unwrap();
    let selected = run(json!({"input":{"module":"task"}})).await;
    assert_eq!(selected.is_error, Some(false));
    let value: Value = typed_value(&selected);
    assert_eq!(value["origin"], "Project");
    assert_eq!(value["source"], "project graphit graphit_task_get");
    std::fs::remove_file(project.join("entry.md")).unwrap();
    let selected = run(json!({"input":{"module":"task"}})).await;
    let value: Value = typed_value(&selected);
    assert_eq!(value["origin"], "Global");
    assert_eq!(value["source"], "global graphit graphit_task_get");
    std::fs::remove_file(global.join("entry.md")).unwrap();
    let (_scout_session, _scout_run, scout_reference, scout_bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[
            Permission::Read(Module::Task),
            Permission::Read(Module::Hub),
        ],
        Duration::from_secs(60),
    );
    let hub_root = server
        .0
        .policy
        .source
        .installation
        .root()
        .join("skill-source");
    std::fs::create_dir_all(hub_root.join("skills/task")).unwrap();
    std::fs::write(hub_root.join("SKILL.md"), "# Actual published skill").unwrap();
    std::fs::write(
        hub_root.join("skills/task/entry.md"),
        "hub {{ brand_display }} {{ mcp_tool_task_get }}",
    )
    .unwrap();
    // The registry retains this logical source spelling; confinement is physical.
    let installation = server.0.policy.source.installation.root();
    #[cfg(unix)]
    let logical_source = {
        let alias = installation.parent().unwrap().join("skill-source-alias");
        std::os::unix::fs::symlink(installation, &alias).unwrap();
        alias.join("skill-source")
    };
    #[cfg(not(unix))]
    let logical_source = installation.join("skill-source");
    #[cfg(unix)]
    assert!(!logical_source.starts_with(installation));
    let hub = &server.0.policy.source.native_install.hub;
    let source = graphit_hub::SourceRef::local_link(
        graphit_core::native_path::NativePath::new(logical_source).unwrap(),
        project_snapshot(&server).projects[0].1,
        graphit_hub::ArtifactType::Skill,
        graphit_hub::ArtifactId::new("vendor/task", hub.projection()).unwrap(),
        graphit_hub::ConcreteVersion::new("v1", hub.projection()).unwrap(),
        hub.projection(),
    )
    .unwrap();
    let store = graphit_core::artifact_store::ArtifactStore::new(
        server.0.auth.installation().live.clone(),
        hub.artifacts(),
    );
    let coordinator = graphit_hub::publication::ScopedCoordinator::new(
        server.0.policy.source.registry.clone(),
        store,
        graphit_core::native_path::NativePath::new(
            server.0.policy.source.installation.root().to_owned(),
        )
        .unwrap(),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    coordinator
        .writer(&original)
        .unwrap()
        .publish(
            graphit_hub::ProjectionRequest {
                binding: server.0.task.binding,
                source: &source,
                alias: None,
                entry_name: None,
            },
            graphit_core::artifact_store::ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
            None,
        )
        .unwrap();
    let registry_before = std::fs::read(server.0.policy.source.registry.path()).unwrap();
    let selected = server
        .call(
            "graphit_module_skill".into(),
            serde_json::from_value(
                json!({"input":{"module":"task","hub":{"id":"vendor/task","version":"v1"}}}),
            )
            .unwrap(),
            role_credential(&scout_reference, &scout_bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(selected.is_error, Some(false));
    let value: Value = typed_value(&selected);
    assert_eq!(value["origin"], "Hub");
    assert_eq!(value["source"], "hub graphit graphit_task_get");
    assert_eq!(
        std::fs::read(server.0.policy.source.registry.path()).unwrap(),
        registry_before
    );
    std::fs::write(project.join("entry.md"), "{{ missing_live_secret }}").unwrap();
    assert_eq!(
        run(json!({"input":{"module":"task"}})).await.is_error,
        Some(true)
    );
    server.shutdown().await.unwrap();
}
#[tokio::test]
async fn role_scope_reference_expiry_and_no_runtime_fallback() {
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        Duration::from_secs(60),
    );
    let args = |v: Value| serde_json::from_value::<Arguments>(v).unwrap();
    let token = || CancellationToken::new();
    let runtime = Credential {
        role_binding: None,
        bearer: Zeroizing::new(raw.clone()),
        origin: Origin::Runtime,
    };
    let denied = server
        .call(
            "graphit_memory_list".into(),
            args(json!({"input":{}})),
            runtime,
            token(),
        )
        .await;
    assert_eq!(denied.is_error, Some(true));
    let wrong = RoleCredentialReference::new(Role::Scout, reference.source().clone(), [9; 32]);
    assert_eq!(
        server
            .call(
                "graphit_memory_list".into(),
                args(json!({"input":{}})),
                role_credential(&wrong, &bearer, server.0.task.content),
                token()
            )
            .await
            .is_error,
        Some(true)
    );
    let peer = graphit_core::native_path::NativePath::new(
        project_snapshot(&server).projects[1].0.root().to_owned(),
    )
    .unwrap();
    assert_eq!(
        server
            .call(
                "graphit_memory_list".into(),
                args(json!({"selection":{"project_dir":peer},"input":{}})),
                role_credential(&reference, &bearer, server.0.task.content),
                token()
            )
            .await
            .is_error,
        Some(true)
    );
    assert_eq!(
        server
            .call(
                "graphit_memory_list".into(),
                args(json!({"selection":{"project_version":"other"},"input":{}})),
                role_credential(&reference, &bearer, server.0.task.content),
                token()
            )
            .await
            .is_error,
        Some(true)
    );
    assert_eq!(
        server
            .call(
                "graphit_memory_list".into(),
                args(json!({"input":{"actor":"forged"}})),
                role_credential(&reference, &bearer, server.0.task.content),
                token()
            )
            .await
            .is_error,
        Some(true)
    );
    let (_peer_session, _peer_run, peer_reference, peer_bearer) = credential_at(
        &server,
        &raw,
        1,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        Duration::from_secs(60),
    );
    let (uri, serving) = serve(&server).await;
    let selected = json!({"project_dir":peer});
    let peer_client = connect_at(
        &uri,
        role_credential(&peer_reference, &peer_bearer, server.0.task.content),
        Some(selected),
    )
    .await;
    assert!(
        peer_client
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .any(|tool| tool.name == "graphit_memory_list")
    );
    let peer_list = call(
        &peer_client,
        "graphit_memory_list",
        json!({"input":{}}),
        false,
    )
    .await;
    assert_eq!(peer_list["kind"], "no_corpus");
    peer_client.cancel().await.unwrap();
    // Header selection only picks facts: it never widens this run to the daemon current PROJECT.
    let current_project = graphit_core::native_path::NativePath::new(
        project_snapshot(&server).projects[0].0.root().to_owned(),
    )
    .unwrap();
    let current_client = connect_at(
        &uri,
        role_credential(&peer_reference, &peer_bearer, server.0.task.content),
        Some(json!({"project_dir":current_project})),
    )
    .await;
    assert!(current_client.list_all_tools().await.is_err());
    current_client.cancel().await.unwrap();
    let (_short_session, _short_run, short_reference, short_bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        Duration::from_millis(100),
    );
    tokio::time::sleep(Duration::from_millis(110)).await;
    assert_eq!(
        server
            .call(
                "graphit_memory_list".into(),
                args(json!({"input":{}})),
                role_credential(&short_reference, &short_bearer, server.0.task.content),
                token()
            )
            .await
            .is_error,
        Some(true)
    );
    // Foreign or expired originals cannot pass the actual execute path.
    let expired = execute(
        &server.0,
        server.0.live_projects.snapshot().unwrap().projects[0].clone(),
        catalog::Operation::MemoryList,
        Some(json!({})),
        role_credential(&reference, &bearer, server.0.task.content),
        &Cancellation::default(),
        Instant::now() - Duration::from_secs(1),
    )
    .await;
    assert!(expired.is_err());
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
#[tokio::test]
async fn actual_role_cancel_eof_and_confirmed_memory_delivery() {
    use rmcp::model::{CallToolRequest, ClientRequest};
    let (_temp, server, raw) = fixture();
    let (owner, mut batches) = memory_rescan_owner(memory_context(&server, 0)).await;
    server
        .attach_memory_rescans(Arc::from([owner.rescan_handle()]))
        .unwrap();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteMemory],
        Duration::from_secs(60),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let (started, observed) = std::sync::mpsc::channel();
    let (release, waiting) = oneshot::channel();
    *server.0.gate.lock().unwrap() = Some((started, waiting));
    let params = || {
        CallToolRequestParams::new("graphit_memory_list").with_arguments(
            json!({"ai_optimized":false,"input":{}})
                .as_object()
                .unwrap()
                .clone(),
        )
    };
    let pending = client
        .send_cancellable_request(
            ClientRequest::CallToolRequest(CallToolRequest::new(params())),
            Default::default(),
        )
        .await
        .unwrap();
    let cancelled =
        tokio::task::spawn_blocking(move || observed.recv_timeout(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
    pending.cancel(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !cancelled.is_cancelled() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        call(&client, "graphit_memory_list", json!({"input":{}}), false).await["kind"],
        "no_corpus"
    );
    release.send(()).unwrap();
    let (published, observed) = std::sync::mpsc::channel();
    let (release, waiting) = oneshot::channel();
    *server.0.before_delivery.lock().unwrap() = Some((published, waiting));
    let created=client.call_tool(CallToolRequestParams::new("graphit_memory_insert").with_arguments(json!({"ai_optimized":false,"input":{"idempotency_key":"confirmed-role","title":"Confirmed","body":"real committed body","classification":"decision"}}).as_object().unwrap().clone()));
    let observe = tokio::task::spawn_blocking(move || {
        observed.recv_timeout(Duration::from_secs(10)).unwrap()
    });
    let cancellation = async {
        let original = observe.await.unwrap();
        expect_memory_rescan(&mut batches).await;
        original.cancel();
        release.send(()).unwrap();
    };
    let (result, ()) = tokio::join!(created, cancellation);
    let result = result.unwrap();
    assert_eq!(result.is_error, Some(true));
    let value: Value = typed_value(&result);
    assert_eq!(value["code"], "write_confirmed");
    let id = value["memory"]["id"].clone();
    assert_eq!(
        call(
            &client,
            "graphit_memory_get",
            json!({"input":{"id":id}}),
            false
        )
        .await["memory"]["body"],
        "real committed body"
    );
    let (started, observed) = std::sync::mpsc::channel();
    let (release, waiting) = oneshot::channel();
    *server.0.gate.lock().unwrap() = Some((started, waiting));
    let _pending = client
        .send_cancellable_request(
            ClientRequest::CallToolRequest(CallToolRequest::new(params())),
            Default::default(),
        )
        .await
        .unwrap();
    let eof =
        tokio::task::spawn_blocking(move || observed.recv_timeout(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
    client.cancel().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !eof.is_cancelled() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut waiter = Box::pin(server.shutdown());
    tokio::time::timeout(
        Duration::from_secs(5),
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(waiter.as_mut(), cx).is_pending());
            if server.0.jobs.try_lock().is_err() {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        }),
    )
    .await
    .unwrap();
    // Cancel shutdown only after it is actually joining the retained job.
    drop(waiter);
    release.send(()).unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
    owner.stop().await.unwrap();
}

async fn serve_run_proxy(
    server: &RoleToolServer,
) -> (
    std::net::SocketAddr,
    tokio::task::JoinHandle<io::Result<()>>,
) {
    let socket = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let address = socket.address();
    let socket = socket.into_listener();
    socket.set_nonblocking(true).unwrap();
    let socket = tokio::net::TcpListener::from_std(socket).unwrap();
    let owner = server.clone();
    let serving = tokio::spawn(async move {
        axum::serve(socket, axum::Router::new().nest("/roles", owner.router()))
            .with_graceful_shutdown(owner.0.stop.clone().cancelled_owned())
            .await?;
        owner.shutdown().await
    });
    (address, serving)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_proxy_stdio_keeps_real_role_grants_and_never_accepts_runtime() {
    use rmcp::ClientServiceExt;
    use secrecy::SecretString;
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[
            Permission::Read(Module::Ast),
            Permission::Read(Module::Task),
        ],
        Duration::from_secs(60),
    );
    let (address, serving) = serve_run_proxy(&server).await;
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let encoded = String::from_utf8(
        reference
            .encode(server.0.task.content.max_input_bytes, &original)
            .unwrap(),
    )
    .unwrap();
    let policy = (
        server.0.task.content,
        server.0.task.role_reference,
        server.0.task.timeout,
    );
    let proxy = crate::role_proxy::RoleProxy::new(
        address,
        loopback_client(),
        policy,
        (
            SecretString::from(bearer.expose_for_transport().to_owned()),
            &encoded,
            None,
            None,
        ),
        &original,
    )
    .await
    .unwrap();
    assert!(
        crate::role_proxy::RoleProxy::new(
            address,
            loopback_client(),
            policy,
            (SecretString::from(raw.clone()), &encoded, None, None),
            &original
        )
        .await
        .is_err()
    );
    assert!(
        crate::role_proxy::RoleProxy::new(
            address,
            loopback_client(),
            policy,
            (SecretString::from(String::new()), &encoded, None, None),
            &original
        )
        .await
        .is_err()
    );
    let (client_io, server_io) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(server_io);
    let owned = proxy.clone();
    let stdio = tokio::spawn(async move { owned.serve_stream(read, write).await });
    let client = ()
        .serve_with_lifecycle(
            client_io,
            rmcp::service::ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap();
    let tools = client.peer().list_all_tools().await.unwrap();
    assert!(tools.iter().any(|tool| tool.name == "graphit_ast_source"));
    assert!(tools.iter().any(|tool| tool.name == "graphit_task_get"));
    let answer = call(
        &client,
        "graphit_ast_source",
        json!({"input":{"path":"raw"}}),
        false,
    )
    .await;
    assert_eq!(answer["result"]["source"], "actual native payload");
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
    let parent = issuer.admin_parent(&caller, &_session).unwrap();
    issuer.close_run(&parent, &_run).unwrap();
    assert!(
        client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_ast_source").with_arguments(
                    json!({"ai_optimized":false,"input":{"path":"raw"}})
                        .as_object()
                        .unwrap()
                        .clone()
                )
            )
            .await
            .unwrap()
            .is_error
            .unwrap_or(false)
    );
    client.cancel().await.unwrap();
    stdio.await.unwrap().unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[cfg(unix)]
fn native_role_peer(temp: &std::path::Path, case: Value) -> std::path::PathBuf {
    let program = temp.join("role-peer");
    if !program.exists() {
        let source = std::env::var_os("GRAPHIT_TEST_APP_RUNTIME_FIXTURE")
            .expect("prepare the Cargo Runtime fixture through cargo xtask cargo test");
        std::fs::copy(source, &program).unwrap();
    }
    std::fs::write(temp.join("case.json"), serde_json::to_vec(&case).unwrap()).unwrap();
    program
}
#[cfg(unix)]
fn acp_role_child(
    temp: &std::path::Path,
    address: std::net::SocketAddr,
    phase: Option<std::net::SocketAddr>,
) -> std::path::PathBuf {
    native_role_peer(
        temp,
        json!({
            "fixture":"role-http", "binding":"kiro-v3", "role":"scout", "brand":"graphit",
            "endpoint":format!("http://{address}/roles/mcp"),
            "phase":phase.map(|address|address.to_string())
        }),
    )
}
#[cfg(unix)]
fn launch_input(server: &RoleToolServer) -> crate::managed_role_run::RunRequest {
    crate::managed_role_run::RunRequest {
        install: crate::managed_roles::InstallRequest {
            destination: crate::managed_roles::InstallDestination::Project,
            project_dir: Some(project_snapshot(server).projects[0].0.root().to_owned()),
            project_version: Some(project_snapshot(server).projects[0].2.version().to_owned()),
            binding: Some(server.0.task.binding),
            roles: vec![crate::managed_roles::ManagedRoleName::Scout],
            skills_only: false,
            rules: vec![],
            skill_aliases: std::collections::BTreeMap::new(),
            skill_bundles: Default::default(),
            module_skill: None,
        },
        role: crate::managed_roles::ManagedRoleName::Scout,
        model: None,
        prompt: "Read the actual native source".into(),
    }
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_owned_acp_v3_calls_actual_role_http_and_closes_without_secret_output() {
    {
        let binding = NativeBinding::KiroV3;
        let (temp, server, raw) = fixture_with_launch(binding, true);
        let (address, serving) = serve_run_proxy(&server).await;
        let program = acp_role_child(temp.path(), address, None);
        let cancel = Cancellation::default();
        let original = request(&cancel, server.0.task.content);
        let input = launch_input(&server);
        let context = crate::source_server::project_hook::HookPolicy::capture(
            &server.0.policy.source,
            &server.0.policy.config,
            &project_snapshot(&server).projects,
        )
        .unwrap();
        let modules = captured_module_settings(&server, &context, &original);
        let execution = crate::managed_role_run::execute_with_program(
            crate::managed_roles::InstallResources {
                auth: &server.0.auth,
                global_paths: None,
                projects: &project_snapshot(&server).projects,
                current: project_snapshot(&server).current.unwrap(),
                policy: &server.0.policy.source,
                modules: &modules,
                memory_read: context.memory_limits(),
            },
            &raw,
            &input,
            &original,
            &program,
        )
        .await;
        assert!(
            execution.cause.is_none(),
            "launch failed class: {:?}",
            execution.cause.as_ref().map(io::Error::kind)
        );
        assert!(
            execution.native.is_none(),
            "ACP child failed class: {:?}; public fixture phase: {:?}",
            execution
                .native
                .as_ref()
                .and_then(|failure| failure.downcast_ref::<graphit_ai::cli_kiro::KiroFailure>())
                .map(|failure| failure.reason()),
            std::fs::read_to_string(temp.path().join("acp-failure.json")).ok()
        );
        assert!(execution.outcome.run_closed);
        assert!(execution.residual.is_none());
        assert_eq!(
            execution.outcome.answer.as_deref(),
            Some("actual Role source [redacted]")
        );
        let profile = project_snapshot(&server).projects[0]
            .0
            .root()
            .join(".kiro/agents/graphit-scout.md");
        let content = std::fs::read_to_string(profile).unwrap();
        assert!(content.contains("--role"));
        assert!(!content.contains("GRAPHIT_ROLE_BEARER"));
        assert!(!content.contains(&raw));
        server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_owned_acp_cancel_closes_current_run_and_preserves_sibling() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (temp, server, raw) = fixture_with_launch(NativeBinding::KiroV3, true);
    let (session, sibling, reference, bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::Read(Module::Ast)],
        Duration::from_secs(90),
    );
    let (address, serving) = serve_run_proxy(&server).await;
    let phase = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let program = acp_role_child(temp.path(), address, Some(phase.local_addr().unwrap()));
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let input = launch_input(&server);
    let context = crate::source_server::project_hook::HookPolicy::capture(
        &server.0.policy.source,
        &server.0.policy.config,
        &project_snapshot(&server).projects,
    )
    .unwrap();
    let modules = captured_module_settings(&server, &context, &original);
    let selected_snapshot = project_snapshot(&server);
    let run = crate::managed_role_run::execute_with_program(
        crate::managed_roles::InstallResources {
            auth: &server.0.auth,
            global_paths: None,
            projects: &selected_snapshot.projects,
            current: selected_snapshot.current.unwrap(),
            policy: &server.0.policy.source,
            modules: &modules,
            memory_read: context.memory_limits(),
        },
        &raw,
        &input,
        &original,
        &program,
    );
    let stop = async {
        let observed = tokio::time::timeout(Duration::from_secs(10), async {
            let (mut phase, _) = phase.accept().await?;
            let mut byte = [0];
            phase.read_exact(&mut byte).await?;
            if byte != *b"R" {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            cancel.cancel();
            phase.write_all(b"C").await
        })
        .await;
        // Even a missing phase must cancel and join the actual operation.
        cancel.cancel();
        observed.map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
    };
    let (execution, phase) = tokio::join!(run, stop);
    if let Err(error) = phase {
        server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
        panic!("ACP child prompt phase failed: {error:?}");
    }
    assert!(execution.native.is_some());
    assert!(execution.outcome.run_closed);
    assert!(execution.residual.is_none());
    assert!(execution.outcome.answer.is_none());
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let project = CapturedPublicationScope::project(
        &project_snapshot(&server).projects[0].0,
        project_snapshot(&server).projects[0].1,
        server.0.task.scope,
        fresh.snapshot(),
    )
    .unwrap();
    let record = project.binding().unwrap();
    let selected_snapshot = project_snapshot(&server);
    let grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            scope(&server, &project, &record, &selected_snapshot.projects[0].2),
            &fresh,
        )
        .unwrap();
    grant.authorize(Permission::Read(Module::Ast)).unwrap();
    let caller = issuer.authenticate_runtime(&raw, &fresh).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    issuer.close_run(&parent, &sibling).unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_launch_requires_current_effect_and_retained_project() {
    let (_temp, denied, raw) = fixture();
    let cancel = Cancellation::default();
    let original = request(&cancel, denied.0.task.content);
    let input = launch_input(&denied);
    let denied_context = crate::source_server::project_hook::HookPolicy::capture(
        &denied.0.policy.source,
        &denied.0.policy.config,
        &project_snapshot(&denied).projects,
    )
    .unwrap();
    let denied_modules = captured_module_settings(&denied, &denied_context, &original);
    let execution = crate::managed_role_run::execute_with_program(
        crate::managed_roles::InstallResources {
            auth: &denied.0.auth,
            global_paths: None,
            projects: &project_snapshot(&denied).projects,
            current: project_snapshot(&denied).current.unwrap(),
            policy: &denied.0.policy.source,
            modules: &denied_modules,
            memory_read: denied_context.memory_limits(),
        },
        &raw,
        &input,
        &original,
        std::path::Path::new("no-native-effect-allowed"),
    )
    .await;
    assert!(execution.cause.is_some());
    assert!(execution.native.is_none());
    assert!(execution.residual.is_none());
    assert!(
        !project_snapshot(&denied).projects[0]
            .0
            .root()
            .join(".kiro/agents")
            .exists()
    );
    let (_temp, server, raw) = fixture_with_launch(NativeBinding::KiroV3, true);
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let mut input = launch_input(&server);
    let context = crate::source_server::project_hook::HookPolicy::capture(
        &server.0.policy.source,
        &server.0.policy.config,
        &project_snapshot(&server).projects,
    )
    .unwrap();
    let modules = captured_module_settings(&server, &context, &original);
    let selected_snapshot = project_snapshot(&server);
    let resources = || crate::managed_roles::InstallResources {
        auth: &server.0.auth,
        global_paths: None,
        projects: &selected_snapshot.projects,
        current: selected_snapshot.current.unwrap(),
        policy: &server.0.policy.source,
        modules: &modules,
        memory_read: context.memory_limits(),
    };
    input.install.project_version = Some("not-retained-by-runtime".into());
    let execution = crate::managed_role_run::execute_with_program(
        resources(),
        &raw,
        &input,
        &original,
        std::path::Path::new("no-foreign-project-effect"),
    )
    .await;
    assert!(execution.cause.is_some());
    assert!(execution.native.is_none());
    assert!(execution.residual.is_none());
    assert!(
        !project_snapshot(&server).projects[0]
            .0
            .root()
            .join(".kiro/agents")
            .exists()
    );
    denied.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
}

async fn memory_rescan_owner(
    context: graphit_daemon::supervisor::ProjectContext,
) -> (
    graphit_daemon::supervisor::ProjectSupervisor,
    tokio::sync::mpsc::Receiver<graphit_daemon::watcher::ChangeBatch>,
) {
    use graphit_daemon::{
        supervisor::ProjectSupervisor,
        watcher::{IgnoreConfig, WatchConfig},
    };
    let (sender, mut batches) = tokio::sync::mpsc::channel(8);
    let ignore = IgnoreConfig {
        filename: None,
        defaults: vec!["**".into()],
    };
    let owner = ProjectSupervisor::start(
        context.clone(),
        WatchConfig {
            root: context.root().to_owned(),
            ast: ignore.clone(),
            wiki: ignore,
            control_files: Vec::new(),
            control_directories: Vec::new(),
            debounce: Duration::from_millis(40),
        },
        move |_, batch, _| {
            let sender = sender.clone();
            async move { sender.send(batch).await.map_err(io::Error::other) }
        },
    )
    .await
    .unwrap();
    let initial = tokio::time::timeout(Duration::from_secs(8), batches.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(initial.rescan);
    assert!(!initial.records_changed);
    (owner, batches)
}
async fn expect_memory_rescan(
    batches: &mut tokio::sync::mpsc::Receiver<graphit_daemon::watcher::ChangeBatch>,
) {
    let batch = tokio::time::timeout(Duration::from_secs(8), batches.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(!batch.rescan);
    assert!(batch.records_changed);
    assert!(batch.paths.is_empty());
    assert!(batch.error.is_none());
}
async fn expect_no_memory_rescan(
    batches: &mut tokio::sync::mpsc::Receiver<graphit_daemon::watcher::ChangeBatch>,
) {
    assert!(
        tokio::time::timeout(Duration::from_millis(100), batches.recv())
            .await
            .is_err()
    );
}
fn memory_context(
    server: &RoleToolServer,
    selected: usize,
) -> graphit_daemon::supervisor::ProjectContext {
    graphit_daemon::supervisor::ProjectContext {
        id: project_snapshot(server).projects[selected].1,
        version: project_snapshot(server).projects[selected].2.clone(),
    }
}

#[tokio::test]
async fn actual_role_memory_rescan_crud_filters_reads_and_pre_effect_errors() {
    let (_temp, server, raw) = fixture();
    let (owner, mut batches) = memory_rescan_owner(memory_context(&server, 0)).await;
    let handle = owner.rescan_handle();
    server
        .attach_memory_rescans(Arc::from([handle.clone()]))
        .unwrap();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteMemory],
        Duration::from_secs(180),
    );
    let (_denied_session, _denied_run, denied_reference, denied_bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        Duration::from_secs(180),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let denied = connect(
        &uri,
        role_credential(&denied_reference, &denied_bearer, server.0.task.content),
    )
    .await;
    let insert = json!({"input":{"idempotency_key":"rescan-crud","title":"Rescan","body":"real stored body","classification":"decision"}});
    assert_eq!(
        call(&denied, "graphit_memory_insert", insert.clone(), true).await["code"],
        "authentication_or_permission_denied"
    );
    expect_no_memory_rescan(&mut batches).await;
    call(
        &client,
        "graphit_memory_insert",
        json!({"input":{"idempotency_key":"malformed"}}),
        true,
    )
    .await;
    expect_no_memory_rescan(&mut batches).await;
    let created = call(&client, "graphit_memory_insert", insert.clone(), false).await;
    expect_memory_rescan(&mut batches).await;
    let id = created["result"]["id"].clone();
    let same = call(&client, "graphit_memory_insert", insert, false).await;
    assert_eq!(same["result"]["revision"], created["result"]["revision"]);
    expect_memory_rescan(&mut batches).await;
    assert_eq!(
        call(
            &client,
            "graphit_memory_get",
            json!({"input":{"id":id}}),
            false
        )
        .await["memory"]["body"],
        "real stored body"
    );
    expect_no_memory_rescan(&mut batches).await;
    let unchanged = call(&client, "graphit_memory_update", json!({"input":{"id":id,"expected_revision":1,"idempotency_key":"same-title","reason":"unchanged","title":"Rescan"}}), true).await;
    assert_eq!(unchanged["code"], "no_change");
    expect_no_memory_rescan(&mut batches).await;
    assert_eq!(call(&client, "graphit_memory_update", json!({"input":{"id":id,"expected_revision":99,"idempotency_key":"conflict","reason":"stale","body":"no effect"}}), true).await["code"], "revision_conflict");
    expect_no_memory_rescan(&mut batches).await;
    let updated = call(&client, "graphit_memory_update", json!({"input":{"id":id,"expected_revision":1,"idempotency_key":"changed-body","reason":"new decision","body":"updated real body"}}), false).await;
    expect_memory_rescan(&mut batches).await;
    let mut revision = updated["result"]["revision"].as_u64().unwrap();
    for name in [
        "graphit_memory_mark_mandatory",
        "graphit_memory_unmark_mandatory",
        "graphit_memory_promote",
        "graphit_memory_demote",
    ] {
        let changed = call(&client, name, json!({"input":{"id":id,"expected_revision":revision,"idempotency_key":name,"reason":"change flag"}}), false).await;
        assert_eq!(
            changed["result"]["revision"].as_u64().unwrap(),
            revision + 1
        );
        revision += 1;
        expect_memory_rescan(&mut batches).await;
    }
    assert_eq!(
        call(
            &client,
            "graphit_memory_get",
            json!({"input":{"id":id}}),
            false
        )
        .await["memory"]["body"],
        "updated real body"
    );
    expect_no_memory_rescan(&mut batches).await;
    let removed = call(&client, "graphit_memory_delete", json!({"input":{"id":id,"expected_revision":revision,"idempotency_key":"removed","reason":"retire decision"}}), false).await;
    assert_eq!(
        removed["result"]["revision"].as_u64().unwrap(),
        revision + 1
    );
    expect_memory_rescan(&mut batches).await;
    owner.stop().await.unwrap();
    assert!(!handle.request_rescan());
    let after_stop = call(&client, "graphit_memory_insert", json!({"input":{"idempotency_key":"stale-owner","title":"After stop","body":"valid CRUD without compute","classification":"decision"}}), false).await;
    assert_eq!(
        call(
            &client,
            "graphit_memory_get",
            json!({"input":{"id":after_stop["result"]["id"]}}),
            false
        )
        .await["memory"]["body"],
        "valid CRUD without compute"
    );
    assert!(batches.recv().await.is_none());
    client.cancel().await.unwrap();
    denied.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn actual_role_memory_rescan_exact_selection_empty_projection_and_attachment() {
    use graphit_core::version_context::Checkout;
    let (_temp, server, raw) = fixture();
    server.attach_memory_rescans(Arc::from([])).unwrap();
    let mut source = crate::source_server::SourceServer::new(
        Arc::clone(&server.0.auth),
        Arc::clone(&project_snapshot(&server).projects),
        project_snapshot(&server).current.unwrap(),
        server.0.policy.source.clone(),
        CancellationToken::new(),
    )
    .unwrap();
    assert!(source.attach_memory_rescans(Arc::from([])).is_err());
    source
        .attach_role_tools(
            ConfigSnapshot::new(project_snapshot(&server).projects[0].0.brand(), []).unwrap(),
        )
        .unwrap();
    source.attach_memory_rescans(Arc::from([])).unwrap();
    let source_clone = source.clone();
    assert!(source.attach_memory_rescans(Arc::from([])).is_ok());
    drop(source_clone);
    let shared = server.clone();
    assert!(server.attach_memory_rescans(Arc::from([])).is_ok());
    drop(shared);
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteMemory],
        Duration::from_secs(180),
    );
    let direct = |key: &str| json!({"input":{"idempotency_key":key,"title":key,"body":"actual isolated publication","classification":"decision"}});
    // Empty projection is legitimate when no compute owner was selected.
    let empty = server
        .call(
            "graphit_memory_insert".into(),
            serde_json::from_value(direct("empty")).unwrap(),
            role_credential(&reference, &bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(empty.is_error, Some(false));
    let real = memory_context(&server, 0);
    let mut wrong_id = real.clone();
    wrong_id.id = project_snapshot(&server).projects[1].1;
    let mut wrong_root = memory_context(&server, 1);
    wrong_root.id = real.id;
    let mut wrong_version = real;
    wrong_version.version = VersionContext::select(
        &project_snapshot(&server).projects[0].0,
        Checkout::Branch {
            name: "unselected",
            default: "main",
        },
        "",
        None,
        "",
    )
    .unwrap();
    let mut owners = Vec::new();
    let mut receivers = Vec::new();
    let mut handles = Vec::new();
    for context in [wrong_id, wrong_root, wrong_version] {
        let (owner, batches) = memory_rescan_owner(context).await;
        handles.push(owner.rescan_handle());
        owners.push(owner);
        receivers.push(batches);
    }
    server.attach_memory_rescans(Arc::from(handles)).unwrap();
    let result = server
        .call(
            "graphit_memory_insert".into(),
            serde_json::from_value(direct("mismatch")).unwrap(),
            role_credential(&reference, &bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(result.is_error, Some(false));
    for receiver in &mut receivers {
        expect_no_memory_rescan(receiver).await;
    }
    for owner in owners {
        owner.stop().await.unwrap();
    }
    source.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn actual_role_memory_rescan_typed_confirmed_classification() {
    use graphit_records::memory_store::MemoryStoreError;
    let (_temp, server, _raw) = fixture();
    let (owner, mut batches) = memory_rescan_owner(memory_context(&server, 0)).await;
    let handle = owner.rescan_handle();
    for error in [
        MemoryStoreError::NoChange,
        MemoryStoreError::Invalid("classification fixture"),
    ] {
        memory::request_published_rescan(&Err(error), Some(&handle));
        expect_no_memory_rescan(&mut batches).await;
    }
    // This is a typed classification fixture, not a fabricated SDK commit.
    let confirmed = Err(MemoryStoreError::Confirmed {
        version: 37,
        cause: Box::new(MemoryStoreError::Invalid("retained cause")),
    });
    memory::request_published_rescan(&confirmed, Some(&handle));
    expect_memory_rescan(&mut batches).await;
    assert!(matches!(
        confirmed,
        Err(MemoryStoreError::Confirmed { version: 37, .. })
    ));
    owner.stop().await.unwrap();
    server.shutdown().await.unwrap();
}

#[cfg(unix)]
async fn seed_role_prompt_memory(
    server: &RoleToolServer,
    raw: &str,
    selected: usize,
) -> Vec<graphit_records::memory_store::Memory> {
    use graphit_auth::ProjectWriteOperation;
    use graphit_records::{
        memory_attributes::{MemoryAttributes, MemoryClass},
        memory_store::{CreateMemory, NativeMemoryStore},
    };
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let (paths, id, version) = &project_snapshot(server).projects[selected];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.task.scope, original.snapshot())
            .unwrap();
    let record = project.binding().unwrap();
    let captured = ScopeInput {
        installation: server.0.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: server.0.task.binding,
        global_version: "",
    };
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_write(&caller, &original, captured, ProjectWriteOperation::Memory)
        .unwrap();
    let mut mandatory = Vec::new();
    for (name, required, important) in [
        ("first", true, false),
        ("second", true, true),
        ("important", false, true),
        ("normal", false, false),
    ] {
        let memory = NativeMemoryStore::create(
            original.snapshot(),
            &admission,
            CreateMemory {
                idempotency_key: format!("prompt-{selected}-{name}"),
                title: format!("project {selected} {name}"),
                body: format!(
                    "literal current PROJECT {selected} {name} ação <&> {{{{ data }}}}\n"
                ),
                attributes: MemoryAttributes {
                    classification: MemoryClass::Convention,
                    mandatory: required,
                    important,
                },
                tags: vec![],
                references: None,
            },
            server.0.policy.memory_write,
        )
        .await
        .unwrap();
        if required {
            mandatory.push(memory);
        }
    }
    mandatory.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
    mandatory
}
#[cfg(unix)]
fn role_prompt_peer(
    temp: &std::path::Path,
    expected: &Value,
    role: crate::managed_roles::ManagedRoleName,
) -> std::path::PathBuf {
    use crate::managed_roles::ManagedRoleName;
    let role = match role {
        ManagedRoleName::Scout => "scout",
        ManagedRoleName::Scribe => "scribe",
        ManagedRoleName::Tracker => "tracker",
    };
    native_role_peer(
        temp,
        json!({
            "fixture":"role-prompt", "binding":"kiro-v3", "role":role, "brand":"graphit",
            "expected_context":expected
        }),
    )
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_bootstrap_project_memory_uses_captured_per_project_gates_and_recipient() {
    use graphit_core::config::{ConfigKey, ConfigStore};
    let (temp, server, raw) = fixture_with_launch(NativeBinding::KiroV3, true);
    let _first = seed_role_prompt_memory(&server, &raw, 0).await;
    let peer = seed_role_prompt_memory(&server, &raw, 1).await;
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let config = ConfigStore::project(&project_snapshot(&server).projects[0].0).unwrap();
    config
        .set(
            ConfigKey::new("modules.memory").unwrap(),
            json!(false),
            Duration::from_secs(5),
            &cancel,
        )
        .unwrap();
    let context = crate::source_server::project_hook::HookPolicy::capture(
        &server.0.policy.source,
        &server.0.policy.config,
        &project_snapshot(&server).projects,
    )
    .unwrap();
    let modules = captured_module_settings(&server, &context, &original);
    assert!(!modules[0].enabled(Module::Memory));
    assert!(modules[1].enabled(Module::Memory));
    // The captured run switches remain fixed after a subsequent physical config change.
    config
        .set(
            ConfigKey::new("modules.memory").unwrap(),
            json!(true),
            Duration::from_secs(5),
            &cancel,
        )
        .unwrap();
    let expected = json!({"project_version":project_snapshot(&server).projects[1].2.version(),"memory_state":"published","memories":peer.iter().map(|memory|json!({"id":memory.id,"revision":memory.revision,"title":memory.title,"body":memory.body,"classification":"convention","mandatory":true,"important":memory.attributes.important})).collect::<Vec<_>>()});
    for (selected, role, payload) in [
        (0, crate::managed_roles::ManagedRoleName::Scout, Value::Null),
        (
            1,
            crate::managed_roles::ManagedRoleName::Scout,
            expected.clone(),
        ),
        (1, crate::managed_roles::ManagedRoleName::Scribe, expected),
        (
            1,
            crate::managed_roles::ManagedRoleName::Tracker,
            Value::Null,
        ),
    ] {
        let mut input = launch_input(&server);
        input.role = role;
        input.install.roles = vec![role];
        input.install.project_dir = Some(
            project_snapshot(&server).projects[selected]
                .0
                .root()
                .to_owned(),
        );
        input.install.project_version = Some(
            project_snapshot(&server).projects[selected]
                .2
                .version()
                .to_owned(),
        );
        let mut read = context.memory_limits();
        read.max_query_bytes = n(1);
        let program = role_prompt_peer(temp.path(), &payload, role);
        let execution = crate::managed_role_run::execute_with_program(
            crate::managed_roles::InstallResources {
                auth: &server.0.auth,
                global_paths: None,
                projects: &project_snapshot(&server).projects,
                current: project_snapshot(&server).current.unwrap(),
                policy: &server.0.policy.source,
                modules: &modules,
                memory_read: read,
            },
            &raw,
            &input,
            &original,
            &program,
        )
        .await;
        assert!(
            execution.cause.is_none(),
            "bootstrap failed for project={selected}, role={:?}: {:?}, Auth={}, Memory={}",
            role,
            execution.cause.as_ref().map(io::Error::kind),
            execution
                .cause
                .as_ref()
                .and_then(io::Error::get_ref)
                .is_some_and(|cause| cause.is::<graphit_auth::AuthError>()),
            execution
                .cause
                .as_ref()
                .and_then(io::Error::get_ref)
                .is_some_and(|cause| cause.is::<graphit_records::memory_store::MemoryStoreError>())
        );
        assert!(
            execution.native.is_none(),
            "peer failed: {:?}",
            execution
                .native
                .as_ref()
                .and_then(|failure| failure.downcast_ref::<graphit_ai::cli_kiro::KiroFailure>())
                .map(|failure| failure.reason())
        );
        assert!(execution.outcome.run_closed);
        assert!(execution.residual.is_none());
        assert_eq!(
            execution.outcome.answer.as_deref(),
            Some("context verified")
        );
        let profile = std::fs::read_to_string(
            project_snapshot(&server).projects[selected]
                .0
                .root()
                .join(format!(".kiro/agents/graphit-{}.md", role.role().key())),
        )
        .unwrap();
        assert!(!profile.contains("Standing context"));
        assert!(!profile.contains("literal current PROJECT"));
        assert!(!profile.contains(&raw));
    }
    server.shutdown().await.unwrap();
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_bootstrap_failure_preserves_causes_and_closes_before_native_child() {
    let (_temp, server, raw) = fixture_with_launch(NativeBinding::KiroV3, true);
    seed_role_prompt_memory(&server, &raw, 0).await;
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let context = crate::source_server::project_hook::HookPolicy::capture(
        &server.0.policy.source,
        &server.0.policy.config,
        &project_snapshot(&server).projects,
    )
    .unwrap();
    let modules = captured_module_settings(&server, &context, &original);
    let input = launch_input(&server);
    let mut bounded = server.0.policy.source.clone();
    bounded.content.max_input_bytes = 512;
    {
        let policy = &bounded;
        let limits = context.memory_limits();
        let execution = crate::managed_role_run::execute_with_program(
            crate::managed_roles::InstallResources {
                auth: &server.0.auth,
                global_paths: None,
                projects: &project_snapshot(&server).projects,
                current: project_snapshot(&server).current.unwrap(),
                policy,
                modules: &modules,
                memory_read: limits,
            },
            &raw,
            &input,
            &original,
            std::path::Path::new("must-not-launch-native-child"),
        )
        .await;
        assert!(execution.cause.is_some());
        assert!(execution.native.is_none());
        assert!(
            execution.outcome.run_closed,
            "failure must occur after issued run: {:?}",
            execution.cause.as_ref().map(io::Error::kind)
        );
        assert!(execution.residual.is_none());
        assert!(execution.outcome.answer.is_none());
        assert_eq!(
            execution.cause.as_ref().unwrap().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    let invalid = crate::managed_role_run::execute_with_program(
        crate::managed_roles::InstallResources {
            auth: &server.0.auth,
            global_paths: None,
            projects: &project_snapshot(&server).projects,
            current: project_snapshot(&server).current.unwrap(),
            policy: &server.0.policy.source,
            modules: &modules[..1],
            memory_read: context.memory_limits(),
        },
        &raw,
        &input,
        &original,
        std::path::Path::new("no-misaligned-native-effect"),
    )
    .await;
    assert_eq!(
        invalid.cause.as_ref().map(io::Error::kind),
        Some(io::ErrorKind::InvalidInput)
    );
    assert!(invalid.native.is_none());
    assert!(invalid.residual.is_none());
    for (cancelled, expected) in [
        (true, io::ErrorKind::Interrupted),
        (false, io::ErrorKind::TimedOut),
    ] {
        let signal = Cancellation::default();
        let original = RequestContext::new(
            server.0.task.content,
            &signal,
            Instant::now() + Duration::from_millis(20),
        )
        .unwrap();
        if cancelled {
            signal.cancel();
        } else {
            tokio::time::sleep_until(
                original
                    .snapshot()
                    .deadline()
                    .expect("finite Original")
                    .into(),
            )
            .await;
        }
        let execution = crate::managed_role_run::execute_with_program(
            crate::managed_roles::InstallResources {
                auth: &server.0.auth,
                global_paths: None,
                projects: &project_snapshot(&server).projects,
                current: project_snapshot(&server).current.unwrap(),
                policy: &server.0.policy.source,
                modules: &modules,
                memory_read: context.memory_limits(),
            },
            &raw,
            &input,
            &original,
            std::path::Path::new("no-cancelled-expired-native-effect"),
        )
        .await;
        assert_eq!(
            execution.cause.as_ref().map(io::Error::kind),
            Some(expected)
        );
        assert!(execution.native.is_none());
        assert!(execution.residual.is_none());
        assert!(execution.outcome.answer.is_none());
    }
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn run_bootstrap_http_requires_attached_context_and_preserves_attachment_boundary() {
    let (_temp, server, raw) = fixture_with_launch(NativeBinding::KiroV3, true);
    let context = crate::source_server::project_hook::HookPolicy::capture(
        &server.0.policy.source,
        &server.0.policy.config,
        &project_snapshot(&server).projects,
    )
    .unwrap();
    let stop = CancellationToken::new();
    let http = crate::managed_roles::http::ManagedRolesHttp::new(
        Arc::clone(&server.0.auth),
        Arc::clone(&project_snapshot(&server).projects),
        project_snapshot(&server).current.unwrap(),
        server.0.policy.source.clone(),
        stop.clone(),
    );

    let socket = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = socket.local_addr().unwrap();
    let router = http.router();
    let stopping = stop.clone();
    let serving = tokio::spawn(async move {
        axum::serve(socket, router)
            .with_graceful_shutdown(stopping.cancelled_owned())
            .await
    });
    let response = loopback_client()
        .post(format!("http://{address}/admin/roles/run"))
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(&raw)
        .json(&launch_input(&server))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(
        !project_snapshot(&server).projects[0]
            .0
            .root()
            .join(".kiro/agents")
            .exists()
    );
    stop.cancel();
    serving.await.unwrap().unwrap();
    http.shutdown().await.unwrap();
    let mut captured = crate::managed_roles::http::ManagedRolesHttp::new(
        Arc::clone(&server.0.auth),
        Arc::clone(&project_snapshot(&server).projects),
        project_snapshot(&server).current.unwrap(),
        server.0.policy.source.clone(),
        CancellationToken::new(),
    );
    captured
        .attach_context(
            server.0.policy.config.clone(),
            context.memory_limits(),
            Arc::clone(&server.0.live_projects),
        )
        .unwrap();
    assert_eq!(
        captured
            .attach_context(
                server.0.policy.config.clone(),
                context.memory_limits(),
                Arc::clone(&server.0.live_projects)
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    captured.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
}

#[cfg(unix)]
fn git_fixture_command(root: &std::path::Path, args: &[&str]) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .args(["-c", "init.templateDir="])
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "temporary Git fixture command failed"
    );
    output.stdout
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn git_inspection_native_sdk_socket_preserves_scope_formats_and_no_helpers() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::os::unix::fs::PermissionsExt;
    let (temp, server, raw) = fixture();
    let helper = temp.path().join("clean-helper");
    let marker = temp.path().join("helper-executed");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf executed >> '{}'\ncat\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    for (index, format) in [(0, "sha1"), (1, "sha256")] {
        let selected_snapshot = project_snapshot(&server);
        let root = selected_snapshot.projects[index].0.root();
        git_fixture_command(root, &["init", "-q", &format!("--object-format={format}")]);
        std::fs::write(root.join(".gitignore"), ".graphit/\n").unwrap();
        git_fixture_command(
            root,
            &["config", "filter.probe.clean", helper.to_str().unwrap()],
        );
        git_fixture_command(root, &["config", "filter.probe.smudge", "cat"]);
        git_fixture_command(
            root,
            &["config", "diff.probe.textconv", helper.to_str().unwrap()],
        );
        git_fixture_command(
            root,
            &["config", "diff.probe.command", helper.to_str().unwrap()],
        );
        std::fs::write(
            root.join(".gitattributes"),
            "tracked filter=probe diff=probe\n",
        )
        .unwrap();
        std::fs::write(root.join("tracked"), b"alpha\n").unwrap();
        std::fs::write(root.join("binary"), [0, 1, 2]).unwrap();
        git_fixture_command(
            root,
            &["add", "tracked", "binary", ".gitattributes", ".gitignore"],
        );
        std::fs::write(root.join("tracked"), b"bravo\n").unwrap();
        std::fs::write(root.join("binary"), [0, 2, 3, 4]).unwrap();
        std::fs::write(root.join("untracked"), b"untracked").unwrap();
        #[cfg(target_os = "linux")]
        if index == 0 {
            use std::os::unix::ffi::OsStrExt;
            let path = root.join(std::ffi::OsStr::from_bytes(b"non-utf8-\xff"));
            std::fs::write(&path, b"before\n").unwrap();
            let out = std::process::Command::new("git")
                .args(["add", "--"])
                .arg(&path)
                .current_dir(root)
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap())
                .env("HOME", root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .unwrap();
            assert!(out.status.success());
            std::fs::write(path, b"after!\n").unwrap();
        }
    }
    std::fs::remove_file(&marker).unwrap();
    let before: Vec<_> = project_snapshot(&server)
        .projects
        .iter()
        .map(|(p, _, _)| {
            [
                std::fs::read(p.root().join(".git/index")).unwrap(),
                std::fs::read(p.root().join(".git/config")).unwrap(),
            ]
        })
        .collect();
    let (uri, serving) = serve(&server).await;
    for (selected, format) in [(0, "sha1"), (1, "sha256")] {
        let (_session, _run, reference, bearer) = credential_at(
            &server,
            &raw,
            selected,
            Role::Tracker,
            &[Permission::InspectGit],
            Duration::from_secs(90),
        );
        let selection = json!({"project_dir":graphit_core::native_path::NativePath::new(project_snapshot(&server).projects[selected].0.root().to_owned()).unwrap()});
        let client = connect_at(
            &uri,
            role_credential(&reference, &bearer, server.0.task.content),
            Some(selection.clone()),
        )
        .await;
        let tools = client.list_all_tools().await.unwrap();
        assert!(tools.iter().any(|t| t.name == "graphit_git_status"));
        assert!(tools.iter().any(|t| t.name == "graphit_git_diff"));
        assert!(!tools.iter().any(|t| t.name.starts_with("graphit_memory_")));
        let status = call(&client, "graphit_git_status", json!({"input":{}}), false).await;
        assert_eq!(status["object_format"], format);
        assert!(status["entries"].as_array().unwrap().iter().any(|row| {
            row["phase"] == "index_worktree"
                && row["kind"] == "modified"
                && STANDARD
                    .decode(row["path_base64"].as_str().unwrap())
                    .unwrap()
                    == b"tracked"
        }));
        let diff = call(&client, "graphit_git_diff", json!({"input":{}}), false).await;
        let patches = diff["patches"].as_array().unwrap();
        let text = patches
            .iter()
            .find(|p| STANDARD.decode(p["path_base64"].as_str().unwrap()).unwrap() == b"tracked")
            .unwrap();
        let patch = String::from_utf8(
            STANDARD
                .decode(text["hunks_base64"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        assert!(patch.contains("-alpha\n") && patch.contains("+bravo\n"));
        let binary = patches
            .iter()
            .find(|p| STANDARD.decode(p["path_base64"].as_str().unwrap()).unwrap() == b"binary")
            .unwrap();
        assert_eq!(binary["kind"], "binary");
        assert_eq!(binary["old_bytes"], 3);
        assert_eq!(binary["new_bytes"], 4);
        assert!(
            !patches.iter().any(
                |p| STANDARD.decode(p["path_base64"].as_str().unwrap()).unwrap() == b"untracked"
            )
        );
        #[cfg(target_os = "linux")]
        if selected == 0 {
            assert!(patches.iter().any(|p| {
                STANDARD.decode(p["path_base64"].as_str().unwrap()).unwrap() == b"non-utf8-\xff"
            }));
        }
        let staged = call(
            &client,
            "graphit_git_diff",
            json!({"input":{"comparison":"staged"}}),
            false,
        )
        .await;
        let added = staged["patches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| STANDARD.decode(p["path_base64"].as_str().unwrap()).unwrap() == b"tracked")
            .unwrap();
        assert_eq!(added["old_bytes"], 0);
        assert_eq!(added["new_bytes"], 6);
        assert_eq!(
            call(
                &client,
                "graphit_git_status",
                json!({"input":{"cwd":"forbidden"}}),
                true
            )
            .await["code"],
            "invalid_arguments"
        );
        let other = json!({"selection":{"project_dir":graphit_core::native_path::NativePath::new(project_snapshot(&server).projects[1-selected].0.root().to_owned()).unwrap()},"input":{}});
        assert_eq!(
            call(&client, "graphit_git_status", other, true).await["code"],
            "authentication_or_permission_denied"
        );
        client.cancel().await.unwrap();
    }
    for role in [Role::Scout, Role::Scribe] {
        let (_session, _run, reference, bearer) = credential(
            &server,
            &raw,
            role,
            &[Permission::Read(Module::Memory)],
            Duration::from_secs(90),
        );
        let client = connect(
            &uri,
            role_credential(&reference, &bearer, server.0.task.content),
        )
        .await;
        assert!(
            !client
                .list_all_tools()
                .await
                .unwrap()
                .iter()
                .any(|t| t.name.starts_with("graphit_git_"))
        );
        assert_eq!(
            call(&client, "graphit_git_status", json!({"input":{}}), true).await["code"],
            "authentication_or_permission_denied"
        );
        client.cancel().await.unwrap();
    }
    assert!(!marker.exists());
    for (index, (paths, _, _)) in project_snapshot(&server).projects.iter().enumerate() {
        assert_eq!(
            std::fs::read(paths.root().join(".git/index")).unwrap(),
            before[index][0]
        );
        assert_eq!(
            std::fs::read(paths.root().join(".git/config")).unwrap(),
            before[index][1]
        );
    }
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn git_inspection_whole_response_bounds_and_owned_cancel_join() {
    let (_temp, mut server, raw) = fixture();
    let selected_snapshot = project_snapshot(&server);
    let root = selected_snapshot.projects[0].0.root();
    git_fixture_command(root, &["init", "-q"]);
    std::fs::write(root.join(".gitignore"), ".graphit/\n").unwrap();
    std::fs::write(root.join("tracked"), "alpha\n").unwrap();
    git_fixture_command(root, &["add", "tracked"]);
    std::fs::write(root.join("tracked"), "bravo\n").unwrap();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::InspectGit],
        Duration::from_secs(90),
    );
    Arc::get_mut(&mut server.0)
        .unwrap()
        .task
        .content
        .max_output_bytes = 16;
    let result = server
        .call(
            "graphit_git_diff".into(),
            serde_json::from_value(json!({"input":{}})).unwrap(),
            role_credential(&reference, &bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(result.is_error, Some(true));
    let value: Value = typed_value(&result);
    assert_eq!(value["io_kind"], "InvalidInput");
    assert!(value.get("patches").is_none());
    Arc::get_mut(&mut server.0)
        .unwrap()
        .task
        .content
        .max_output_bytes = 65536;
    let (started, entered) = std::sync::mpsc::channel();
    let (release, resume) = oneshot::channel();
    *server.0.gate.lock().unwrap() = Some((started, resume));
    let token = CancellationToken::new();
    let client_cancel = token.clone();
    let caller = server.clone();
    let credential = role_credential(&reference, &bearer, server.0.task.content);
    let call = tokio::spawn(async move {
        caller
            .call(
                "graphit_git_status".into(),
                serde_json::from_value(json!({"input":{}})).unwrap(),
                credential,
                client_cancel,
            )
            .await
    });
    let original_cancel = entered.recv_timeout(Duration::from_secs(5)).unwrap();
    token.cancel();
    assert_eq!(call.await.unwrap().is_error, Some(true));
    let mut stopping = Box::pin(server.shutdown());
    tokio::time::timeout(
        Duration::from_secs(5),
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(stopping.as_mut(), cx).is_pending());
            // shutdown retains this guard while joining the actual gated job.
            if server.0.jobs.try_lock().is_err() {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        }),
    )
    .await
    .unwrap();
    assert!(original_cancel.is_cancelled());
    release.send(()).unwrap();
    stopping.await.unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn git_inspection_nested_linked_scope_revocation_and_deadline() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (temp, mut server, raw) = fixture();
    let root = project_snapshot(&server).projects[0].0.root().to_owned();
    git_fixture_command(&root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("tracked"), "alpha\n").unwrap();
    std::fs::write(root.join(".gitignore"), ".graphit/\n").unwrap();
    let nested = root.join("nested");
    std::fs::create_dir(&nested).unwrap();
    git_fixture_command(&nested, &["init", "-q", "-b", "main"]);
    let marker = temp.path().join("nested-helper-ran");
    let helper = temp.path().join("nested-clean-helper");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf executed >> '{}'\ncat\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    git_fixture_command(
        &nested,
        &["config", "filter.probe.clean", helper.to_str().unwrap()],
    );
    git_fixture_command(
        &nested,
        &["config", "diff.probe.textconv", helper.to_str().unwrap()],
    );
    std::fs::write(
        nested.join(".gitattributes"),
        "tracked filter=probe diff=probe\n",
    )
    .unwrap();
    std::fs::write(nested.join("tracked"), "alpha\n").unwrap();
    git_fixture_command(&nested, &["add", "tracked", ".gitattributes"]);
    git_fixture_command(
        &nested,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-q",
            "-m",
            "temporary nested fixture",
        ],
    );
    let id = String::from_utf8(git_fixture_command(&nested, &["rev-parse", "HEAD"])).unwrap();
    let id = id.trim();
    std::fs::write(
        root.join(".gitmodules"),
        "[submodule \"nested\"]\n path=nested\n url=./nested\n",
    )
    .unwrap();
    git_fixture_command(&root, &["add", "tracked", ".gitignore", ".gitmodules"]);
    git_fixture_command(
        &root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{id},nested"),
        ],
    );
    git_fixture_command(
        &root,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-q",
            "-m",
            "temporary parent fixture",
        ],
    );
    let peer = project_snapshot(&server).projects[1].0.root().to_owned();
    let saved = temp.path().join("saved-peer");
    std::fs::rename(&peer, &saved).unwrap();
    git_fixture_command(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            peer.to_str().unwrap(),
            "HEAD",
        ],
    );
    std::fs::rename(saved.join(".graphit"), peer.join(".graphit")).unwrap();
    std::fs::write(nested.join("tracked"), "bravo\n").unwrap();
    std::fs::write(peer.join("tracked"), "peer!\n").unwrap();
    std::fs::remove_file(&marker).unwrap();
    let originals = [
        root.join(".git/index"),
        root.join(".git/config"),
        nested.join(".git/index"),
        nested.join(".git/config"),
    ]
    .map(|p| std::fs::read(p).unwrap());
    let (session, run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::InspectGit],
        Duration::from_secs(90),
    );
    let invoke = |name: &str, input: Value| {
        let service = server.clone();
        let credential = role_credential(&reference, &bearer, server.0.task.content);
        let name = name.to_owned();
        async move {
            service
                .call(
                    name,
                    serde_json::from_value(input).unwrap(),
                    credential,
                    CancellationToken::new(),
                )
                .await
        }
    };
    let response = invoke("graphit_git_status", json!({"input":{}})).await;
    assert_eq!(response.is_error, Some(false));
    let value: Value = typed_value(&response);
    let nested_row = value["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| {
            row["phase"] == "index_worktree"
                && STANDARD
                    .decode(row["path_base64"].as_str().unwrap())
                    .unwrap()
                    == b"nested"
        })
        .unwrap();
    assert_eq!(nested_row["submodule"]["worktree_changed"], true);
    assert_eq!(nested_row["submodule"]["index_id"], id);
    assert_eq!(nested_row["submodule"]["checked_out_head_id"], id);
    let response = invoke("graphit_git_diff", json!({"input":{}})).await;
    assert_eq!(response.is_error, Some(false));
    let value: Value = typed_value(&response);
    let gitlink = value["patches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["kind"] == "gitlink")
        .unwrap();
    assert_eq!(gitlink["gitlink_old_id"], id);
    assert_eq!(gitlink["gitlink_new_id"], id);
    let (_session, _run, peer_ref, peer_bearer) = credential_at(
        &server,
        &raw,
        1,
        Role::Tracker,
        &[Permission::InspectGit],
        Duration::from_secs(90),
    );
    let response=server.call("graphit_git_diff".into(),serde_json::from_value(json!({"selection":{"project_dir":graphit_core::native_path::NativePath::new(peer.clone()).unwrap()},"input":{}})).unwrap(),role_credential(&peer_ref,&peer_bearer,server.0.task.content),CancellationToken::new()).await;
    assert_eq!(response.is_error, Some(false));
    let value: Value = typed_value(&response);
    assert!(value["patches"].as_array().unwrap().iter().any(|p| {
        String::from_utf8(
            STANDARD
                .decode(p["hunks_base64"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap()
        .contains("+peer!")
    }));
    assert!(!marker.exists());
    for (p, bytes) in [
        root.join(".git/index"),
        root.join(".git/config"),
        nested.join(".git/index"),
        nested.join(".git/config"),
    ]
    .into_iter()
    .zip(originals)
    {
        assert_eq!(std::fs::read(p).unwrap(), bytes);
    }
    // A gitlink replaced by a symlink is a type change; the SDK never follows it.
    let outside = temp.path().join("outside");
    std::fs::rename(&nested, &outside).unwrap();
    symlink(&outside, &nested).unwrap();
    let response = invoke("graphit_git_status", json!({"input":{}})).await;
    assert_eq!(response.is_error, Some(false));
    let value: Value = typed_value(&response);
    let changed: Vec<_> = value["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            row["phase"] == "index_worktree"
                && STANDARD
                    .decode(row["path_base64"].as_str().unwrap())
                    .unwrap()
                    == b"nested"
        })
        .collect();
    assert!(
        changed.iter().any(|row| row["kind"] == "type_changed"),
        "{value}"
    );
    assert!(changed.iter().all(|row| row.get("submodule").is_none()));
    assert!(!marker.exists());
    std::fs::remove_file(&nested).unwrap();
    std::fs::rename(outside, &nested).unwrap();
    // A real SDK working-tree mapping outside the captured PROJECT is denied before scanning.
    let outside = temp.path().join("foreign-worktree");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("tracked"), "private foreign fixture").unwrap();
    git_fixture_command(
        &root,
        &["config", "core.worktree", outside.to_str().unwrap()],
    );
    let response = invoke("graphit_git_status", json!({"input":{}})).await;
    assert_eq!(response.is_error, Some(true));
    let value: Value = typed_value(&response);
    assert_eq!(value["io_kind"], "PermissionDenied");
    assert!(
        !serde_json::to_string(&typed_value(&response))
            .unwrap()
            .contains("foreign")
    );
    git_fixture_command(&root, &["config", "--unset", "core.worktree"]);
    // Real staged SDK modes distinguish blob/link changes and gitlink identities.
    let index_path = root.join(".git/index");
    let saved_index = std::fs::read(&index_path).unwrap();
    std::fs::remove_file(root.join("tracked")).unwrap();
    symlink("link-target", root.join("tracked")).unwrap();
    git_fixture_command(&root, &["add", "tracked"]);
    let response = invoke("graphit_git_status", json!({"input":{}})).await;
    assert_eq!(response.is_error, Some(false));
    let value: Value = typed_value(&response);
    assert!(value["entries"].as_array().unwrap().iter().any(|row| {
        row["phase"] == "tree_index"
            && row["kind"] == "type_changed"
            && row["old_entry_kind"] == "blob"
            && row["new_entry_kind"] == "link"
    }));
    let response = invoke("graphit_git_diff", json!({"input":{"comparison":"staged"}})).await;
    assert_eq!(response.is_error, Some(false));
    std::fs::remove_file(root.join("tracked")).unwrap();
    std::fs::write(root.join("tracked"), "alpha\n").unwrap();
    std::fs::write(&index_path, &saved_index).unwrap();
    // A real commit OID is used for staged gitlink modification/addition/deletion.
    let parent_id = String::from_utf8(git_fixture_command(&root, &["rev-parse", "HEAD"])).unwrap();
    let parent_id = parent_id.trim();
    git_fixture_command(
        &root,
        &[
            "update-index",
            "--cacheinfo",
            &format!("160000,{parent_id},nested"),
        ],
    );
    git_fixture_command(
        &root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{parent_id},added-module"),
        ],
    );
    let response = invoke("graphit_git_status", json!({"input":{}})).await;
    assert_eq!(response.is_error, Some(false));
    let value: Value = typed_value(&response);
    let rows = value["entries"].as_array().unwrap();
    assert!(rows.iter().any(|r| r["phase"] == "tree_index"
        && r["kind"] == "modified"
        && r["old_entry_kind"] == "commit"
        && r["new_entry_kind"] == "commit"));
    assert!(rows.iter().any(|r| r["phase"] == "tree_index"
        && r["kind"] == "added"
        && r["new_entry_kind"] == "commit"));
    let response = invoke("graphit_git_diff", json!({"input":{"comparison":"staged"}})).await;
    assert_eq!(response.is_error, Some(false));
    let value: Value = typed_value(&response);
    assert!(
        value["patches"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == "gitlink"
                && p["gitlink_old_id"] == id
                && p["gitlink_new_id"] == parent_id)
    );
    git_fixture_command(&root, &["update-index", "--force-remove", "nested"]);
    let response = invoke("graphit_git_status", json!({"input":{}})).await;
    assert_eq!(response.is_error, Some(false));
    let value: Value = typed_value(&response);
    assert!(
        value["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["phase"] == "tree_index"
                && r["kind"] == "deleted"
                && r["old_entry_kind"] == "commit"
                && r.get("new_entry_kind").is_none())
    );
    std::fs::write(&index_path, &saved_index).unwrap();
    let blob = String::from_utf8(git_fixture_command(
        &root,
        &["hash-object", "-w", "tracked"],
    ))
    .unwrap();
    git_fixture_command(
        &root,
        &[
            "update-index",
            "--cacheinfo",
            &format!("100644,{},nested", blob.trim()),
        ],
    );
    let response = invoke("graphit_git_diff", json!({"input":{"comparison":"staged"}})).await;
    assert_eq!(response.is_error, Some(true));
    let value: Value = typed_value(&response);
    assert_eq!(value["io_kind"], "Unsupported");
    assert!(value.get("patches").is_none());
    std::fs::write(&index_path, &saved_index).unwrap();
    // An actual unmerged index must be explicit for status and fail both diff comparisons whole.
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("git")
        .args(["update-index", "--index-info"])
        .current_dir(&root)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", &root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let null = "0".repeat(blob.trim().len());
    let entries = format!(
        "0 {null}\ttracked\n100644 {} 1\ttracked\n100644 {} 2\ttracked\n100644 {} 3\ttracked\n",
        blob.trim(),
        blob.trim(),
        blob.trim()
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(entries.as_bytes())
        .unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    let response = invoke("graphit_git_status", json!({"input":{}})).await;
    assert_eq!(response.is_error, Some(false));
    let value: Value = typed_value(&response);
    assert!(
        value["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "conflict")
    );
    for comparison in ["worktree", "staged"] {
        let response = invoke(
            "graphit_git_diff",
            json!({"input":{"comparison":comparison}}),
        )
        .await;
        assert_eq!(response.is_error, Some(true));
        let value: Value = typed_value(&response);
        assert_eq!(value["io_kind"], "Unsupported");
        assert!(value.get("patches").is_none());
    }
    std::fs::write(&index_path, &saved_index).unwrap();
    // Revoke the actual run after the SDK read, while its owned native job still retains Original.
    let (started, entered) = std::sync::mpsc::channel();
    let (release, resume) = oneshot::channel();
    *server.0.before_delivery.lock().unwrap() = Some((started, resume));
    let pending = tokio::spawn(invoke("graphit_git_status", json!({"input":{}})));
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    issuer.close_run(&parent, &run).unwrap();
    release.send(()).unwrap();
    let response = pending.await.unwrap();
    assert_eq!(response.is_error, Some(true));
    let value: Value = typed_value(&response);
    assert_eq!(value["io_kind"], "PermissionDenied");
    assert!(value.get("entries").is_none());
    // Expire the actual request while its blocking owner is retained; shutdown must join it.
    while server.0.jobs.lock().await.join_next().await.is_some() {}
    Arc::get_mut(&mut server.0).unwrap().task.timeout = Duration::from_millis(100);
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::InspectGit],
        Duration::from_secs(90),
    );
    let (started, entered) = std::sync::mpsc::channel();
    let (release, resume) = oneshot::channel();
    *server.0.gate.lock().unwrap() = Some((started, resume));
    let service = server.clone();
    let credential = role_credential(&reference, &bearer, server.0.task.content);
    let pending = tokio::spawn(async move {
        service
            .call(
                "graphit_git_status".into(),
                serde_json::from_value(json!({"input":{}})).unwrap(),
                credential,
                CancellationToken::new(),
            )
            .await
    });
    let cancel = entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let response = pending.await.unwrap();
    assert_eq!(response.is_error, Some(true));
    assert!(cancel.is_cancelled());
    release.send(()).unwrap();
    server.shutdown().await.unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
}

#[test]
fn git_inspection_tracker_renderer_keeps_native_shell_closed_and_generic_negative() {
    use graphit_hub::native::{
        AgentDescriptor, ContentError, NativeAgentRuntime, NativeToolGrant, RenderContext,
        ToolAccess, ToolGrants,
        mcp_config::{NativeServerSpec, NativeServerTransport},
        render_agent, render_role_agent_with_mcp,
    };
    let content = ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    };
    {
        let binding = NativeBinding::KiroV3;
        let cancel = Cancellation::default();
        let mut context =
            RenderContext::new(content, &cancel, Instant::now() + Duration::from_secs(5)).unwrap();
        let descriptor = AgentDescriptor::new(
            "graphit-tracker",
            "Readonly Git inspection",
            "Static role instructions",
            true,
            true,
            &mut context,
        )
        .unwrap();
        let names: &[&str] = &["read"];
        let grants = ToolGrants {
            binding,
            read_only: true,
            allows_shell: true,
            tools: names
                .iter()
                .map(|name| NativeToolGrant {
                    name: (*name).into(),
                    access: ToolAccess::Read,
                })
                .collect(),
        };
        let plan = NativeServerSpec {
            name: "graphit".into(),
            transport: NativeServerTransport::Stdio {
                command: "graphit".into(),
                args: vec!["mcp".into(), "--stdio".into(), "--role".into()],
                env: std::collections::BTreeMap::new(),
                env_vars: Vec::new(),
                cwd: None,
            },
        }
        .plan(binding)
        .unwrap();
        assert!(matches!(
            render_agent(binding, &descriptor, None, &grants, &mut context),
            Err(ContentError::UnsupportedCapability)
        ));
        assert!(matches!(
            render_role_agent_with_mcp(
                binding,
                Role::Scout,
                &descriptor,
                None,
                &grants,
                NativeAgentRuntime {
                    mcp: &plan,
                    kiro_hooks: &[]
                },
                &mut context
            ),
            Err(ContentError::UnsupportedCapability)
        ));
        let rendered = render_role_agent_with_mcp(
            binding,
            Role::Tracker,
            &descriptor,
            None,
            &grants,
            NativeAgentRuntime {
                mcp: &plan,
                kiro_hooks: &[],
            },
            &mut context,
        )
        .unwrap();
        let native = String::from_utf8(rendered.bytes).unwrap();

        for capability in ["fs_write", "shell"] {
            let lines: Vec<_> = native.lines().collect();
            let index = lines
                .iter()
                .position(|line| line.contains(&format!("capability: {capability}")))
                .unwrap();
            assert!(
                lines[index + 1..]
                    .iter()
                    .take(2)
                    .any(|line| line.trim() == "effect: deny")
            );
        }
        assert!(native.contains("includePowers: false"));

        let descriptor = AgentDescriptor::new(
            "graphit-tracker",
            "wrong mutable declaration",
            "Static",
            false,
            true,
            &mut context,
        )
        .unwrap();
        let grants = ToolGrants {
            read_only: false,
            tools: Vec::new(),
            ..grants
        };
        assert!(matches!(
            render_role_agent_with_mcp(
                binding,
                Role::Tracker,
                &descriptor,
                None,
                &grants,
                NativeAgentRuntime {
                    mcp: &plan,
                    kiro_hooks: &[]
                },
                &mut context
            ),
            Err(ContentError::UnsupportedCapability)
        ));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_role_live_selection_add_switch_and_current_rescan() {
    let (_temp, server, raw) = fixture();
    // Both roots are already trusted by the actual fixture issuer. Membership adds no trust.
    let projects = Arc::clone(&project_snapshot(&server).projects);
    let live = Arc::new(crate::live_project_selections::LiveProjectSelections::new(
        Arc::new(vec![projects[0].clone()]),
        0,
    ));
    let permissions = [Permission::Read(Module::Memory), Permission::WriteMemory];
    let (_first_session, _first_run, first_reference, first_bearer) = credential_at(
        &server,
        &raw,
        0,
        Role::Scribe,
        &permissions,
        Duration::from_secs(180),
    );
    let (_peer_session, _peer_run, peer_reference, peer_bearer) = credential_at(
        &server,
        &raw,
        1,
        Role::Scribe,
        &permissions,
        Duration::from_secs(180),
    );
    let (first_owner, mut first_batches) = memory_rescan_owner(memory_context(&server, 0)).await;
    let peer_context = memory_context(&server, 1);
    let server = server.with_live_projects(Arc::clone(&live)).unwrap();
    server
        .attach_memory_rescans(Arc::from([first_owner.rescan_handle()]))
        .unwrap();
    let (uri, serving) = serve(&server).await;
    let first = connect(
        &uri,
        role_credential(&first_reference, &first_bearer, server.0.task.content),
    )
    .await;
    assert!(!first.list_all_tools().await.unwrap().is_empty());
    live.replace(Arc::clone(&projects), 1).unwrap();
    let peer = connect(
        &uri,
        role_credential(&peer_reference, &peer_bearer, server.0.task.content),
    )
    .await;
    assert!(!peer.list_all_tools().await.unwrap().is_empty());
    assert!(first.list_all_tools().await.is_err());
    let denied = call(&first, "graphit_memory_list", json!({"input":{}}), true).await;
    assert_eq!(denied["code"], "authentication_or_permission_denied");
    let (peer_owner, mut peer_batches) = memory_rescan_owner(peer_context).await;
    // Publish the current owner's handles after the HTTP handler was already cloned.
    server
        .attach_memory_rescans(Arc::from([peer_owner.rescan_handle()]))
        .unwrap();
    let created = call(&peer, "graphit_memory_insert", json!({"input":{
        "idempotency_key":"live-peer", "title":"Current peer", "body":"actual peer body", "classification":"decision"
    }}), false).await;
    expect_memory_rescan(&mut peer_batches).await;
    expect_no_memory_rescan(&mut first_batches).await;
    let found = call(
        &peer,
        "graphit_memory_get",
        json!({"input":{"id":created["result"]["id"]}}),
        false,
    )
    .await;
    assert_eq!(found["memory"]["body"], "actual peer body");
    let foreign = call(&peer, "graphit_memory_list", json!({
        "selection":{"project_dir":graphit_core::native_path::NativePath::new(projects[0].0.root().to_owned()).unwrap()}, "input":{}
    }), true).await;
    assert_eq!(foreign["code"], "authentication_or_permission_denied");
    live.retire_checkout(projects[1].1, projects[1].0.root())
        .unwrap();
    assert!(peer.list_all_tools().await.is_err());
    assert!(first.list_all_tools().await.is_ok());
    first.cancel().await.unwrap();
    peer.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
    first_owner.stop().await.unwrap();
    peer_owner.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_role_live_retirement_gates_before_store_and_after_confirmed_write() {
    let (_temp, server, raw) = fixture();
    let projects = Arc::clone(&project_snapshot(&server).projects);
    let live = Arc::new(crate::live_project_selections::LiveProjectSelections::new(
        Arc::clone(&projects),
        0,
    ));
    let server = server.with_live_projects(Arc::clone(&live)).unwrap();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteMemory],
        Duration::from_secs(180),
    );
    for after_commit in [false, true] {
        live.replace(Arc::clone(&projects), 0).unwrap();
        let (entered, observed) = std::sync::mpsc::channel();
        let (release, waiting) = oneshot::channel();
        let gate = if after_commit {
            &server.0.before_delivery
        } else {
            &server.0.gate
        };
        *gate.lock().unwrap() = Some((entered, waiting));
        let handler = server.clone();
        let credential = role_credential(&reference, &bearer, server.0.task.content);
        let key = if after_commit {
            "retired-confirmed"
        } else {
            "retired-before"
        };
        let pending = tokio::spawn(async move {
            handler.call("graphit_memory_insert".into(), serde_json::from_value(json!({"input":{
                "idempotency_key":key,"title":key,"body":"actual native publication","classification":"decision"
            }})).unwrap(), credential, CancellationToken::new()).await
        });
        tokio::task::spawn_blocking(move || {
            observed.recv_timeout(Duration::from_secs(10)).unwrap()
        })
        .await
        .unwrap();
        live.retire_checkout(projects[0].1, projects[0].0.root())
            .unwrap();
        release.send(()).unwrap();
        let result = pending.await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let value: Value = typed_value(&result);
        if after_commit {
            assert_eq!(value["code"], "write_confirmed");
            assert_eq!(value["cause"]["code"], "selection_unavailable");
            assert!(value["memory"]["id"].is_string());
            assert_eq!(value["memory"]["revision"], 1);
        } else {
            assert_eq!(value["code"], "selection_unavailable");
        }
    }
    // Reinstating operational membership leaves the original real grant unchanged.
    live.replace(Arc::clone(&projects), 0).unwrap();
    let result = server
        .call(
            "graphit_memory_list".into(),
            serde_json::from_value(json!({"input":{}})).unwrap(),
            role_credential(&reference, &bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(result.is_error, Some(false));
    let value: Value = typed_value(&result);
    assert!(
        serde_json::to_string(&value)
            .unwrap()
            .contains("retired-confirmed")
    );
    assert!(
        !serde_json::to_string(&value)
            .unwrap()
            .contains("retired-before")
    );
    server.shutdown().await.unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_bootstrap_http_live_context_add_switch_versions_and_current_modules() {
    use graphit_core::config::{ConfigKey, ConfigStore};
    let (temp, server, raw) = fixture_with_launch(NativeBinding::KiroV3, true);
    let peer = seed_role_prompt_memory(&server, &raw, 1).await;
    let projects = Arc::clone(&project_snapshot(&server).projects);
    let live = Arc::new(crate::live_project_selections::LiveProjectSelections::new(
        Arc::new(vec![projects[0].clone()]),
        0,
    ));
    let limits = server.0.policy.memory_read;
    let stop = CancellationToken::new();
    let mut http = crate::managed_roles::http::ManagedRolesHttp::new(
        Arc::clone(&server.0.auth),
        Arc::clone(&projects),
        0,
        server.0.policy.source.clone(),
        stop.clone(),
    );
    http.attach_context(server.0.policy.config.clone(), limits, Arc::clone(&live))
        .unwrap();
    let socket = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = socket.local_addr().unwrap();
    let router = http.router();
    let stopping = stop.clone();
    let serving = tokio::spawn(async move {
        axum::serve(socket, router)
            .with_graceful_shutdown(stopping.cancelled_owned())
            .await
    });
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let initial = http.capture_context(&original, None).unwrap();
    assert_eq!(initial.0.projects.len(), 1);
    let feature = VersionContext::select(
        &projects[1].0,
        graphit_core::version_context::Checkout::Branch {
            name: "feature",
            default: "main",
        },
        "",
        None,
        "",
    )
    .unwrap();
    let active = Arc::new(vec![
        projects[0].clone(),
        projects[1].clone(),
        (projects[1].0.clone(), projects[1].1, feature.clone()),
    ]);
    live.replace(Arc::clone(&active), 2).unwrap();
    let captured = http.capture_context(&original, None).unwrap();
    assert_eq!(captured.0.projects.len(), 3);
    assert_eq!(captured.0.current, Some(2));
    let mut input = launch_input(&server);
    input.install.project_dir = Some(projects[1].0.root().to_owned());
    input.install.project_version = None;
    let selected = crate::managed_roles::select_project(
        &captured.0.projects,
        captured.0.current.unwrap(),
        &input.install,
    )
    .unwrap();
    assert_eq!(selected.2, feature);
    let published = loopback_client()
        .post(format!("http://{address}/admin/roles"))
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(&raw)
        .json(&input.install)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(published["failure"].is_null(), "{published}");
    let expected = json!({"project_version":projects[1].2.version(),"memory_state":"published","memories":peer.iter().map(|memory|json!({"id":memory.id,"revision":memory.revision,"title":memory.title,"body":memory.body,"classification":"convention","mandatory":true,"important":memory.attributes.important})).collect::<Vec<_>>()});
    input.install.project_version = Some(projects[1].2.version().to_owned());
    for enabled in [true, false] {
        ConfigStore::project(&projects[1].0)
            .unwrap()
            .set(
                ConfigKey::new("modules.memory").unwrap(),
                json!(enabled),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
        let (snapshot, modules, _) = http.capture_context(&original, None).unwrap();
        assert_eq!(modules[1].enabled(Module::Memory), enabled);
        assert_eq!(modules[2].enabled(Module::Memory), enabled);
        let payload = if enabled { &expected } else { &Value::Null };
        let program = role_prompt_peer(temp.path(), payload, input.role);
        let execution = crate::managed_role_run::execute_with_program(
            crate::managed_roles::InstallResources {
                auth: &server.0.auth,
                global_paths: None,
                projects: &snapshot.projects,
                current: snapshot.current.unwrap(),
                policy: &server.0.policy.source,
                modules: &modules,
                memory_read: limits,
            },
            &raw,
            &input,
            &original,
            &program,
        )
        .await;
        assert!(
            execution.cause.is_none(),
            "cause kind: {:?}",
            execution.cause.as_ref().map(io::Error::kind)
        );
        assert!(
            execution.native.is_none(),
            "controlled ACP context peer failed"
        );
        assert!(execution.outcome.run_closed);
        assert!(execution.residual.is_none());
        assert_eq!(
            execution.outcome.answer.as_deref(),
            Some("context verified")
        );
    }
    assert_eq!(captured.0.projects.len(), 3);
    assert!(captured.1[1].enabled(Module::Memory));
    let retained_program = role_prompt_peer(temp.path(), &expected, input.role);
    let retained = crate::managed_role_run::execute_with_program(
        crate::managed_roles::InstallResources {
            auth: &server.0.auth,
            global_paths: None,
            projects: &captured.0.projects,
            current: captured.0.current.unwrap(),
            policy: &server.0.policy.source,
            modules: &captured.1,
            memory_read: limits,
        },
        &raw,
        &input,
        &original,
        &retained_program,
    )
    .await;
    assert!(
        retained.cause.is_none(),
        "cause kind: {:?}",
        retained.cause.as_ref().map(io::Error::kind)
    );
    assert!(
        retained.native.is_none(),
        "controlled ACP retained frame peer failed"
    );
    assert!(retained.outcome.run_closed);
    assert!(retained.residual.is_none());
    assert_eq!(retained.outcome.answer.as_deref(), Some("context verified"));
    live.retire_checkout(projects[1].1, projects[1].0.root())
        .unwrap();
    let denied = loopback_client()
        .post(format!("http://{address}/admin/roles"))
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(&raw)
        .json(&input.install)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(denied["failure"].is_string(), "{denied}");
    assert_eq!(denied["roles"].as_array().unwrap().len(), 0);
    stop.cancel();
    serving.await.unwrap().unwrap();
    http.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_role_live_git_retirement_before_inspection_and_delivery() {
    let (_temp, server, raw) = fixture();
    let projects = Arc::clone(&project_snapshot(&server).projects);
    git_fixture_command(projects[0].0.root(), &["init", "-q"]);
    let live = Arc::new(crate::live_project_selections::LiveProjectSelections::new(
        Arc::clone(&projects),
        0,
    ));
    let server = server.with_live_projects(Arc::clone(&live)).unwrap();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::InspectGit],
        Duration::from_secs(180),
    );
    for after_read in [false, true] {
        live.replace(Arc::clone(&projects), 0).unwrap();
        let (entered, observed) = std::sync::mpsc::channel();
        let (release, waiting) = oneshot::channel();
        let gate = if after_read {
            &server.0.before_delivery
        } else {
            &server.0.gate
        };
        *gate.lock().unwrap() = Some((entered, waiting));
        let handler = server.clone();
        let credential = role_credential(&reference, &bearer, server.0.task.content);
        let pending = tokio::spawn(async move {
            handler
                .call(
                    "graphit_git_status".into(),
                    serde_json::from_value(json!({"input":{}})).unwrap(),
                    credential,
                    CancellationToken::new(),
                )
                .await
        });
        tokio::task::spawn_blocking(move || {
            observed.recv_timeout(Duration::from_secs(10)).unwrap()
        })
        .await
        .unwrap();
        live.retire_checkout(projects[0].1, projects[0].0.root())
            .unwrap();
        release.send(()).unwrap();
        let result = pending.await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let value: Value = typed_value(&result);
        assert_eq!(value["code"], "selection_unavailable");
        assert!(value.get("entries").is_none());
    }
    server.shutdown().await.unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
}

#[cfg(unix)]
fn role_hook_v3_factory_credential(
    server: &RoleToolServer,
    raw: &str,
    selected: crate::managed_roles::ManagedRoleName,
) -> (
    graphit_auth::AdminSessionHandle,
    graphit_auth::RunHandle,
    RoleCredentialReference,
    graphit_auth::RoleBearer,
) {
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let snapshot = project_snapshot(server);
    let (paths, id, version) = &snapshot.projects[0];
    let input = crate::managed_roles::InstallRequest {
        destination: crate::managed_roles::InstallDestination::Project,
        project_dir: Some(paths.root().to_owned()),
        project_version: Some(version.version().to_owned()),
        binding: Some(NativeBinding::KiroV3),
        roles: vec![selected],
        skills_only: false,
        rules: vec![],
        skill_aliases: Default::default(),
        skill_bundles: Default::default(),
        module_skill: None,
    };
    let docs_dir = crate::compiled_skills::docs_dir(
        &server.0.policy.config,
        server.0.policy.source.content,
        &original,
    )
    .unwrap();
    let installed = crate::managed_roles::execute_install(
        crate::managed_roles::InstallResources {
            auth: &server.0.auth,
            global_paths: None,
            projects: &snapshot.projects,
            current: 0,
            policy: &server.0.policy.source,
            modules: &[],
            memory_read: server.0.policy.memory_read,
        },
        raw,
        &input,
        Some(&docs_dir),
        &original,
        None,
    );
    assert!(
        installed.cause.is_none(),
        "actual role publication failed: {:?}",
        installed
            .cause
            .as_ref()
            .map(|cause| (cause.kind(), cause.to_string()))
    );
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    // Recompute the same factory data without configuring another definition.
    let compiled = crate::managed_roles::compile_definition(
        Some(paths),
        selected,
        &input,
        &server.0.policy.source,
        &parent,
        issuer,
        &original,
    )
    .unwrap();
    let reference = compiled.definition.reference().clone();
    let profile_text =
        std::fs::read_to_string(paths.root().join(&compiled.native.relative_entry)).unwrap();
    let header = profile_text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n")
        .unwrap()
        .0;
    let profile: Value = serde_saphyr::from_str(header).unwrap();
    for event in ["SessionStart", "UserPromptSubmit"] {
        let command = profile["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|hook| hook["trigger"] == event)
            .unwrap()["action"]["command"]
            .as_str()
            .unwrap();
        let argv = shlex::split(command).unwrap();
        assert!(argv.windows(2).any(|pair| pair == ["--recipient", "role"]));
        assert!(argv.windows(2).any(|pair| pair == ["--event", event]));
        assert!(!command.contains("GRAPHIT_ROLE_BEARER"));
        assert!(!command.contains("Standing context"));
    }
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.task.scope, original.snapshot())
            .unwrap();
    let record = project.binding().unwrap();
    let captured = scope(server, &project, &record, version);
    let permissions = if selected.role() == graphit_runtime::ManagedRole::Tracker {
        vec![Permission::InspectGit]
    } else {
        vec![Permission::Read(Module::Memory)]
    };
    let run = issuer
        .start_run(&parent, &captured, Duration::from_secs(60))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &permissions, &captured)
        .unwrap();
    (session, run, reference, bearer)
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hook_v3_actual_factory_catalogue_complete_memory_and_tracker_quiet() {
    let (_temp, server, raw) = fixture_with_launch(NativeBinding::KiroV3, true);
    let expected = seed_role_prompt_memory(&server, &raw, 0).await;
    let (uri, serving) = serve(&server).await;
    for role in [
        crate::managed_roles::ManagedRoleName::Scout,
        crate::managed_roles::ManagedRoleName::Scribe,
        crate::managed_roles::ManagedRoleName::Tracker,
    ] {
        let (_session, _run, reference, bearer) =
            role_hook_v3_factory_credential(&server, &raw, role);
        let client = connect(
            &uri,
            role_credential(&reference, &bearer, server.0.task.content),
        )
        .await;
        assert!(
            client
                .peer()
                .list_all_tools()
                .await
                .unwrap()
                .iter()
                .any(|tool| tool.name == "graphit_session_hook")
        );
        for event in ["SessionStart", "UserPromptSubmit"] {
            let result = client
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_session_hook").with_arguments(
                        json!({"input":{"engine":"kiro","event":event}})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(false));
            assert_eq!(result.content.len(), 1);
            let text = &result.content[0].as_text().unwrap().text;
            if role.role() == graphit_runtime::ManagedRole::Tracker {
                assert!(text.is_empty());
            } else {
                let data: Value = serde_json::from_str(
                    text.strip_prefix("[Standing context]\nPROJECT mandatory Memory:\n")
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(data["memory_state"], "published");
                assert_eq!(data["memories"].as_array().unwrap().len(), expected.len());
                for (row, memory) in data["memories"].as_array().unwrap().iter().zip(&expected) {
                    assert_eq!(row["id"], json!(memory.id));
                    assert_eq!(row["body"], memory.body);
                    assert_eq!(row["mandatory"], true);
                }
                assert!(data.get("task").is_none());
                assert!(data.get("user").is_none());
            }
        }
        client.cancel().await.unwrap();
    }
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
#[cfg(unix)]
#[tokio::test]
async fn role_hook_v3_authentication_scope_modules_absence_errors_and_bounds() {
    use graphit_core::config::{ConfigKey, ConfigStore};
    let (_temp, mut server, raw) = fixture();
    let (session, run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        Duration::from_secs(60),
    );
    let input = || {
        serde_json::from_value::<Arguments>(
            json!({"input":{"engine":"kiro","event":"SessionStart"}}),
        )
        .unwrap()
    };
    let original_credential = || role_credential(&reference, &bearer, server.0.task.content);
    let absent = server
        .call(
            "graphit_session_hook".into(),
            input(),
            original_credential(),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(absent.is_error, Some(false));
    assert!(
        absent.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("\"memory_state\":\"no_corpus\"")
    );
    let wrong = RoleCredentialReference::new(Role::Tracker, reference.source().clone(), [9; 32]);
    for args in [
        json!({"input":{"engine":"kiro","event":"SessionStart","v3":true}}),
        json!({"selection":{"project_version":"other"},"input":{"engine":"kiro","event":"SessionStart"}}),
        json!({"input":{"engine":"kiro","event":"SessionStart","recipient":"tracker"}}),
    ] {
        assert_eq!(
            server
                .call(
                    "graphit_session_hook".into(),
                    serde_json::from_value(args).unwrap(),
                    original_credential(),
                    CancellationToken::new()
                )
                .await
                .is_error,
            Some(true)
        );
    }
    assert_eq!(
        server
            .call(
                "graphit_session_hook".into(),
                input(),
                role_credential(&wrong, &bearer, server.0.task.content),
                CancellationToken::new()
            )
            .await
            .is_error,
        Some(true)
    );
    assert_eq!(
        server
            .call(
                "graphit_session_hook".into(),
                input(),
                Credential {
                    role_binding: None,
                    bearer: Zeroizing::new(raw.clone()),
                    origin: Origin::Runtime
                },
                CancellationToken::new()
            )
            .await
            .is_error,
        Some(true)
    );
    let cancel = Cancellation::default();
    let snapshot = project_snapshot(&server);
    let config = ConfigStore::project(&snapshot.projects[0].0).unwrap();
    for module in ["modules.memory", "modules.hooks"] {
        config
            .set(
                ConfigKey::new(module).unwrap(),
                json!(false),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
        let quiet = server
            .call(
                "graphit_session_hook".into(),
                input(),
                original_credential(),
                CancellationToken::new(),
            )
            .await;
        assert_eq!(quiet.is_error, Some(false));
        assert!(quiet.content[0].as_text().unwrap().text.is_empty());
        assert_eq!(
            server
                .call(
                    "graphit_session_hook".into(),
                    input(),
                    role_credential(&wrong, &bearer, server.0.task.content),
                    CancellationToken::new()
                )
                .await
                .is_error,
            Some(true)
        );
        config
            .set(
                ConfigKey::new(module).unwrap(),
                json!(true),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
    }
    seed_role_prompt_memory(&server, &raw, 0).await;
    let maximum = server.0.policy.source.content.max_output_bytes;
    Arc::get_mut(&mut server.0)
        .unwrap()
        .policy
        .source
        .content
        .max_output_bytes = 8;
    let bounded = server
        .call(
            "graphit_session_hook".into(),
            input(),
            role_credential(&reference, &bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(bounded.is_error, Some(true));
    let failure = typed_value(&bounded);
    assert_eq!(failure["code"], "hook_context_failed");
    assert!(
        !serde_json::to_string(&failure)
            .unwrap()
            .contains("Standing context")
    );
    let original = request(&cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    issuer.close_run(&parent, &run).unwrap();
    Arc::get_mut(&mut server.0)
        .unwrap()
        .policy
        .source
        .content
        .max_output_bytes = maximum;
    assert_eq!(
        server
            .call(
                "graphit_session_hook".into(),
                input(),
                role_credential(&reference, &bearer, server.0.task.content),
                CancellationToken::new()
            )
            .await
            .is_error,
        Some(true)
    );
    server.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hook_v3_original_cancel_drop_and_transport_owner_join() {
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        Duration::from_secs(60),
    );
    let socket = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let address = socket.address();
    let socket = socket.into_listener();
    socket.set_nonblocking(true).unwrap();
    let socket = tokio::net::TcpListener::from_std(socket).unwrap();
    let service = server.clone();
    let serving = tokio::spawn(async move {
        axum::serve(socket, Router::new().nest("/roles", service.router()))
            .with_graceful_shutdown(service.0.stop.clone().cancelled_owned())
            .await
            .unwrap();
        service.shutdown().await.unwrap();
    });
    let startup_cancel = Cancellation::default();
    let startup = request(&startup_cancel, server.0.task.content);
    let reference = String::from_utf8(
        reference
            .encode(server.0.task.content.max_input_bytes, &startup)
            .unwrap(),
    )
    .unwrap();
    let hub = &server.0.policy.source.native_install.hub;
    for mode in ["cancel", "drop", "deadline"] {
        let proxy = crate::role_proxy::RoleProxy::new(
            address,
            loopback_client(),
            (
                server.0.task.content,
                hub.projection(),
                Duration::from_secs(60),
            ),
            (
                secrecy::SecretString::from(bearer.expose_for_transport().to_owned()),
                &reference,
                None,
                None,
            ),
            &startup,
        )
        .await
        .unwrap();
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            server.0.task.content,
            &cancel,
            Instant::now()
                + if mode == "deadline" {
                    Duration::from_secs(1)
                } else {
                    Duration::from_secs(60)
                },
        )
        .unwrap();
        let (started, ready) = std::sync::mpsc::channel();
        let (release, waiting) = oneshot::channel();
        *server.0.gate.lock().unwrap() = Some((started, waiting));
        let mut call = Box::pin(
            proxy.call_tool_original(
                CallToolRequestParams::new("graphit_session_hook").with_arguments(
                    json!({"input":{"engine":"kiro","event":"SessionStart"}})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
                &original,
            ),
        );
        let ready = tokio::task::spawn_blocking(move || {
            ready.recv_timeout(Duration::from_secs(5)).unwrap()
        });
        tokio::select! { result = ready => { result.unwrap(); }, _ = &mut call => panic!("actual request ended before the authentication gate") }
        match mode {
            "drop" => drop(call),
            "cancel" => {
                cancel.cancel();
                assert_eq!(call.await.unwrap_err().kind(), io::ErrorKind::Interrupted);
            }
            "deadline" => {
                assert_eq!(call.await.unwrap_err().kind(), io::ErrorKind::TimedOut);
            }
            _ => unreachable!("closed test modes"),
        }
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), proxy.shutdown())
            .await
            .unwrap()
            .unwrap();
    }
    server.shutdown().await.unwrap();
    serving.await.unwrap();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hook_v3_run_pins_selected_version_when_input_omits_it() {
    let (temp, server, raw) = fixture_with_launch(NativeBinding::KiroV3, true);
    let initial = project_snapshot(&server);
    let selected = &initial.projects[1];
    let feature = VersionContext::select(
        &selected.0,
        graphit_core::version_context::Checkout::Branch {
            name: "feature",
            default: "main",
        },
        "",
        None,
        "",
    )
    .unwrap();
    let active = vec![
        selected.clone(),
        (selected.0.clone(), selected.1, feature.clone()),
    ];
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let modules = active
        .iter()
        .map(|_| {
            graphit_core::module_settings::ModuleSettings::resolve(&server.0.policy.config).unwrap()
        })
        .collect::<Vec<_>>();
    let mut input = launch_input(&server);
    input.role = crate::managed_roles::ManagedRoleName::Scout;
    input.install.roles = vec![input.role];
    input.install.project_dir = Some(selected.0.root().to_owned());
    input.install.project_version = None;
    let expected =
        json!({"project_version":feature.version(),"memory_state":"no_corpus","memories":[]});
    let peer = role_prompt_peer(temp.path(), &expected, input.role);
    let result = crate::managed_role_run::execute_with_program(
        crate::managed_roles::InstallResources {
            auth: &server.0.auth,
            global_paths: None,
            projects: &active,
            current: 1,
            policy: &server.0.policy.source,
            modules: &modules,
            memory_read: server.0.policy.memory_read,
        },
        &raw,
        &input,
        &original,
        &peer,
    )
    .await;
    assert!(
        result.cause.is_none(),
        "actual version-selected run failed: {:?}",
        result
            .cause
            .as_ref()
            .map(|cause| (cause.kind(), cause.to_string()))
    );
    assert!(result.native.is_none());
    assert!(result.outcome.run_closed);
    let bytes = std::fs::read(selected.0.root().join(".kiro/agents/graphit-scout.md")).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let header = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n")
        .unwrap()
        .0;
    let profile: Value = serde_saphyr::from_str(header).unwrap();
    for event in ["SessionStart", "UserPromptSubmit"] {
        let hook = profile["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|hook| hook["trigger"] == event)
            .unwrap();
        let argv = shlex::split(hook["action"]["command"].as_str().unwrap()).unwrap();
        assert!(
            argv.windows(2)
                .any(|pair| pair == ["--project-version", feature.version()])
        );
    }
    server.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_hook_v3_published_empty_store_failure_expiry_and_retirement() {
    let (_temp, server, raw) = fixture();
    let memories = seed_role_prompt_memory(&server, &raw, 0).await;
    let (_scribe_session, _scribe_run, scribe_ref, scribe_bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteMemory],
        Duration::from_secs(60),
    );
    for memory in &memories {
        let result = server.call("graphit_memory_unmark_mandatory".into(),
            serde_json::from_value(json!({"input":{"id":memory.id,"expected_revision":memory.revision,
                "idempotency_key":format!("hook-empty-{}",memory.id),"reason":"Qualify published empty mandatory selection"}})).unwrap(),
            role_credential(&scribe_ref,&scribe_bearer,server.0.task.content),CancellationToken::new()).await;
        assert_eq!(result.is_error, Some(false));
    }
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        Duration::from_secs(60),
    );
    let args = || {
        serde_json::from_value::<Arguments>(
            json!({"input":{"engine":"kiro","event":"SessionStart"}}),
        )
        .unwrap()
    };
    let empty = server
        .call(
            "graphit_session_hook".into(),
            args(),
            role_credential(&reference, &bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(empty.is_error, Some(false));
    let text = &empty.content[0].as_text().unwrap().text;
    let empty: Value = serde_json::from_str(
        text.strip_prefix("[Standing context]\nPROJECT mandatory Memory:\n")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(empty["memory_state"], "published");
    assert_eq!(empty["memories"], json!([]));
    let snapshot = project_snapshot(&server);
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let (paths, id, version) = &snapshot.projects[0];
    let scope =
        CapturedPublicationScope::project(paths, *id, server.0.task.scope, original.snapshot())
            .unwrap();
    let relative = graphit_core::local_corpus::resolve(
        &scope,
        version.version(),
        "memory",
        "index.lance",
        server.0.policy.memory_read.native,
        original.snapshot(),
    )
    .unwrap();
    let corpus = scope.runtime_root().join(relative);
    let retained = corpus.with_file_name("index-retained.lance");
    std::fs::rename(&corpus, &retained).unwrap();
    std::fs::write(&corpus, b"actual invalid native corpus shape").unwrap();
    let failed = server
        .call(
            "graphit_session_hook".into(),
            args(),
            role_credential(&reference, &bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(failed.is_error, Some(true));
    let failed: Value = typed_value(&failed);
    assert_eq!(failed["code"], "memory_read_failed");
    std::fs::remove_file(&corpus).unwrap();
    std::fs::rename(&retained, &corpus).unwrap();
    let (_expired_session, _expired_run, expired_ref, expired_bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        Duration::from_millis(20),
    );
    tokio::time::sleep(Duration::from_millis(30)).await;
    let expired = server
        .call(
            "graphit_session_hook".into(),
            args(),
            role_credential(&expired_ref, &expired_bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(expired.is_error, Some(true));
    let expired: Value = typed_value(&expired);
    assert_eq!(expired["code"], "authentication_or_permission_denied");
    let (started, ready) = std::sync::mpsc::channel();
    let (release, waiting) = oneshot::channel();
    *server.0.gate.lock().unwrap() = Some((started, waiting));
    let mut call = Box::pin(server.call(
        "graphit_session_hook".into(),
        args(),
        role_credential(&reference, &bearer, server.0.task.content),
        CancellationToken::new(),
    ));
    let ready =
        tokio::task::spawn_blocking(move || ready.recv_timeout(Duration::from_secs(5)).unwrap());
    tokio::select! { result=ready=>{result.unwrap();}, _=&mut call=>panic!("request ended before authenticated retirement gate") }
    let projects = project_snapshot(&server);
    server
        .0
        .live_projects
        .replace(Arc::new(vec![projects.projects[1].clone()]), 0)
        .unwrap();
    release.send(()).unwrap();
    let retired = call.await;
    assert_eq!(retired.is_error, Some(true));
    let retired: Value = typed_value(&retired);
    assert_eq!(retired["code"], "selection_unavailable");
    server.shutdown().await.unwrap();
}

mod ast;
#[cfg(target_os = "linux")]
mod hub;
#[cfg(target_os = "linux")]
mod instructions;
mod knowledge_search;
#[cfg(target_os = "linux")]
mod references;
#[cfg(target_os = "linux")]
mod sync_history;

#[tokio::test]
async fn role_socket_output_modes_preserve_typed_results_errors_and_hook_contract() {
    use graphit_core::structured_output::{Limits, OutputFormat, encode};
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        Duration::from_secs(60),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let tools = client.list_all_tools().await.unwrap();
    let schema = &tools
        .iter()
        .find(|tool| tool.name == "graphit_memory_list")
        .unwrap()
        .input_schema;
    assert_eq!(schema["properties"]["ai_optimized"]["type"], "boolean");
    assert_eq!(schema["properties"]["ai_optimized"]["default"], true);
    let hook = tools
        .iter()
        .find(|tool| tool.name == "graphit_session_hook")
        .unwrap();
    assert!(
        hook.input_schema["properties"]
            .get("ai_optimized")
            .is_none()
    );
    let json = client
        .call_tool(
            CallToolRequestParams::new("graphit_memory_list").with_arguments(
                json!({"input":{},"ai_optimized":false})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(json.is_error, Some(false));
    let value = json.structured_content.as_ref().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&json.content[0].as_text().unwrap().text).unwrap(),
        *value
    );
    let expected = encode(
        value,
        OutputFormat::Toon,
        Limits {
            max_depth: 64,
            max_nodes: 10_000,
            max_input_bytes: 1_048_576,
            max_output_bytes: 1_048_576,
        },
    )
    .unwrap();
    for arguments in [json!({"input":{}}), json!({"input":{},"ai_optimized":true})] {
        let result = client
            .call_tool(
                CallToolRequestParams::new("graphit_memory_list")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        assert!(result.structured_content.is_none());
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].as_text().unwrap().text, expected.as_str());
    }
    for invalid in [Value::Null, json!("true"), json!(1)] {
        assert!(
            client
                .call_tool(
                    CallToolRequestParams::new("graphit_memory_list").with_arguments(
                        json!({"input":{},"ai_optimized":invalid})
                            .as_object()
                            .unwrap()
                            .clone()
                    )
                )
                .await
                .is_err()
        );
    }
    let denied = client
        .call_tool(
            CallToolRequestParams::new("graphit_memory_insert").with_arguments(
                json!({"input":{"idempotency_key":"toon-denied","title":"never admitted","body":"denied","classification":"decision"}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    assert!(denied.structured_content.is_none());
    assert!(
        denied.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("authentication_or_permission_denied")
    );
    let credential = role_credential(&reference, &bearer, server.0.task.content);
    let internal = server
        .call(
            "graphit_memory_list".into(),
            serde_json::from_value(json!({"input":{}})).unwrap(),
            credential,
            CancellationToken::new(),
        )
        .await;
    assert!(internal.structured_content.is_some());
    assert!(internal.content.is_empty());
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn canonical_installation_binding_pins_global_reads_and_rejects_foreign_root() {
    let (temp, original_server, _) = fixture();
    let root = original_server
        .0
        .auth
        .installation()
        .live
        .owner_root()
        .to_owned();
    let alias = temp.path().join("installation-alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let foreign = temp.path().join("foreign-installation");
    std::fs::create_dir(&foreign).unwrap();
    for (directory, body) in [(&root, "authorized"), (&foreign, "foreign")] {
        let entry = directory.join("rules/skills/ast/entry.md");
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(entry, body).unwrap();
    }
    let snapshot = original_server.0.live_projects.snapshot().unwrap();
    let mut policy = original_server.0.policy.clone();
    let paths = &policy.source.installation;
    policy.source.installation = InstallationPaths::resolve(
        paths.brand().clone(),
        paths.start_directory(),
        paths.home_directory(),
        Some(&alias),
    )
    .unwrap();
    let bound = RoleToolServer::new(
        Arc::clone(&original_server.0.auth),
        Arc::clone(&snapshot.projects),
        snapshot.current.unwrap(),
        policy.clone(),
        CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(bound.0.policy.source.installation.root(), root);
    std::fs::remove_file(&alias).unwrap();
    std::os::unix::fs::symlink(&foreign, &alias).unwrap();
    let cancel = Cancellation::default();
    let original = request(&cancel, bound.0.policy.source.content);
    let observed = crate::managed_roles::capture_template(
        bound.0.policy.source.installation.root(),
        std::path::Path::new("rules/skills/ast/entry.md"),
        graphit_runtime::instructions::InstructionOrigin::Global,
        bound.0.policy.source.index,
        &original,
    )
    .unwrap();
    assert_eq!(observed.body.as_deref(), Some("authorized"));
    observed.revalidate(&original).unwrap();
    // The input now resolves to a different installation and cannot be rebound.
    let denied = RoleToolServer::new(
        Arc::clone(&original_server.0.auth),
        Arc::clone(&snapshot.projects),
        snapshot.current.unwrap(),
        policy,
        CancellationToken::new(),
    );
    assert!(matches!(denied, Err(error) if error.kind() == io::ErrorKind::InvalidInput));
    bound.shutdown().await.unwrap();
    original_server.shutdown().await.unwrap();
}

#[tokio::test]
async fn role_hub_registry_captures_equivalent_parent_without_rewriting_metadata() {
    let (_temp, original_server, raw) = fixture();
    let snapshot = original_server.0.live_projects.snapshot().unwrap();
    let mut policy = original_server.0.policy.clone();
    let root = policy.source.installation.root();
    // Windows canonical captures use a verbatim prefix; the native configured
    // spelling is also valid without introducing an unsupported linked parent.
    #[cfg(windows)]
    let logical = {
        use std::path::{Component, Prefix};
        let mut components = root.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            panic!("Windows installation prefix expected")
        };
        let Prefix::VerbatimDisk(drive) = prefix.kind() else {
            panic!("canonical Windows fixture must use a verbatim disk prefix")
        };
        let mut native = std::path::PathBuf::from(format!("{}:", char::from(drive)));
        native.extend(components.map(|component| component.as_os_str()));
        native
    };
    #[cfg(not(windows))]
    let logical = root.to_path_buf();
    let logical_paths = InstallationPaths::resolve(
        policy.source.installation.brand().clone(),
        policy.source.installation.start_directory(),
        policy.source.installation.home_directory(),
        Some(&logical),
    )
    .unwrap();
    let StoreOwner::Installation { installation_id } =
        original_server.0.auth.installation().live.owner()
    else {
        panic!("installation scope expected")
    };
    policy.source.registry = HubRegistry::global(
        &logical_paths,
        installation_id,
        policy.source.registry.limits(),
    )
    .unwrap();
    let expected_path = policy.source.registry.path();
    let bound = RoleToolServer::new(
        Arc::clone(&original_server.0.auth),
        Arc::clone(&snapshot.projects),
        snapshot.current.unwrap(),
        policy,
        CancellationToken::new(),
    )
    .unwrap();
    let (_session, _run, reference, bearer) = credential(
        &bound,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Hub)],
        Duration::from_secs(60),
    );
    let response = bound
        .call(
            "graphit_hub_list".into(),
            serde_json::from_value(json!({"input":{"scope":"global_installation","limit":8}}))
                .unwrap(),
            role_credential(&reference, &bearer, bound.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(response.is_error, Some(false));
    assert_eq!(typed_value(&response)["entries"], json!([]));
    assert_eq!(bound.0.policy.source.registry.path(), expected_path);
    assert!(!expected_path.exists());
    bound.shutdown().await.unwrap();
    original_server.shutdown().await.unwrap();
}

mod knowledge_id;
