use super::*;

const TOOL: &str = "graphit_module_skill";
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot, ConfigStore},
};
use rmcp::{
    RoleClient,
    model::{CallToolRequest, ClientRequest, ProtocolVersion},
    service::{ClientLifecycleMode, ClientServiceExt, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::sync::mpsc;

fn fixture(permissions: &[Permission]) -> super::super::test_fixture::Fixture {
    fixture_with_brand(permissions, Brand::default())
}
fn fixture_with_brand(
    permissions: &[Permission],
    brand: Brand,
) -> super::super::test_fixture::Fixture {
    let mut fixture = super::super::test_fixture::fixture_with_projects(
        permissions,
        brand,
        NativeBinding::KiroV3,
        &["a", "b"],
    );
    let config = ConfigSnapshot::new(fixture.paths.brand(), []).unwrap();
    fixture.server.attach_project_hook(&config).unwrap();
    fixture
}
async fn serve(server: &SourceServer) -> (String, tokio::task::JoinHandle<io::Result<()>>) {
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    graphit_daemon::source_discovery::publish_discovery(
        &server.0.auth,
        &listener,
        server.0.policy.index,
        original.snapshot(),
    )
    .unwrap();
    let uri = format!("http://{}/mcp", listener.address());
    let serving = server.clone();
    (
        uri,
        tokio::spawn(async move { SourceServer::serve(&serving, listener).await }),
    )
}
async fn client(uri: &str, raw: &str, origin: &str) -> RunningService<RoleClient, ()> {
    let mut headers = std::collections::HashMap::new();
    let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    bearer.set_sensitive(true);
    headers.insert(axum::http::header::AUTHORIZATION, bearer);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_str(origin).unwrap(),
    );
    ().serve_with_lifecycle(
        StreamableHttpClientTransport::with_client(
            crate::source_server::test_http_client(),
            StreamableHttpClientTransportConfig::with_uri(uri.to_owned()).custom_headers(headers),
        ),
        ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::LATEST],
        },
    )
    .await
    .unwrap()
}
fn params(mut arguments: Value) -> CallToolRequestParams {
    arguments["ai_optimized"] = json!(false);
    CallToolRequestParams::new(TOOL).with_arguments(arguments.as_object().unwrap().clone())
}
fn value(result: CallToolResult) -> Value {
    assert_ne!(
        result.is_error,
        Some(true),
        "Root skill call failed: {:?}",
        result.content
    );
    serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap()
}
fn set(store: &ConfigStore, key: &str, value: Value) {
    store
        .set(
            ConfigKey::new(key).unwrap(),
            value,
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
}
fn project_input(
    fixture: &super::super::test_fixture::Fixture,
    index: usize,
    module: &str,
) -> Value {
    json!({"module":module,"project_dir":NativePath::new(fixture.projects[index].0.root().to_owned()).unwrap(),"project_version":fixture.projects[index].2.version()})
}

#[tokio::test]
async fn canonical_installation_binding_reads_global_override_and_rejects_foreign_root() {
    let mut fixture = fixture(&[Permission::Read(Module::Ast)]);
    let root = fixture.paths.root().to_owned();
    let input_parent = root.join("bootstrap-input");
    std::fs::create_dir(&input_parent).unwrap();
    let spelling = input_parent.join("..");
    let entry = root.join("rules/skills/ast/entry.md");
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
    std::fs::write(&entry, "# Authorized Global guidance\n").unwrap();
    let resolved = graphit_core::project_paths::InstallationPaths::resolve(
        fixture.paths.brand().clone(),
        fixture.paths.start_directory(),
        fixture.paths.home_directory(),
        Some(&spelling),
    )
    .unwrap();
    assert_ne!(
        resolved.root(),
        fixture.auth.installation().live.owner_root()
    );
    Arc::get_mut(&mut fixture.server.0)
        .unwrap()
        .policy
        .installation = resolved;
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        fixture.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let input = || Arguments {
        module: "ast".into(),
        reference: None,
        project_dir: None,
        project_version: None,
    };
    let result = execute(
        &fixture.server.0,
        None,
        input(),
        &fixture.raw,
        false,
        &original,
    )
    .unwrap();
    assert_eq!(result["content"], "# Authorized Global guidance\n");

    let foreign = fixture.temp.path().join("foreign-installation");
    std::fs::create_dir(&foreign).unwrap();
    Arc::get_mut(&mut fixture.server.0)
        .unwrap()
        .policy
        .installation = graphit_core::project_paths::InstallationPaths::resolve(
        fixture.paths.brand().clone(),
        fixture.paths.start_directory(),
        fixture.paths.home_directory(),
        Some(&foreign),
    )
    .unwrap();
    assert_eq!(
        execute(
            &fixture.server.0,
            None,
            input(),
            &fixture.raw,
            false,
            &original
        ),
        Err("Module skill installation mismatch")
    );
    fixture.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn root_catalog_dispatch_current_project_settings_and_neutral_global() {
    let fixture = fixture(&[
        Permission::Read(Module::Ast),
        Permission::Read(Module::Knowledge),
        Permission::Read(Module::Task),
        Permission::Read(Module::Memory),
        Permission::Read(Module::Hub),
    ]);
    let a = ConfigStore::project(&fixture.projects[0].0).unwrap();
    let b = ConfigStore::project(&fixture.projects[1].0).unwrap();
    set(&a, "knowledge.docs_dir", json!("guides-a"));
    set(&b, "knowledge.docs_dir", json!("guides-b"));
    // A captured daemon PROJECT setting must not become the Root Global value.
    set(
        &ConfigStore::global(&fixture.paths).unwrap(),
        "knowledge.docs_dir",
        json!("installation-guides"),
    );
    let (uri, served) = serve(&fixture.server).await;
    let runtime = client(&uri, &fixture.raw, "runtime").await;
    let catalog = runtime.list_all_tools().await.unwrap();
    let advertised: Vec<_> = catalog.iter().filter(|tool| tool.name == TOOL).collect();
    assert_eq!(advertised.len(), 1);
    assert_eq!(
        advertised[0].annotations.as_ref().unwrap().read_only_hint,
        Some(true)
    );
    assert_eq!(advertised[0].input_schema["additionalProperties"], false);
    for (index, expected) in [(0, "guides-a/"), (1, "guides-b/")] {
        let result = value(
            runtime
                .call_tool(params(project_input(&fixture, index, "knowledge")))
                .await
                .unwrap(),
        );
        assert!(result["content"].as_str().unwrap().contains(expected));
        assert_eq!(result["module"], "knowledge");
        assert_eq!(
            result["name"],
            fixture.paths.brand().skill_name("knowledge").unwrap()
        );
    }
    set(&a, "knowledge.docs_dir", json!("guides-a-current"));
    let updated = value(
        runtime
            .call_tool(params(project_input(&fixture, 0, "knowledge")))
            .await
            .unwrap(),
    );
    assert!(
        updated["content"]
            .as_str()
            .unwrap()
            .contains("guides-a-current/")
    );
    assert!(!updated["content"].as_str().unwrap().contains("guides-a/"));
    set(&b, "modules.knowledge", json!(false));
    let disabled = value(
        runtime
            .call_tool(params(project_input(&fixture, 1, "knowledge")))
            .await
            .unwrap(),
    );
    assert_eq!(disabled["enabled"], false);
    assert!(disabled["content"].as_str().unwrap().contains("guides-b/"));
    let global = value(
        runtime
            .call_tool(params(json!({"module":"knowledge"})))
            .await
            .unwrap(),
    );
    let content = global["content"].as_str().unwrap();
    assert!(content.contains("active PROJECT"));
    assert!(!content.contains("guides-a-current"));
    assert!(!content.contains("guides-b"));
    assert!(!content.contains("installation-guides"));
    for module in ["ast", "hub", "task", "memory", "knowledge"] {
        let result = value(
            runtime
                .call_tool(params(json!({"module":module})))
                .await
                .unwrap(),
        );
        assert_eq!(result["module"], module);
        assert!(!result["content"].as_str().unwrap().is_empty());
    }
    let ast = value(
        runtime
            .call_tool(params(json!({"module":" AST "})))
            .await
            .unwrap(),
    );
    assert_eq!(ast["module"], "ast");
    let references = ast["references"].as_array().unwrap();
    assert_eq!(references, &vec![json!("references/impact-assessment.md")]);
    let reference = value(
        runtime
            .call_tool(params(
                json!({"module":"ast","reference":"references/impact-assessment.md"}),
            ))
            .await
            .unwrap(),
    );
    assert_eq!(reference["reference"], "references/impact-assessment.md");
    assert!(reference["content"].as_str().unwrap().len() > 100);
    assert!(reference.get("references").is_none());
    for invalid in [
        json!({"module":"fs"}),
        json!({"module":"ast","reference":"../config.json"}),
        json!({"module":"ast","project_version":"main"}),
        json!({"module":"ast","project_dir":null}),
        json!({"module":"ast","role":"scout"}),
    ] {
        assert_eq!(
            runtime.call_tool(params(invalid)).await.unwrap().is_error,
            Some(true)
        );
    }
    fixture
        .server
        .live_projects()
        .replace(Arc::new(Vec::new()), 0)
        .unwrap();
    assert_eq!(
        value(
            runtime
                .call_tool(params(json!({"module":"knowledge"})))
                .await
                .unwrap()
        )["module"],
        "knowledge"
    );
    assert_eq!(
        runtime
            .call_tool(params(project_input(&fixture, 0, "ast")))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    let oversized = runtime
        .call_tool(params(
            json!({"module":"ast","reference":"x".repeat(5_000)}),
        ))
        .await
        .unwrap();
    assert_eq!(oversized.is_error, Some(true));
    assert_eq!(
        oversized.content[0].as_text().unwrap().text,
        "Module skill argument limit or encoding invalid"
    );
    runtime.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn root_authority_exact_reference_and_observed_override_fail_closed() {
    let fixture = fixture(&[Permission::Read(Module::Ast)]);
    let (uri, served) = serve(&fixture.server).await;
    let runtime = client(&uri, &fixture.raw, "runtime").await;
    assert_eq!(
        runtime
            .call_tool(params(json!({"module":"memory"})))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    let wrong = client(&uri, "invalid-runtime-bearer", "runtime").await;
    assert_eq!(
        wrong
            .call_tool(params(json!({"module":"ast"})))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    let unsupported = client(&uri, &fixture.raw, "role").await;
    assert_eq!(
        unsupported
            .call_tool(params(json!({"module":"ast"})))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    let global_entry = fixture.paths.root().join("rules/skills/ast/entry.md");
    std::fs::create_dir_all(global_entry.parent().unwrap()).unwrap();
    std::fs::write(
        &global_entry,
        "Root Global {{ tool_transport }} {{ brand }}",
    )
    .unwrap();
    assert_eq!(
        value(
            runtime
                .call_tool(params(json!({"module":"ast"})))
                .await
                .unwrap()
        )["content"],
        "Root Global root graphit"
    );
    let project_entry = fixture.projects[0]
        .0
        .brand_directory()
        .join("rules/skills/ast/entry.md");
    std::fs::create_dir_all(project_entry.parent().unwrap()).unwrap();
    std::fs::write(&project_entry, "Root PROJECT {{ publication_scope }}").unwrap();
    assert_eq!(
        value(
            runtime
                .call_tool(params(project_input(&fixture, 0, "ast")))
                .await
                .unwrap()
        )["content"],
        "Root PROJECT project"
    );
    std::fs::write(&project_entry, "{{ missing_value }}").unwrap();
    assert_eq!(
        runtime
            .call_tool(params(project_input(&fixture, 0, "ast")))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    // An exact reference uses the same captured/validated companion bundle.
    let companion = fixture
        .paths
        .root()
        .join("rules/skills/ast/references/impact-assessment.md");
    std::fs::create_dir_all(companion.parent().unwrap()).unwrap();
    std::fs::write(&companion, "{{ unknown_reference_value }}").unwrap();
    assert_eq!(
        runtime
            .call_tool(params(
                json!({"module":"ast","reference":"references/impact-assessment.md"})
            ))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    wrong.cancel().await.unwrap();
    unsupported.cancel().await.unwrap();
    runtime.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn root_cancellation_joins_original_worker_and_preserves_sibling_read() {
    let fixture = fixture(&[Permission::Read(Module::Ast)]);
    let (uri, served) = serve(&fixture.server).await;
    let runtime = client(&uri, &fixture.raw, "runtime").await;
    let (started, observed) = mpsc::channel();
    let (release, waiting) = mpsc::channel();
    *fixture.server.0.gate.lock().unwrap() = Some((started, waiting));
    let pending = runtime
        .send_cancellable_request(
            ClientRequest::CallToolRequest(CallToolRequest::new(params(json!({"module":"ast"})))),
            Default::default(),
        )
        .await
        .unwrap();
    let original =
        tokio::task::spawn_blocking(move || observed.recv_timeout(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
    pending.cancel(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !original.is_cancelled() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        value(
            runtime
                .call_tool(params(json!({"module":"ast"})))
                .await
                .unwrap()
        )["module"],
        "ast"
    );
    runtime.cancel().await.unwrap();
    let closing = fixture.server.clone();
    let shutdown = tokio::spawn(async move { closing.shutdown().await });
    tokio::task::yield_now().await;
    assert!(!shutdown.is_finished());
    release.send(()).unwrap();
    shutdown.await.unwrap().unwrap();
    served.await.unwrap().unwrap();
    assert!(fixture.server.0.jobs.lock().await.tasks.is_empty());
}

#[test]
fn observed_template_rejects_replacement_and_absent_override_creation() {
    let fixture = fixture(&[Permission::Read(Module::Ast)]);
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        fixture.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let relative = Path::new("rules/skills/ast/entry.md");
    let absent = crate::managed_roles::capture_template(
        fixture.paths.root(),
        relative,
        InstructionOrigin::Global,
        fixture.server.0.policy.index,
        &original,
    )
    .unwrap();
    let destination = fixture.paths.root().join(relative);
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::fs::write(&destination, "first").unwrap();
    assert!(absent.revalidate(&original).is_err());
    let captured = crate::managed_roles::capture_template(
        fixture.paths.root(),
        relative,
        InstructionOrigin::Global,
        fixture.server.0.policy.index,
        &original,
    )
    .unwrap();
    let replacement = destination.with_extension("replacement");
    std::fs::write(&replacement, "second").unwrap();
    std::fs::rename(replacement, &destination).unwrap();
    assert!(captured.revalidate(&original).is_err());
}

#[tokio::test]
async fn root_proxy_preserves_global_omission_and_explicit_other_project() {
    use rmcp::ServiceExt;
    let fixture = fixture(&[Permission::Read(Module::Ast)]);
    let (uri, served) = serve(&fixture.server).await;
    let address = uri
        .strip_prefix("http://")
        .unwrap()
        .strip_suffix("/mcp")
        .unwrap()
        .parse()
        .unwrap();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        fixture.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let proxy = crate::mcp_source::SourceProxy::new(
        fixture.paths.clone(),
        address,
        crate::source_server::test_http_client(),
        (
            fixture.server.0.policy.index,
            fixture.server.0.policy.content,
            Duration::from_secs(30),
        ),
        Some((fixture.projects[0].0.root(), Some("not-an-active-version"))),
        &original,
    )
    .await
    .unwrap();
    let (local, remote) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(remote);
    let serving = proxy.clone();
    let joined = tokio::spawn(async move { serving.serve_stream(read, write).await });
    let client = ().serve(local).await.unwrap();
    assert_eq!(
        client
            .peer()
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .filter(|tool| tool.name == TOOL)
            .count(),
        1
    );
    assert_eq!(
        value(
            client
                .peer()
                .call_tool(params(json!({"module":"ast"})))
                .await
                .unwrap()
        )["module"],
        "ast"
    );
    assert_eq!(
        value(
            client
                .peer()
                .call_tool(params(project_input(&fixture, 1, "ast")))
                .await
                .unwrap()
        )["module"],
        "ast"
    );
    assert_eq!(
        client
            .peer()
            .call_tool(params(
                json!({"module":"ast","project_version":"not-an-active-version"})
            ))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    client.cancel().await.unwrap();
    joined.await.unwrap().unwrap();
    proxy.shutdown().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn root_local_reads_guidance_with_actual_temporary_profile() {
    use graphit_auth::{
        profile::{
            AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
            ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
        },
        store::AuthStore,
    };
    use graphit_core::auth_document::{AuthDocumentLimits, AuthDocumentStore};
    let fixture = fixture(&[Permission::Read(Module::Ast)]);
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        fixture.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let installation_id =
        graphit_core::installation_identity::InstallationIdentityStore::new(&fixture.paths)
            .unwrap()
            .load()
            .unwrap()
            .unit_id()
            .unwrap()
            .clone();
    let live = CapturedPublicationScope::installation(
        &fixture.paths,
        installation_id,
        [fixture.paths.root().to_owned()],
        fixture.server.0.policy.scope,
        original.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            live,
            AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
            original.snapshot(),
        )
        .unwrap(),
        ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        &original,
    )
    .unwrap();
    let before = store.load(&original).unwrap();
    let published = store
        .mutate(before, &original, |doc, request| {
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
                request,
            )?;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("local").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(
                ProfileData {
                    name: "root-skill-local".into(),
                    provider: "local".into(),
                    username: "fixture".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new("root-skill-fixture-local".into())),
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expected,
                1,
                request,
            )?;
            doc.activate("root-skill-local", doc.revision(), request)?;
            Ok(())
        })
        .unwrap();
    assert!(published.failure().is_none());
    let (uri, served) = serve(&fixture.server).await;
    let local = client(&uri, "root-skill-fixture-local", "local").await;
    assert_eq!(
        value(
            local
                .call_tool(params(json!({"module":"ast"})))
                .await
                .unwrap()
        )["module"],
        "ast"
    );
    assert_eq!(
        value(
            local
                .call_tool(params(project_input(&fixture, 1, "ast")))
                .await
                .unwrap()
        )["module"],
        "ast"
    );
    assert_eq!(
        local
            .call_tool(params(json!({"module":"memory"})))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    local.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn root_alternate_brand_catalog_dispatch_and_proxy_keep_global_neutral() {
    use rmcp::ServiceExt;
    let fixture = fixture_with_brand(
        &[Permission::Read(Module::Knowledge)],
        Brand::new("orbit").unwrap(),
    );
    let a = ConfigStore::project(&fixture.projects[0].0).unwrap();
    let b = ConfigStore::project(&fixture.projects[1].0).unwrap();
    set(&a, "knowledge.docs_dir", json!("launcher-docs"));
    set(&b, "knowledge.docs_dir", json!("other-project-docs"));
    let name = fixture
        .paths
        .brand()
        .mcp_tool_name(&["module", "skill"])
        .unwrap();
    assert_ne!(name, TOOL);
    assert_eq!(fixture.server.get_tool(&name).unwrap().name, name);
    assert!(fixture.server.get_tool(TOOL).is_none());
    let named = |mut arguments: Value| {
        arguments["ai_optimized"] = json!(false);
        CallToolRequestParams::new(name.clone())
            .with_arguments(arguments.as_object().unwrap().clone())
    };
    let (uri, served) = serve(&fixture.server).await;
    let runtime = client(&uri, &fixture.raw, "runtime").await;
    let tools = runtime.list_all_tools().await.unwrap();
    assert_eq!(
        tools
            .iter()
            .filter(|tool| tool.name.as_ref() == name)
            .count(),
        1
    );
    assert!(tools.iter().all(|tool| tool.name != TOOL));
    let direct = value(
        runtime
            .call_tool(named(json!({"module":"knowledge"})))
            .await
            .unwrap(),
    );
    assert_eq!(
        direct["name"],
        fixture.paths.brand().skill_name("knowledge").unwrap()
    );
    assert!(direct["content"].as_str().unwrap().contains(&name));
    let address = uri
        .strip_prefix("http://")
        .unwrap()
        .strip_suffix("/mcp")
        .unwrap()
        .parse()
        .unwrap();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        fixture.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let proxy = crate::mcp_source::SourceProxy::new(
        fixture.paths.clone(),
        address,
        crate::source_server::test_http_client(),
        (
            fixture.server.0.policy.index,
            fixture.server.0.policy.content,
            Duration::from_secs(30),
        ),
        Some((fixture.projects[0].0.root(), Some("not-an-active-version"))),
        &original,
    )
    .await
    .unwrap();
    let (local, remote) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(remote);
    let serving = proxy.clone();
    let joined = tokio::spawn(async move { serving.serve_stream(read, write).await });
    let forwarded = ().serve(local).await.unwrap();
    assert_eq!(
        forwarded
            .peer()
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .filter(|tool| tool.name.as_ref() == name)
            .count(),
        1
    );
    let global = value(
        forwarded
            .peer()
            .call_tool(named(json!({"module":"knowledge"})))
            .await
            .unwrap(),
    );
    assert_eq!(global, direct);
    assert!(
        global["content"]
            .as_str()
            .unwrap()
            .contains("active PROJECT")
    );
    assert!(
        !global["content"]
            .as_str()
            .unwrap()
            .contains("launcher-docs")
    );
    let other = value(
        forwarded
            .peer()
            .call_tool(named(project_input(&fixture, 1, "knowledge")))
            .await
            .unwrap(),
    );
    assert!(
        other["content"]
            .as_str()
            .unwrap()
            .contains("other-project-docs/")
    );
    forwarded.cancel().await.unwrap();
    joined.await.unwrap().unwrap();
    proxy.shutdown().await.unwrap();
    runtime.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn http_mcp_module_skill_output_modes_preserve_rendered_unicode_and_markdown() {
    use graphit_core::structured_output::{Limits, OutputFormat, encode};
    let fixture = fixture(&[Permission::Read(Module::Ast)]);
    let entry = fixture.paths.root().join("rules/skills/ast/entry.md");
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
    let template = "# Instruções de ação — 二\n\nUse `{{ brand }}` sem reescrever o conteúdo.\n\n```json\n{\"literal\":true,\"path\":\"a\\\\b\"}\n```\n\n- Preservar acentos: memória e revisão.";
    std::fs::write(&entry, template).unwrap();
    let (uri, served) = serve(&fixture.server).await;
    let runtime = client(&uri, &fixture.raw, "runtime").await;
    let tools = runtime.list_all_tools().await.unwrap();
    let tool = tools.iter().find(|tool| tool.name == TOOL).unwrap();
    assert_eq!(
        tool.input_schema["properties"]["ai_optimized"]["type"],
        "boolean"
    );
    assert_eq!(
        tool.input_schema["properties"]["ai_optimized"]["default"],
        true
    );
    assert!(tool.output_schema.is_none());
    let json = runtime
        .call_tool(params(json!({"module":"ast"})))
        .await
        .unwrap();
    assert_eq!(json.is_error, Some(false));
    let value: Value = serde_json::from_str(&json.content[0].as_text().unwrap().text).unwrap();
    assert_eq!(json.structured_content.as_ref(), Some(&value));
    assert_eq!(value["content"], template.replace("{{ brand }}", "graphit"));
    let expected = encode(
        &value,
        OutputFormat::Toon,
        Limits {
            max_depth: 64,
            max_nodes: 100_000,
            max_input_bytes: 1_048_576,
            max_output_bytes: 1_048_576,
        },
    )
    .unwrap();
    for arguments in [
        json!({"module":"ast"}),
        json!({"module":"ast","ai_optimized":true}),
    ] {
        let result = runtime
            .call_tool(
                CallToolRequestParams::new(TOOL)
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
            runtime
                .call_tool(
                    CallToolRequestParams::new(TOOL).with_arguments(
                        json!({"module":"ast","ai_optimized":invalid})
                            .as_object()
                            .unwrap()
                            .clone()
                    )
                )
                .await
                .is_err()
        );
    }
    let wrong = client(&uri, "invalid-runtime-bearer", "runtime").await;
    let denied = wrong
        .call_tool(
            CallToolRequestParams::new(TOOL)
                .with_arguments(json!({"module":"ast"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    assert!(
        !serde_json::to_string(&denied)
            .unwrap()
            .contains("Instruções de ação")
    );
    wrong.cancel().await.unwrap();
    runtime.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn synced_broker_skill_is_below_local_rules_and_deactivation_restores_default() {
    use graphit_auth::{
        profile::{
            AiService, BrokerDeviceMetadata, BrokerDeviceProfileData, BrokerDeviceTokens,
            BrokerService, ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind,
            S3Location, S3Source, Secret,
        },
        store::AuthStore,
    };
    use graphit_core::auth_document::{AuthDocumentLimits, AuthDocumentStore};
    use std::{
        fs,
        sync::atomic::{AtomicU8, Ordering},
    };

    const NAME: &str = "skills/ast/entry.md";
    let mut f = super::super::test_fixture::fixture_with_projects_and_authority(
        &[Permission::Read(Module::Ast)],
        &[
            Permission::Configure,
            Permission::ManageAccessGrants,
            Permission::External(graphit_auth::ExternalEffect::PublishedFilesystem),
        ],
        Brand::default(),
        NativeBinding::Codex,
        &["sample"],
        &[],
    );
    let config = ConfigSnapshot::new(f.paths.brand(), []).unwrap();
    f.server.attach_project_hook(&config).unwrap();
    std::sync::Arc::get_mut(&mut f.server.0)
        .unwrap()
        .roles
        .attach_hub_transport(crate::source_server::test_http_client(), true, None)
        .unwrap();
    let state = &f.server.0;
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        state.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let issuer = f.auth.issuer();
    let caller = issuer.authenticate_runtime(&f.raw, &original).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let phase = std::sync::Arc::new(AtomicU8::new(0));
    let server_phase = phase.clone();
    let broker = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route(
                "/v1/agent-instructions",
                axum::routing::get(move |headers: axum::http::HeaderMap| {
                    let phase = server_phase.load(Ordering::Relaxed);
                    async move {
                        use axum::response::IntoResponse;
                        assert_eq!(
                            headers[axum::http::header::AUTHORIZATION],
                            "Bearer fixture-token"
                        );
                        match phase {
                            0 => axum::Json(
                                json!({"version":1,"templates":{NAME:"Broker {{ brand }}."}}),
                            )
                            .into_response(),
                            1 => axum::http::StatusCode::UNAUTHORIZED.into_response(),
                            _ => axum::Json(json!({"version":1,"templates":{}})).into_response(),
                        }
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });

    let store = AuthStore::new(
        AuthDocumentStore::capture(
            f.auth.installation().live.clone(),
            AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
            original.snapshot(),
        )
        .unwrap(),
        ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 8192,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        &original,
    )
    .unwrap();
    let publication = store
        .mutate(store.load(&original).unwrap(), &original, |doc, request| {
            doc.add_provider(
                "broker".into(),
                ProviderConfig {
                    kind: ProviderKind::Broker(BrokerService {
                        endpoint: origin.clone(),
                        mcp_resource: None,
                        allow_anonymous: false,
                    }),
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: S3Source::BrokerScope,
                    },
                },
                doc.revision(),
                1,
                request,
            )?;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("broker").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_broker_device_profile(
                BrokerDeviceProfileData {
                    name: "native".into(),
                    provider: "broker".into(),
                    issuer: origin,
                    subject: "fixture".into(),
                    metadata: BrokerDeviceMetadata::default(),
                    s3_disabled: true,
                    tokens: BrokerDeviceTokens {
                        access_token: Secret::new("fixture-token".to_owned().into()),
                        expires_at: i64::MAX / 2,
                    },
                },
                expected,
                1,
                request,
            )?;
            doc.activate("native", doc.revision(), request)
        })
        .unwrap();
    assert!(publication.failure().is_none());

    let project = &f.projects[0];
    let read = || {
        execute(
            state,
            Some(project),
            serde_json::from_value(json!({"module":"ast"})).unwrap(),
            &f.raw,
            false,
            &original,
        )
        .unwrap()["content"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let sync = || crate::instruction_overrides::sync(issuer, &caller, &f.paths, true, &original);
    let default = read();
    // A specific sync fetches and applies the layer even before the general sync.
    let (uri, job) = serve(&f.server).await;
    let specific = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!(
            "{}{}",
            uri.strip_suffix("/mcp").unwrap(),
            crate::managed_roles::http::ROUTE
        ))
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(&f.raw)
        .json(&json!({
            "destination":"project","project_dir":f.projects[0].0.root(),"binding":"codex",
            "roles":[],"skills_only":true,"rules":[],
        }))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert_eq!(specific.status(), reqwest::StatusCode::OK);
    let installed: Value = serde_json::from_slice(&specific.bytes().await.unwrap()).unwrap();
    assert_eq!(installed["failure"], Value::Null, "{installed}");
    let skill = f.projects[0]
        .0
        .root()
        .join(".agents/skills/graphit-ast/SKILL.md");
    assert!(
        fs::read_to_string(skill)
            .unwrap()
            .contains("Broker graphit.")
    );
    assert_eq!(read(), "Broker graphit.");
    assert_eq!(sync().await.unwrap()["templates"], 1);
    let global = f.paths.root().join("rules").join(NAME);
    let project = project
        .0
        .root()
        .join(project.0.brand().directory())
        .join("rules")
        .join(NAME);
    for (path, text) in [
        (&global, "Personal override."),
        (&project, "Project override."),
    ] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        assert_eq!(read(), text);
    }
    fs::remove_file(project).unwrap();
    assert_eq!(read(), "Personal override.");
    fs::remove_file(global).unwrap();
    phase.store(1, Ordering::Relaxed);
    assert_eq!(
        sync().await.unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(read(), "Broker graphit.");
    phase.store(2, Ordering::Relaxed);
    assert_eq!(sync().await.unwrap()["templates"], 0);
    assert_eq!(read(), default);

    phase.store(0, Ordering::Relaxed);
    sync().await.unwrap();
    assert_eq!(read(), "Broker graphit.");
    let logout = store
        .mutate(store.load(&original).unwrap(), &original, |doc, request| {
            doc.logout(None, doc.revision(), request)
        })
        .unwrap();
    assert!(logout.failure().is_none());
    phase.store(1, Ordering::Relaxed);
    assert_eq!(sync().await.unwrap()["status"], "local_provider");
    assert_eq!(read(), default);
    f.stop.cancel();
    job.await.unwrap().unwrap();
    broker.abort();
    assert!(broker.await.unwrap_err().is_cancelled());
}
