//! Real Web OAuth, USER journal, PROJECT Broker and registered imported Knowledge.
mod ast;
mod cpu;
mod expiry;
use super::*;
use crate::{
    browser_auth::live_mcp::{LiveMcpBroker, Registration, RunContext},
    knowledge_kiro_process::{KiroProcess, KiroProcessOwner},
    knowledge_live::{
        Access, ArtifactSelection, CreateRequest, EventData, HistoryLimits, LiveSessionService,
        ProjectSelection, SendRequest, SessionState,
    },
    live_agent_tools::{self, SelectedContext, stdio::LiveMcpProxy},
    reference_explorer::Resources,
    task_server::{browser::records_fixture as native, tests::fixture_with_reference_permissions},
};
use graphit_auth::{Permission, ScopeCapture};
use graphit_core::{
    artifact_store::ArtifactStore, file_lock::Cancellation, module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
};
use graphit_hub::{NativeBinding, policy::RequestContext};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ProtocolVersion},
    service::{ClientLifecycleMode, ClientServiceExt, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::{ffi::OsString, num::NonZeroUsize};

fn artifact(kind: &str, id: &str) -> ArtifactSelection {
    ArtifactSelection {
        source: "hub".into(),
        project_id: None,
        instance_id: None,
        project_kind: None,
        artifact_type: kind.into(),
        id: id.into(),
        version: "1.0.0".into(),
    }
}
fn launch_installation(
    resources: &Resources<'_>,
) -> graphit_core::project_paths::InstallationPaths {
    let installation = resources.auth.installation().live;
    graphit_core::project_paths::InstallationPaths::resolve(
        installation.brand().clone(),
        installation.owner_root(),
        None,
        Some(installation.owner_root()),
    )
    .unwrap()
}
fn history() -> LiveSessionService {
    LiveSessionService::new(HistoryLimits {
        native: NativeDocumentFileLimits::new(2 * 1024 * 1024, 4096, 128, 8192).unwrap(),
        envelope_bytes: NonZeroUsize::new(1024 * 1024).unwrap(),
        id_bytes: NonZeroUsize::new(4096).unwrap(),
        prompt_bytes: NonZeroUsize::new(65536).unwrap(),
        event_bytes: NonZeroUsize::new(65536).unwrap(),
    })
}
fn process(original: &RequestContext<'_>) -> KiroProcess {
    KiroProcess {
        program: "/bin/false".into(),
        args: vec![],
        cwd: "/".into(),
        environment: vec![("BASE".into(), "preserved".into())],
        original: Arc::clone(original.snapshot()),
        service_stop: Cancellation::default(),
        limits: graphit_process::Limits {
            stdin_bytes: 65536,
            stdout_bytes: 65536,
            stderr_bytes: 65536,
            termination_grace: Duration::from_secs(1),
            cleanup: Duration::from_secs(2),
        },
        frame_bytes: 65536,
        permit: None,
        owner: KiroProcessOwner::new(None),
    }
}
fn env(process: &KiroProcess, key: &str) -> String {
    process
        .environment
        .iter()
        .find(|(name, _)| name == key)
        .unwrap()
        .1
        .to_str()
        .unwrap()
        .to_owned()
}
async fn client(uri: &str, bearer: &str, web: &str) -> RunningService<rmcp::RoleClient, ()> {
    let mut authorization = header::HeaderValue::from_str(&format!("Bearer {bearer}")).unwrap();
    authorization.set_sensitive(true);
    let mut session = header::HeaderValue::from_str(web).unwrap();
    session.set_sensitive(true);
    let config = StreamableHttpClientTransportConfig::with_uri(uri.to_owned()).custom_headers(
        HashMap::from([
            (header::AUTHORIZATION, authorization),
            (
                header::HeaderName::from_static("x-graphit-live-web-session"),
                session,
            ),
        ]),
    );
    let transport = StreamableHttpClientTransport::with_client(
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
        config,
    );
    ().serve_with_lifecycle(
        transport,
        ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::LATEST],
        },
    )
    .await
    .unwrap()
}
fn call(name: &'static str, input: Value) -> CallToolRequestParams {
    let mut arguments = input.as_object().unwrap().clone();
    arguments.insert("ai_optimized".into(), serde_json::Value::Bool(false));
    CallToolRequestParams::new(name).with_arguments(arguments)
}
fn value(result: CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "readonly call failed");
    result
        .structured_content
        .expect("structured Knowledge response")
}
fn run_context<'a>(
    resources: &'a Resources<'a>,
    backend: &'a graphit_auth::broker::project_access::BrokerProjectAccess,
    original: &'a RequestContext<'a>,
    access: &'a Access<'a>,
    live: &'a LiveSessionService,
    run: (&'a crate::knowledge_live::Session, &'a SendRequest),
    selected: &'a [SelectedContext<'a>],
) -> RunContext<'a> {
    RunContext {
        resources,
        backend,
        original,
        access,
        live,
        session: run.0,
        turn: run.1,
        selected,
        source: None,
        ast: &[],
        model: None,
        allowed: &always,
    }
}
fn always() -> bool {
    true
}

const MCP_AGENT: &[u8] = b"---\nname: documentary-agent\ndescription: Documentary fixture\nread_only: true\nallows_shell: false\n---\nUse selected sources.\n";
fn render_mcp_agent(original: &mut RequestContext<'_>) -> graphit_hub::native::RenderedContent {
    let descriptor = original.parse_agent(MCP_AGENT).unwrap();
    original
        .render_agent(
            NativeBinding::KiroV3,
            &descriptor,
            None,
            &graphit_hub::native::ToolGrants {
                binding: NativeBinding::KiroV3,
                read_only: true,
                allows_shell: false,
                tools: vec![],
            },
        )
        .unwrap()
}

/// Admit the fixture through the real PROJECT/imported read boundary.
async fn observe_mcp_knowledge(
    resources: &Resources<'_>,
    backend: &graphit_auth::broker::project_access::BrokerProjectAccess,
    identity: crate::reference_explorer::Identity<'_>,
    original: &RequestContext<'_>,
    selection: &mut ArtifactSelection,
) -> bool {
    let (caller, verifier, provider) = identity;
    let issuer = resources.auth.issuer();

    if selection.source == "project" {
        use graphit_auth::{
            ProjectCapture, ProjectReadModule, ProjectRecordAuthority,
            broker::project_access::{BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead},
        };
        let selected = resources
            .projects
            .iter()
            .find(|(_, id, version)| {
                id.to_string() == selection.id && version.version() == selection.version
            })
            .unwrap();
        resources
            .require_active(selected)
            .unwrap_or_else(|_| panic!("active fixture PROJECT"));
        let (paths, id, version) = selected;
        let scope = graphit_core::artifact_store::CapturedPublicationScope::project(
            paths,
            *id,
            resources.policy.scope,
            original.snapshot(),
        )
        .unwrap();
        let binding = scope.binding().unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let read = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(caller),
                verifier,
                backend,
                original,
                scope: BrokerProjectScope {
                    installation: resources.auth.installation(),
                    project: ProjectCapture {
                        scope: ScopeCapture {
                            live: &scope,
                            record: &binding,
                        },
                        version,
                    },
                    providers: &[],
                },
                provider,
                module: ProjectReadModule::Knowledge,
            })
            .await
            .unwrap();
        graphit_data::knowledge_embeddings::capture_native_knowledge_context(
            original.snapshot(),
            &read,
            resources.policy.read.native,
        )
        .await
        .unwrap()
        .is_some()
    } else {
        use graphit_knowledge::imported_knowledge::{
            ImportedKnowledgeInput, ImportedKnowledgeOpen, ImportedKnowledgeReadLimits,
            open_imported_knowledge,
        };
        let read = issuer
            .admit_web_imported_knowledge_read(
                caller,
                verifier,
                original,
                provider,
                resources.auth.installation(),
            )
            .unwrap();
        let policy = resources.policy.imported_knowledge.as_ref().unwrap();
        let id =
            graphit_hub::ArtifactId::new(&selection.id, resources.policy.role_reference).unwrap();
        let version =
            graphit_hub::ConcreteVersion::new(&selection.version, resources.policy.role_reference)
                .unwrap();
        let opened = open_imported_knowledge(
            &read,
            ImportedKnowledgeInput {
                registry: &policy.registry,
                id: &id,
                version: &version,
                context_alias: None,
            },
            ImportedKnowledgeReadLimits {
                directory: policy.directory,
                index: crate::foreground_knowledge::limits(resources.policy).unwrap(),
            },
        )
        .await
        .unwrap();
        let ImportedKnowledgeOpen::Published(reader) = opened else {
            panic!("published imported fixture");
        };
        reader.recheck().unwrap();
        selection.project_id = Some(reader.observation().producer.to_string());
        true
    }
}

fn prepare_mcp_contexts(
    live: &LiveSessionService,
    access: &Access<'_>,
    session: &crate::knowledge_live::Session,
    original: &RequestContext<'_>,
    rendered: &graphit_hub::native::RenderedContent,
    selections: &[crate::knowledge_live::PreparedSelection<'_>],
) -> crate::knowledge_live::ReadContextsSnapshot {
    use crate::knowledge_live::{CapturedArtifact, Preparation};
    use graphit_core::artifact_store::{
        CapturedSourceReader, CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet,
        SnapshotPolicy, SourceReadLimits,
    };
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("AGENT.md");
    std::fs::write(&source, MCP_AGENT).unwrap();
    let retained = RetainedBytesLimits::new(1024 * 1024, 1024, 1024 * 1024).unwrap();
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(1024 * 1024, 1024, 1024 * 1024, 64).unwrap(),
        RetainedFileSet::new(Vec::<std::path::PathBuf>::new(), retained).unwrap(),
        retained,
    )
    .unwrap();
    let reader = CapturedSourceReader::capture(
        &source,
        graphit_fs::NodeKind::File,
        &policy,
        SourceReadLimits::new(65536, 65536, 1024 * 1024).unwrap(),
        Arc::clone(original.snapshot()),
    )
    .unwrap();
    live.prepare(
        access,
        &session.id,
        Preparation {
            agent: CapturedArtifact {
                selection: &session.agent,
                reader: &reader,
            },
            agent_source_entry: std::path::Path::new(""),
            rendered,
            artifacts: selections,
            allowed: &|| access.read().is_ok(),
        },
    )
    .unwrap();
    live.read_contexts(access, &session.id, &session.cursor.generation)
        .unwrap()
}

/// Prepare selected identities through the genuine Core/renderer path.
#[expect(
    clippy::too_many_arguments,
    reason = "Keeps USER access, PROJECT admission, Original and rendered preparation explicit in this test helper"
)]
async fn prepare_mcp_history(
    live: &LiveSessionService,
    access: &Access<'_>,
    session: &crate::knowledge_live::Session,
    resources: &Resources<'_>,
    backend: &graphit_auth::broker::project_access::BrokerProjectAccess,
    identity: crate::reference_explorer::Identity<'_>,
    original: &RequestContext<'_>,
    rendered: &graphit_hub::native::RenderedContent,
) {
    use crate::knowledge_live::PreparedSelection;
    for selection in &session.artifacts {
        let mut observed = selection.clone();
        assert!(observe_mcp_knowledge(resources, backend, identity, original, &mut observed).await);
        assert_eq!(
            &observed, selection,
            "fixture must use the producer full tuple"
        );
    }
    let selections = session
        .artifacts
        .iter()
        .map(|selection| PreparedSelection::ReadContext {
            selection,
            original: original.snapshot(),
        })
        .collect::<Vec<_>>();
    let stored = prepare_mcp_contexts(live, access, session, original, rendered, &selections);
    assert_eq!(
        stored
            .contexts
            .selected
            .iter()
            .map(|value| &value.selection)
            .collect::<Vec<_>>(),
        session.artifacts.iter().collect::<Vec<_>>()
    );
}

#[derive(Clone, Copy)]
enum KnowledgeScenario {
    SelectedSources { retire_selected: bool },
    NoCorpusPublished,
}

#[tokio::test]
async fn live_mcp_reads_new_publication_without_repreparing_the_turn() {
    tokio::spawn(selected_sources_and_identity_changes(
        KnowledgeScenario::NoCorpusPublished,
    ))
    .await
    .unwrap();
}

#[tokio::test]
async fn live_mcp_real_web_reads_selected_sources_and_denies_identity_and_scope_changes() {
    for retire_selected in [false, true] {
        tokio::spawn(async move {
            selected_sources_and_identity_changes(KnowledgeScenario::SelectedSources {
                retire_selected,
            })
            .await;
        })
        .await
        .unwrap();
    }
}

async fn selected_sources_and_identity_changes(scenario: KnowledgeScenario) {
    let no_corpus = matches!(scenario, KnowledgeScenario::NoCorpusPublished);
    let retire_selected = matches!(
        scenario,
        KnowledgeScenario::SelectedSources {
            retire_selected: true
        }
    );
    let published_index = usize::from(no_corpus);
    let (temp, tasks, raw) = fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &["second"],
        Some(&[
            Permission::ReadUser,
            Permission::MutateUser,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ]),
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let second = native::add_current_project(&tasks, &temp.path().join("second"));
    let imported = native::publish(
        &tasks,
        &raw,
        published_index,
        &[native::page("same", "Imported text", &[])],
        None,
    )
    .await;
    let source = native::register_copy(
        &tasks,
        &imported,
        "vendor/live-knowledge",
        "1.0.0",
        published_index,
    )
    .await;
    if !no_corpus {
        native::update_preserving(
            &tasks,
            &raw,
            0,
            &[native::page("same", "Project text", &[])],
        )
        .await;
    }
    if no_corpus {
        Box::pin(native::update_preserving(
            &tasks,
            &raw,
            1,
            &[native::page("same", "Selected project text", &[])],
        ))
        .await;
    } else {
        native::publish(
            &tasks,
            &raw,
            1,
            &[native::page("same", "Selected project text", &[])],
            None,
        )
        .await;
    }
    let mut fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let (web, entry) = fixture
        .service
        .0
        .sessions
        .lock()
        .await
        .iter()
        .next()
        .map(|(key, value)| (key.clone(), Arc::clone(value)))
        .unwrap();
    let tasks = &fixture.tasks.as_ref().unwrap().0;
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live MCP metadata fixture"));
    let resources = tasks.record_resources(&snapshot);
    let cancellation = Cancellation::default();
    let mut original = RequestContext::new(
        resources.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap();
    let rendered = render_mcp_agent(&mut original);
    let bearer = zeroize::Zeroizing::new(entry.credentials.lock().await.access_token().to_owned());
    let flow = fixture
        .service
        .0
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("Live MCP Web flow fixture"));
    let (provider, verifier) = &fixture.service.0.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &bearer,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .unwrap();
    let identity = (&caller, verifier.as_ref(), provider.as_str());
    let issuer = resources.auth.issuer();
    let scope = crate::foreground_user_memory::capture(
        issuer,
        resources.auth.installation(),
        &caller,
        verifier,
        &original,
        resources.policy,
    )
    .unwrap();
    let binding = scope.binding().unwrap();
    let capture = || ScopeCapture {
        live: &scope,
        record: &binding,
    };
    let read = issuer
        .admit_web_user_knowledge_read(&caller, verifier, &original, provider, capture())
        .unwrap();
    let write = issuer
        .admit_web_user_knowledge_write(&caller, verifier, &original, provider, capture())
        .unwrap();
    let access = Access::user(&read, Some(&write)).unwrap();
    drop(
        ArtifactStore::new(scope.clone(), resources.policy.write.artifacts)
            .writer_with_context(original.snapshot())
            .unwrap(),
    );
    let backend = task::backend(&format!(
        "{}/v1/project/access/authorize",
        fixture.issuer.origin
    ));
    let live = history();
    let mut selected = artifact("knowledge", "vendor/live-knowledge");
    observe_mcp_knowledge(&resources, &backend, identity, &original, &mut selected).await;
    let selected_project = ArtifactSelection {
        source: "project".into(),
        project_id: Some(second["project_id"].as_str().unwrap().into()),
        instance_id: None,
        project_kind: None,
        artifact_type: "knowledge".into(),
        id: second["project_id"].as_str().unwrap().into(),
        version: second["project_version"].as_str().unwrap().into(),
    };
    let project_source = crate::reference_explorer::SourceKey::ProjectKnowledge;
    let created = live
        .create(
            &access,
            CreateRequest {
                request_id: "live-mcp-real".into(),
                project: ProjectSelection {
                    project_id: resources.projects[0].1.to_string(),
                    project_version: resources.projects[0].2.version().into(),
                },
                binding: NativeBinding::KiroV3,
                agent: artifact("agent", "fixture/agent"),
                artifacts: vec![selected.clone(), selected_project.clone()],
                title: None,
                prompt: None,
                model: "fixture-model".into(),
            },
        )
        .unwrap();
    let turn = SendRequest {
        turn_id: "turn one".into(),
        text: "read actual Knowledge".into(),
    };
    prepare_mcp_history(
        &live, &access, &created, &resources, &backend, identity, &original, &rendered,
    )
    .await;
    live.emit_fixture(
        &access,
        &created.id,
        Some(&turn.turn_id),
        EventData::State {
            state: SessionState::Running,
        },
    )
    .unwrap();
    live.emit_fixture(
        &access,
        &created.id,
        Some(&turn.turn_id),
        EventData::Prompt {
            text: turn.text.clone(),
        },
    )
    .unwrap();
    let stored = live
        .read_contexts(&access, &created.id, &created.cursor.generation)
        .unwrap();
    let session = stored.session;
    let read_contexts = stored.contexts;
    let selected_contexts = [
        SelectedContext {
            selection: &selected,
            source: &source,
        },
        SelectedContext {
            selection: &selected_project,
            source: &project_source,
        },
    ];
    let broker = LiveMcpBroker::new();
    let mut route = broker
        .begin_run(Registration {
            sessions: &fixture.service,
            web_session: &web,
            entry: Arc::clone(&entry),
            session: &session,
            read_contexts: &read_contexts,
            turn: &turn,
            identity,
            original: &original,
        })
        .await
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = broker.router(resources.policy.content, fixture.stop.clone());
    fixture.servers.push(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let installation = launch_installation(&resources);
    let launch = route
        .launch(
            identity,
            &original,
            address,
            &installation,
            &installation.root().join("native-bin/graphit"),
        )
        .unwrap();
    let mut transport = process(&original);
    let other_cancel = Cancellation::default();
    let other = RequestContext::new(
        resources.policy.content,
        &other_cancel,
        original.deadline().unwrap(),
    )
    .unwrap();
    assert_eq!(
        launch
            .apply_to(other.snapshot(), &mut transport)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        transport.environment,
        vec![(OsString::from("BASE"), OsString::from("preserved"))]
    );
    launch
        .apply_to(original.snapshot(), &mut transport)
        .unwrap();
    // A different owner for the same key cannot publish or remove the live route.
    let duplicate = broker
        .begin_run(Registration {
            sessions: &fixture.service,
            web_session: &web,
            entry: Arc::clone(&entry),
            session: &session,
            read_contexts: &read_contexts,
            turn: &turn,
            identity,
            original: &original,
        })
        .await
        .unwrap();
    let duplicate_launch = duplicate
        .launch(
            identity,
            &original,
            address,
            &installation,
            &installation.root().join("native-bin/graphit"),
        )
        .unwrap();
    let mut duplicate_process = process(&original);
    assert_eq!(
        duplicate_launch
            .apply_to(original.snapshot(), &mut duplicate_process)
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        duplicate_process.environment,
        vec![(OsString::from("BASE"), OsString::from("preserved"))]
    );
    drop(duplicate_launch);
    drop(duplicate);
    assert_eq!(transport.environment.len(), 6);
    assert_eq!(env(&transport, "BASE"), "preserved");
    assert_eq!(
        env(
            &transport,
            &installation
                .brand()
                .environment_variable("GLOBAL_DIR")
                .unwrap()
        ),
        installation.root().to_str().unwrap()
    );
    let path = env(&transport, "PATH");
    assert_eq!(
        std::env::split_paths(&path).next().unwrap(),
        installation.root().join("native-bin")
    );
    let uri = env(&transport, "GRAPHIT_LIVE_MCP_URL");
    let mut wrong = session.clone();
    wrong.id.push_str("-other");
    assert_eq!(
        route
            .serve(run_context(
                &resources,
                &backend,
                &original,
                &access,
                &live,
                (&wrong, &turn),
                &selected_contexts
            ))
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let mut wrong_turn = turn.clone();
    wrong_turn.turn_id.push_str("-other");
    assert_eq!(
        route
            .serve(run_context(
                &resources,
                &backend,
                &original,
                &access,
                &live,
                (&session, &wrong_turn),
                &selected_contexts
            ))
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    if !no_corpus {
        // Expected contexts are part of this registration, never adopted from a later read.
        let mut forged_contexts = read_contexts.clone();
        forged_contexts.selected[0]
            .selection
            .version
            .push_str("-forged");
        assert_ne!(forged_contexts, read_contexts);
        let mut forged = broker
            .begin_run(Registration {
                sessions: &fixture.service,
                web_session: &web,
                entry: Arc::clone(&entry),
                session: &session,
                read_contexts: &forged_contexts,
                turn: &turn,
                identity,
                original: &original,
            })
            .await
            .unwrap();
        assert_eq!(
            forged
                .serve(run_context(
                    &resources,
                    &backend,
                    &original,
                    &access,
                    &live,
                    (&session, &turn),
                    &selected_contexts,
                ))
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        drop(forged);
    }
    let actions = async {
        let connection = client(&uri, &bearer, &web).await;
        if no_corpus {
            Box::pin(async {
                let absent = value(
                    connection
                        .peer()
                        .call_tool(call(live_agent_tools::HEADS, json!({})))
                        .await
                        .unwrap(),
                );
                assert_eq!(absent["native"]["state"], "no_corpus");
                Box::pin(native::publish(
                    tasks,
                    &fixture.tasks.as_ref().unwrap().1,
                    0,
                    &[native::page(
                        "same",
                        "Published after the prepared absence",
                        &[],
                    )],
                    None,
                ))
                .await;
                let published = value(
                    connection
                        .peer()
                        .call_tool(call(live_agent_tools::HEADS, json!({})))
                        .await
                        .unwrap(),
                );
                assert_eq!(
                    published["native"]["heads"][0]["title"],
                    "Published after the prepared absence"
                );
                let again = value(
                    connection
                        .peer()
                        .call_tool(call(live_agent_tools::HEADS, json!({})))
                        .await
                        .unwrap(),
                );
                assert_eq!(
                    again["native"]["heads"][0]["title"],
                    "Published after the prepared absence"
                );
                assert_eq!(
                    live.read_contexts(&access, &session.id, &session.cursor.generation)
                        .unwrap()
                        .contexts,
                    read_contexts
                );
                original.check().unwrap();
                access.read().unwrap();
            })
            .await;
            connection.cancel().await.unwrap();
            return;
        }
        let mut stale_url = url::Url::parse(&uri).unwrap();
        {
            let mut path = stale_url.path_segments_mut().unwrap();
            path.pop();
            path.pop();
            path.push("old-turn");
            path.push("mcp");
        }
        let wrong_web = format!("{web}-other");
        for (endpoint, session_ref) in [
            (stale_url.as_str(), web.as_str()),
            (uri.as_str(), wrong_web.as_str()),
        ] {
            let denied = client(endpoint, &bearer, session_ref).await;
            assert_eq!(
                denied
                    .peer()
                    .call_tool(call(live_agent_tools::HEADS, json!({})))
                    .await
                    .unwrap()
                    .is_error,
                Some(true)
            );
            assert!(denied.peer().list_all_tools().await.is_err());
            denied.cancel().await.unwrap();
        }
        let tools = connection.peer().list_all_tools().await.unwrap();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>(),
            vec![
                live_agent_tools::CONTEXT,
                live_agent_tools::HEADS,
                live_agent_tools::PAGE,
                live_agent_tools::SEARCH
            ]
        );
        let viewer = value(
            connection
                .peer()
                .call_tool(call(live_agent_tools::CONTEXT, json!({})))
                .await
                .unwrap(),
        );
        for tool in &tools {
            assert_eq!(
                tool.input_schema["properties"]["ai_optimized"]["type"],
                "boolean"
            );
            assert!(tool.output_schema.is_none());
        }
        let expected = graphit_core::structured_output::encode(
            &viewer,
            graphit_core::structured_output::OutputFormat::Toon,
            graphit_core::structured_output::Limits {
                max_depth: 64,
                max_nodes: 100_000,
                max_input_bytes: 1_048_576,
                max_output_bytes: 1_048_576,
            },
        )
        .unwrap();
        for arguments in [json!({}), json!({"ai_optimized":true})] {
            let optimized = connection
                .peer()
                .call_tool(
                    CallToolRequestParams::new(live_agent_tools::CONTEXT)
                        .with_arguments(arguments.as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_eq!(optimized.is_error, Some(false));
            assert!(optimized.structured_content.is_none());
            assert_eq!(optimized.content.len(), 1);
            assert_eq!(
                optimized.content[0].as_text().unwrap().text,
                expected.as_str()
            );
        }
        assert!(
            connection
                .peer()
                .call_tool(
                    CallToolRequestParams::new(live_agent_tools::CONTEXT)
                        .with_arguments(json!({"ai_optimized":null}).as_object().unwrap().clone())
                )
                .await
                .is_err()
        );
        assert_eq!(viewer["contexts"].as_array().unwrap().len(), 2);
        assert_eq!(
            viewer["contexts"][0]["context"],
            serde_json::to_value(&selected).unwrap()
        );
        assert_eq!(
            viewer["contexts"][1]["context"],
            serde_json::to_value(&selected_project).unwrap()
        );
        let project = value(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({})))
                .await
                .unwrap(),
        );
        assert_eq!(project["native"]["heads"][0]["title"], "Project text");
        let imported = value(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({"context":selected})))
                .await
                .unwrap(),
        );
        assert_eq!(imported["native"]["heads"][0]["title"], "Imported text");
        assert_eq!(imported["source"], serde_json::to_value(&source).unwrap());
        let explicit_project = value(
            connection
                .peer()
                .call_tool(call(
                    live_agent_tools::HEADS,
                    json!({"context": selected_project}),
                ))
                .await
                .unwrap(),
        );
        assert_eq!(
            explicit_project["native"]["heads"][0]["title"],
            "Selected project text"
        );
        assert_eq!(
            explicit_project["selection"]["project_id"],
            selected_project.id
        );
        for context in [None, Some(selected.clone()), Some(selected_project.clone())] {
            let mut input = json!({"slug":"same"});
            if let Some(context) = &context {
                input["context"] = serde_json::to_value(context).unwrap();
            }
            let page = value(
                connection
                    .peer()
                    .call_tool(call(live_agent_tools::PAGE, input))
                    .await
                    .unwrap(),
            );
            assert!(page.get("native").is_some());
            let mut input = json!({"search":{"query":"text","mode":"fts"}});
            if context.as_ref() == Some(&selected_project) {
                assert_eq!(page["selection"]["project_id"], selected_project.id);
            }
            if let Some(context) = &context {
                input["context"] = serde_json::to_value(context).unwrap();
            }
            let search = value(
                connection
                    .peer()
                    .call_tool(call(live_agent_tools::SEARCH, input))
                    .await
                    .unwrap(),
            );
            assert!(search.get("native").is_some());
            if context.as_ref() == Some(&selected_project) {
                assert_eq!(search["selection"]["project_id"], selected_project.id);
            }
        }
        for (tool, input) in [
            (live_agent_tools::HEADS, json!({"unknown":true})),
            (live_agent_tools::HEADS, json!({"context":null})),
            (
                live_agent_tools::SEARCH,
                json!({"search":{"query":"text","mode":"invalid"}}),
            ),
        ] {
            let invalid = connection
                .peer()
                .call_tool(call(tool, input))
                .await
                .unwrap();
            assert_eq!(invalid.is_error, Some(true));
            assert_eq!(
                invalid.structured_content.as_ref().unwrap()["error"]["code"],
                "invalid_arguments"
            );
        }
        let mut unselected = selected.clone();
        unselected.instance_id = Some("different-instance".into());
        let denied = connection
            .peer()
            .call_tool(call(live_agent_tools::HEADS, json!({"context":unselected})))
            .await
            .unwrap();
        assert_eq!(denied.is_error, Some(true));
        for mismatch in ["version", "project_id"] {
            let mut invalid = serde_json::to_value(&selected_project).unwrap();
            invalid[mismatch] = json!("not-selected");
            assert_eq!(
                connection
                    .peer()
                    .call_tool(call(live_agent_tools::HEADS, json!({"context":invalid})))
                    .await
                    .unwrap()
                    .is_error,
                Some(true)
            );
        }
        // The real stdio server forwards through HTTP with the process-only launch values.
        let proxy = LiveMcpProxy::configured(
            zeroize::Zeroizing::new(uri.clone()),
            zeroize::Zeroizing::new(env(&transport, "GRAPHIT_LIVE_MCP_BEARER")),
            zeroize::Zeroizing::new(env(&transport, "GRAPHIT_LIVE_WEB_SESSION")),
            resources.policy.content,
            Duration::from_secs(10),
        )
        .unwrap();
        let (native_side, proxy_side) = tokio::io::duplex(65536);
        let (proxy_read, proxy_write) = tokio::io::split(proxy_side);
        let proxy_owner = tokio::spawn(proxy.serve_stream(proxy_read, proxy_write));
        let (native_read, native_write) = tokio::io::split(native_side);
        let native_client = ().serve((native_read, native_write)).await.unwrap();
        let proxied_tools = native_client.peer().list_all_tools().await.unwrap();
        assert_eq!(proxied_tools.len(), 4);
        for tool in &proxied_tools {
            assert_eq!(
                tool.input_schema["properties"]["ai_optimized"]["default"],
                true
            );
        }
        let proxied = value(
            native_client
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({})))
                .await
                .unwrap(),
        );
        let optimized = native_client
            .peer()
            .call_tool(
                CallToolRequestParams::new(live_agent_tools::HEADS)
                    .with_arguments(serde_json::Map::new()),
            )
            .await
            .unwrap();
        assert!(optimized.structured_content.is_none());
        let expected = graphit_core::structured_output::encode(
            &proxied,
            graphit_core::structured_output::OutputFormat::Toon,
            graphit_core::structured_output::Limits {
                max_depth: 64,
                max_nodes: 100_000,
                max_input_bytes: 1_048_576,
                max_output_bytes: 1_048_576,
            },
        )
        .unwrap();
        assert_eq!(
            optimized.content[0].as_text().unwrap().text,
            expected.as_str()
        );
        assert!(
            native_client
                .peer()
                .call_tool(call("graphit_task_create", json!({})))
                .await
                .is_err()
        );
        let calls = fixture.issuer.project_calls.load(Ordering::SeqCst);
        let original_claims: Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(bearer.split('.').nth(1).unwrap())
                .unwrap(),
        )
        .unwrap();
        let mut wrong_user = original_claims.clone();
        wrong_user["sub"] = json!("other-user");
        let mut expired = original_claims;
        expired["exp"] = json!(1);
        for token in [
            "invalid-jwt".to_owned(),
            fixture.issuer.jwt(wrong_user),
            fixture.issuer.jwt(expired),
        ] {
            let denied = client(&uri, &token, &web).await;
            assert_eq!(
                denied
                    .peer()
                    .call_tool(call(live_agent_tools::HEADS, json!({})))
                    .await
                    .unwrap()
                    .is_error,
                Some(true)
            );
            assert!(denied.peer().list_all_tools().await.is_err());
            denied.cancel().await.unwrap();
        }
        assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), calls);
        fixture.issuer.project_grant.store(false, Ordering::Release);
        assert_eq!(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({})))
                .await
                .unwrap()
                .is_error,
            Some(true)
        );
        assert_eq!(connection.peer().list_all_tools().await.unwrap().len(), 4);
        assert_eq!(
            connection
                .peer()
                .call_tool(call(
                    live_agent_tools::HEADS,
                    json!({"context":selected_project})
                ))
                .await
                .unwrap()
                .is_error,
            Some(true)
        );
        // Imported read remains independent from a PROJECT grant.
        value(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({"context":selected})))
                .await
                .unwrap(),
        );
        fixture.issuer.project_grant.store(true, Ordering::Release);
        assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
        // EOF of the real stdio client must join its HTTP owner and free the borrowed job.
        fixture.issuer.hold_project.store(true, Ordering::Release);
        let request = native_client
            .peer()
            .send_cancellable_request(
                rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                    call(live_agent_tools::HEADS, json!({})),
                )),
                rmcp::service::PeerRequestOptions::no_options(),
            )
            .await
            .unwrap();
        fixture.issuer.project_started.notified().await;
        native_client.cancel().await.unwrap();
        drop(request);
        let mut proxy_owner = proxy_owner;
        match tokio::time::timeout(Duration::from_secs(5), &mut proxy_owner).await {
            Ok(joined) => joined.unwrap().unwrap(),
            Err(_) => {
                fixture.issuer.hold_project.store(false, Ordering::Release);
                fixture.issuer.release_project.notify_waiters();
                proxy_owner.await.unwrap().unwrap();
                panic!("stdio EOF failed to join its pending HTTP owner before Broker release");
            }
        }
        fixture.issuer.hold_project.store(false, Ordering::Release);
        assert!(!cancellation.is_cancelled());
        let resumed = tokio::time::timeout(
            Duration::from_secs(5),
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({}))),
        )
        .await;
        fixture.issuer.release_project.notify_waiters();
        value(
            resumed
                .expect("disconnected reader must release the run queue before held Broker returns")
                .unwrap(),
        );
        {
            // Revoke PROJECT membership during the fresh async Broker admission.
            fixture.issuer.hold_project.store(true, Ordering::Release);
            let (tool, input) = if retire_selected {
                (live_agent_tools::HEADS, json!({"context":selected_project}))
            } else {
                (live_agent_tools::CONTEXT, json!({}))
            };
            let response = connection.peer().call_tool(call(tool, input));
            tokio::pin!(response);
            tokio::select! {()=fixture.issuer.project_started.notified()=>{},result=&mut response=>panic!("membership gate did not hold: {}",result.is_ok())};
            native::retire_checkout(tasks, usize::from(retire_selected));
            fixture.issuer.hold_project.store(false, Ordering::Release);
            fixture.issuer.release_project.notify_waiters();
            assert_eq!(response.await.unwrap().is_error, Some(true));
        }
        let remaining = if retire_selected {
            json!({})
        } else {
            json!({"context":selected_project})
        };
        let remaining = value(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, remaining))
                .await
                .unwrap(),
        );
        assert_eq!(
            remaining["native"]["heads"][0]["title"],
            if retire_selected {
                "Project text"
            } else {
                "Selected project text"
            }
        );
        // Imported context remains healthy after unrelated PROJECT membership retirement.
        value(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({"context":selected})))
                .await
                .unwrap(),
        );
        let raw = &fixture.tasks.as_ref().unwrap().1;
        if !retire_selected {
            // Advance the selected PROJECT after preparation while this call awaits its
            // real Broker admission. The newly published title is available to the fresh read.
            fixture.issuer.hold_project.store(true, Ordering::Release);
            let response = connection.peer().call_tool(call(
                live_agent_tools::PAGE,
                json!({"context":selected_project,"slug":"same"}),
            ));
            tokio::pin!(response);
            tokio::select! {
                () = fixture.issuer.project_started.notified() => {},
                result = &mut response => panic!("Broker admission did not hold: {}",result.is_ok()),
            }
            let current = tasks
                .record_snapshot()
                .unwrap_or_else(|_| panic!("current selected PROJECT fixture metadata"));
            let current_resources = tasks.record_resources(&current);
            let selected_index = current_resources
                .projects
                .iter()
                .position(|(_, id, version)| {
                    id.to_string() == selected_project.id
                        && version.version() == selected_project.version
                })
                .expect("selected PROJECT remains active after retiring the default");
            native::update_preserving(
                tasks,
                raw,
                selected_index,
                &[native::page("same", "New publication is available", &[])],
            )
            .await;
            fixture.issuer.hold_project.store(false, Ordering::Release);
            fixture.issuer.release_project.notify_waiters();
            let changed = value(response.await.unwrap());
            assert_eq!(
                changed["native"]["page"]["document"]["title"],
                "New publication is available"
            );
            original.check().unwrap();
            access.read().unwrap();
        }
        let old_revision = caller.provider_revision();
        crate::task_server::tests::browser_provider(tasks, raw, &fixture.issuer.origin, true);
        assert_eq!(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({"context":selected})))
                .await
                .unwrap()
                .is_error,
            Some(true)
        );
        let new_revision =
            crate::task_server::tests::browser_provider(tasks, raw, &fixture.issuer.origin, false);
        assert_ne!(new_revision, old_revision);
        // Reinstalling a provider cannot restore this run's old admission.
        assert_eq!(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({"context":selected})))
                .await
                .unwrap()
                .is_error,
            Some(true)
        );
        // Actual logout invalidates every tool/list before another source read.
        let logged_out = fixture
            .client
            .post(format!("{}/auth/logout", fixture.origin))
            .header(header::ORIGIN, &fixture.origin)
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            connection
                .peer()
                .call_tool(call(live_agent_tools::HEADS, json!({"context":selected})))
                .await
                .unwrap()
                .is_error,
            Some(true)
        );
        assert!(connection.peer().list_all_tools().await.is_err());
        connection.cancel().await.unwrap();
    };
    tokio::pin!(actions);
    {
        let mut serving = Box::pin(route.serve(run_context(
            &resources,
            &backend,
            &original,
            &access,
            &live,
            (&session, &turn),
            &selected_contexts,
        )));
        tokio::select! {()=&mut actions=>{},result=&mut serving=>panic!("run bridge ended before calls: {}",result.is_ok())};
    }
    drop(route);
    let stale = client(&uri, &bearer, &web).await;
    assert_eq!(
        stale
            .peer()
            .call_tool(call(live_agent_tools::HEADS, json!({})))
            .await
            .unwrap()
            .is_error,
        Some(true)
    );
    stale.cancel().await.unwrap();
    fixture.finish_tasks().await;
}
