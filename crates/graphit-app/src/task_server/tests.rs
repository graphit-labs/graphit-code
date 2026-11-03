use super::*;
mod git_records;
mod query;

fn query_arguments(table: &str) -> Value {
    json!({"input":{"selection":{"table":table},"columns":["id","status"],"page_size":1,"top_k":1}})
}

#[tokio::test]
async fn retained_project_version_reads_real_branch_across_checkouts_and_fences_retirement() {
    let (temp, server, raw) = fixture_with_root_names(&[], &["retained"]);
    let credential = || Credential {
        role_binding: None,
        bearer: zeroize::Zeroizing::new(raw.clone()),
        origin: Origin::Runtime,
    };
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("retained metadata snapshot"));
    let (origin, id, current) = snapshot.projects[0].clone();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let retained = ProjectPaths::at(temp.path().join("retained"), &installation).unwrap();
    ProjectIdentityStore::at(&retained)
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .ensure_with_id(&ProjectMetadata::new("project").unwrap(), id)
        .unwrap();
    let checkout = GitCheckout::inspect(
        retained.root(),
        Duration::from_secs(5),
        &Cancellation::default(),
    )
    .unwrap();
    let version =
        VersionContext::select(&retained, checkout.as_checkout(), "retained", None, "").unwrap();
    server
        .0
        .live_projects
        .replace(
            Arc::new(vec![
                (origin.clone(), id, current),
                (retained.clone(), id, version),
            ]),
            0,
        )
        .unwrap();
    // This server has no watcher or background producer. The writes and reads
    // below use its real authenticated Task protocol and native dataset branches.
    let main = server.call(
        serde_json::from_value(json!({"input":{"idempotency_key":"main-version","title":"Main","description":"Main branch record","strategy":"Preserve isolation"}})).unwrap(),
        "create_session", true, credential(), CancellationToken::new(),
    ).await.unwrap_or_else(|e| panic!("main write: {}", e.code));
    let created = server.call(
        serde_json::from_value(json!({"selection":{"project_dir":retained.root(),"project_id":id,"project_version":"retained"},"input":{"idempotency_key":"retained-version","title":"Retained","description":"Retained branch record","strategy":"Read through another checkout selector"}})).unwrap(),
        "create_session", true, credential(), CancellationToken::new(),
    ).await.unwrap_or_else(|e| panic!("retained write: {}", e.code));
    let session = created["result"]["id"].clone();
    let arguments = json!({"selection":{"project_dir":origin.root(),"project_id":id,"project_version":"retained"},"input":{"id":session}});
    let selected = server
        .call(
            serde_json::from_value(arguments.clone()).unwrap(),
            "get_session",
            false,
            credential(),
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|e| panic!("retained read: {}", e.code));
    assert_eq!(selected["result"]["session"]["id"], session);
    assert_ne!(selected["result"]["session"]["id"], main["result"]["id"]);
    assert!(selected["result"]["generation"]["physical_branch"].is_string());
    for invalid in [
        json!({"selection":{"project_dir":origin.root(),"project_version":"absent"},"input":{"id":session}}),
        json!({"selection":{"project_dir":origin.root(),"project_id":"01ARZ3NDEKTSV4RRFFQ69G5FAW","project_version":"retained"},"input":{"id":session}}),
    ] {
        let error = server
            .call(
                serde_json::from_value(invalid).unwrap(),
                "get_session",
                false,
                credential(),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "selection_unavailable");
    }
    // Removing only the selected checkout fences it despite the retained
    // physical corpus and another active checkout of the same PROJECT.
    server
        .0
        .live_projects
        .retire_checkout(id, retained.root())
        .unwrap();
    let error = server
        .call(
            serde_json::from_value(arguments).unwrap(),
            "get_session",
            false,
            credential(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "selection_unavailable");
    server.shutdown().await.unwrap();
}

#[test]
fn typed_response_bound_counts_encoded_unicode_and_escaping() {
    let native = json!({"literal": "π\n\"\\"});
    let exact = serde_json::to_vec(&native).unwrap().len();
    assert_eq!(super::bounded_value(&native, exact).unwrap(), native);
    assert_eq!(
        super::bounded_value(&native, exact - 1)
            .unwrap_err()
            .io_error_kind(),
        Some(std::io::ErrorKind::FileTooLarge)
    );
}

#[tokio::test]
async fn bounded_native_encoding_keeps_the_confirmed_write_observable() {
    let (_temp, mut server, raw) = fixture();
    Arc::get_mut(&mut server.0)
        .unwrap()
        .policy
        .content
        .max_output_bytes = 8;
    let credential = || Credential {
        role_binding: None,
        bearer: zeroize::Zeroizing::new(raw.clone()),
        origin: Origin::Runtime,
    };
    let failure = server.call(
        serde_json::from_value(json!({"input":{"idempotency_key":"bounded-write","title":"Committed bounded write","description":"Record before encoding","strategy":"Read the persisted current state"}})).unwrap(),
        "create_session", true, credential(), CancellationToken::new(),
    ).await.unwrap_err();
    assert_eq!(failure.code, "response_limit");
    assert!(failure.committed);
    while server.0.jobs.lock().await.join_next().await.is_some() {}
    Arc::get_mut(&mut server.0)
        .unwrap()
        .policy
        .content
        .max_output_bytes = 65536;
    let mut query = query_arguments("sessions");
    query["input"]["columns"] = json!(["id", "title"]);
    let observed = server
        .call(
            serde_json::from_value(query).unwrap(),
            "query",
            false,
            credential(),
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|_| panic!("read committed session after bounded response failure"));
    assert!(observed.to_string().contains("Committed bounded write"));
    server.shutdown().await.unwrap();
}

use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy, profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::{ArtifactLimits, CopyLimits},
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
use rmcp::{
    RoleClient, ServiceExt,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::num::NonZeroU32;
fn fixture() -> (tempfile::TempDir, TaskServer, String) {
    fixture_with_permissions(&[])
}
pub(crate) fn browser_fixture() -> (tempfile::TempDir, TaskServer, String) {
    fixture_with_permissions(&[Permission::ManageAuthProfiles])
}
fn fixture_with_permissions(extra: &[Permission]) -> (tempfile::TempDir, TaskServer, String) {
    fixture_with_root_names(extra, &[])
}
pub(crate) fn fixture_with_root_names(
    extra: &[Permission],
    additional_names: &[&str],
) -> (tempfile::TempDir, TaskServer, String) {
    fixture_with_user_permissions(extra, additional_names, None)
}
pub(crate) fn fixture_with_user_permissions(
    extra: &[Permission],
    additional_names: &[&str],
    user_permissions: Option<&[Permission]>,
) -> (tempfile::TempDir, TaskServer, String) {
    fixture_with_reference_permissions(extra, additional_names, user_permissions, false)
}
pub(crate) fn fixture_with_reference_permissions(
    extra: &[Permission],
    additional_names: &[&str],
    user_permissions: Option<&[Permission]>,
    imports: bool,
) -> (tempfile::TempDir, TaskServer, String) {
    // This bare TaskServer fixture tests local persistence with signed identity
    // providers. Source fixtures and S3 product fixtures construct their own Auth.
    struct LocalRecordStorage;
    impl graphit_auth::RecordStorageResolver for LocalRecordStorage {
        fn resolve<'a>(
            &'a self,
            admission: graphit_auth::RecordStorageAdmission<'a>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = std::io::Result<Option<graphit_auth::RemoteRecordStorage>>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                admission.recheck().map_err(std::io::Error::other)?;
                Ok(None)
            })
        }
    }
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
        RequestContext::new(content, &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let scope = ScopeLimits::new(16, 4096, 256).unwrap();
    let permissions: Vec<_> = [
        Permission::Read(Module::Task),
        Permission::WriteTask,
        Permission::WriteOwnTask,
        Permission::CoordinateTask,
        Permission::ManageAccessGrants,
        Permission::Delegate,
    ]
    .iter()
    .chain(extra)
    .copied()
    .collect();
    let source = || SourceAuthorityPolicy {
        ordinary: &permissions,
        administrative: &permissions,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    let mut authority = IssuerAuthorityPolicy::new(
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
    if let Some(permissions) = user_permissions {
        authority = authority
            .with_incoming_user_permissions("broker", permissions)
            .unwrap()
            .with_incoming_user_permissions(
                "seed",
                &[
                    Permission::ReadUser,
                    Permission::Read(Module::Memory),
                    Permission::MutateUser,
                    Permission::WriteMemory,
                ],
            )
            .unwrap();
    }
    if imports {
        authority = authority
            .with_web_imported_knowledge_read("broker")
            .unwrap()
            .with_web_installed_hub_read("broker")
            .unwrap();
    }
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
    let mut trusted_projects = vec![project.clone()];
    for name in additional_names {
        let additional_root = temp.path().join(name);
        std::fs::create_dir(&additional_root).unwrap();
        trusted_projects.push(ProjectPaths::at(&additional_root, &paths).unwrap());
    }
    let auth = Arc::new(DaemonAuth::start(&paths, &trusted_projects, policy, &ctx).unwrap());
    auth.issuer()
        .set_record_storage_resolver(Arc::new(LocalRecordStorage))
        .unwrap();
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
    let policy = TaskPolicy {
        imported_knowledge: None,
        content,
        timeout: Duration::from_secs(30),
        scope,
        read,
        write: TaskStoreWriteLimits {
            read,
            artifacts: ArtifactLimits::new(1048576, 65536, copy).unwrap(),
            references: graphit_records::task_store::TaskReferencesInputLimits {
                max_entries: std::num::NonZeroUsize::new(content.max_collection_entries).unwrap(),
                max_text_bytes: std::num::NonZeroUsize::new(content.max_input_bytes).unwrap(),
            },
        },
        role_reference: ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap(),
        binding: NativeBinding::KiroV3,
    };
    let server = TaskServer::new(
        auth,
        Arc::new(vec![(project, id, version)]),
        0,
        policy,
        CancellationToken::new(),
    )
    .unwrap();
    (temp, server, raw)
}

fn listener() -> BoundListener {
    graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap()
}
async fn connect(uri: String, raw: Option<&str>) -> RunningService<RoleClient, ()> {
    let mut headers = std::collections::HashMap::new();
    if let Some(raw) = raw {
        let mut header = axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
        header.set_sensitive(true);
        headers.insert(axum::http::header::AUTHORIZATION, header);
        headers.insert(
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static("runtime"),
        );
    }
    ().serve(StreamableHttpClientTransport::with_client(
        reqwest::Client::new(),
        StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
    ))
    .await
    .unwrap()
}
async fn call(
    client: &RunningService<RoleClient, ()>,
    name: &str,
    arguments: Value,
    error: bool,
) -> Value {
    let mut arguments = arguments.as_object().unwrap().clone();
    arguments.insert("ai_optimized".into(), Value::Bool(false));
    let result = client
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments))
        .await
        .unwrap();
    let value: Value = serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap();
    assert_eq!(
        result.is_error,
        Some(error),
        "{name}: {:?}",
        value.get("code")
    );
    value
}
#[tokio::test]
async fn task_mcp_uses_toon_by_default_and_strict_explicit_json() {
    let (_temp, server, raw) = fixture();
    let listener = listener();
    let address = listener.address();
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(listener).await });
    let client = connect(format!("http://{address}/mcp"), Some(&raw)).await;
    let tools = client.peer().list_all_tools().await.unwrap();
    let query = tools
        .iter()
        .find(|tool| tool.name == "graphit_task_query")
        .unwrap();
    assert_eq!(
        query.input_schema["properties"]["ai_optimized"]["default"],
        true
    );
    let input =
        json!({"input":{"selection":{"table":"tasks"},"columns":["id"],"page_size":1,"top_k":1}});
    let toon = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_task_query")
                .with_arguments(input.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(toon.is_error, Some(false));
    assert!(toon.structured_content.is_none());
    assert!(
        toon.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("state: no_corpus")
    );
    let explicit = call(&client, "graphit_task_query", input.clone(), false).await;
    assert_eq!(explicit["result"]["state"], "no_corpus");
    let mut invalid = input.as_object().unwrap().clone();
    invalid.insert("ai_optimized".into(), Value::String("false".into()));
    assert!(
        client
            .peer()
            .call_tool(CallToolRequestParams::new("graphit_task_query").with_arguments(invalid))
            .await
            .is_err()
    );
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn actual_mcp_writes_http_catalogue_dates_history_and_reopen() {
    let (_temp, server, raw) = fixture();
    let socket = listener();
    let base = format!("http://{}", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let client = connect(format!("{base}/mcp"), Some(&raw)).await;
    let listed = client.list_all_tools().await.unwrap();
    assert_eq!(listed.len(), super::catalog::OPERATIONS.len());
    assert!(
        listed
            .iter()
            .all(|t| !t.name.ends_with("_read") && !t.name.ends_with("_write"))
    );
    let empty = call(
        &client,
        "graphit_task_query",
        query_arguments("tasks"),
        false,
    )
    .await;
    assert_eq!(empty["result"]["state"], "no_corpus");
    let session=call(&client,"graphit_task_session_create",json!({"input":{
        "idempotency_key":"socket-session","title":"Native socket","description":"Real Task corpus","strategy":"native"
    }}),false).await;
    let session_id = session["result"]["id"].clone();
    let task=call(&client,"graphit_task_create",json!({"input":{
        "idempotency_key":"socket-task","session_id":session_id,"title":"Calendar","description":"Real native dates and dependency",
        "task_type":"TASK","priority":1,"schedule":{"start_date":"2028-02-28","end_date":"2028-03-01"},
        "checks":[{"kind":"acceptance","text":"Deliver the physical Task"},{"kind":"test","text":"Validate the physical Task over the socket"}]
    }}),false).await;
    let id = task["result"]["specification"]["id"].clone();
    let associated = call(
        &client,
        "graphit_task_commit_set",
        json!({"input":{
            "id":id, "expected_revision":task["result"]["specification"]["revision"],
            "commit":"a".repeat(40), "reason":"Associate actual delivery"
        }}),
        false,
    )
    .await;
    assert_eq!(associated["result"]["commit"], "a".repeat(40));
    let claimed = call(
        &client,
        "graphit_task_claim",
        json!({"input":{"id":id,"lease_ms":120000}}),
        false,
    )
    .await;
    let token = claimed["result"]["token"].as_str().unwrap().to_owned();
    let revision = claimed["result"]["record"]["specification"]["revision"]
        .as_u64()
        .unwrap();
    let revised=call(&client,"graphit_task_revise",json!({"input":{
        "target":{"id":id,"claim":{"token":token,"expected_revision":revision}},
        "lease_ms":120000,"reason":"Keep real dates visible","schedule":{"due_date":"2028-03-02"}
    }}),false).await;
    assert_eq!(
        revised["result"]["specification"]["schedule"]["due_date"],
        "2028-03-02"
    );
    let stale = call(
        &client,
        "graphit_task_revise",
        json!({"input":{
            "target":{"id":id,"claim":{"token":token,"expected_revision":revision}},
            "lease_ms":120000,"reason":"Stale revision","title":"Rejected"
        }}),
        true,
    )
    .await;
    assert_eq!(stale["code"], "revision_conflict");
    let body = "  **literal Markdown**\n\n    code β\n";
    let comment_input = json!({
        "target":{"id":id,"claim":{"token":token,"expected_revision":revised["result"]["specification"]["revision"]}},
        "lease_ms":120000,"idempotency_key":"socket-comment","kind":"decision","body":body
    });
    let added = call(
        &client,
        "graphit_task_comment_add",
        json!({"input":comment_input}),
        false,
    )
    .await;
    assert_eq!(added["kind"], "comment");
    assert_eq!(added["result"]["kind"], "created");
    let comment = added["result"]["result"]["comment"].clone();
    assert_eq!(comment["body"], body);
    assert_eq!(comment["kind"], "decision");
    assert_eq!(
        comment["actor"],
        claimed["result"]["record"]["ownership"]["owner"]
    );
    let replay = call(
        &client,
        "graphit_task_comment_add",
        json!({"input":comment_input}),
        false,
    )
    .await;
    assert_eq!(replay["result"]["kind"], "existing");
    assert_eq!(replay["result"]["result"]["comment"], comment);
    let mut current_input = comment_input.clone();
    current_input["target"]["claim"]["expected_revision"] =
        added["result"]["result"]["task"]["specification"]["revision"].clone();
    let replay_current = call(
        &client,
        "graphit_task_comment_add",
        json!({"input":current_input}),
        false,
    )
    .await;
    assert_eq!(replay_current["result"]["kind"], "existing");
    assert_eq!(replay_current["result"]["result"]["comment"], comment);
    for (field, value, expected) in [
        ("kind", json!("unknown"), "invalid_arguments"),
        ("body", json!(" \n "), "invalid_native_input"),
        ("actor", json!("forged-client-actor"), "invalid_arguments"),
        ("lease_ms", json!(0), "invalid_arguments"),
    ] {
        let mut invalid_input = current_input.clone();
        invalid_input["idempotency_key"] = json!(format!("invalid-{field}"));
        invalid_input[field] = value;
        let rejected = call(
            &client,
            "graphit_task_comment_add",
            json!({"input":invalid_input}),
            true,
        )
        .await;
        assert_eq!(rejected["code"], expected);
    }
    let mut wrong_fence = current_input.clone();
    wrong_fence["idempotency_key"] = json!("wrong-fence");
    wrong_fence["target"]["claim"]["token"] = json!("A".repeat(43));
    let rejected = call(
        &client,
        "graphit_task_comment_add",
        json!({"input":wrong_fence}),
        true,
    )
    .await;
    assert_eq!(rejected["code"], "fence_mismatch");
    let dependent = call(
        &client,
        "graphit_task_create",
        json!({"input":{
            "idempotency_key":"socket-dependent","session_id":session_id,"depends_on":[id],
            "title":"Dependent","description":"A real dependency","task_type":"TASK","priority":2,
            "checks":[{"kind":"acceptance","text":"Retain the dependency"},{"kind":"test","text":"Validate dependency completion"}]
        }}),
        false,
    )
    .await;
    let dependent_id = dependent["result"]["specification"]["id"].clone();
    let http = reqwest::Client::new();
    let mut credential = axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    credential.set_sensitive(true);
    let commit_result: Value = http
        .post(format!("{base}/api/tasks/native/set_task_commit"))
        .header(axum::http::header::AUTHORIZATION, credential.clone())
        .header("x-graphit-auth-origin", "runtime")
        .json(&json!({"input":{"id":id,
            "expected_revision":added["result"]["result"]["task"]["specification"]["revision"],
            "claim":{"token":token,"expected_revision":added["result"]["result"]["task"]["specification"]["revision"]},
            "commit":"b".repeat(40), "reason":"Record replacement commit through HTTP"
        }}))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    assert_eq!(commit_result["result"]["commit"], "b".repeat(40));
    let page: Value = http
        .post(format!("{base}/api/tasks/native/catalogue"))
        .header(axum::http::header::AUTHORIZATION, credential.clone())
        .header("x-graphit-auth-origin", "runtime")
        .json(&json!({"input":{"filter":{},"page_size":1,"cursor":null}}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page["result"]["results"].as_array().unwrap().len(), 1);
    assert!(page["result"]["next_cursor"].is_object());
    let second: Value = http
        .post(format!("{base}/api/tasks/native/catalogue"))
        .header(axum::http::header::AUTHORIZATION, credential.clone())
        .header("x-graphit-auth-origin", "runtime")
        .json(&json!({"input":{"filter":{},"page_size":1,"cursor":page["result"]["next_cursor"]}}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(second["result"]["results"].as_array().unwrap().len(), 1);
    let detail = call(
        &client,
        "graphit_task_get",
        json!({"input":{"id":id,"history":{"page_size":100},"comments":{"page_size":100}}}),
        false,
    )
    .await;
    let current_detail = call(
        &client,
        "graphit_task_get",
        json!({"input":{"id":id}}),
        false,
    )
    .await;
    assert!(current_detail["result"]["history"].is_null());
    assert!(current_detail["result"]["comments"].is_null());
    assert_eq!(current_detail["result"]["task"], detail["result"]["task"]);
    let first_page = call(&client, "graphit_task_get", json!({"input":{"id":id,"history":{"page_size":1,"cursor":current_detail["result"]["history_start_cursor"]}}}), false).await;
    assert_eq!(
        first_page["result"]["generation"],
        current_detail["result"]["generation"]
    );
    assert_eq!(
        first_page["result"]["history"]["before"],
        current_detail["result"]["creation"]["record"]
    );
    assert_eq!(
        first_page["result"]["history"]["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(first_page["result"]["history"]["next_cursor"].is_string());
    let encoded = detail.to_string();
    assert!(!encoded.contains(&token));
    assert_eq!(
        detail["result"]["task"]["specification"]["schedule"]["due_date"],
        "2028-03-02"
    );
    assert!(
        detail["result"]["history"]["events"]
            .as_array()
            .unwrap()
            .len()
            >= 2
    );
    assert_eq!(detail["result"]["comments"]["comments"], json!([comment]));
    let dependency = call(
        &client,
        "graphit_task_get",
        json!({"input":{"id":dependent_id,"history":{"page_size":100},"comments":{"page_size":100}}}),
        false,
    )
    .await;
    assert_eq!(dependency["result"]["task"]["depends_on"], json!([id]));
    let invalid=call(&client,"graphit_task_create",json!({"input":{
        "idempotency_key":"invalid","session_id":session_id,"title":"Invalid date","description":"Rejected",
        "task_type":"TASK","priority":1,"schedule":{"due_date":"2027-02-29"},"checks":[]
    }}),true).await;
    assert_eq!(invalid["code"], "invalid_arguments");
    let missing = connect(format!("{base}/mcp"), None).await;
    let denied = call(
        &missing,
        "graphit_task_query",
        query_arguments("tasks"),
        true,
    )
    .await;
    assert_eq!(denied["code"], "credential_required");
    let denied = call(
        &missing,
        "graphit_task_comment_add",
        json!({"input":current_input}),
        true,
    )
    .await;
    assert_eq!(denied["code"], "credential_required");
    missing.cancel().await.unwrap();
    let wrong = connect(format!("{base}/mcp"), Some("wrong-runtime-key")).await;
    let denied = call(&wrong, "graphit_task_query", query_arguments("tasks"), true).await;
    assert_eq!(denied["code"], "authentication_or_permission_denied");
    wrong.cancel().await.unwrap();
    let selected = call(
        &client,
        "graphit_task_get",
        json!({"selection":{"project_version":"unretained"},"input":{"id":id}}),
        true,
    )
    .await;
    assert_eq!(selected["code"], "selection_unavailable");
    assert_eq!(
        http.post(format!("{base}/api/tasks/native/get_task"))
            .json(&json!({"input":{"id":id}}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
    // A new component reopens the same real persisted Task through the native SDK.
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let reopened = TaskServer::new(
        Arc::clone(&server.0.auth),
        Arc::clone(&snapshot.projects),
        0,
        server.0.policy.clone(),
        CancellationToken::new(),
    )
    .unwrap();
    let credential = Credential {
        role_binding: None,
        bearer: zeroize::Zeroizing::new(raw),
        origin: Origin::Runtime,
    };
    let value = reopened
        .call(
            serde_json::from_value(
                json!({"input":{"id":id,"history":{"page_size":100},"comments":{"page_size":100}}}),
            )
            .unwrap(),
            "get_task",
            false,
            credential,
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|_| panic!("reopen native Task"));
    assert_eq!(
        value["result"]["task"]["specification"]["schedule"]["due_date"],
        "2028-03-02"
    );
    assert_eq!(value["result"]["task"]["commit"], "b".repeat(40));
    assert_eq!(value["result"]["comments"]["comments"], json!([comment]));
    assert!(!value.to_string().contains(&token));
    reopened.shutdown().await.unwrap();
}
#[test]
fn schema_selection_and_confirmed_errors_preserve_native_contract() {
    let (_temp, server, _raw) = fixture();
    let comment_tool = catalog::tools()
        .iter()
        .find(|tool| tool.name == "graphit_task_comment_add")
        .unwrap();
    let input = &comment_tool.input_schema["properties"]["input"];
    assert_eq!(input["additionalProperties"], false);
    assert_eq!(
        input["required"],
        json!(["target", "lease_ms", "idempotency_key", "kind", "body"])
    );
    assert_eq!(input["properties"]["lease_ms"]["minimum"], 1);
    assert_eq!(
        input["properties"]["kind"]["enum"],
        json!(["note", "decision", "problem", "lesson", "knowledge"])
    );
    assert!(input["properties"].get("actor").is_none());
    let target = &input["properties"]["target"];
    assert_eq!(target["additionalProperties"], false);
    assert_eq!(target["required"], json!(["id", "claim"]));
    assert_eq!(target["properties"]["claim"]["additionalProperties"], false);
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let root = &snapshot.projects[0].0;
    let selection: request::Selection = serde_json::from_value(json!({
        "project_dir":graphit_core::native_path::NativePath::new(root.root().to_owned()).unwrap(),
        "project_id":snapshot.projects[0].1
    }))
    .unwrap();
    assert_eq!(selection.select(&snapshot.projects, 0).unwrap(), 0);
    let duplicated = vec![snapshot.projects[0].clone(), snapshot.projects[0].clone()];
    assert_eq!(
        selection.select(&duplicated, 0).unwrap_err(),
        "selection_ambiguous"
    );
    let failure = Failure::store(TaskStoreError::Confirmed {
        version: 42,
        cause: Box::new(TaskStoreError::LeaseExpired),
    });
    let value = serde_json::to_value(failure).unwrap();
    assert_eq!(value["confirmed_version"], 42);
    assert_eq!(value["cause"]["code"], "lease_expired");
    assert_eq!(value["committed"], true);
    assert!(
        catalog::tools()
            .iter()
            .all(|tool| tool.input_schema["additionalProperties"] == false)
    );
}

#[tokio::test]
async fn managed_role_is_verified_per_call_and_revocation_denies_next_request() {
    use graphit_auth::{Permission, RoleDefinition};
    use graphit_core::native_path::NativePath;
    use graphit_hub::{ArtifactType, SourceOrigin, SourceRecord, SourceRef, native::Role};
    let (_temp, server, raw) = fixture();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, id, version) = &snapshot.projects[0];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let binding = project.binding().unwrap();
    let scope = ScopeInput {
        installation: server.0.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &binding,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let source = SourceRef::restore(
        SourceRecord {
            remote_file_origin: None,
            archive_sha256: None,
            origin: SourceOrigin::LocalLink,
            producer: *id,
            artifact_type: ArtifactType::Agent,
            id: "roles/scout".into(),
            physical_id: None,
            version: "1.0.0".into(),
            root: NativePath::new(paths.root().to_owned()).unwrap(),
        },
        server.0.policy.role_reference,
    )
    .unwrap();
    let permissions = [Permission::Read(Module::Task)];
    let definition = RoleDefinition::new(
        Role::Scout,
        &source,
        NativeBinding::KiroV3,
        &permissions,
        [7; 32],
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
    )
    .unwrap();
    let reference = definition.reference().clone();
    issuer.register_definition(&parent, definition).unwrap();
    let run = issuer
        .start_run(&parent, &scope, Duration::from_secs(30))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &permissions, &scope)
        .unwrap();
    let reference = String::from_utf8(
        reference
            .encode(server.0.policy.content.max_input_bytes, &original)
            .unwrap(),
    )
    .unwrap();
    let socket = listener();
    let uri = format!("http://{}/mcp", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let mut headers = std::collections::HashMap::new();
    let mut header =
        axum::http::HeaderValue::from_str(&format!("Bearer {}", bearer.expose_for_transport()))
            .unwrap();
    header.set_sensitive(true);
    headers.insert(axum::http::header::AUTHORIZATION, header);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_static("role"),
    );
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-role-reference"),
        axum::http::HeaderValue::from_str(&reference).unwrap(),
    );
    let client = ()
        .serve(StreamableHttpClientTransport::with_client(
            reqwest::Client::new(),
            StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
        ))
        .await
        .unwrap();
    let read = call(
        &client,
        "graphit_task_query",
        query_arguments("tasks"),
        false,
    )
    .await;
    assert_eq!(read["result"]["state"], "no_corpus");
    let denied=call(&client,"graphit_task_session_create",json!({"input":{
        "idempotency_key":"scout-denied","title":"Denied","description":"No write grant","strategy":"read"
    }}),true).await;
    assert_eq!(denied["code"], "authentication_or_permission_denied");
    let denied=call(&client,"graphit_task_session_revise",json!({"input":{
        "target":{"id":"00000000000000000000000001","claim":{"token":"AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE","expected_revision":2}},
        "lease_ms":30000,"reason":"Missing coordinator authority","strategy":"Denied"
    }}),true).await;
    assert_eq!(denied["code"], "authentication_or_permission_denied");
    issuer.revoke(&parent, &bearer).unwrap();
    let denied = call(
        &client,
        "graphit_task_query",
        query_arguments("tasks"),
        true,
    )
    .await;
    assert_eq!(denied["code"], "authentication_or_permission_denied");
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn blocking_native_job_keeps_scheduler_cancel_and_owned_shutdown_live() {
    let (_temp, server, raw) = fixture();
    let (started, observed) = oneshot::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    *server.0.gate.lock().unwrap() = Some((started, waiting));
    let (progressed, progress) = std::sync::mpsc::channel();
    // This is a deadlock watchdog, not a latency assertion. A regression that runs
    // the blocking receive on this current-thread scheduler must still release it.
    let watchdog = std::thread::spawn(move || {
        let scheduler_progressed = progress.recv_timeout(Duration::from_secs(5)).is_ok();
        let _ = release.send(());
        scheduler_progressed
    });
    let transport_cancel = CancellationToken::new();
    let caller = server.clone();
    let transport = transport_cancel.clone();
    let pending = tokio::spawn(async move {
        caller
            .call(
                serde_json::from_value(json!({})).unwrap(),
                "read_view",
                false,
                Credential {
                    role_binding: None,
                    bearer: zeroize::Zeroizing::new(raw),
                    origin: Origin::Runtime,
                },
                transport,
            )
            .await
    });
    let original_cancel = observed.await.unwrap();
    transport_cancel.cancel();
    assert_eq!(pending.await.unwrap().unwrap_err().code, "cancelled");
    assert!(original_cancel.is_cancelled());
    // The native job remains owned after its transport ends. Cancelling one
    // shutdown waiter must leave it available for the next waiter to join.
    let first = server.clone();
    let (closing, closing_started) = oneshot::channel();
    let shutdown = tokio::spawn(async move {
        closing.send(()).unwrap();
        first.shutdown().await
    });
    closing_started.await.unwrap();
    assert!(!shutdown.is_finished());
    shutdown.abort();
    assert!(shutdown.await.unwrap_err().is_cancelled());
    let second = server.clone();
    let (closing, closing_started) = oneshot::channel();
    let shutdown = tokio::spawn(async move {
        closing.send(()).unwrap();
        second.shutdown().await
    });
    closing_started.await.unwrap();
    assert!(!shutdown.is_finished());
    progressed
        .send(())
        .expect("scheduler must progress while native work blocks");
    shutdown.await.unwrap().unwrap();
    assert!(
        watchdog.join().unwrap(),
        "native work blocked the async scheduler"
    );
    assert!(server.0.jobs.lock().await.is_empty());
}

#[tokio::test]
async fn sdk_cancellation_drains_native_writer_wait_without_publishing() {
    use graphit_core::artifact_store::ArtifactStore;
    use rmcp::model::{CallToolRequest, ClientRequest};
    let (_temp, server, raw) = fixture();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, id, _version) = &snapshot.projects[0];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let artifacts = ArtifactStore::new(project, server.0.policy.write.artifacts);
    // Hold the actual Core writer lease. The operation must wait on this real
    // filesystem lock rather than a fixture marker or a second supervisor.
    let writer = artifacts.writer_with_context(original.snapshot()).unwrap();
    let socket = listener();
    let uri = format!("http://{}/mcp", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let client = connect(uri, Some(&raw)).await;
    let pending=client.send_cancellable_request(
        ClientRequest::CallToolRequest(CallToolRequest::new(
            CallToolRequestParams::new("graphit_task_session_create").with_arguments(json!({"input":{
                "idempotency_key":"cancel-locked-writer","title":"Cancelled","description":"Actual SDK writer wait","strategy":"cancel"
            }}).as_object().unwrap().clone())
        )),Default::default()
    ).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !server.0.jobs.lock().await.is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    pending.cancel(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), server.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
    served.await.unwrap().unwrap();
    client.cancel().await.unwrap();
    drop(writer);
    cancel.cancel();
    let cancelled_original = Box::pin(execute(
        &server.0,
        &server.0.live_projects.snapshot().unwrap().projects,
        0,
        Operation::Read(graphit_records::task_protocol::ReadRequest::ReadView),
        Credential {
            role_binding: None,
            bearer: zeroize::Zeroizing::new(raw.clone()),
            origin: Origin::Runtime,
        },
        &original,
    ))
    .await;
    assert!(matches!(
        cancelled_original,
        Err(Failure {
            code: "request_cancelled_or_expired",
            ..
        })
    ));
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let reopened = TaskServer::new(
        Arc::clone(&server.0.auth),
        Arc::clone(&snapshot.projects),
        0,
        server.0.policy.clone(),
        CancellationToken::new(),
    )
    .unwrap();
    let observed = reopened
        .call(
            serde_json::from_value(json!({})).unwrap(),
            "read_view",
            false,
            Credential {
                role_binding: None,
                bearer: zeroize::Zeroizing::new(raw),
                origin: Origin::Runtime,
            },
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|_| panic!("read native corpus after cancellation"));
    assert_eq!(observed["result"], "NoCorpus");
    reopened.shutdown().await.unwrap();
}

pub(crate) fn browser_provider(
    server: &TaskServer,
    raw: &str,
    endpoint: &str,
    remove: bool,
) -> u64 {
    use graphit_auth::{
        profile::{AiService, BrokerService, ProviderConfig, ProviderKind, S3Location, S3Source},
        profile_manager::{NativeProfileManager, ProfileManagerAuthority},
    };
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let manager = NativeProfileManager::new(issuer);
    let snapshot = manager
        .begin(ProfileManagerAuthority::Admin(&parent))
        .unwrap();
    let revision = snapshot.revision();
    let publication = if remove {
        manager
            .remove_provider(snapshot, "broker", false, revision)
            .unwrap()
    } else {
        manager
            .add_provider(
                snapshot,
                "broker".into(),
                ProviderConfig {
                    kind: ProviderKind::Broker(BrokerService {
                        endpoint: endpoint.into(),
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
                revision,
                1,
            )
            .unwrap()
    };
    assert!(publication.failure().is_none() && publication.cleanup_failure().is_none());
    assert!(publication.into_pending_cleanup().is_none());
    if remove {
        return 0;
    }
    let snapshot = manager
        .begin(ProfileManagerAuthority::Admin(&parent))
        .unwrap();
    snapshot
        .providers()
        .find(|p| p.name() == "broker")
        .unwrap()
        .revision()
}
pub(crate) async fn seed_browser(
    server: &TaskServer,
    raw: &str,
    operation: &str,
    input: Value,
) -> Value {
    server
        .call(
            serde_json::from_value(json!({"input":input})).unwrap(),
            operation,
            true,
            Credential {
                role_binding: None,
                bearer: zeroize::Zeroizing::new(raw.to_owned()),
                origin: Origin::Runtime,
            },
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|e| panic!("native seed: {}", e.code))["result"]
        .clone()
}
pub(crate) fn reopen_browser(server: &TaskServer) -> TaskServer {
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    TaskServer::new(
        Arc::clone(&server.0.auth),
        Arc::clone(&snapshot.projects),
        snapshot.current.expect("active fixture"),
        server.0.policy.clone(),
        CancellationToken::new(),
    )
    .unwrap()
}
pub(crate) fn browser_selection(server: &TaskServer) -> Value {
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, id, version) = &snapshot.projects[snapshot.current.expect("active fixture")];
    json!({"project_dir":graphit_core::native_path::NativePath::new(paths.root().to_owned()).unwrap(),"project_id":id,"project_version":version.version()})
}

mod workspace;
pub(crate) use workspace::browser_workspace_fixture;
#[cfg(target_os = "linux")]
pub(crate) use workspace::{knowledge_workspace_fixture, memory_workspace_fixture};

#[test]
fn reference_failure_mapping_preserves_request_status_and_confirmed_cause() {
    use graphit_records::reference_projection::ReferenceProjectionError;
    let request = Failure::store(TaskStoreError::Reference(
        ReferenceProjectionError::Request(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "fixture deadline",
        )),
    ));
    assert_eq!(request.code, "request_cancelled_or_expired");
    assert_eq!(request.status(), StatusCode::REQUEST_TIMEOUT);
    let projection = Failure::store(TaskStoreError::Reference(
        ReferenceProjectionError::InvalidShape,
    ));
    assert_eq!(projection.code, "reference_projection_failure");
    assert_eq!(projection.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let confirmed = Failure::store(TaskStoreError::Confirmed {
        version: 42,
        cause: Box::new(TaskStoreError::Reference(
            ReferenceProjectionError::InvalidShape,
        )),
    });
    assert_eq!(confirmed.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let value = serde_json::to_value(confirmed).unwrap();
    assert_eq!(value["code"], "write_confirmed");
    assert_eq!(value["confirmed_version"], 42);
    assert_eq!(value["committed"], true);
    assert_eq!(value["cause"]["code"], "reference_projection_failure");
}

#[tokio::test]
async fn actual_mcp_cancelled_child_descendants_and_cancelled_dependency_preserve_native_gates() {
    use graphit_records::task_protocol::{self, WriteRequest};
    let (_temp, server, raw) = fixture();
    let socket = listener();
    let base = format!("http://{}", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let client = connect(format!("{base}/mcp"), Some(&raw)).await;
    let session = call(&client, "graphit_task_session_create", json!({"input": {
        "idempotency_key":"cancelled-tree-session", "title":"Cancelled child scope",
        "description":"Actual HTTP and native SDK gate routing", "strategy":"Preserve withdrawal and ordering"
    }}), false).await;
    let session_id = session["result"]["id"].clone();
    let mut records: Vec<Value> = Vec::new();
    // Terminal parents cannot acquire new children, so the complete tree exists first.
    for (key, parent, dependency) in [
        ("parent", None, None),
        ("withdrawn-child", Some(0), None),
        ("grandchild-first", Some(1), None),
        ("grandchild-second", Some(1), None),
        ("explicit-dependent", None, Some(1)),
    ] {
        let parent_id = parent.map(|index| records[index]["specification"]["id"].clone());
        let depends_on: Vec<Value> = dependency
            .into_iter()
            .map(|index| records[index]["specification"]["id"].clone())
            .collect();
        let result = call(&client, "graphit_task_create", json!({"input": {
            "idempotency_key":key, "session_id":session_id, "parent_id":parent_id,
            "depends_on":depends_on, "title":key, "description":"Actual persisted Task gate proof",
            "task_type":"task", "priority":1,
            "checks":[{"kind":"acceptance","text":"Complete the required scope"},
                      {"kind":"test","text":"Observe native HTTP and SDK history"}]
        }}), false).await;
        records.push(result["result"].clone());
    }
    let ids: Vec<Value> = records
        .iter()
        .map(|record| record["specification"]["id"].clone())
        .collect();
    let mut tokens = Vec::new();
    let child_claim = call(
        &client,
        "graphit_task_claim",
        json!({"input": {
            "id":ids[1], "lease_ms":600000
        }}),
        false,
    )
    .await;
    let child_token = child_claim["result"]["token"].as_str().unwrap().to_owned();
    tokens.push(child_token.clone());
    let reason = "Withdraw child scope without certifying its delivery";
    let cancelled = call(
        &client,
        "graphit_task_cancel",
        json!({"input": {
            "id":ids[1], "claim":{"token":child_token,
                "expected_revision":child_claim["result"]["record"]["specification"]["revision"]},
            "reason":reason
        }}),
        false,
    )
    .await;
    assert_eq!(cancelled["result"]["status"], "cancelled");
    assert_eq!(cancelled["result"]["ownership"]["owner"], Value::Null);
    assert_eq!(cancelled["result"]["ownership"]["claim_epoch"], 1);
    assert!(
        cancelled["result"]["specification"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|check| check["status"] == "pending")
    );
    let child_snapshot = call(
        &client,
        "graphit_task_get",
        json!({"input":{"id":ids[1],"history":{"page_size":100},"comments":{"page_size":100}}}),
        false,
    )
    .await;
    assert_eq!(child_snapshot["result"]["task"], cancelled["result"]);
    assert_eq!(
        child_snapshot["result"]["history"]["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        child_snapshot["result"]["history"]["events"][1]["kind"],
        json!({"cancelled":{"reason":reason}})
    );

    // The transport intentionally maps Invalid to one public category. Inspect the
    // same native protocol under fresh real authority to prove the particular gate cause.
    let native_error = async |input: WriteRequest| {
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            server.0.policy.content,
            &cancel,
            Instant::now() + server.0.policy.timeout,
        )
        .unwrap();
        let snapshot = server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live metadata fixture"));
        let (paths, id, version) = &snapshot.projects[0];
        let project = CapturedPublicationScope::project(
            paths,
            *id,
            server.0.policy.scope,
            original.snapshot(),
        )
        .unwrap();
        let record = project.binding().unwrap();
        let scope = ScopeInput {
            installation: server.0.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &project,
                    record: &record,
                },
                version,
            }),
            providers: &[],
            binding: server.0.policy.binding,
            global_version: "",
        };
        let issuer = server.0.auth.issuer();
        let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let admission = authority
            .admit_authenticated_write(&caller, &original, scope, input.required_operation())
            .unwrap();
        match task_protocol::execute_write(
            input,
            original.snapshot(),
            &admission,
            server.0.policy.write,
        )
        .await
        {
            Err(error) => error,
            Ok(_) => panic!("native gate must reject before publication"),
        }
    };
    // Each blocked parent is released before claiming a grandchild; one actor never
    // carries two live Task claims, and cancelled-child checks are never faked Passed.
    for completed_children in 0..=2 {
        let claimed = call(
            &client,
            "graphit_task_claim",
            json!({"input": {
                "id":ids[0], "lease_ms":600000
            }}),
            false,
        )
        .await;
        let token = claimed["result"]["token"].as_str().unwrap().to_owned();
        tokens.push(token.clone());
        let mut current = claimed["result"]["record"].clone();
        if completed_children == 0 {
            for check in records[0]["specification"]["checks"].as_array().unwrap() {
                let verified = call(
                    &client,
                    "graphit_task_check",
                    json!({"input": {
                        "target":{"id":ids[0], "claim":{"token":token,
                            "expected_revision":current["specification"]["revision"]}},
                        "lease_ms":600000, "check_id":check["id"], "passed":true,
                        "evidence":"Actual TaskServer HTTP and SDK required checks"
                    }}),
                    false,
                )
                .await;
                current = verified["result"].clone();
            }
        }
        assert!(!current["flagged"].as_bool().unwrap());
        assert!(
            current["specification"]["checks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|check| check["status"] == "passed")
        );
        let completion = json!({"id":ids[0], "claim":{"token":token,
            "expected_revision":current["specification"]["revision"]}});
        if completed_children == 2 {
            let completed = call(
                &client,
                "graphit_task_complete",
                json!({"input":completion}),
                false,
            )
            .await;
            assert_eq!(completed["result"]["status"], "completed");
            assert_eq!(completed["result"]["ownership"]["owner"], Value::Null);
            break;
        }
        let before = call(
            &client,
            "graphit_task_get",
            json!({"input":{"id":ids[0],"history":{"page_size":100},"comments":{"page_size":100}}}),
            false,
        )
        .await;
        let rejected = call(
            &client,
            "graphit_task_complete",
            json!({"input":completion}),
            true,
        )
        .await;
        assert_eq!(rejected["code"], "invalid_native_input");
        assert!(rejected.get("committed").is_none());
        assert!(matches!(
            native_error(WriteRequest::CompleteTask(
                serde_json::from_value(completion).unwrap()
            ))
            .await,
            TaskStoreError::Invalid("completion gates are pending")
        ));
        let after = call(
            &client,
            "graphit_task_get",
            json!({"input":{"id":ids[0],"history":{"page_size":100},"comments":{"page_size":100}}}),
            false,
        )
        .await;
        assert_eq!(after, before);
        call(&client, "graphit_task_release", json!({"input": {
            "target":{"id":ids[0], "claim":{"token":token,
                "expected_revision":current["specification"]["revision"]}},
            "summary":"Descendant scope still required", "next_step":"Complete the next grandchild"
        }}), false).await;
        let descendant_id = &ids[completed_children + 2];
        let claimed = call(
            &client,
            "graphit_task_claim",
            json!({"input": {
                "id":descendant_id, "lease_ms":600000
            }}),
            false,
        )
        .await;
        let descendant_token = claimed["result"]["token"].as_str().unwrap().to_owned();
        tokens.push(descendant_token.clone());
        let mut descendant = claimed["result"]["record"].clone();
        for check in records[completed_children + 2]["specification"]["checks"]
            .as_array()
            .unwrap()
        {
            let verified = call(
                &client,
                "graphit_task_check",
                json!({"input": {
                    "target":{"id":descendant_id, "claim":{"token":descendant_token,
                        "expected_revision":descendant["specification"]["revision"]}},
                    "lease_ms":600000, "check_id":check["id"], "passed":true,
                    "evidence":"Required descendant really verified through TaskServer HTTP"
                }}),
                false,
            )
            .await;
            descendant = verified["result"].clone();
        }
        let completed = call(
            &client,
            "graphit_task_complete",
            json!({"input": {
                "id":descendant_id, "claim":{"token":descendant_token,
                    "expected_revision":descendant["specification"]["revision"]}
            }}),
            false,
        )
        .await;
        assert_eq!(completed["result"]["status"], "completed");
    }
    let dependent_before = call(
        &client,
        "graphit_task_get",
        json!({"input":{"id":ids[4],"history":{"page_size":100},"comments":{"page_size":100}}}),
        false,
    )
    .await;
    let claim_input = json!({"id":ids[4], "lease_ms":600000});
    let rejected = call(
        &client,
        "graphit_task_claim",
        json!({"input":claim_input}),
        true,
    )
    .await;
    assert_eq!(rejected["code"], "invalid_native_input");
    assert!(matches!(
        native_error(WriteRequest::ClaimTask(
            serde_json::from_value(claim_input).unwrap()
        ))
        .await,
        TaskStoreError::Invalid("task is not ready")
    ));
    assert_eq!(
        call(
            &client,
            "graphit_task_get",
            json!({"input":{"id":ids[4],"history":{"page_size":100},"comments":{"page_size":100}}}),
            false
        )
        .await,
        dependent_before
    );
    assert_eq!(
        call(
            &client,
            "graphit_task_get",
            json!({"input":{"id":ids[1],"history":{"page_size":100},"comments":{"page_size":100}}}),
            false
        )
        .await["result"]["task"],
        child_snapshot["result"]["task"]
    );

    let mut credential = axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    credential.set_sensitive(true);
    let catalogue: Value = reqwest::Client::new()
        .post(format!("{base}/api/tasks/native/catalogue"))
        .header(axum::http::header::AUTHORIZATION, credential)
        .header("x-graphit-auth-origin", "runtime")
        .json(&json!({"input":{"filter":{},"page_size":100,"cursor":null}}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let summaries = catalogue["result"]["results"].as_array().unwrap();
    let dependent = summaries
        .iter()
        .find(|record| record["id"] == ids[4])
        .unwrap();
    assert_eq!(dependent["status"], "open");
    assert_eq!(dependent["ready"], false);
    for (index, status) in [
        (0, "completed"),
        (1, "cancelled"),
        (2, "completed"),
        (3, "completed"),
        (4, "open"),
    ] {
        let detail = call(
            &client,
            "graphit_task_get",
            json!({"input":{"id":ids[index],"history":{"page_size":100},"comments":{"page_size":100}}}),
            false,
        )
        .await;
        let current = &detail["result"]["task"];
        assert_eq!(current["status"], status);
        assert_eq!(current["parent_id"], records[index]["parent_id"]);
        assert_eq!(detail["result"]["creation"]["record"], records[index]);
        let mut previous = 1;
        for event in detail["result"]["history"]["events"].as_array().unwrap() {
            let revision = event["record"]["specification"]["revision"]
                .as_u64()
                .unwrap();
            assert_eq!(revision, previous + 1);
            assert_eq!(event["record"]["reference_generation"], revision);
            assert_eq!(event["actor"], detail["result"]["creation"]["actor"]);
            previous = revision;
        }
        assert_eq!(current["specification"]["revision"], previous);
        if let Some(event) = detail["result"]["history"]["events"]
            .as_array()
            .unwrap()
            .last()
        {
            assert_eq!(event["record"], *current);
        }
        assert_eq!(current["ownership"]["owner"], Value::Null);
        let encoded = detail.to_string();
        assert!(tokens.iter().all(|token| !encoded.contains(token)));
    }
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
}

#[tokio::test]
async fn retired_selection_after_admission_wait_cannot_deliver_healthy_read() {
    let (_temp, server, raw) = fixture();
    let credential = || Credential {
        role_binding: None,
        bearer: zeroize::Zeroizing::new(raw.clone()),
        origin: Origin::Runtime,
    };
    let created = server.call(
        serde_json::from_value(json!({"input":{"idempotency_key":"retired-read-session","title":"Retirement","description":"Real SDK corpus remains present","strategy":"Fence accepted work"}})).unwrap(),
        "create_session", true, credential(), CancellationToken::new(),
    ).await.unwrap_or_else(|_| panic!("fixture Session creation failed"));
    let id = created["result"]["id"].clone();
    let before = server
        .call(
            serde_json::from_value(json!({"input":{"id":id}})).unwrap(),
            "get_session",
            false,
            credential(),
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|_| panic!("fixture Session read failed"));
    assert_eq!(before["result"]["session"]["id"], id);
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, project_id, _) = &snapshot.projects[0];
    let queue = server.0.jobs.lock().await;
    let mut pending = Box::pin(server.call(
        serde_json::from_value(json!({"input":{"id":id}})).unwrap(),
        "get_session",
        false,
        credential(),
        CancellationToken::new(),
    ));
    std::future::poll_fn(|context| {
        assert!(pending.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    server
        .0
        .live_projects
        .retire_checkout(*project_id, paths.root())
        .unwrap();
    assert!(paths.brand_directory().exists());
    drop(queue);
    let result = pending.await;
    assert!(matches!(
        result,
        Err(Failure {
            code: "selection_unavailable",
            ..
        })
    ));
    server.shutdown().await.unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
}

#[path = "catalog/reference_consumer.rs"]
mod instruction_references;

#[tokio::test]
async fn actual_mcp_and_http_session_revision_preserves_history_and_denies_foreign_owner() {
    use graphit_auth::{Permission, RoleDefinition};
    use graphit_core::native_path::NativePath;
    use graphit_hub::{ArtifactType, SourceOrigin, SourceRecord, SourceRef, native::Role};
    let (_temp, server, raw) = fixture();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, id, version) = &snapshot.projects[0];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let binding = project.binding().unwrap();
    let scope = ScopeInput {
        installation: server.0.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &binding,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let source = SourceRef::restore(
        SourceRecord {
            remote_file_origin: None,
            archive_sha256: None,
            origin: SourceOrigin::LocalLink,
            producer: *id,
            artifact_type: ArtifactType::Agent,
            id: "roles/other-coordinator".into(),
            physical_id: None,
            version: "1.0.0".into(),
            root: NativePath::new(paths.root().to_owned()).unwrap(),
        },
        server.0.policy.role_reference,
    )
    .unwrap();
    let permissions = [Permission::Read(Module::Task), Permission::CoordinateTask];
    let definition = RoleDefinition::new(
        Role::Coordinator,
        &source,
        NativeBinding::KiroV3,
        &permissions,
        [7; 32],
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
    )
    .unwrap();
    let reference = definition.reference().clone();
    issuer.register_definition(&parent, definition).unwrap();
    let run = issuer
        .start_run(&parent, &scope, Duration::from_secs(30))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &permissions, &scope)
        .unwrap();
    let reference = String::from_utf8(
        reference
            .encode(server.0.policy.content.max_input_bytes, &original)
            .unwrap(),
    )
    .unwrap();

    let socket = listener();
    let base = format!("http://{}", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let client = connect(format!("{base}/mcp"), Some(&raw)).await;
    let listed = client.list_all_tools().await.unwrap();
    let get = listed
        .iter()
        .find(|tool| tool.name == "graphit_task_session_get")
        .unwrap();
    let input = &get.input_schema["properties"]["input"];
    assert!(input["properties"].get("history").is_some());
    assert_eq!(input["required"], json!(["id"]));
    let tool = listed
        .iter()
        .find(|tool| tool.name == "graphit_task_session_revise")
        .unwrap();
    let schema = &tool.input_schema["properties"]["input"];
    for field in ["title", "description", "strategy"] {
        assert_eq!(schema["properties"][field]["type"], "string");
        assert!(
            !schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!(field))
        );
    }
    let session=call(&client,"graphit_task_session_create",json!({"input":{
        "idempotency_key":"session-revision","title":"Original title","description":"Original demand","strategy":"Original strategy"
    }}),false).await;
    let id = session["result"]["id"].clone();
    let claimed = call(
        &client,
        "graphit_task_session_claim",
        json!({"input":{"id":id,"lease_ms":300000}}),
        false,
    )
    .await;
    let token = claimed["result"]["token"].clone();
    let mut target = json!({"id":id,"claim":{"token":token,"expected_revision":claimed["result"]["record"]["revision"]}});
    let checkpoint=call(&client,"graphit_task_session_checkpoint",json!({"input":{
        "target":target,"lease_ms":300000,"summary":"Confirmed work","next_step":"Revise the remaining strategy"
    }}),false).await;
    target["claim"]["expected_revision"] = checkpoint["result"]["revision"].clone();
    let revised=call(&client,"graphit_task_session_revise",json!({"input":{
        "target":target,"lease_ms":300000,"reason":"Update accepted strategy","strategy":"Current strategy"
    }}),false).await;
    assert_eq!(revised["result"]["input"]["title"], "Original title");
    assert_eq!(revised["result"]["input"]["description"], "Original demand");
    assert_eq!(
        revised["result"]["progress"],
        checkpoint["result"]["progress"]
    );
    let stale = call(
        &client,
        "graphit_task_session_revise",
        json!({"input":{
            "target":target,"lease_ms":300000,"reason":"Stale request","title":"Stale title"
        }}),
        true,
    )
    .await;
    assert_eq!(stale["code"], "revision_conflict");
    target["claim"]["expected_revision"] = revised["result"]["revision"].clone();
    let http = reqwest::Client::new();
    let foreign:Value=http.post(format!("{base}/api/tasks/native/revise_session"))
        .bearer_auth(bearer.expose_for_transport()).header("x-graphit-auth-origin","role")
        .header("x-graphit-role-reference",reference)
        .json(&json!({"input":{"target":target,"lease_ms":300000,"reason":"Foreign owner","title":"Denied title"}}))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(foreign["error"]["code"], "fence_mismatch");
    let references = json!([{"target":{"type":"session","id":id},"relation":"supports"}]);
    let referenced:Value=http.post(format!("{base}/api/tasks/native/revise_session"))
        .bearer_auth(&raw).header("x-graphit-auth-origin","runtime")
        .json(&json!({"input":{"target":target,"lease_ms":300000,"reason":"Record accepted relation","references":references}}))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    assert_eq!(
        referenced["result"]["references"].as_array().unwrap().len(),
        1
    );
    target["claim"]["expected_revision"] = referenced["result"]["revision"].clone();
    let kept=call(&client,"graphit_task_session_revise",json!({"input":{
        "target":target,"lease_ms":300000,"reason":"Update title and retain relation","title":"Current title"
    }}),false).await;
    assert_eq!(
        kept["result"]["references"],
        referenced["result"]["references"]
    );
    target["claim"]["expected_revision"] = kept["result"]["revision"].clone();
    let cleared = call(
        &client,
        "graphit_task_session_revise",
        json!({"input":{
            "target":target,"lease_ms":300000,"reason":"Clear relation","references":[]
        }}),
        false,
    )
    .await;
    assert_eq!(cleared["result"]["references"], json!([]));
    target["claim"]["expected_revision"] = cleared["result"]["revision"].clone();
    let no_op = call(
        &client,
        "graphit_task_session_revise",
        json!({"input":{
            "target":target,"lease_ms":300000,"reason":"No change","strategy":"Current strategy"
        }}),
        true,
    )
    .await;
    assert_eq!(no_op["code"], "invalid_native_input");
    let detail = call(
        &client,
        "graphit_task_session_get",
        json!({"input":{"id":id}}),
        false,
    )
    .await;
    assert_eq!(detail["result"]["session"], cleared["result"]);
    assert_eq!(detail["result"]["creation"]["record"], session["result"]);
    assert!(detail["result"]["history"].is_null());
    assert!(detail["result"].get("events").is_none());
    assert_eq!(
        detail["result"]["last_event"]["record"],
        detail["result"]["session"]
    );
    assert!(!detail.to_string().contains(token.as_str().unwrap()));
    let mut cursor = detail["result"]["history_start_cursor"].clone();
    let mut selected = call(
        &client,
        "graphit_task_session_get",
        json!({"input":{"id":id,"history":{"page_size":2,"cursor":cursor}}}),
        false,
    )
    .await;
    let mut before = session["result"].clone();
    let mut count = 0;
    loop {
        assert_eq!(
            selected["result"]["generation"],
            detail["result"]["generation"]
        );
        let history = &selected["result"]["history"];
        assert_eq!(history["before"], before);
        let events = history["events"].as_array().unwrap();
        assert!(events.len() <= 2);
        for event in events {
            assert_eq!(
                event["record"]["revision"].as_u64().unwrap(),
                before["revision"].as_u64().unwrap() + 1
            );
            before = event["record"].clone();
            count += 1;
        }
        assert!(!selected.to_string().contains(token.as_str().unwrap()));
        let next = history["next_cursor"].clone();
        if next.is_null() {
            break;
        }
        assert_ne!(next, cursor);
        cursor = next;
        selected = http
            .post(format!("{base}/api/tasks/native/get_session"))
            .bearer_auth(&raw)
            .header("x-graphit-auth-origin", "runtime")
            .json(&json!({"input":{"id":id,"history":{"page_size":2,"cursor":cursor}}}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
    }
    assert_eq!(count, 6);
    assert_eq!(before, cleared["result"]);
    let invalid = call(
        &client,
        "graphit_task_session_get",
        json!({"input":{"id":id,"history":{"page_size":0}}}),
        true,
    )
    .await;
    assert_eq!(invalid["code"], "invalid_arguments");
    assert_eq!(
        detail["result"]["session"]["input"]["strategy"],
        "Current strategy"
    );
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[test]
fn storage_io_failure_mapping_preserves_auth_and_never_exposes_private_causes() {
    use graphit_auth::{ServiceUseError, profile::ProfileError};
    for kind in [
        std::io::ErrorKind::Interrupted,
        std::io::ErrorKind::TimedOut,
    ] {
        let failure = Failure::store(TaskStoreError::Request(std::io::Error::new(
            kind,
            "private storage canary",
        )));
        assert_eq!(failure.code, "request_cancelled_or_expired");
        assert_eq!(failure.status(), StatusCode::REQUEST_TIMEOUT);
    }
    for error in [
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, "private token canary"),
        std::io::Error::other(ServiceUseError::Profile(ProfileError::Stale)),
    ] {
        let failure = Failure::store(TaskStoreError::Request(error));
        assert_eq!(failure.code, "authentication_or_permission_denied");
        assert_eq!(failure.status(), StatusCode::FORBIDDEN);
        assert!(!serde_json::to_string(&failure).unwrap().contains("canary"));
    }
    for kind in [
        std::io::ErrorKind::Other,
        std::io::ErrorKind::NotFound,
        std::io::ErrorKind::InvalidData,
    ] {
        let failure = Failure::store(TaskStoreError::Request(std::io::Error::new(
            kind,
            "private storage canary",
        )));
        assert_eq!(failure.code, "store_failure");
        assert_eq!(failure.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!serde_json::to_string(&failure).unwrap().contains("canary"));
    }
}
