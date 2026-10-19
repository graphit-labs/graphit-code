//! Actual Web login and HTTP create/prepare/run; the ACP peer is controlled test infrastructure.
mod no_corpus;
use super::*;
use crate::{
    knowledge_ask_service::AskAvailability,
    knowledge_live::{HistoryLimits, LiveSessionService},
    source_server::{SourcePolicy, project_hook::HookPolicy},
    task_server::{
        TaskServer, browser::records_fixture as native, tests::fixture_with_reference_permissions,
    },
};
use graphit_auth::Permission;
use graphit_core::{
    artifact_store::{
        ArtifactMode, ArtifactStore, CapturedPublicationScope, CopyPolicy, PayloadMode,
        PayloadOverlay, PayloadPolicy, PayloadSelection, RetainedBytesLimits, RetainedFileSet,
        SnapshotPolicy, SourceReadLimits, SourceSnapshot, StoreOwner, ValidatedPayloadPlan,
    },
    config::ConfigSnapshot,
    file_lock::Cancellation,
    module_settings::Module,
    native_path::NativePath,
    project_paths::InstallationPaths,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, NativeBinding, ProjectionRequest, SourceRef,
    materialization::DerivedArtifact,
    native::{NativeToolGrant, ToolAccess, ToolGrants},
    policy::{HubResourcePolicy, RequestContext},
    publication::ScopedCoordinator,
    registry::ArtifactDisplayMetadata,
};
use std::path::{Path, PathBuf};

const PROMPT: &str = r#"{"question":"Read the selected instructions","pages":[{"slug":"fixture-page","body":"Literal authorized input"}]}"#;

fn installation(tasks: &TaskServer) -> InstallationPaths {
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let root = resources.auth.installation().live;
    InstallationPaths::resolve(
        root.brand().clone(),
        root.owner_root(),
        None,
        Some(root.owner_root()),
    )
    .unwrap()
}

fn publication_coordinator(tasks: &TaskServer, original: &RequestContext<'_>) -> ScopedCoordinator {
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let registry = &resources
        .policy
        .imported_knowledge
        .as_ref()
        .unwrap()
        .registry;
    let paths = installation(tasks);
    let destination = paths.root().join("fixture-projection");
    std::fs::create_dir_all(destination.join(".kiro/agents")).unwrap();
    std::fs::create_dir_all(destination.join(".kiro/skills")).unwrap();
    let StoreOwner::Installation { installation_id } = resources.auth.installation().live.owner()
    else {
        panic!("installation owner");
    };
    let scope = CapturedPublicationScope::installation(
        &paths,
        installation_id.clone(),
        [destination.clone()],
        resources.policy.scope,
        original.snapshot(),
    )
    .unwrap();
    let store = ArtifactStore::new(scope, registry.limits().artifacts());
    ScopedCoordinator::new(
        registry.clone(),
        store,
        NativePath::new(destination.clone()).unwrap(),
    )
    .unwrap()
}

/// Validated SDK bindings and registry metadata, never an authentication fixture.
pub(super) fn installed_texts(tasks: &TaskServer) -> Vec<Value> {
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let registry = &resources
        .policy
        .imported_knowledge
        .as_ref()
        .unwrap()
        .registry;
    let paths = installation(tasks);
    let projection = registry.limits().projection();
    let (project, producer, _) = &resources.projects[0];
    let cancel = Cancellation::default();
    let mut original = RequestContext::new(
        resources.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let coordinator = publication_coordinator(tasks, &original);
    let mut selections = Vec::new();
    for (kind, id, body) in [
        (
            ArtifactType::Agent,
            "fixture/live-agent",
            "---\nname: live-agent\ndescription: HTTP Live agent\nread_only: true\nallows_shell: false\n---\nFollow the selected rule. Read [helper](prompt.md).\n",
        ),
        (
            ArtifactType::Skill,
            "fixture/live-skill",
            "---\nname: live-skill\ndescription: Selected HTTP skill\n---\nUse [the reference](references/reference.md).\n",
        ),
        (
            ArtifactType::Rule,
            "fixture/live-rule",
            "Keep the literal rule marker rule-from-installed-source.\n",
        ),
    ] {
        let id = ArtifactId::new(id, projection).unwrap();
        let version = ConcreteVersion::new("1.0.0", projection).unwrap();
        let source = if kind == ArtifactType::Agent {
            SourceRef::resources(project, *producer, kind, id.clone(), version, projection).unwrap()
        } else {
            SourceRef::hub_cache(
                &paths,
                *producer,
                kind,
                id.clone(),
                None,
                version,
                projection,
            )
            .unwrap()
        };
        std::fs::create_dir_all(source.directory().as_path()).unwrap();
        let entry = match kind {
            ArtifactType::Agent => "AGENT.md",
            ArtifactType::Skill => "SKILL.md",
            _ => "RULE.md",
        };
        std::fs::write(source.directory().as_path().join(entry), body).unwrap();
        if kind == ArtifactType::Agent {
            std::fs::write(
                source.directory().as_path().join("prompt.md"),
                "agent-auxiliary-marker",
            )
            .unwrap();
        }
        if kind == ArtifactType::Skill {
            std::fs::create_dir(source.directory().as_path().join("references")).unwrap();
            std::fs::write(
                source.directory().as_path().join("references/reference.md"),
                "skill-auxiliary-marker",
            )
            .unwrap();
        }
        let projection_request = ProjectionRequest {
            binding: NativeBinding::KiroV3,
            source: &source,
            alias: None,
            entry_name: None,
        };
        let mut writer = coordinator.writer(&original).unwrap();
        if kind == ArtifactType::Agent {
            let limits = registry.limits();
            let retained = RetainedBytesLimits::new(
                resources.policy.content.max_input_bytes,
                resources.policy.content.max_collection_entries,
                resources.policy.content.max_scalar_bytes,
            )
            .unwrap();
            let snapshot = SourceSnapshot::capture(
                source.directory().as_path(),
                graphit_fs::NodeKind::Directory,
                &SnapshotPolicy::new(
                    CopyPolicy::RejectFileLinks,
                    limits.materialization().snapshots,
                    RetainedFileSet::new(["AGENT.md".into()], retained).unwrap(),
                    retained,
                )
                .unwrap(),
                original.snapshot(),
            )
            .unwrap();
            let descriptor = original
                .parse_agent(snapshot.retained(Path::new("AGENT.md")).unwrap())
                .unwrap();
            let display = ArtifactDisplayMetadata::new(
                descriptor.name(),
                Some(descriptor.description()),
                &[],
                limits,
            )
            .unwrap();
            let grants = ToolGrants {
                binding: NativeBinding::KiroV3,
                read_only: true,
                allows_shell: false,
                tools: ["read"]
                    .into_iter()
                    .map(|name| NativeToolGrant {
                        name: name.into(),
                        access: ToolAccess::Read,
                    })
                    .collect(),
            };
            let rendered = original
                .render_agent(
                    NativeBinding::KiroV3,
                    &descriptor,
                    Some("live-agent"),
                    &grants,
                )
                .unwrap();
            let root_mode = snapshot.nodes().next().unwrap().mode();
            let mode = PayloadMode::from_permissions(
                &std::fs::metadata(source.directory().as_path().join("AGENT.md"))
                    .unwrap()
                    .permissions(),
            )
            .unwrap();
            let plan = ValidatedPayloadPlan::new(
                snapshot,
                [PayloadOverlay::new(
                    "AGENT.md".into(),
                    "rendered.md".into(),
                    rendered.bytes,
                    mode,
                )
                .unwrap()],
                &PayloadPolicy::new(
                    graphit_fs::NodeKind::Directory,
                    root_mode,
                    limits.materialization().snapshots,
                ),
                original.snapshot(),
            )
            .unwrap();
            let derived = DerivedArtifact::new(
                source.clone(),
                plan,
                NativeBinding::KiroV3.schema(),
                "live-agent",
                (
                    PayloadSelection::new("AGENT.md".into(), graphit_fs::NodeKind::File).unwrap(),
                    PayloadSelection::new("rendered.md".into(), graphit_fs::NodeKind::File)
                        .unwrap(),
                ),
                limits.materialization().lineage,
            )
            .unwrap();
            writer
                .publish_derived(
                    projection_request,
                    ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
                    derived,
                    Some(&display),
                )
                .unwrap();
        } else {
            let display = if kind == ArtifactType::Skill {
                let descriptor = original.parse_skill(body.as_bytes()).unwrap();
                Some(
                    ArtifactDisplayMetadata::new(
                        descriptor.name(),
                        Some(descriptor.description()),
                        &[],
                        registry.limits(),
                    )
                    .unwrap(),
                )
            } else {
                None
            };
            writer
                .publish(
                    projection_request,
                    ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
                    display.as_ref(),
                )
                .unwrap();
        }
        drop(writer);
        selections
            .push(serde_json::to_value(crate::live_agent_catalog::selection(&source)).unwrap());
    }
    selections
}

pub(super) fn configure(tasks: &TaskServer) {
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let installation = installation(tasks);
    let config = ConfigSnapshot::new(
        installation.brand(),
        [HubResourcePolicy::distribution_defaults(installation.brand()).unwrap()],
    )
    .unwrap();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        resources.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let native_install =
        crate::native_install_http::NativeInstallPolicy::capture(&config, original.snapshot())
            .unwrap();
    let hub = &native_install.hub;
    let policy = SourcePolicy {
        graph_query: graphit_ast::icebug_query::QueryOptions {
            buffer_pool_bytes: 128 * 1024 * 1024,
            query_threads: 2,
            max_rows: hub.content().max_collection_entries,
            max_bytes: hub.content().max_output_bytes,
            max_depth: hub.content().max_depth,
        },
        installation: installation.clone(),
        content: hub.content(),
        timeout: native_install.timeout,
        scope: resources.policy.scope,
        file: hub
            .snapshot_policy(CopyPolicy::RejectFileLinks, [])
            .unwrap(),
        read: SourceReadLimits::new(
            hub.content().max_input_bytes,
            hub.content().max_scalar_bytes,
            hub.copy().total_file_bytes_limit(),
        )
        .unwrap(),
        path: hub.retained(),
        index: resources.policy.read.native,
        binding: NativeBinding::KiroV3,
        registry: resources
            .policy
            .imported_knowledge
            .as_ref()
            .unwrap()
            .registry
            .clone(),
        native_install,
    };
    let hook = HookPolicy::capture(&policy, &config, resources.projects).unwrap();
    tasks
        .attach_project_configuration((installation, Arc::new(hook)))
        .unwrap();
}

pub(super) fn attach_peer(temp: &Path, tasks: &TaskServer) -> (Arc<AskAvailability>, PathBuf) {
    attach_peer_with_mode(temp, tasks, "live_http", None)
}

pub(super) fn attach_peer_with_mode(
    temp: &Path,
    tasks: &TaskServer,
    mode: &str,
    ast_context: Option<&Value>,
) -> (Arc<AskAvailability>, PathBuf) {
    let executable = std::env::current_exe().unwrap();
    let binary = executable
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("kiro-knowledge-fixture");
    assert!(
        binary.is_file(),
        "build the existing controlled ACP fixture target"
    );
    let bin = temp.join("peer-bin");
    std::fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink(binary, bin.join("kiro-cli")).unwrap();
    let journal = temp.join("live-peer.log");
    let mut environment = vec![
        ("PATH".into(), bin.as_os_str().to_owned()),
        ("KIRO_CAPTURED".into(), "literal-captured".into()),
        ("KIRO_FIXTURE_MODE".into(), mode.into()),
        ("KIRO_FIXTURE_MODEL".into(), "fixture-model".into()),
        (
            "KIRO_FIXTURE_JOURNAL".into(),
            journal.as_os_str().to_owned(),
        ),
    ];
    if let Some(context) = ast_context {
        environment.push((
            "KIRO_FIXTURE_AST_CONTEXT".into(),
            serde_json::to_string(context).unwrap().into(),
        ));
    }
    let available = Arc::new(
        AskAvailability::capture(
            graphit_ai::cli_provider::CliCompletionProvider::KiroV3,
            std::ffi::OsStr::new("kiro-cli"),
            environment,
            temp,
            graphit_core::native_document_file::NativeDocumentFileLimits::new(
                2 * 1024 * 1024,
                4096,
                128,
                8192,
            )
            .unwrap(),
        )
        .unwrap(),
    );
    tasks.attach_ask(Some(Arc::clone(&available))).unwrap();
    (available, journal)
}

async fn request(
    f: &Fixture,
    origin: &str,
    cookie: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> reqwest::Response {
    let mut call = f
        .client
        .request(method, format!("{origin}{path}"))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie);
    if let Some(body) = body {
        call = call.json(body);
    }
    call.send().await.unwrap()
}

fn log(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

async fn replay(f: &Fixture, origin: &str, cookie: &str, id: &str) -> Value {
    let response = request(
        f,
        origin,
        cookie,
        reqwest::Method::GET,
        &format!("/api/live/sessions/{id}"),
        None,
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "Live history: {body}");
    serde_json::from_str(&body).unwrap()
}

async fn wait_for(
    f: &Fixture,
    origin: &str,
    cookie: &str,
    id: &str,
    turn: &str,
    terminal: bool,
) -> Value {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let value = replay(f, origin, cookie, id).await;
            if value["events"].as_array().unwrap().iter().any(|event| {
                event["turn_id"] == turn
                    && event["data"]["type"] == if terminal { "turn_done" } else { "text" }
            }) {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("real native turn must reach the observed phase")
}

async fn wait_for_terminal(f: &Fixture, origin: &str, cookie: &str, id: &str, turn: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let value = replay(f, origin, cookie, id).await;
            if value["terminal"]["turn_id"] == turn {
                return value["terminal"].clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the retained native owner must report a known terminal")
}

#[tokio::test]
async fn web_live_create_prepare_run_retry_busy_load_cancel_and_logout() {
    let (temp, tasks, raw) = fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Hub),
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &[],
        Some(&[
            Permission::ReadUser,
            Permission::MutateUser,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ]),
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let limits = HistoryLimits::capture(
        graphit_core::native_document_file::NativeDocumentFileLimits::new(
            2 * 1024 * 1024,
            4096,
            128,
            8192,
        )
        .unwrap(),
        tasks.browser_policy().0,
    )
    .unwrap();
    let tasks = tasks
        .with_live_sessions(Arc::new(LiveSessionService::new(limits)))
        .unwrap();
    configure(&tasks);
    native::publish(
        &tasks,
        &raw,
        0,
        &[native::page("same", "Live productive Knowledge", &[])],
        None,
    )
    .await;
    let selections = installed_texts(&tasks);
    let (available, journal) = attach_peer(temp.path(), &tasks);
    let mut f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let tasks = f.tasks.as_ref().unwrap().0.clone();
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let (_, id, version) = &resources.projects[0];
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let router = f
        .service
        .task_router(
            tasks.clone(),
            Arc::new(task::backend(&format!(
                "{}/v1/project/access/authorize",
                f.issuer.origin
            ))),
            None,
        )
        .merge(
            f.service.live_execution_router(
                tasks.clone(),
                Arc::new(task::backend(&format!(
                    "{}/v1/project/access/authorize",
                    f.issuer.origin
                ))),
                address,
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("graphit"),
                None,
            ),
        );
    let stop = f.stop.clone();
    f.servers.push(tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    }));
    let create = json!({"request_id":"http-live", "project":{"project_id":id.to_string(),"project_version":version.version()},
        "binding":NativeBinding::KiroV3,"agent":selections[0],"artifacts":[selections[1],selections[2]],
        "title":"HTTP execution","prompt":PROMPT,"model":"fixture-model"});
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::POST,
        "/api/live/sessions",
        Some(&create),
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "actual create: {body}");
    let created: Value = serde_json::from_str(&body).unwrap();
    let session = created["id"].as_str().unwrap();
    wait_for(&f, &origin, &cookie, session, "http-live", false).await;
    assert_eq!(
        log(&journal)
            .lines()
            .filter(|line| *line == "live_assets_checked")
            .count(),
        1
    );
    assert_eq!(
        log(&journal)
            .lines()
            .filter(|line| *line == "live_mcp_four_tools_joined")
            .count(),
        1
    );
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::POST,
        "/api/live/sessions",
        Some(&create),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        log(&journal)
            .lines()
            .filter(|line| *line == "prompt")
            .count(),
        1
    );
    let busy = json!({"turn_id":"busy-turn","text":PROMPT});
    assert_eq!(
        request(
            &f,
            &origin,
            &cookie,
            reqwest::Method::POST,
            &format!("/api/live/sessions/{session}/messages"),
            Some(&busy)
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    std::fs::write(
        journal.with_extension("finish"),
        b"release first observed turn",
    )
    .unwrap();
    let completed = wait_for(&f, &origin, &cookie, session, "http-live", true).await;
    let done = completed["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["turn_id"] == "http-live" && event["data"]["type"] == "turn_done")
        .unwrap();
    assert_eq!(done["data"]["outcome"], "completed");
    assert_eq!(done["data"]["joined"], true);
    assert_eq!(done["data"]["clean"], true);
    let next = json!({"turn_id":"http-second","text":PROMPT});
    assert_eq!(
        request(
            &f,
            &origin,
            &cookie,
            reqwest::Method::POST,
            &format!("/api/live/sessions/{session}/messages"),
            Some(&next)
        )
        .await
        .status(),
        StatusCode::OK
    );
    wait_for(&f, &origin, &cookie, session, "http-second", true).await;
    assert_eq!(
        log(&journal).lines().filter(|line| *line == "new").count(),
        1
    );
    assert_eq!(
        log(&journal).lines().filter(|line| *line == "load").count(),
        1
    );
    assert_eq!(
        log(&journal)
            .lines()
            .filter(|line| *line == "prompt")
            .count(),
        2
    );
    std::fs::remove_file(journal.with_extension("finish")).unwrap();
    let next = json!({"turn_id":"http-cancel","text":PROMPT});
    assert_eq!(
        request(
            &f,
            &origin,
            &cookie,
            reqwest::Method::POST,
            &format!("/api/live/sessions/{session}/messages"),
            Some(&next)
        )
        .await
        .status(),
        StatusCode::OK
    );
    wait_for(&f, &origin, &cookie, session, "http-cancel", false).await;
    assert_eq!(
        request(
            &f,
            &origin,
            &cookie,
            reqwest::Method::POST,
            &format!("/api/live/sessions/{session}/cancel"),
            None
        )
        .await
        .status(),
        StatusCode::OK
    );
    let done = wait_for_terminal(&f, &origin, &cookie, session, "http-cancel").await;
    assert_eq!(done["outcome"], "cancelled");
    assert_eq!(done["joined"], true);
    assert_eq!(done["clean"], true);
    let next = json!({"turn_id":"http-logout","text":PROMPT});
    assert_eq!(
        request(
            &f,
            &origin,
            &cookie,
            reqwest::Method::POST,
            &format!("/api/live/sessions/{session}/messages"),
            Some(&next)
        )
        .await
        .status(),
        StatusCode::OK
    );
    wait_for(&f, &origin, &cookie, session, "http-logout", false).await;
    // The controlled peer now waits on its gate without sending another notification.
    tokio::time::timeout(Duration::from_secs(2), async {
        while log(&journal)
            .lines()
            .filter(|line| *line == "live_quiet_wait")
            .count()
            != 4
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the fourth prompt must be waiting quietly");
    let natural_terminals = log(&journal)
        .lines()
        .filter(|line| *line == "terminal")
        .count();
    let logout = request(
        &f,
        &f.origin,
        &cookie,
        reqwest::Method::POST,
        "/auth/logout",
        None,
    )
    .await;
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        request(
            &f,
            &origin,
            &cookie,
            reqwest::Method::GET,
            &format!("/api/live/sessions/{session}"),
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    // A new OAuth login has the same verified subject, while the old entry stays invalid.
    let fresh_cookie = f.login().await;
    let identity: Value = f.identity(&fresh_cookie).await.json().await.unwrap();
    assert_eq!(identity["subject"], "web-subject");
    let done = wait_for_terminal(&f, &origin, &fresh_cookie, session, "http-logout").await;
    assert_eq!(done["outcome"], "cancelled");
    assert_eq!(done["joined"], true);
    assert_eq!(done["clean"], true);
    assert_eq!(
        done["persisted"], false,
        "a cancelled Original cannot persist the terminal"
    );
    assert_eq!(
        log(&journal)
            .lines()
            .filter(|line| *line == "terminal")
            .count(),
        natural_terminals,
        "the quiet prompt must be cancelled instead of completing through its gate"
    );
    // A retry of the original prompt-free creation remains idempotent during a later turn.
    let mut create_ready = create.clone();
    create_ready["request_id"] = json!("http-ready-only");
    create_ready.as_object_mut().unwrap().remove("prompt");
    let ready_response = request(
        &f,
        &origin,
        &fresh_cookie,
        reqwest::Method::POST,
        "/api/live/sessions",
        Some(&create_ready),
    )
    .await;
    let ready_status = ready_response.status();
    let ready_body = ready_response.text().await.unwrap();
    assert_eq!(
        ready_status,
        StatusCode::OK,
        "prompt-free creation: {ready_body}"
    );
    let ready: Value = serde_json::from_str(&ready_body).unwrap();
    assert_eq!(ready["state"], "ready");
    let ready_id = ready["id"].as_str().unwrap();
    assert!(!journal.with_extension("finish").exists());
    let send_ready = json!({"turn_id":"http-ready-send","text":PROMPT});
    assert_eq!(
        request(
            &f,
            &origin,
            &fresh_cookie,
            reqwest::Method::POST,
            &format!("/api/live/sessions/{ready_id}/messages"),
            Some(&send_ready),
        )
        .await
        .status(),
        StatusCode::OK
    );
    wait_for(
        &f,
        &origin,
        &fresh_cookie,
        ready_id,
        "http-ready-send",
        false,
    )
    .await;
    let prompts_before_retry = log(&journal)
        .lines()
        .filter(|line| *line == "prompt")
        .count();
    let retry = request(
        &f,
        &origin,
        &fresh_cookie,
        reqwest::Method::POST,
        "/api/live/sessions",
        Some(&create_ready),
    )
    .await;
    let retry_status = retry.status();
    let retry_body = retry.text().await.unwrap();
    assert_eq!(
        retry_status,
        StatusCode::OK,
        "busy prompt-free retry: {retry_body}"
    );
    let retried: Value = serde_json::from_str(&retry_body).unwrap();
    assert_eq!(retried["id"], ready["id"]);
    assert_eq!(
        log(&journal)
            .lines()
            .filter(|line| *line == "prompt")
            .count(),
        prompts_before_retry
    );
    assert_eq!(
        request(
            &f,
            &origin,
            &fresh_cookie,
            reqwest::Method::POST,
            &format!("/api/live/sessions/{ready_id}/cancel"),
            None,
        )
        .await
        .status(),
        StatusCode::OK
    );
    let ready_terminal =
        wait_for_terminal(&f, &origin, &fresh_cookie, ready_id, "http-ready-send").await;
    assert_eq!(ready_terminal["outcome"], "cancelled");
    assert_eq!(ready_terminal["joined"], true);
    assert_eq!(ready_terminal["clean"], true);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    tasks.shutdown().await.unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn web_live_project_hub_denial_precedes_missing_source_and_native_launch() {
    let (temp, tasks, raw) = fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Hub),
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &[],
        Some(&[
            Permission::ReadUser,
            Permission::MutateUser,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ]),
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let limits = HistoryLimits::capture(
        graphit_core::native_document_file::NativeDocumentFileLimits::new(
            2 * 1024 * 1024,
            4096,
            128,
            8192,
        )
        .unwrap(),
        tasks.browser_policy().0,
    )
    .unwrap();
    let tasks = tasks
        .with_live_sessions(Arc::new(LiveSessionService::new(limits)))
        .unwrap();
    configure(&tasks);
    native::publish(
        &tasks,
        &raw,
        0,
        &[native::page("same", "Live productive Knowledge", &[])],
        None,
    )
    .await;
    let selections = installed_texts(&tasks);
    let (available, journal) = attach_peer(temp.path(), &tasks);
    let mut f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let tasks = f.tasks.as_ref().unwrap().0.clone();
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let (_, id, _) = &resources.projects[0];
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let router = f
        .service
        .task_router(
            tasks.clone(),
            Arc::new(task::backend(&format!(
                "{}/v1/project/access/authorize",
                f.issuer.origin
            ))),
            None,
        )
        .merge(
            f.service.live_execution_router(
                tasks.clone(),
                Arc::new(task::backend(&format!(
                    "{}/v1/project/access/authorize",
                    f.issuer.origin
                ))),
                address,
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("graphit"),
                None,
            ),
        );
    let stop = f.stop.clone();
    f.servers.push(tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    }));

    let registry = &resources
        .policy
        .imported_knowledge
        .as_ref()
        .unwrap()
        .registry;
    let version = ConcreteVersion::new("1.0.0", registry.limits().projection()).unwrap();
    let document = registry.load().unwrap();
    let agent = document
        .get_version(ArtifactType::Agent, "fixture/live-agent", &version)
        .unwrap();
    let source = agent.installations[0].original_source.directory().as_path();
    std::fs::remove_file(source.join("AGENT.md")).unwrap();
    f.issuer.denied_modules.lock().unwrap().push("hub".into());
    let create = json!({"request_id":"denied-source", "project":{"project_id":id.to_string(),"project_version":resources.projects[0].2.version()},
        "binding":NativeBinding::KiroV3,"agent":selections[0],"artifacts":[selections[1],selections[2]],
        "title":"Denied source","prompt":PROMPT,"model":"fixture-model"});
    let before = f.issuer.project_denials.lock().unwrap().len();
    let denied = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::POST,
        "/api/live/sessions",
        Some(&create),
    )
    .await;
    let status = denied.status();
    let body = denied.text().await.unwrap();
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "PROJECT denial must precede missing source: {body}"
    );
    assert!(
        f.issuer.project_denials.lock().unwrap()[before..]
            .iter()
            .any(|request| request["module"] == "hub"
                && request["action"] == "read"
                && request["project_id"] == id.to_string()),
        "denial must come from the selected PROJECT Hub read"
    );
    assert!(
        log(&journal).is_empty(),
        "denied source must not launch a native child"
    );
    let observed = replay(&f, &origin, &cookie, "denied-source").await;
    assert_eq!(observed["session"]["state"], "failed");
    assert!(
        observed["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["data"]["type"] == "error"
                && event["data"]["code"] == "permission_denied")
    );
    assert!(
        !observed["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["data"]["type"] == "prompt")
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    tasks.shutdown().await.unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn installed_reader_keeps_display_updates_and_rejects_changed_source() {
    let (_temp, tasks, raw) = fixture_with_reference_permissions(
        &[Permission::ManageAuthProfiles],
        &[],
        Some(&[Permission::ReadUser]),
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let selections = installed_texts(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    f.login().await;
    let entry = f
        .service
        .0
        .sessions
        .lock()
        .await
        .values()
        .next()
        .cloned()
        .unwrap();
    let tasks = &f.tasks.as_ref().unwrap().0;
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        resources.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let bearer = zeroize::Zeroizing::new(entry.credentials.lock().await.access_token().to_owned());
    let flow = f
        .service
        .0
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("OAuth flow"));
    let (provider, verifier) = &f.service.0.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &bearer,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .unwrap();
    let read = resources
        .auth
        .issuer()
        .admit_web_installed_hub_read(
            &caller,
            verifier,
            &original,
            provider,
            resources.auth.installation(),
        )
        .unwrap();
    let registry = &resources
        .policy
        .imported_knowledge
        .as_ref()
        .unwrap()
        .registry;
    let paths = installation(tasks);
    let config = ConfigSnapshot::new(
        paths.brand(),
        [HubResourcePolicy::distribution_defaults(paths.brand()).unwrap()],
    )
    .unwrap();
    let policy = HubResourcePolicy::resolve(&config).unwrap();
    let selected: crate::knowledge_live::ArtifactSelection =
        serde_json::from_value(selections[1].clone()).unwrap();
    let captured = crate::live_agent_preparation::capture_text(
        &read,
        registry,
        &policy,
        &selected,
        NativeBinding::KiroV3,
        None,
    )
    .unwrap();
    let before = captured
        .markdown(resources.policy.content.max_input_bytes)
        .unwrap();
    let version = ConcreteVersion::new("1.0.0", registry.limits().projection()).unwrap();
    let mut installed = registry
        .load()
        .unwrap()
        .get_version(ArtifactType::Skill, "fixture/live-skill", &version)
        .unwrap()
        .clone();
    installed.installations[0].display = Some(
        ArtifactDisplayMetadata::new(
            "Skill é",
            Some("**Updated description**"),
            &["selected".into()],
            registry.limits(),
        )
        .unwrap(),
    );
    let coordinator = publication_coordinator(tasks, &original);
    let installation = &installed.installations[0];
    coordinator
        .writer(&original)
        .unwrap()
        .publish(
            ProjectionRequest {
                binding: NativeBinding::KiroV3,
                source: &installation.original_source,
                alias: installation.alias.as_deref(),
                entry_name: installation.entry_name.as_deref(),
            },
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
            installation.display.as_ref(),
        )
        .unwrap();
    let updated = registry.load().unwrap();
    assert_eq!(
        updated
            .get_version(ArtifactType::Skill, "fixture/live-skill", &version)
            .unwrap()
            .installations[0]
            .display,
        installed.installations[0].display,
        "the physical publication must actually update its display metadata",
    );
    captured
        .recheck(&read, registry, NativeBinding::KiroV3, None)
        .unwrap();
    assert_eq!(
        captured
            .markdown(resources.policy.content.max_input_bytes)
            .unwrap(),
        before
    );
    std::fs::write(
        installed.installations[0]
            .original_source
            .directory()
            .as_path()
            .join("SKILL.md"),
        "changed physical source with a different length",
    )
    .unwrap();
    assert!(
        captured
            .recheck(&read, registry, NativeBinding::KiroV3, None)
            .is_err()
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    tasks.shutdown().await.unwrap();
    f.service.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn web_live_ast_catalog_prepare_ready_send_uses_actual_main_mcp_eight_tools() {
    let (issuer, first) = Fixture::start_issuer().await;
    let (_storage, foreground, tasks, sources, raw, selected) =
        crate::foreground_source::foreground_memory_jobs_tests::live_ast_search_fixture(
            &issuer.origin,
        )
        .await;
    let limits = HistoryLimits::capture(
        graphit_core::native_document_file::NativeDocumentFileLimits::new(
            2 * 1024 * 1024,
            4096,
            128,
            8192,
        )
        .unwrap(),
        tasks.browser_policy().0,
    )
    .unwrap();
    let tasks = tasks
        .with_live_sessions(Arc::new(LiveSessionService::new(limits)))
        .unwrap();
    native::publish(
        &tasks,
        &raw,
        0,
        &[native::page("same", "Live productive Knowledge", &[])],
        None,
    )
    .await;
    let selections = installed_texts(&tasks);
    let peer_root = installation(&tasks).root().to_path_buf();
    let mut f = Fixture::configured_with_issuer(
        policy(),
        Some((tasks.clone(), raw.clone())),
        None,
        Some((sources.clone(), raw)),
        issuer,
        first,
    )
    .await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let backend = Arc::new(task::backend(&format!(
        "{}/v1/project/access/authorize",
        f.issuer.origin
    )));
    let router = f
        .service
        .task_router(tasks.clone(), Arc::clone(&backend), Some(sources.clone()))
        .merge(
            f.service.live_execution_router(
                tasks.clone(),
                backend,
                address,
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("graphit"),
                Some(sources.clone()),
            ),
        );
    let stop = f.stop.clone();
    f.servers.push(tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    }));
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::GET,
        "/api/live/artifacts",
        None,
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "actual AST catalogue: {body}");
    let catalogue: Value = serde_json::from_str(&body).unwrap();
    let ast = catalogue["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["ref"]["source"] == "project"
                && entry["ref"]["type"] == "ast"
                && entry["ref"]["project_id"] == selected.1.to_string()
                && entry["ref"]["version"] == selected.2.version()
        })
        .expect("the composed Source/Graph must expose its real project AST tuple");
    assert!(ast.get("unavailable").is_none(), "AST readiness: {ast}");
    let ast_context = ast["ref"].clone();
    let (available, journal) =
        attach_peer_with_mode(&peer_root, &tasks, "live_http", Some(&ast_context));
    let create = json!({
        "request_id":"http-live-ast-ready",
        "project":{"project_id":selected.1.to_string(),"project_version":selected.2.version()},
        "binding":NativeBinding::KiroV3,"agent":selections[0],
        "artifacts":[selections[1],selections[2],ast_context],
        "title":"Actual Live AST","model":"fixture-model"
    });
    // The selected AST requires its own current PROJECT grant even with Knowledge admitted.
    f.issuer.denied_modules.lock().unwrap().push("ast".into());
    let mut denied_create = create.clone();
    denied_create["request_id"] = json!("http-live-ast-denied");
    let before = f.issuer.project_denials.lock().unwrap().len();
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::POST,
        "/api/live/sessions",
        Some(&denied_create),
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "own AST admission: {body}");
    assert!(
        f.issuer.project_denials.lock().unwrap()[before..]
            .iter()
            .any(|request| request["module"] == "ast"
                && request["action"] == "read"
                && request["project_id"] == selected.1.to_string())
    );
    assert!(
        log(&journal).is_empty(),
        "denied AST must not launch the native peer"
    );
    f.issuer.denied_modules.lock().unwrap().clear();
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::POST,
        "/api/live/sessions",
        Some(&create),
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "AST prepare: {body}");
    let ready: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(ready["state"], "ready");
    assert!(!log(&journal).lines().any(|line| line == "prompt"));
    let session = ready["id"].as_str().unwrap();
    // An AST selection persisted beside Knowledge must not enter the Knowledge viewer.
    let generation = ready["cursor"]["generation"].as_str().unwrap();
    let mut page_url = url::Url::parse(&format!(
        "{origin}/api/live/sessions/{session}/knowledge/page"
    ))
    .unwrap();
    page_url
        .query_pairs_mut()
        .append_pair("generation", generation)
        .append_pair("page", "same");
    let response = f
        .client
        .get(page_url.clone())
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "unambiguous Knowledge page beside AST: {body}"
    );
    let page: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(page["title"], "Live productive Knowledge");
    assert_eq!(page["content"], "# Complete native body é\n");
    assert_eq!(page["context"]["type"], "knowledge");
    assert!(page.get("candidates").is_none());
    page_url
        .query_pairs_mut()
        .append_pair("context", &ast_context.to_string());
    let response = f
        .client
        .get(page_url)
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "AST is not a Knowledge page context: {body}"
    );
    let rejected: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(rejected["error"]["code"], "selected_context_changed");

    // Release the existing controlled peer gate; only this explicit Send starts the turn.
    std::fs::write(
        journal.with_extension("finish"),
        b"complete actual AST reads",
    )
    .unwrap();
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::POST,
        &format!("/api/live/sessions/{session}/messages"),
        Some(&json!({"turn_id":"http-live-ast-send","text":PROMPT})),
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "AST Send: {body}");
    let completed = wait_for(&f, &origin, &cookie, session, "http-live-ast-send", true).await;
    let done = completed["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| {
            event["turn_id"] == "http-live-ast-send" && event["data"]["type"] == "turn_done"
        })
        .unwrap();
    assert_eq!(done["data"]["outcome"], "completed");
    assert_eq!(done["data"]["joined"], true);
    assert_eq!(done["data"]["clean"], true);
    let observed = log(&journal);
    assert_eq!(
        observed
            .lines()
            .filter(|line| *line == "live_mcp_eight_tools_joined")
            .count(),
        1
    );
    assert_eq!(observed.lines().filter(|line| *line == "prompt").count(), 1);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    tasks.shutdown().await.unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    f.finish_graphs().await;
    sources.shutdown().await.unwrap();
    drop(sources);
    foreground.shutdown().await.unwrap();
}
