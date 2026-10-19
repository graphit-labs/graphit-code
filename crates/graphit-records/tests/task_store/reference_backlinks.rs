use super::*;
use graphit_records::{
    reference_projection::{AuthoredReferenceEntry, AuthoredReferencesField, ReferenceCandidate},
    task_spec::FieldUpdate,
};

fn project_id() -> graphit_core::project_identity::ProjectId {
    "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()
}

fn target(kind: TaskReferenceKind, id: &str) -> TaskReferenceTarget {
    TaskReferenceTarget {
        target_kind: kind,
        scope: TaskReferenceScope::Project(project_id()),
        id: id.into(),
        context: None,
    }
}

fn query(target: TaskReferenceTarget, size: usize) -> TaskReferenceBacklinksQuery {
    TaskReferenceBacklinksQuery {
        target,
        page_size: NonZeroUsize::new(size).unwrap(),
        cursor: None,
    }
}

type BacklinkPages = (
    TaskGeneration,
    TaskReferenceBacklinksPage,
    TaskReferencesCoverage,
    Vec<TaskReferencesWarning>,
);

fn pages(result: TaskReferenceBacklinksRead) -> BacklinkPages {
    let TaskReferenceBacklinksRead::Published {
        generation,
        incoming,
        coverage,
        warnings,
    } = result
    else {
        panic!("published backlinks")
    };
    (generation, *incoming, coverage, warnings)
}

fn candidate<'a>(
    kind: &'a str,
    id: &'a str,
    scope: (&'a str, &'a str),
    context: Option<&'a str>,
    relation: &'a str,
) -> AuthoredReferenceEntry<'a> {
    AuthoredReferenceEntry::Tuple(ReferenceCandidate {
        target_type: kind,
        id,
        scope: Some(scope.0),
        scope_id: Some(scope.1),
        context,
        relation,
        field: Some(" body literal "),
    })
}

fn limits() -> TaskStoreReadLimits {
    read_limits()
}

fn legacy_query(session: SessionId) -> TaskReferencesQuery {
    TaskReferencesQuery {
        focus: TaskReferenceRecord::Session(session),
        page_size: NonZeroUsize::new(100).unwrap(),
        outgoing_cursor: None,
        incoming_cursor: None,
    }
}

fn select_version(f: &mut Fixture, version: &str) {
    use graphit_core::{
        project_paths::{InstallationPaths, ProjectPaths},
        version_context::{Checkout, VersionContext},
    };
    let install = InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = ProjectPaths::at(&f.root, &install).unwrap();
    f.version = VersionContext::select(&paths, Checkout::NonGit, version, None, "").unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_backlinks_inherited_and_forked_publication_labels_do_not_authorize_selection() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    let (session, main_generation) = {
        admissions!(f, ctx, issuer, authority, grant, read, write);
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("default-origin"),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let TaskReferencesRead::Published { generation, .. } =
            NativeTaskStore::references(ctx.snapshot(), &read, legacy_query(session.id), limits())
                .await
                .unwrap()
        else {
            panic!("published default")
        };
        (session, generation)
    };
    select_version(&mut f, "feature/reference-origin");
    {
        admissions!(f, ctx, issuer, authority, grant, read, write);
        let TaskReadView::Published { generation, .. } =
            NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
        else {
            panic!("inherited SDK pin")
        };
        assert_eq!(generation.version, main_generation.version);
        assert!(generation.physical_branch.is_none() && generation.inherited_target.is_some());
        let inherited =
            NativeTaskStore::references(ctx.snapshot(), &read, legacy_query(session.id), limits())
                .await;
        assert!(
            matches!(&inherited, Ok(TaskReferencesRead::Published { generation: g, .. }) if *g == generation),
            "legitimate inherited publication rejected: {inherited:?}"
        );
        let (_, backlinks, coverage, warnings) = pages(
            NativeTaskStore::reference_backlinks(
                ctx.snapshot(),
                &read,
                query(target(TaskReferenceKind::Memory, "dangling"), 100),
                limits(),
            )
            .await
            .unwrap(),
        );
        assert!(backlinks.complete && backlinks.results.is_empty() && warnings.is_empty());
        assert!(
            coverage.task_session_incoming_complete && coverage.inbound_other_sources_not_qualified
        );
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("named-origin"),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let TaskReferencesRead::Published { generation, .. } =
            NativeTaskStore::references(ctx.snapshot(), &read, legacy_query(session.id), limits())
                .await
                .unwrap()
        else {
            panic!("forked mixed-origin pin")
        };
        assert!(generation.physical_branch.is_some() && generation.inherited_target.is_none());
        let (g, incoming, _, warnings) = pages(
            NativeTaskStore::reference_backlinks(
                ctx.snapshot(),
                &read,
                query(target(TaskReferenceKind::Memory, "dangling"), 100),
                limits(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(g, generation);
        assert!(incoming.complete && warnings.is_empty());
    }
    select_version(&mut f, "main");
    admissions!(f, ctx, issuer, authority, grant, read, _write);
    let TaskReferencesRead::Published { generation, .. } =
        NativeTaskStore::references(ctx.snapshot(), &read, legacy_query(session.id), limits())
            .await
            .unwrap()
    else {
        panic!("default still published")
    };
    assert_eq!(generation, main_generation);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_backlinks_literal_namespaces_duplicates_structural_and_own_rich() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let local = ("project", "01KSH1CRFFG8Z74B5ZS78WW808");
    let foreign = ("project", "01KSH1CRFFG8Z74B5ZS78WW809");
    let entries = [
        candidate("memory", " same β ", local, None, " literal\nrelation "),
        candidate("memory", " same β ", local, None, " literal\nrelation "),
        candidate("memory", " same β ", foreign, None, "foreign"),
        candidate("memory", " same β ", ("user", "UserLiteral"), None, "user"),
        candidate("knowledge", " same β ", local, Some("Import@1"), "import"),
        candidate("knowledge", " same β ", local, None, "local-wiki"),
        candidate("task", " same β ", local, None, "opaque-task"),
        candidate("session", " same β ", local, None, "opaque-session"),
    ];
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("literal"),
        AuthoredReferencesField::Sequence(&entries),
        write_limits(),
    )
    .await
    .unwrap();
    let session_text = serde_json::to_value(session.id)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let task_entries = [
        entries[0],
        entries[1],
        candidate("session", &session_text, local, None, "authored-session"),
    ];
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "literal-task"),
        AuthoredReferencesField::Sequence(&task_entries),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(
        authority
            .admit_read(&grant, &ctx, ProjectReadModule::Memory)
            .is_err()
    );
    let (generation, incoming, coverage, warnings) = pages(
        NativeTaskStore::reference_backlinks(
            ctx.snapshot(),
            &read,
            query(target(TaskReferenceKind::Memory, " same β "), 100),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(incoming.total, 4);
    assert!(incoming.complete && warnings.is_empty());
    assert_eq!(
        incoming
            .results
            .iter()
            .map(|e| e.ordinal)
            .collect::<Vec<_>>(),
        vec![1, 2, 0, 1]
    );
    assert!(
        incoming
            .results
            .iter()
            .all(|e| e.provenance == TaskReferenceProvenance::Authored
                && e.source_snapshot == generation
                && e.target_resolution == TaskReferenceResolution::Unresolved)
    );
    assert!(
        coverage.task_session_incoming_complete && coverage.inbound_other_sources_not_qualified
    );
    let mut variants = vec![
        (target(TaskReferenceKind::Task, " same β "), 1),
        (target(TaskReferenceKind::Session, " same β "), 1),
        (target(TaskReferenceKind::Knowledge, " same β "), 1),
        (target(TaskReferenceKind::Memory, "different"), 0),
    ];
    let mut imported = target(TaskReferenceKind::Knowledge, " same β ");
    imported.context = Some("Import@1".into());
    variants.push((imported.clone(), 1));
    imported.context = Some("Import@2".into());
    variants.push((imported, 0));
    let mut user = target(TaskReferenceKind::Memory, " same β ");
    user.scope = TaskReferenceScope::User("UserLiteral".into());
    variants.push((user.clone(), 1));
    user.scope = TaskReferenceScope::User("userliteral".into());
    variants.push((user, 0));
    let mut foreign = target(TaskReferenceKind::Memory, " same β ");
    foreign.scope = TaskReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap());
    variants.push((foreign, 1));
    for (target, total) in variants {
        let (g, page, _, warnings) = pages(
            NativeTaskStore::reference_backlinks(
                ctx.snapshot(),
                &read,
                query(target, 100),
                limits(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(g, generation);
        assert_eq!(page.total, total);
        assert!(page.complete && warnings.is_empty());
        assert!(
            page.results
                .iter()
                .all(|e| e.target_resolution == TaskReferenceResolution::Unresolved)
        );
    }
    let (_, page, _, _) = pages(
        NativeTaskStore::reference_backlinks(
            ctx.snapshot(),
            &read,
            query(target(TaskReferenceKind::Session, &session_text), 100),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(page.total, 2);
    assert_eq!(
        page.results[0].source,
        TaskReferenceRecord::Task(task.specification.id)
    );
    assert_eq!(
        page.results[0].provenance,
        TaskReferenceProvenance::Structural
    );
    assert_eq!(
        page.results[1].provenance,
        TaskReferenceProvenance::Authored
    );
    assert!(page.results.iter().all(|e| matches!(e.target_resolution, TaskReferenceResolution::Session { id, .. } if id == session.id)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_backlinks_current_generation_clear_and_history_never_adopt_old_edges() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let own = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("clear"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let refs = [candidate(
        "memory",
        "old",
        ("project", "01KSH1CRFFG8Z74B5ZS78WW808"),
        None,
        "supports",
    ); 2];
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "clear-task"),
        AuthoredReferencesField::Sequence(&refs),
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let task = claim.record;
    let mut q = query(target(TaskReferenceKind::Memory, "old"), 1);
    let (_, page, _, _) = pages(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, q.clone(), limits())
            .await
            .unwrap(),
    );
    assert_eq!(page.total, 2);
    assert_eq!(
        page.results[0].reference_generation,
        task.specification.revision
    );
    q.cursor = page.next_cursor;
    let changed = NativeTaskStore::revise_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        RevisionUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: task.specification.revision,
            },
            lease: Duration::from_secs(120),
            reason: "clear refs",
            title: FieldUpdate::Keep,
            description: FieldUpdate::Keep,
            task_type: FieldUpdate::Keep,
            priority: None,
            schedule: FieldUpdate::Keep,
            add_checks: &[],
        },
        AuthoredReferencesField::Sequence(&[]),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, q, limits()).await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let (_, page, _, warnings) = pages(
        NativeTaskStore::reference_backlinks(
            ctx.snapshot(),
            &read,
            query(target(TaskReferenceKind::Memory, "old"), 100),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(page.total, 0);
    assert!(page.complete && warnings.is_empty());
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id: changed.specification.id,
            history: Some(graphit_records::task_store::TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(64).unwrap(),
                cursor: None,
            }),
            comments: None,
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(detail.history.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(detail.creation.record.references.len(), 2);
    assert_eq!(
        detail.history.as_ref().unwrap().events[0]
            .record
            .references
            .len(),
        2
    );
    assert!(detail.task.references.is_empty());
    assert!(
        detail
            .history
            .as_ref()
            .unwrap()
            .events
            .last()
            .unwrap()
            .record
            .references
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_backlinks_closed_cursors_batch_independent_and_zero_targets() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let refs = [
        candidate(
            "memory",
            "bounded",
            ("project", "01KSH1CRFFG8Z74B5ZS78WW808"),
            None,
            "one",
        ),
        candidate(
            "memory",
            "bounded",
            ("project", "01KSH1CRFFG8Z74B5ZS78WW808"),
            None,
            "two",
        ),
    ];
    NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("bounds"),
        AuthoredReferencesField::Sequence(&refs),
        write_limits(),
    )
    .await
    .unwrap();
    let q = query(target(TaskReferenceKind::Memory, "bounded"), 1);
    let (generation, first, _, _) = pages(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, q.clone(), limits())
            .await
            .unwrap(),
    );
    assert_eq!(first.total, 2);
    assert_eq!(first.results[0].ordinal, 0);
    let cursor = first.next_cursor.unwrap();
    let mut next = q.clone();
    next.cursor = Some(cursor.clone());
    next.page_size = NonZeroUsize::new(100).unwrap();
    let (g, second, _, _) = pages(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, next.clone(), limits())
            .await
            .unwrap(),
    );
    assert_eq!(g, generation);
    assert_eq!(second.results[0].ordinal, 1);
    assert!(second.next_cursor.is_none());
    let raw = serde_json::to_value(&cursor).unwrap();
    for (path, value) in [
        (vec!["direction"], serde_json::json!("outgoing")),
        (vec!["offset"], serde_json::json!(3)),
        (vec!["offset"], serde_json::json!(usize::MAX)),
        (
            vec!["binding", "project_version"],
            serde_json::json!("foreign"),
        ),
        (
            vec!["binding", "target", "id"],
            serde_json::json!("different"),
        ),
        (
            vec!["binding", "pin", "version"],
            serde_json::json!(u64::MAX),
        ),
        (
            vec!["binding", "pin", "physical_branch"],
            serde_json::json!("foreign"),
        ),
    ] {
        let mut changed = raw.clone();
        let mut slot = &mut changed;
        for key in path {
            slot = &mut slot[key];
        }
        *slot = value;
        let mut bad = q.clone();
        bad.cursor = Some(serde_json::from_value(changed).unwrap());
        assert!(matches!(
            NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, bad, limits()).await,
            Err(TaskStoreError::InvalidCursor)
        ));
    }
    let mut extra = raw.clone();
    extra["binding"]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TaskReferenceBacklinksCursor>(extra).is_err());
    let mut wrong_family = raw.clone();
    wrong_family["binding"]["family"] = serde_json::json!("other_query");
    assert!(serde_json::from_value::<TaskReferenceBacklinksCursor>(wrong_family).is_err());
    let mut exact_end = raw.clone();
    exact_end["offset"] = serde_json::json!(2);
    next.cursor = Some(serde_json::from_value(exact_end).unwrap());
    let (_, empty, _, _) = pages(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, next, limits())
            .await
            .unwrap(),
    );
    assert!(empty.results.is_empty() && empty.next_cursor.is_none() && empty.complete);
    let legacy = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    let TaskReadView::Published { sessions, .. } = legacy else {
        panic!("published")
    };
    let TaskReferencesRead::Published { outgoing, .. } = NativeTaskStore::references(
        ctx.snapshot(),
        &read,
        TaskReferencesQuery {
            focus: TaskReferenceRecord::Session(sessions[0].id),
            page_size: NonZeroUsize::new(1).unwrap(),
            outgoing_cursor: None,
            incoming_cursor: None,
        },
        limits(),
    )
    .await
    .unwrap() else {
        panic!("legacy")
    };
    assert!(
        serde_json::from_value::<TaskReferenceBacklinksCursor>(
            serde_json::to_value(outgoing.next_cursor.unwrap()).unwrap()
        )
        .is_err()
    );
    let mut changed_limits = limits();
    changed_limits.batch_rows = std::num::NonZeroU32::new(1).unwrap();
    let mut bound_cursor = q.clone();
    bound_cursor.cursor = Some(cursor);
    let (_, continued, _, _) = pages(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, bound_cursor, changed_limits)
            .await
            .unwrap(),
    );
    assert_eq!(continued.results.len(), 1);
    assert!(continued.next_cursor.is_none());
    let (_, zero, _, warnings) = pages(
        NativeTaskStore::reference_backlinks(
            ctx.snapshot(),
            &read,
            query(target(TaskReferenceKind::Memory, "nonexistent"), 100),
            limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(zero.total, 0);
    assert!(zero.complete && warnings.is_empty());
    assert!(
        NativeTaskStore::reference_backlinks(
            ctx.snapshot(),
            &read,
            query(q.target.clone(), 101),
            limits()
        )
        .await
        .is_err()
    );
    let (g, _, _, _) = pages(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, q.clone(), limits())
            .await
            .unwrap(),
    );
    assert_eq!(g, generation);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reference_backlinks_no_corpus_raw_preflight_original_and_actual_revoke_cancel() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (_, reference, _) = f.issue(&issuer, &ctx);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(120))
        .unwrap();
    let bearer = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[graphit_auth::Permission::Read(
                graphit_core::module_settings::Module::Task,
            )],
            &f.scope(),
        )
        .unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let q = query(target(TaskReferenceKind::Memory, "dangling"), 100);
    assert_eq!(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, q.clone(), limits())
            .await
            .unwrap(),
        TaskReferenceBacklinksRead::NoCorpus
    );
    let mut invalid = q.clone();
    invalid.target.context = Some("bad-memory-context".into());
    assert!(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, invalid, limits())
            .await
            .is_err()
    );
    let mut empty = q.clone();
    empty.target.id.clear();
    assert!(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, empty, limits())
            .await
            .is_err()
    );
    let other = request(&cancel);
    assert!(matches!(
        NativeTaskStore::reference_backlinks(other.snapshot(), &read, q.clone(), limits()).await,
        Err(TaskStoreError::DifferentRequest)
    ));
    assert!(!f.project.runtime_root().join("task").exists());
    assert!(
        authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Memory)
            .is_err()
    );
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, q.clone(), limits()).await,
        Err(TaskStoreError::Auth(_))
    ));
    let fresh = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    cancel.cancel();
    assert!(matches!(
        NativeTaskStore::reference_backlinks(ctx.snapshot(), &fresh, q, limits()).await,
        Err(TaskStoreError::Request(_))
    ));
    assert!(!f.project.runtime_root().join("task").exists());
}
