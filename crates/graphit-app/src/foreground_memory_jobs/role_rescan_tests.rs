use super::*;
use graphit_auth::{IssuerLimits, Permission, RoleDefinition};
use graphit_core::{module_settings::Module, native_path::NativePath};
use graphit_hub::native::{Role, RoleCredentialReference};
use rmcp::{
    RoleClient,
    model::{CallToolRequestParams, ProtocolVersion},
    service::{ClientLifecycleMode, ClientServiceExt, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::Value;

type Client = RunningService<RoleClient, ()>;

async fn serve_foreground(
    foreground: &mut ForegroundSource,
) -> (String, tokio::task::JoinHandle<io::Result<()>>) {
    let listener = foreground.listener.take().unwrap();
    let uri = format!("http://{}/roles/mcp", listener.address());
    let server = foreground.server.as_ref().unwrap().clone();
    (
        uri,
        tokio::spawn(async move { server.serve(listener).await }),
    )
}

async fn role_client(
    foreground: &ForegroundSource,
    uri: &str,
    selected: usize,
    role: Role,
    permissions: &[Permission],
) -> (
    Client,
    graphit_auth::AdminSessionHandle,
    graphit_auth::RunHandle,
) {
    let cancel = Cancellation::default();
    let original = foreground
        .policy
        .request_context(Instant::now(), &cancel)
        .unwrap();
    let issuer = foreground.auth.issuer();
    let runtime = foreground
        .auth
        .runtime()
        .check(original.snapshot())
        .unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(runtime.secret(original.snapshot()).unwrap()).unwrap(),
            &original,
        )
        .unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (paths, id, version) = &foreground.projects.as_ref().unwrap()[selected];
    let definition = RoleDefinition::tool_owned(
        role,
        paths.brand(),
        NativeBinding::KiroV3,
        permissions,
        [37; 32],
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
    )
    .unwrap();
    let reference: RoleCredentialReference = issuer
        .configure_tool_owned_definition(&parent, definition)
        .unwrap();
    let project =
        CapturedPublicationScope::project(paths, *id, foreground.scope, original.snapshot())
            .unwrap();
    let record = project.binding().unwrap();
    let scope = ScopeInput {
        installation: foreground.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let run = issuer
        .start_run(&parent, &scope, Duration::from_secs(300))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, permissions, &scope)
        .unwrap();
    let mut headers = std::collections::HashMap::new();
    let mut authorization =
        axum::http::HeaderValue::from_str(&format!("Bearer {}", bearer.expose_for_transport()))
            .unwrap();
    authorization.set_sensitive(true);
    headers.insert(axum::http::header::AUTHORIZATION, authorization);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_static("role"),
    );
    let encoded = reference
        .encode(foreground.policy.content().max_input_bytes, &original)
        .unwrap();
    let mut header = axum::http::HeaderValue::from_bytes(&encoded).unwrap();
    header.set_sensitive(true);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-role-reference"),
        header,
    );
    let selection = json!({"project_dir":NativePath::new(paths.root().to_owned()).unwrap(),"project_version":version.version()});
    headers.insert(
        axum::http::HeaderName::from_static(crate::role_tool_server::ROLE_SELECTION_HEADER),
        axum::http::HeaderValue::from_str(&selection.to_string()).unwrap(),
    );
    let client = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::new(),
                StreamableHttpClientTransportConfig::with_uri(uri.to_owned())
                    .custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap();
    (client, session, run)
}

async fn call(client: &Client, name: &'static str, input: Value, failure: bool) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new(name).with_arguments(
                json!({"ai_optimized":false,"input":input})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(failure));
    serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap()
}

async fn insert(client: &Client, key: &str) -> Memory {
    let value = call(client, "graphit_memory_insert", json!({"idempotency_key":key,"title":key,"body":"Tokyo is the capital of Japan.","classification":"decision"}), false).await;
    serde_json::from_value(value["result"].clone()).unwrap()
}

async fn snapshot_generation(
    resources: &crate::foreground_memory_jobs::MemoryJobResources,
    foreground: &ForegroundSource,
    selected: usize,
) -> model_api::MemoryEmbeddingIndexRead {
    let (paths, id, version) = &foreground.projects.as_ref().unwrap()[selected];
    let context = graphit_daemon::supervisor::ProjectContext {
        id: *id,
        version: version.clone(),
    };
    let cancel = Cancellation::default();
    tokio::time::timeout(
        Duration::from_secs(15),
        resources.index(paths, &context, &cancel),
    )
    .await
    .unwrap()
    .unwrap()
}

// Observe live FILE and dataset generations, after durable NN work is drained.
async fn ast_generations(foreground: &ForegroundSource, selected: usize) -> (u64, u64, [u8; 32]) {
    use graphit_data::lance_branch_store::BranchSnapshot;
    let (paths, id, version) = &foreground.projects.as_ref().unwrap()[selected];
    let source = foreground.server.as_ref().unwrap();
    let resources = foreground.memory_job_resources.upgrade().unwrap();
    let model =
        crate::ast_semantic::expected_native_nn_model(&resources.local.as_ref().unwrap().expected)
            .unwrap();
    let root = source
        .policy()
        .installation
        .root()
        .join("projects")
        .join(id.to_string())
        .join("ast");
    let cancel = Cancellation::default();
    let request = foreground
        .policy
        .request_context(Instant::now(), &cancel)
        .unwrap();
    let original = request.snapshot();
    let capture =
        CapturedPublicationScope::project(paths, *id, foreground.scope, original).unwrap();
    let record = capture.binding().unwrap();
    let project = ProjectCapture {
        scope: ScopeCapture {
            live: &capture,
            record: &record,
        },
        version,
    };
    let registry = source.graph_server().live_ast();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let observed: io::Result<_> = async {
            let (index, _directory) = registry
                .capture(
                    foreground.auth.installation(),
                    project,
                    foreground.native,
                    original,
                    false,
                )
                .await?
                .ok_or(io::ErrorKind::NotFound)?;

            if index.graph.files(original)?.is_empty() {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let branch = version.lance_branch();
            let nn = BranchSnapshot::capture_observed(
                root.join("corpus.lance").to_str().unwrap(),
                (!branch.is_empty()).then_some(branch.as_str()),
                None,
            )
            .await
            .map_err(io::Error::other)?;
            let expected = graphit_ast::corpus::vector_fields(&model)?;
            if nn.schema().field_with_name("vector").ok() != Some(expected.field(0)) {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let mut pending = nn.scan();
            pending.filter_expr(
                graphit_data::lance_branch_store::col("projection")
                    .is_not_null()
                    .and(graphit_data::lance_branch_store::col("vector").is_null()),
            );
            pending.limit(Some(1), None).map_err(io::Error::other)?;
            if pending.count_rows().await.map_err(io::Error::other)? > 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let raw = graphit_ast::source_index_locator::open_local_source_index(
                foreground.auth.installation(),
                project,
                original,
                foreground.native,
            )
            .await
            .map_err(io::Error::other)?;
            let graphit_ast::source_index_locator::SourceIndexPresence::Available(raw) = raw else {
                return Err(io::ErrorKind::NotFound.into());
            };
            Ok((raw.version(), nn.version(), index.graph.generation()?))
        }
        .await;
        match observed {
            Ok(generation) => return generation,
            Err(error) if Instant::now() >= deadline => {
                panic!("live AST publication unavailable: {error}")
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
}

async fn drain_memory_rescan(foreground: &ForegroundSource, selected: usize) {
    let (paths, id, version) = &foreground.projects.as_ref().unwrap()[selected];
    let context = graphit_daemon::supervisor::ProjectContext {
        id: *id,
        version: version.clone(),
    };
    assert_eq!(context.root(), paths.root());
    let handle = foreground
        .supervisors
        .rescan_handle(&context.key())
        .unwrap();
    let ast = foreground.ast_jobs.as_ref().unwrap();
    // Starting the following real batch proves that the preceding one completed.
    // Two admissions cover a CRUD notification already coalesced with our first rescan.
    for _ in 0..2 {
        let observed = ast.observe_next_raw_batch();
        assert!(handle.request_rescan());
        tokio::time::timeout(Duration::from_secs(30), observed)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn memory_crud_and_unselected_watcher_leave_ast_publications_untouched() {
    let fixture = Fixture::new();
    fixture.install_model();
    let cancel = Cancellation::default();
    ConfigStore::global(&fixture.installation)
        .unwrap()
        .set(
            Module::Ast.key(),
            json!(true),
            Duration::from_secs(5),
            &cancel,
        )
        .unwrap();
    std::fs::write(
        fixture.projects[0].root().join("source.json"),
        "{\"answer\":42}\n",
    )
    .unwrap();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .unwrap();
    // One real supervisor makes its existing next-batch observation unambiguous.
    let mut foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs()[..1],
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &request,
    )
    .await
    .unwrap();
    assert_eq!(foreground.supervisors.supervised().count(), 1);
    let (uri, serving) = serve_foreground(&mut foreground).await;
    let (writer, _session, _run) = role_client(
        &foreground,
        &uri,
        0,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteMemory],
    )
    .await;
    let before = ast_generations(&foreground, 0).await;
    // This genuine watcher event has no AST profile. Actual Memory CRUD below
    // also requests a published rescan; neither changes AST inputs or model.
    std::fs::write(
        fixture.projects[0]
            .root()
            .join("notes.unselected-extension"),
        "non-AST note",
    )
    .unwrap();
    let memory = insert(&writer, "memory-only-rescan").await;
    assert_indexed(&foreground, 0, &memory).await;
    drain_memory_rescan(&foreground, 0).await;
    assert_eq!(
        ast_generations(&foreground, 0).await,
        before,
        "non-AST watcher events and Memory rescans must not rewrite Raw/NN/Graph"
    );
    // A replacement execution resource with the same vector identity uses the
    // ordinary rescan without rewriting already compatible Raw, NN or graph.
    let replacement = foreground
        .memory_jobs
        .as_ref()
        .unwrap()
        .resources(foreground.scope, foreground.native, NativeBinding::KiroV3)
        .unwrap()
        .unwrap();
    foreground
        .publish_semantic_model(Some(replacement))
        .unwrap();
    drain_memory_rescan(&foreground, 0).await;
    assert_eq!(
        ast_generations(&foreground, 0).await,
        before,
        "a new execution owner with the same model space must not rewrite existing indexes"
    );
    writer.cancel().await.unwrap();
    foreground.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn role_http_mutations_reach_real_main_model_on_two_projects_and_park_independently() {
    let fixture = Fixture::new();
    fixture.install_model();
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .unwrap();
    let mut foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &request,
    )
    .await
    .unwrap();
    let resources = foreground.memory_job_resources.upgrade().unwrap();
    let (uri, serving) = serve_foreground(&mut foreground).await;
    let perms = [Permission::Read(Module::Memory), Permission::WriteMemory];
    let (a, _admin_a, _run_a) = role_client(&foreground, &uri, 0, Role::Scribe, &perms).await;
    let (b, _admin_b, _run_b) = role_client(&foreground, &uri, 1, Role::Scribe, &perms).await;
    let (reader, _admin_reader, _run_reader) = role_client(
        &foreground,
        &uri,
        0,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
    )
    .await;
    let first_a = insert(&a, "queued-a").await;
    let first_b = insert(&b, "queued-b").await;
    let updated = call(&a, "graphit_memory_update", json!({"id":first_a.id,"expected_revision":first_a.revision,"idempotency_key":"update-a","reason":"latest source after native mutation","body":"Japan has Tokyo as its capital."}), false).await;
    let mut current_a: Memory = serde_json::from_value(updated["result"].clone()).unwrap();
    assert_eq!(current_a.revision, first_a.revision + 1);
    assert_eq!(call(&reader,"graphit_memory_insert",json!({"idempotency_key":"denied","title":"denied","body":"no effect","classification":"decision"}),true).await["code"],"authentication_or_permission_denied");
    call(
        &a,
        "graphit_memory_insert",
        json!({"idempotency_key":"malformed"}),
        true,
    )
    .await;
    assert_eq!(
        call(&a, "graphit_memory_get", json!({"id":current_a.id}), false).await["memory"]["revision"],
        current_a.revision
    );
    assert_indexed(&foreground, 0, &current_a).await;
    assert_indexed(&foreground, 1, &first_b).await;
    for name in [
        "graphit_memory_mark_mandatory",
        "graphit_memory_unmark_mandatory",
        "graphit_memory_promote",
        "graphit_memory_demote",
    ] {
        let value=call(&a,name,json!({"id":current_a.id,"expected_revision":current_a.revision,"idempotency_key":name,"reason":"flag transition through actual Role ingress"}),false).await;
        current_a = serde_json::from_value(value["result"].clone()).unwrap();
        assert_indexed(&foreground, 0, &current_a).await;
    }
    assert_eq!(call(&a,"graphit_memory_update",json!({"id":current_a.id,"expected_revision":current_a.revision,"idempotency_key":"unchanged","reason":"same source","body":current_a.body}),true).await["code"],"no_change");
    // This separate real resource uses a bound TCP endpoint without a model responder.
    // Fully covered captures must return Complete without opening any RPC connection.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut covered = job_resources(&foreground);
    covered.local.as_mut().unwrap().address = listener.local_addr().unwrap();
    for selected in [0, 1] {
        let result = snapshot_generation(&covered, &foreground, selected).await;
        assert!(matches!(
            result,
            model_api::MemoryEmbeddingIndexRead::Complete {
                last_attachment: None,
                ..
            }
        ));
    }
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    let before = snapshot_generation(&covered, &foreground, 1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let after = snapshot_generation(&covered, &foreground, 1).await;
    match (before, after) {
        (
            model_api::MemoryEmbeddingIndexRead::Complete {
                generation: first, ..
            },
            model_api::MemoryEmbeddingIndexRead::Complete {
                generation: second, ..
            },
        ) => assert_eq!(first, second),
        _ => panic!("covered corpus must remain complete without self-publication"),
    }
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    let projects = foreground.projects.as_ref().unwrap();
    let a_context = graphit_daemon::supervisor::ProjectContext {
        id: projects[0].1,
        version: projects[0].2.clone(),
    };
    let old = foreground
        .supervisors
        .rescan_handle(&a_context.key())
        .unwrap();
    let desired = projects
        .iter()
        .enumerate()
        .map(
            |(i, (_, id, version))| graphit_daemon::supervisor::DesiredProject {
                context: graphit_daemon::supervisor::ProjectContext {
                    id: *id,
                    version: version.clone(),
                },
                active: i != 0,
            },
        )
        .collect();
    foreground
        .supervisors
        .reconcile(desired, |_| async {
            panic!("parking must retain the sibling producer")
        })
        .await
        .unwrap();
    assert!(!old.request_rescan());
    let _parked = insert(&a, "parked-CRUD-still-valid").await;
    let later_b = insert(&b, "b-after-a-parked").await;
    assert_indexed(&foreground, 1, &later_b).await;
    let deleted=call(&b,"graphit_memory_delete",json!({"id":later_b.id,"expected_revision":later_b.revision,"idempotency_key":"delete-b","reason":"retire new sibling memory"}),false).await;
    assert_eq!(deleted["result"]["revision"], later_b.revision + 1);
    assert_eq!(
        call(&b, "graphit_memory_get", json!({"id": later_b.id}), false).await["kind"],
        "absent"
    );
    assert_eq!(foreground.supervisors.supervised().count(), 1);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
    reader.cancel().await.unwrap();
    drop(resources);
    foreground.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    assert!(!old.request_rescan());
}

#[tokio::test]
async fn role_http_no_model_keeps_actual_crud_without_a_memory_owner() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .unwrap();
    fixture.remove_provider(&request);
    let mut foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        vec![],
        &request,
    )
    .await
    .unwrap();
    assert!(foreground.memory_jobs.is_none());
    assert!(foreground.memory_job_resources.upgrade().is_none());
    assert_eq!(foreground.supervisors.supervised().count(), 0);
    let (uri, serving) = serve_foreground(&mut foreground).await;
    let (client, _admin, _run) = role_client(
        &foreground,
        &uri,
        0,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteMemory],
    )
    .await;
    let memory = insert(&client, "no-model-role").await;
    assert_eq!(
        call(
            &client,
            "graphit_memory_get",
            json!({"id":memory.id}),
            false
        )
        .await["memory"]["body"],
        memory.body
    );
    assert!(foreground.memory_job_resources.upgrade().is_none());
    client.cancel().await.unwrap();
    foreground.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
