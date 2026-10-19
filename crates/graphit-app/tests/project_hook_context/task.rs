#![cfg(target_os = "linux")]
use super::*;
use graphit_app::project_hook_context::TaskContextSelection;
use graphit_core::version_context::ScopedPin;
use graphit_records::{
    reference_projection::AuthoredReferencesField,
    task_spec::CheckKind,
    task_store::{
        CreateCheck, CreateSession, CreateTask, NativeTaskStore, TaskStoreError,
        TaskStoreReadLimits, TaskStoreWriteLimits,
    },
};

fn task_read_limits() -> TaskStoreReadLimits {
    TaskStoreReadLimits {
        native: read_limits().native,
        batch_rows: NonZeroU32::new(1).unwrap(),
    }
}
fn task_write_limits() -> TaskStoreWriteLimits {
    let memory = write_limits();
    TaskStoreWriteLimits {
        read: task_read_limits(),
        artifacts: memory.artifacts,
        references: graphit_records::task_store::TaskReferencesInputLimits {
            max_entries: NonZeroUsize::new(128).unwrap(),
            max_text_bytes: NonZeroUsize::new(1048576).unwrap(),
        },
    }
}
fn selection() -> TaskContextSelection {
    serde_json::from_value(json!({"session_id":"00000000000000000000000001"})).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_context_preserves_real_admission_request_and_recipient_failures() {
    let cancel = Cancellation::default();
    let mut ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run, admin) = f.issue(&issuer, &mut ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let memory = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let settings = modules(&[Module::Hooks, Module::Task]);
    let values = data();
    let variables = vars(&values);
    let selected = selection();
    let selected_input = || {
        let mut input = input(&settings, &variables);
        input.task_context = Some(&selected);
        input
    };
    assert!(matches!(
        error(render_project_hook(&ctx, selected_input(), None, None).await),
        HookContextError::MissingTaskAdmission
    ));
    assert!(matches!(
        error(
            render_project_hook(
                &ctx,
                selected_input(),
                None,
                Some((&read, task_read_limits()))
            )
            .await
        ),
        HookContextError::MissingTaskSession
    ));
    assert!(matches!(
        error(render_project_hook(&ctx, selected_input(), None, Some((&memory, task_read_limits()))).await),
        HookContextError::Task(e) if matches!(*e, TaskStoreError::WrongAdmission)
    ));
    let other = request(&cancel);
    assert!(matches!(
        error(render_project_hook(&other, selected_input(), None, Some((&read, task_read_limits()))).await),
        HookContextError::Task(e) if matches!(*e, TaskStoreError::DifferentRequest)
    ));
    // Two genuine admissions with equal version text still cannot mix PROJECTs.
    let foreign = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    let foreign_issuer = foreign.issuer(&ctx);
    let foreign_secret =
        std::fs::read_to_string(foreign.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let foreign_caller = foreign_issuer
        .authenticate_runtime(&foreign_secret, &ctx)
        .unwrap();
    let foreign_authority = ProjectRecordAuthority::new(&foreign_issuer);
    let foreign_task = foreign_authority
        .admit_authenticated_read(
            &foreign_caller,
            &ctx,
            foreign.scope(),
            ProjectReadModule::Task,
        )
        .unwrap();
    let foreign_write = foreign_authority
        .admit_authenticated_write(
            &foreign_caller,
            &ctx,
            foreign.scope(),
            ProjectWriteOperation::Task,
        )
        .unwrap();
    let foreign_session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &foreign_write,
        CreateSession {
            idempotency_key: "foreign-pair-session".into(),
            title: "Foreign real Task corpus".into(),
            description: "A genuine session in the other project.".into(),
            strategy: "Do not mix this with Memory A.".into(),
        },
        AuthoredReferencesField::Omitted,
        task_write_limits(),
    )
    .await
    .unwrap();
    let local_secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let local_caller = issuer.authenticate_runtime(&local_secret, &ctx).unwrap();
    let local_memory_write = authority
        .admit_authenticated_write(
            &local_caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Memory,
        )
        .unwrap();
    NativeMemoryStore::create(
        ctx.snapshot(),
        &local_memory_write,
        super::memory("pair-a", true),
        write_limits(),
    )
    .await
    .unwrap();
    let foreign_selection = TaskContextSelection {
        session_id: foreign_session.id,
        task_ids: vec![],
    };
    let both = modules(&[Module::Hooks, Module::Memory, Module::Task]);
    let mut mixed = input(&both, &variables);
    mixed.task_context = Some(&foreign_selection);
    assert!(matches!(
        error(
            render_project_hook(
                &ctx,
                mixed,
                Some((&memory, read_limits())),
                Some((&foreign_task, task_read_limits())),
            )
            .await
        ),
        HookContextError::ProjectScopeMismatch
    ));
    let mut delegate = selected_input();
    delegate.audience = MandateAudience::Delegate;
    assert!(matches!(
        error(render_project_hook(&ctx, delegate, None, Some((&read, task_read_limits()))).await),
        HookContextError::TaskRecipientDenied
    ));
    let mut other_version = values.clone();
    other_version.insert("project_version".into(), "other".into());
    let other_variables = vars(&other_version);
    let mut mismatch = input(&settings, &other_variables);
    mismatch.task_context = Some(&selected);
    assert!(matches!(
        error(render_project_hook(&ctx, mismatch, None, Some((&read, task_read_limits()))).await),
        HookContextError::ProjectVersionMismatch
    ));
    for reserved in ["project_task_state", "project_task_json"] {
        let mut variables = vars(&values);
        variables.push(InstructionVariable {
            name: reserved,
            value: InstructionValue::Text("forged"),
        });
        let forbidden = selection();
        let mut selected = input(&settings, &variables);
        selected.task_context = Some(&forbidden);
        assert!(matches!(
            error(
                render_project_hook(&ctx, selected, None, Some((&read, task_read_limits()))).await
            ),
            HookContextError::ReservedVariable
        ));
    }
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        error(
            render_project_hook(
                &ctx,
                selected_input(),
                None,
                Some((&read, task_read_limits()))
            )
            .await
        ),
        HookContextError::Auth(AuthError::Denied)
    ));
    cancel.cancel();
    assert!(matches!(
        error(render_project_hook(&ctx, selected_input(), None, Some((&read, task_read_limits()))).await),
        HookContextError::Request(e) if e.kind() == std::io::ErrorKind::Interrupted
    ));
    let timeout_cancel = Cancellation::default();
    let expired = request_until(&timeout_cancel, Instant::now() + Duration::from_millis(1));
    tokio::time::sleep(Duration::from_millis(2)).await;
    assert!(matches!(
        error(render_project_hook(&expired, selected_input(), None, Some((&read, task_read_limits()))).await),
        HookContextError::Request(e) if e.kind() == std::io::ErrorKind::TimedOut
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selected_task_escaped_json_over_cap_is_causal_and_readonly() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        CreateSession {
            idempotency_key: "bounded-session".into(),
            title: "Bounded full intent".into(),
            description: "Keep the whole session.".into(),
            strategy: "Fail instead of truncating.".into(),
        },
        AuthoredReferencesField::Omitted,
        task_write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        CreateTask {
            commit: None,
            idempotency_key: "bounded-task".into(),
            session_id: session.id,
            parent_id: None,
            depends_on: vec![],
            title: "Complete escaped task".into(),
            description: "α\n\"\\".repeat(2048),
            task_type: "task".into(),
            priority: 1,
            schedule: None,
            checks: vec![
                CreateCheck {
                    kind: CheckKind::Acceptance,
                    text: "Return the whole context or fail.".into(),
                },
                CreateCheck {
                    kind: CheckKind::Test,
                    text: "No useful prefix.".into(),
                },
            ],
        },
        AuthoredReferencesField::Omitted,
        task_write_limits(),
    )
    .await
    .unwrap();
    let before = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput::current(task.specification.id),
        task_read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    let selected = TaskContextSelection {
        session_id: session.id,
        task_ids: vec![task.specification.id],
    };
    let memory_read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Memory)
        .unwrap();
    let memory_write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)
        .unwrap();
    NativeMemoryStore::create(
        ctx.snapshot(),
        &memory_write,
        memory("paired-memory", true),
        write_limits(),
    )
    .await
    .unwrap();
    let settings = modules(&[Module::Hooks, Module::Memory, Module::Task]);
    let mut values = data();
    values.remove("skill_name_task");
    let variables = vars(&values);
    let mut limited = input(&settings, &variables);
    limited.task_context = Some(&selected);
    limited.instruction_limits.max_variable_bytes = NonZeroUsize::new(4096).unwrap();
    let cause = error(
        render_project_hook(
            &ctx,
            limited,
            Some((&memory_read, read_limits())),
            Some((&read, task_read_limits())),
        )
        .await,
    );
    assert!(matches!(cause, HookContextError::Instruction(ref e)
        if e.kind() == InstructionErrorKind::LimitExceeded(graphit_runtime::instructions::InstructionLimit::VariableBytes)));
    assert!(cause.source().is_some());
    assert!(!format!("{cause:?}").contains("Complete escaped task"));
    assert_eq!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput::current(task.specification.id),
            task_read_limits()
        )
        .await
        .unwrap()
        .unwrap(),
        before
    );
    let complete_values = data();
    let complete_variables = vars(&complete_values);
    let mut complete = input(&settings, &complete_variables);
    complete.task_context = Some(&selected);
    let paired = render_project_hook(
        &ctx,
        complete,
        Some((&memory_read, read_limits())),
        Some((&read, task_read_limits())),
    )
    .await
    .unwrap();
    assert!(body(&paired).contains("PROJECT Task state: loaded."));
    assert!(body(&paired).contains("PROJECT Memory state: loaded."));
    assert!(body(&paired).contains("PROJECT paired-memory"));
    assert_eq!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput::current(task.specification.id),
            task_read_limits(),
        )
        .await
        .unwrap()
        .unwrap(),
        before
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn equal_version_text_cannot_mix_distinct_canonical_lance_branches() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "feature");
    let issuer = f.issuer(&ctx);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let installation = InstallationPaths::resolve(
        Brand::default(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = ProjectPaths::at(&f.root, &installation).unwrap();
    // Captured checkout facts exercise the real scoped pin and SDK branch mapping.
    let pin = ScopedPin::new(&paths, "feature").unwrap();
    let base = VersionContext::select(
        &paths,
        Checkout::Branch {
            name: "feature",
            default: "feature",
        },
        "",
        Some(&pin),
        "",
    )
    .unwrap();
    let named = VersionContext::select(
        &paths,
        Checkout::Branch {
            name: "feature",
            default: "main",
        },
        "",
        Some(&pin),
        "",
    )
    .unwrap();
    assert_eq!(base.version(), named.version());
    assert_ne!(base.lance_branch(), named.lance_branch());
    let mut base_scope = f.scope();
    base_scope.project.as_mut().unwrap().version = &base;
    let mut named_scope = f.scope();
    named_scope.project.as_mut().unwrap().version = &named;
    let base_memory = authority
        .admit_authenticated_read(&caller, &ctx, base_scope, ProjectReadModule::Memory)
        .unwrap();
    let named_task = authority
        .admit_authenticated_read(&caller, &ctx, named_scope, ProjectReadModule::Task)
        .unwrap();
    let settings = modules(&[Module::Hooks, Module::Memory, Module::Task]);
    let mut values = data();
    values.insert("project_version".into(), "feature".into());
    let variables = vars(&values);
    let selected = selection();
    let mut mixed = input(&settings, &variables);
    mixed.task_context = Some(&selected);
    assert!(matches!(
        error(
            render_project_hook(
                &ctx,
                mixed,
                Some((&base_memory, read_limits())),
                Some((&named_task, task_read_limits())),
            )
            .await
        ),
        HookContextError::ProjectScopeMismatch
    ));

    // Different defaults are acceptable when the SDK selects the same physical branch.
    let other_default = VersionContext::select(
        &paths,
        Checkout::Branch {
            name: "feature",
            default: "another-default",
        },
        "",
        Some(&pin),
        "",
    )
    .unwrap();
    assert_eq!(named.lance_branch(), other_default.lance_branch());
    let mut other_scope = f.scope();
    other_scope.project.as_mut().unwrap().version = &other_default;
    let same_branch_memory = authority
        .admit_authenticated_read(&caller, &ctx, other_scope, ProjectReadModule::Memory)
        .unwrap();
    let mut paired = input(&settings, &variables);
    paired.task_context = Some(&selected);
    assert!(matches!(
        error(
            render_project_hook(
                &ctx,
                paired,
                Some((&same_branch_memory, read_limits())),
                Some((&named_task, task_read_limits())),
            )
            .await
        ),
        HookContextError::MissingTaskSession
    ));
}
