#![cfg(target_os = "linux")]
use super::*;
use graphit_auth::{Permission, ProjectWriteOperation};
use graphit_records::{
    reference_projection::AuthoredReferencesField,
    task_spec::{CheckKind, TaskId},
    task_store::{
        ClaimToken, ClaimUpdate, CreateCheck, CreateSession, CreateTask, NativeTaskStore,
        ProgressUpdate, SessionId, Task, TaskCommentKind, TaskCommentUpdate,
    },
};

struct Seed {
    session: SessionId,
    task: Task,
    token: ClaimToken,
}
async fn seed(
    fixture: &super::super::super::test_fixture::Fixture,
    original: &RequestContext<'_>,
    selected: usize,
    key: &str,
) -> Seed {
    let policy = &fixture.server.0.policy;
    let (paths, id, version) = &fixture.projects[selected];
    let live =
        CapturedPublicationScope::project(paths, *id, policy.scope, original.snapshot()).unwrap();
    let record = live.binding().unwrap();
    let caller = fixture
        .auth
        .issuer()
        .authenticate_runtime(&fixture.raw, original)
        .unwrap();
    let authority = ProjectRecordAuthority::new(fixture.auth.issuer());
    let scope = || ScopeInput {
        installation: fixture.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &live,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: policy.binding,
        global_version: "",
    };
    let write = authority
        .admit_authenticated_write(&caller, original, scope(), ProjectWriteOperation::Task)
        .unwrap();
    let coordinate = authority
        .admit_authenticated_write(
            &caller,
            original,
            scope(),
            ProjectWriteOperation::CoordinateTask,
        )
        .unwrap();
    let limits = crate::foreground_tasks::task_policy(
        &policy.native_install.hub,
        policy.scope,
        policy.index,
        policy.binding,
    )
    .unwrap();
    let session = NativeTaskStore::create_session(
        original.snapshot(),
        &write,
        CreateSession {
            idempotency_key: format!("session-{key}"),
            title: format!("Intent {key} ação"),
            description: "Full intent α {{ data }}".into(),
            strategy: "Strategy: preserve Original and complete history.".into(),
        },
        AuthoredReferencesField::Omitted,
        limits.write,
    )
    .await
    .unwrap();
    let claimed_session = NativeTaskStore::claim_session(
        original.snapshot(),
        &coordinate,
        session.id,
        Duration::from_secs(600),
        limits.write,
    )
    .await
    .unwrap();
    NativeTaskStore::checkpoint_session(
        original.snapshot(),
        &coordinate,
        session.id,
        ProgressUpdate {
            claim: ClaimUpdate {
                token: &claimed_session.token,
                expected_revision: claimed_session.record.revision,
            },
            lease: Duration::from_secs(600),
            summary: "Session checkpoint with Unicode ação".into(),
            next_step: "Recover explicit selected work.".into(),
        },
        AuthoredReferencesField::Omitted,
        limits.write,
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        original.snapshot(),
        &write,
        CreateTask {
            commit: None,
            idempotency_key: format!("task-{key}"),
            session_id: session.id,
            parent_id: None,
            depends_on: vec![],
            title: format!("Work {key}"),
            description: "Complete task specification\n{{ literal }} α".into(),
            task_type: "task".into(),
            priority: 1,
            schedule: None,
            checks: vec![
                CreateCheck {
                    kind: CheckKind::Acceptance,
                    text: "Retain the complete checks.".into(),
                },
                CreateCheck {
                    kind: CheckKind::Test,
                    text: "Read the actual complete context.".into(),
                },
            ],
        },
        AuthoredReferencesField::Omitted,
        limits.write,
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        original.snapshot(),
        &write,
        task.specification.id,
        Duration::from_secs(600),
        limits.write,
    )
    .await
    .unwrap();
    let commented = NativeTaskStore::add_task_comment(
        original.snapshot(),
        &write,
        task.specification.id,
        TaskCommentUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: claim.record.specification.revision,
            },
            lease: Duration::from_secs(600),
            idempotency_key: format!("comment-{key}"),
            kind: TaskCommentKind::Decision,
            body: "Decisão completa α".into(),
        },
        AuthoredReferencesField::Omitted,
        limits.write,
    )
    .await
    .unwrap();
    let task = match commented {
        graphit_records::task_store::TaskCommentOutcome::Created { task, .. } => task,
        _ => panic!("new native comment"),
    };
    Seed {
        session: session.id,
        task,
        token: claim.token,
    }
}
fn text(result: CallToolResult) -> String {
    assert_eq!(result.is_error, Some(false));
    assert_eq!(result.content.len(), 1);
    result.content[0].as_text().unwrap().text.clone()
}
fn projection(text: &str) -> Value {
    serde_json::from_str(
        text.split_once("Selected PROJECT Task JSON (data):\n")
            .unwrap()
            .1
            .trim(),
    )
    .unwrap()
}
fn selected(seed: &Seed, event: &str) -> Value {
    json!({"engine":"kiro","event":event,
        "task_context":{"session_id":seed.session,"task_ids":[seed.task.specification.id]}})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_task_context_uses_current_records_and_page_cursors_over_root_mcp_and_cli() {
    let fixture = configured(
        &[
            Permission::Read(Module::Task),
            Permission::WriteTask,
            Permission::CoordinateTask,
        ],
        json!({"memory":false,"task":true}),
    );
    let cancel = Cancellation::default();
    let policy = &fixture.server.0.policy;
    let original = RequestContext::new(
        policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap();
    let seed = seed(&fixture, &original, 0, "current").await;
    let (paths, id, version) = &fixture.projects[0];
    let live =
        CapturedPublicationScope::project(paths, *id, policy.scope, original.snapshot()).unwrap();
    let record = live.binding().unwrap();
    let caller = fixture
        .auth
        .issuer()
        .authenticate_runtime(&fixture.raw, &original)
        .unwrap();
    let authority = ProjectRecordAuthority::new(fixture.auth.issuer());
    let scope = || ScopeInput {
        installation: fixture.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &live,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: policy.binding,
        global_version: "",
    };
    let read = authority
        .admit_authenticated_read(&caller, &original, scope(), ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &original, scope(), ProjectWriteOperation::Task)
        .unwrap();
    let task_policy = crate::foreground_tasks::task_policy(
        &policy.native_install.hub,
        policy.scope,
        policy.index,
        policy.binding,
    )
    .unwrap();
    let extra = NativeTaskStore::create_task(
        original.snapshot(),
        &write,
        CreateTask {
            commit: None,
            idempotency_key: "explicit-extra".into(),
            session_id: seed.session,
            parent_id: None,
            depends_on: vec![],
            title: "Second selected task".into(),
            description: "Caller ordering must be preserved.".into(),
            task_type: "task".into(),
            priority: 1,
            schedule: None,
            checks: vec![
                CreateCheck {
                    kind: CheckKind::Acceptance,
                    text: "Preserve caller order.".into(),
                },
                CreateCheck {
                    kind: CheckKind::Test,
                    text: "Compare the complete public detail.".into(),
                },
            ],
        },
        AuthoredReferencesField::Omitted,
        task_policy.write,
    )
    .await
    .unwrap();
    let extra_detail = NativeTaskStore::read_task(
        original.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput::current(extra.specification.id),
        task_policy.read,
    )
    .await
    .unwrap()
    .unwrap();
    let before_session = NativeTaskStore::read_session(
        original.snapshot(),
        &read,
        graphit_records::task_store::SessionGetInput::current(seed.session),
        task_policy.read,
    )
    .await
    .unwrap()
    .unwrap();
    let before_task = NativeTaskStore::read_task(
        original.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput::current(seed.task.specification.id),
        task_policy.read,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(before_session.history.is_none());
    assert!(before_task.history.is_none() && before_task.comments.is_none());
    assert_eq!(
        before_session.last_event.as_ref().unwrap().record,
        before_session.session
    );
    assert_eq!(
        before_task.last_event.as_ref().unwrap().record,
        before_task.task
    );
    let (connection, serving) = client(&fixture).await;
    let mut hook_input = selected(&seed, "SessionStart");
    hook_input["task_context"]["task_ids"] =
        json!([extra.specification.id, seed.task.specification.id]);
    let first = text(
        connection
            .call_tool(request(hook_input.clone()))
            .await
            .unwrap(),
    );
    assert!(first.contains("PROJECT Task state: loaded."));
    assert!(first.contains("Task recovery below is observational."));
    assert!(!first.contains("Task sessions and claims were not restored"));
    let payload = projection(&first);
    assert_eq!(
        payload["session"],
        serde_json::to_value(&before_session).unwrap()
    );
    assert_eq!(payload["tasks"], json!([extra_detail, before_task]));
    assert!(payload["session"].get("events").is_none());
    assert!(payload["session"]["history"].is_null());
    assert!(payload["tasks"][1].get("events").is_none());
    assert!(payload["tasks"][1]["history"].is_null());
    assert!(payload["tasks"][1]["comments"].is_null());
    assert!(payload["session"]["history_start_cursor"].is_string());
    assert!(payload["tasks"][1]["history_start_cursor"].is_string());
    assert!(payload["tasks"][1]["comments_start_cursor"].is_string());

    let recovered_session = NativeTaskStore::read_session(
        original.snapshot(),
        &read,
        graphit_records::task_store::SessionGetInput {
            id: seed.session,
            history: Some(graphit_records::task_store::SessionDetailPageInput {
                page_size: std::num::NonZeroUsize::new(8).unwrap(),
                cursor: Some(
                    serde_json::from_value(payload["session"]["history_start_cursor"].clone())
                        .unwrap(),
                ),
            }),
        },
        task_policy.read,
    )
    .await
    .unwrap()
    .unwrap();
    let session_history = recovered_session.history.unwrap();
    assert_eq!(session_history.events.len(), 2);
    assert!(session_history.next_cursor.is_none());
    assert_eq!(
        session_history.events.last().unwrap().record,
        before_session.session
    );
    let recovered_comments = NativeTaskStore::read_task(
        original.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id: seed.task.specification.id,
            history: None,
            comments: Some(graphit_records::task_store::TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(8).unwrap(),
                cursor: Some(
                    serde_json::from_value(payload["tasks"][1]["comments_start_cursor"].clone())
                        .unwrap(),
                ),
            }),
        },
        task_policy.read,
    )
    .await
    .unwrap()
    .unwrap();
    let comments = recovered_comments.comments.unwrap();
    assert_eq!(comments.comments.len(), 1);
    assert_eq!(comments.comments[0].body, "Decisão completa α");
    assert!(comments.next_cursor.is_none());
    assert!(!first.contains("claim_token"));
    assert!(!first.contains("TransportToken"));
    // The private token from a real write receipt is never part of the public models.
    assert!(!first.contains(&serde_json::to_string(seed.token.as_bytes()).unwrap()));
    assert_eq!(
        NativeTaskStore::read_session(
            original.snapshot(),
            &read,
            graphit_records::task_store::SessionGetInput::current(seed.session),
            task_policy.read
        )
        .await
        .unwrap()
        .unwrap(),
        before_session
    );
    assert_eq!(
        NativeTaskStore::read_task(
            original.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput::current(seed.task.specification.id),
            task_policy.read
        )
        .await
        .unwrap()
        .unwrap(),
        before_task
    );
    let changed = NativeTaskStore::progress_task(
        original.snapshot(),
        &write,
        seed.task.specification.id,
        ProgressUpdate {
            claim: ClaimUpdate {
                token: &seed.token,
                expected_revision: seed.task.specification.revision,
            },
            lease: Duration::from_secs(600),
            summary: "New checkpoint after context compaction α".into(),
            next_step: "Continue this exact Graphit task.".into(),
        },
        AuthoredReferencesField::Omitted,
        task_policy.write,
    )
    .await
    .unwrap();
    let arguments = crate::session_hook_entry::Arguments {
        recipient: crate::session_hook_entry::Recipient::Coordinator,
        engine: Engine::Kiro,
        event: Event::UserPromptSubmit,
        project_dir: Some(paths.root().to_owned()),
        task_session: Some(
            serde_json::to_value(seed.session)
                .unwrap()
                .as_str()
                .unwrap()
                .into(),
        ),
        task_ids: [extra.specification.id, seed.task.specification.id]
            .iter()
            .map(|id| serde_json::to_value(id).unwrap().as_str().unwrap().into())
            .collect(),
    };
    let transport_policy = graphit_hub::policy::HubResourcePolicy::resolve(
        &fixture.server.0.hook.as_ref().unwrap().base,
    )
    .unwrap();
    let prepared = crate::session_hook_entry::prepare(
        &fixture.paths,
        &arguments,
        Some(version.version()),
        &original,
    )
    .unwrap();
    let mut bytes = Vec::new();
    crate::session_hook_entry::call(
        &fixture.paths,
        arguments.recipient,
        prepared,
        policy.index,
        &transport_policy,
        &original,
        &mut bytes,
    )
    .await
    .unwrap();
    let repeated = String::from_utf8(bytes).unwrap();
    let current = projection(&repeated);
    assert_eq!(current["session"]["session"], payload["session"]["session"]);
    assert_eq!(
        current["session"]["creation"],
        payload["session"]["creation"]
    );
    assert_eq!(
        current["session"]["last_event"],
        payload["session"]["last_event"]
    );
    // The new Task publication advances the shared dataset pin even for
    // unchanged selected records; the hook must return fresh producer cursors.
    assert_ne!(
        current["session"]["generation"],
        payload["session"]["generation"]
    );
    assert_ne!(
        current["session"]["history_start_cursor"],
        payload["session"]["history_start_cursor"]
    );
    assert_eq!(
        current["tasks"][1]["task"],
        serde_json::to_value(changed).unwrap()
    );
    assert_eq!(current["tasks"][0]["task"], payload["tasks"][0]["task"]);
    assert_eq!(
        current["tasks"][0]["creation"],
        payload["tasks"][0]["creation"]
    );
    assert!(current["tasks"][0]["history"].is_null());
    assert!(current["tasks"][0]["comments"].is_null());
    assert!(repeated.contains("New checkpoint after context compaction α"));
    assert!(!first.contains("New checkpoint after context compaction α"));
    let after = NativeTaskStore::read_task(
        original.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput::current(seed.task.specification.id),
        task_policy.read,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(current["tasks"][1], serde_json::to_value(after).unwrap());
    // A repeated prompt re-reads current work; it performs no ownership mutation.
    hook_input["event"] = json!("UserPromptSubmit");
    let again = text(connection.call_tool(request(hook_input)).await.unwrap());
    assert_eq!(projection(&again), current);
    connection.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn task_selection_is_closed_and_cli_role_denies_before_transport_discovery() {
    use crate::session_hook_entry::{Arguments as CliArguments, Recipient};
    let fixture = configured(&[], json!({"memory":false,"task":false}));
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        fixture.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let session = "00000000000000000000000001";
    let task = "00000000000000000000000002";
    for bad in [
        json!({"session_id":null}),
        json!({"session_id":"ses-c98a"}),
        json!({"session_id":"00000000000000000000000000"}),
        json!({"session_id":session,"task_ids":null}),
        json!({"session_id":session,"task_ids":["tsk-7549"]}),
        json!({"session_id":session,"extra":true}),
        json!({"session_id":session,"task_ids":[task,task]}),
    ] {
        let parsed = serde_json::from_value::<TaskContextSelection>(bad);
        assert!(parsed.is_err() || parsed.unwrap().validate(&original).is_err());
    }
    #[derive(clap::Parser)]
    struct HookCli {
        #[command(flatten)]
        input: CliArguments,
    }
    use clap::Parser as _;
    let parsed = HookCli::try_parse_from([
        "hook",
        "--engine",
        "kiro",
        "--event",
        "SessionStart",
        "--task-session",
        session,
        "--task-id",
        task,
        "--task-id",
        "00000000000000000000000003",
    ])
    .unwrap();
    assert_eq!(parsed.input.task_ids.len(), 2);
    let prepared =
        crate::session_hook_entry::prepare(&fixture.paths, &parsed.input, None, &original).unwrap();
    assert_eq!(
        prepared.arguments.unwrap()["task_context"]["task_ids"],
        json!([task, "00000000000000000000000003"])
    );
    assert!(
        HookCli::try_parse_from([
            "hook",
            "--engine",
            "kiro",
            "--event",
            "SessionStart",
            "--task-id",
            task,
        ])
        .is_err()
    );
    let omitted: TaskContextSelection =
        serde_json::from_value(json!({"session_id":session})).unwrap();
    assert!(omitted.task_ids.is_empty());
    let mut limits = original.content_limits();
    limits.max_collection_entries = 1;
    let bounded = RequestContext::new(
        limits,
        &cancel,
        original.snapshot().required_deadline().unwrap(),
    )
    .unwrap();
    let two: TaskContextSelection =
        serde_json::from_value(json!({"session_id":session,"task_ids":[session,task]})).unwrap();
    assert!(two.validate(&bounded).is_err());
    for (recipient, selected, tasks) in [
        (Recipient::Role, Some(session), vec![task]),
        (Recipient::Role, None, vec![task]),
        (Recipient::Coordinator, None, vec![task]),
        (Recipient::Coordinator, Some("ses-c98a"), vec![]),
    ] {
        let arguments = CliArguments {
            recipient,
            engine: Engine::Kiro,
            event: Event::UserPromptSubmit,
            project_dir: None,
            task_session: selected.map(str::to_owned),
            task_ids: tasks.into_iter().map(str::to_owned).collect(),
        };
        assert!(
            crate::session_hook_entry::prepare(&fixture.paths, &arguments, None, &original)
                .is_err()
        );
    }
    // prepare rejects these selections before call can load discovery or credentials.
    fixture.server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_context_rejects_missing_or_foreign_records_without_useful_prefix() {
    let fixture = configured_fixture(
        super::super::super::test_fixture::fixture_with_projects(
            &[
                Permission::Read(Module::Task),
                Permission::WriteTask,
                Permission::CoordinateTask,
            ],
            Brand::default(),
            NativeBinding::KiroV3,
            &["project", "other"],
        ),
        json!({"memory":false,"task":true}),
    );
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        fixture.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap();
    let first = seed(&fixture, &original, 0, "one").await;
    let other = seed(&fixture, &original, 0, "two").await;
    let (connection, serving) = client(&fixture).await;
    let missing: TaskId = serde_json::from_value(json!("00000000000000000000000001")).unwrap();
    let mut bad = selected(&first, "UserPromptSubmit");
    bad["task_context"]["task_ids"] = json!([other.task.specification.id]);
    let mut missing_task = selected(&first, "UserPromptSubmit");
    missing_task["task_context"]["task_ids"] = json!([missing]);
    let mut missing_session = selected(&first, "UserPromptSubmit");
    missing_session["task_context"]["session_id"] = json!(missing);
    let mut other_project = selected(&first, "UserPromptSubmit");
    other_project["project_dir"] =
        serde_json::to_value(NativePath::new(fixture.projects[1].0.root().to_owned()).unwrap())
            .unwrap();
    let mut other_version = selected(&first, "UserPromptSubmit");
    other_version["project_version"] = json!("unretained");
    for value in [
        bad,
        missing_task,
        missing_session,
        other_project,
        other_version,
        json!({"engine":"kiro","event":"UserPromptSubmit","task_context":null}),
        json!({"engine":"kiro","event":"UserPromptSubmit","task_context":{"session_id":first.session,"unknown":true}}),
        json!({"engine":"kiro","event":"UserPromptSubmit","session_id":first.session}),
    ] {
        let result = connection.call_tool(request(value)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let diagnostic = result.content[0].as_text().unwrap().text.as_str();
        assert!(!diagnostic.contains("Full intent"));
        assert!(!diagnostic.contains("PROJECT Memory state:"));
        assert!(!diagnostic.contains("Selected PROJECT Task JSON"));
    }
    connection.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn disabled_task_and_hooks_do_not_admit_or_read_task() {
    let missing = json!({"session_id":"00000000000000000000000001"});
    for (settings, expected) in [
        (
            json!({"memory":false,"task":false}),
            "PROJECT Task state: disabled.",
        ),
        (json!({"memory":false,"task":true,"hooks":false}), ""),
    ] {
        let fixture = configured(&[], settings);
        let (connection, serving) = client(&fixture).await;
        let result = text(
            connection
                .call_tool(request(json!({
                    "engine":"kiro","event":"UserPromptSubmit","task_context":missing
                })))
                .await
                .unwrap(),
        );
        if expected.is_empty() {
            assert!(result.is_empty());
        } else {
            assert!(result.contains(expected));
            assert!(!result.contains("Selected PROJECT Task JSON"));
        }
        connection.cancel().await.unwrap();
        fixture.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn task_read_denial_preserves_accepted_human_activity_and_no_context() {
    let mut fixture = configured(&[], json!({"memory":false,"task":true}));
    let (_activity, receiver) = attach_activity(&mut fixture);
    let before = *receiver.borrow();
    let (connection, serving) = client(&fixture).await;
    let result = connection
        .call_tool(request(json!({
            "engine":"kiro","event":"UserPromptSubmit",
            "task_context":{"session_id":"00000000000000000000000001"}
        })))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    assert!(
        result.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("Task permission denied")
    );
    assert!(*receiver.borrow() > before);
    connection.cancel().await.unwrap();
    fixture.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
