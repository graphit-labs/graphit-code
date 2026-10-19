use super::*;
use graphit_auth::{Obligation, Permission};
use graphit_core::module_settings::Module;
use graphit_data::lance_branch_store::LanceBranchStore;
use graphit_hub::native::Role;

macro_rules! dream {
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$grant:ident,$read:ident,$write:ident,$bearer:ident,$credential:ident,$admin:ident) => {
        let $issuer = $f.issuer(&$ctx);
        let ($bearer, $credential, _run, $admin) = $f.issue_role_for(
            &$issuer,
            &$ctx,
            $f.scope(),
            Role::Dream,
            &[
                Permission::Read(Module::Memory),
                Permission::WriteFencedMemory,
            ],
        );
        let $grant = $issuer
            .verify(
                $bearer.expose_for_transport(),
                &$credential,
                $f.scope(),
                &$ctx,
            )
            .unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        let $read = $authority
            .admit_read(&$grant, &$ctx, ProjectReadModule::Memory)
            .unwrap();
        let $write = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::FencedMemory)
            .unwrap();
        assert_eq!($write.recheck().unwrap(), Obligation::MemoryFenceRequired);
    };
}

fn patch(key: &str, expected_revision: u64) -> FencedUpdateMemory {
    FencedUpdateMemory {
        expected_revision,
        idempotency_key: key.into(),
        reason: "Observed durable knowledge".into(),
        title: None,
        body: None,
        references: None,
    }
}
fn deletion(key: &str, expected_revision: u64) -> DeleteMemory {
    DeleteMemory {
        expected_revision,
        idempotency_key: key.into(),
        reason: "Explicit direct deletion".into(),
    }
}
fn reference() -> MemoryReference {
    MemoryReference {
        target_kind: ReferenceTargetKind::Task,
        scope: MemoryReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap()),
        id: "logical-task-no-target-read".into(),
        context: None,
        relation: "supports".into(),
        field: Some("body".into()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fenced_direct_attributes_references_and_authoritative_archival() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(
        f,
        ctx,
        issuer,
        authority,
        grant,
        read,
        write,
        _bearer,
        _credential,
        _admin
    );
    for (i, classification) in [
        MemoryClass::Fact,
        MemoryClass::Skill,
        MemoryClass::Tension,
        MemoryClass::Decision,
        MemoryClass::Convention,
        MemoryClass::Correction,
    ]
    .into_iter()
    .enumerate()
    {
        let mut authored = input(&format!("class-{i}"));
        authored.body = format!("fencedquartz{i}");
        authored.attributes = MemoryAttributes {
            classification,
            mandatory: i % 2 == 0,
            important: i % 3 == 0,
        };
        authored.references = Some(vec![reference()]);
        let initial = NativeMemoryStore::fenced_create(
            ctx.snapshot(),
            &write,
            authored.clone(),
            write_limits(),
        )
        .await
        .unwrap();
        let MemoryLexicalSearchRead::Published {
            hits, total_hits, ..
        } = NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            MemoryLexicalQuery {
                text: authored.body.clone(),
                filter: MemoryListFilter::default(),
                include_deleted_history: false,
                top_k: NonZeroUsize::new(10).unwrap(),
            },
            read_limits(),
        )
        .await
        .unwrap()
        else {
            panic!("fenced create must initialize FTS");
        };
        assert_eq!(total_hits, 1);
        assert_eq!(hits[0].current.id, initial.id);
        assert_eq!(initial.attributes, authored.attributes);
        assert_eq!(initial.tags, authored.tags);
        assert_eq!(initial.references, vec![reference()]);
        let detail = NativeMemoryStore::get(ctx.snapshot(), &read, initial.id, read_limits())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.creation.input, authored);
        assert_eq!(detail.memory, initial);
        let mut update = patch(&format!("body-{i}"), 1);
        update.body = Some(format!("Changed content {i}"));
        let changed = NativeMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            update.clone(),
            write_limits(),
        )
        .await
        .unwrap();
        assert_eq!(changed.attributes, initial.attributes);
        assert_eq!(changed.tags, initial.tags);
        assert_eq!(changed.created_at, initial.created_at);
        assert_eq!(changed.created_by, initial.created_by);
        assert_eq!(changed.updated_by, initial.created_by);
        assert_eq!(changed.references, initial.references);
        assert_eq!(changed.reference_generation, initial.reference_generation);
        assert_eq!(changed.revision, 2);
        let mut clear = patch(&format!("refs-{i}"), 2);
        clear.references = Some(vec![]);
        let cleared = NativeMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            clear,
            write_limits(),
        )
        .await
        .unwrap();
        assert!(cleared.references.is_empty());
        assert_eq!(cleared.reference_generation, 2);
        assert_eq!(cleared.attributes, initial.attributes);
        let deleted = NativeMemoryStore::fenced_delete(
            ctx.snapshot(),
            &write,
            initial.id,
            deletion(&format!("delete-{i}"), 3),
            write_limits(),
        )
        .await
        .unwrap();
        assert_eq!(deleted.revision, 4);
        assert_eq!(deleted.attributes, initial.attributes);
        assert_eq!(deleted.body, changed.body);
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, initial.id, read_limits())
                .await
                .unwrap()
                .is_none()
        );
        let history = NativeMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
            .await
            .unwrap()
            .unwrap();
        assert!(history.deleted);
        assert_eq!(history.creation.memory, initial);
        assert_eq!(history.revisions.len(), 3);
        assert_eq!(history.revisions.last().unwrap().before, cleared);
        assert_eq!(history.current, deleted);
    }
    // This is direct conditional archival, including a mandatory+important memory.
    assert!(
        matches!(NativeMemoryStore::list(ctx.snapshot(), &read, MemoryListFilter::default(), read_limits()).await.unwrap(), MemoryListRead::Published { memories, .. } if memories.is_empty())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fenced_same_attempt_concurrency_cas_and_fresh_historical_recovery() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(
        f, ctx, issuer, authority, grant, read, write, bearer, credential, _admin
    );
    let authored = input("literal'\nβ");
    assert_eq!(
        NativeMemoryStore::reobserve_fenced_create(
            ctx.snapshot(),
            &write,
            authored.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        MemoryCreateObservation::Unresolved
    );
    assert!(!f.project.runtime_root().join("memory").exists());
    let (a, b) = tokio::join!(
        NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored.clone(), write_limits()),
        NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored.clone(), write_limits())
    );
    let initial = a.unwrap();
    assert_eq!(b.unwrap(), initial);
    assert_eq!(sdk::all(&sdk::pin(&f).await).await.num_rows(), 3);
    let mut altered = authored.clone();
    altered.references = Some(vec![]);
    assert!(matches!(
        NativeMemoryStore::fenced_create(ctx.snapshot(), &write, altered, write_limits()).await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    let mut first = patch("first", 1);
    first.title = Some("winner-A".into());
    let mut second = patch("second", 1);
    second.title = Some("winner-B".into());
    let (a, b) = tokio::join!(
        NativeMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            first.clone(),
            write_limits()
        ),
        NativeMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            second.clone(),
            write_limits()
        )
    );
    let (winner, attempt) = match (a, b) {
        (
            Ok(a),
            Err(MemoryStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }),
        ) => (a, first),
        (
            Err(MemoryStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }),
            Ok(b),
        ) => (b, second),
        other => panic!("one transition must win: {other:?}"),
    };
    let mut later = patch("later", 2);
    later.body = Some("Later content".into());
    NativeMemoryStore::fenced_update(ctx.snapshot(), &write, initial.id, later, write_limits())
        .await
        .unwrap();
    let version = sdk::pin(&f).await.version();
    assert_eq!(
        NativeMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            attempt.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        winner
    );
    let mut different = attempt.clone();
    different.reason.push('x');
    assert!(matches!(
        NativeMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            different,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    let fresh = request(&cancel);
    let fresh_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &fresh,
        )
        .unwrap();
    let fresh_write = authority
        .admit_write(&fresh_grant, &fresh, ProjectWriteOperation::FencedMemory)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::reobserve_fenced_create(
            fresh.snapshot(),
            &write,
            authored.clone(),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    assert_eq!(
        NativeMemoryStore::reobserve_fenced_create(
            fresh.snapshot(),
            &fresh_write,
            authored,
            write_limits()
        )
        .await
        .unwrap(),
        MemoryCreateObservation::Found(initial.clone())
    );
    let event = NativeMemoryStore::reobserve_fenced_mutation(
        fresh.snapshot(),
        &fresh_write,
        initial.id,
        FencedMemoryMutation::Update(attempt),
        write_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(event.before, initial);
    assert_eq!(event.after, winner);
    assert_eq!(sdk::pin(&f).await.version(), version);
    assert_eq!(
        NativeMemoryStore::history(ctx.snapshot(), &read, winner.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .revisions
            .len(),
        2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fenced_and_ordinary_authority_boundaries_cancel_deadline_revoke() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(
        f, ctx, issuer, authority, grant, read, write, bearer, credential, admin
    );
    assert!(matches!(
        NativeMemoryStore::create(
            ctx.snapshot(),
            &write,
            input("ordinary-denied"),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    assert!(matches!(
        NativeMemoryStore::reobserve_create(
            ctx.snapshot(),
            &write,
            input("ordinary-denied"),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    assert!(!f.project.runtime_root().join("memory").exists());
    assert!(
        authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
            .is_err()
    );
    assert!(
        authority
            .admit_read(&grant, &ctx, ProjectReadModule::Task)
            .is_err()
    );
    let memory = NativeMemoryStore::fenced_create(
        ctx.snapshot(),
        &write,
        input("authority"),
        write_limits(),
    )
    .await
    .unwrap();
    let ordinary_update = UpdateMemory {
        expected_revision: 1,
        idempotency_key: "denied".into(),
        reason: "Denied ordinary write".into(),
        title: Some("Denied".into()),
        body: None,
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    };
    assert!(matches!(
        NativeMemoryStore::update(
            ctx.snapshot(),
            &write,
            memory.id,
            ordinary_update.clone(),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    assert!(matches!(
        NativeMemoryStore::delete(
            ctx.snapshot(),
            &write,
            memory.id,
            deletion("denied", 1),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    assert!(matches!(
        NativeMemoryStore::reobserve_mutation(
            ctx.snapshot(),
            &write,
            memory.id,
            MemoryMutation::Update(ordinary_update),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let version = sdk::pin(&f).await.version();
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        NativeMemoryStore::fenced_create(
            other.snapshot(),
            &write,
            input("foreign"),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    let other_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &other,
        )
        .unwrap();
    let other_write = authority
        .admit_write(&other_grant, &other, ProjectWriteOperation::FencedMemory)
        .unwrap();
    other_cancel.cancel();
    assert!(matches!(
        NativeMemoryStore::fenced_delete(
            other.snapshot(),
            &other_write,
            memory.id,
            deletion("cancel", 1),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::Request(_))
    ));
    let deadline_cancel = Cancellation::default();
    let deadline = request_until(&deadline_cancel, Instant::now() + Duration::from_secs(1));
    let deadline_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &deadline,
        )
        .unwrap();
    let deadline_write = authority
        .admit_write(
            &deadline_grant,
            &deadline,
            ProjectWriteOperation::FencedMemory,
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(matches!(
        NativeMemoryStore::reobserve_fenced_mutation(
            deadline.snapshot(),
            &deadline_write,
            memory.id,
            FencedMemoryMutation::Delete(deletion("deadline", 1)),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::Request(_))
    ));
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        NativeMemoryStore::fenced_delete(
            ctx.snapshot(),
            &write,
            memory.id,
            deletion("revoke", 1),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::Auth(_))
    ));
    assert_eq!(sdk::pin(&f).await.version(), version);
    // Read admission is retained only as an actual borrowed witness; revoke invalidates it too.
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fenced_named_inheritance_project_isolation_and_wrong_admissions() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    admissions!(
        f,
        ctx,
        issuer,
        authority,
        grant,
        read,
        write,
        _bearer,
        _credential,
        _admin
    );
    let initial =
        NativeMemoryStore::create(ctx.snapshot(), &write, input("default"), write_limits())
            .await
            .unwrap();
    let default_pin = sdk::pin(&f).await;
    assert!(matches!(
        NativeMemoryStore::fenced_create(ctx.snapshot(), &write, input("wrong"), write_limits())
            .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    assert!(matches!(
        NativeMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            patch("wrong", 1),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    assert!(matches!(
        NativeMemoryStore::fenced_delete(
            ctx.snapshot(),
            &write,
            initial.id,
            deletion("wrong", 1),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    assert!(matches!(
        NativeMemoryStore::reobserve_fenced_mutation(
            ctx.snapshot(),
            &write,
            initial.id,
            FencedMemoryMutation::Delete(deletion("wrong", 1)),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let task = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::fenced_create(ctx.snapshot(), &task, input("wrongtask"), write_limits())
            .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    drop(task);
    drop(write);
    drop(read);
    drop(grant);
    drop(issuer);
    let paths = graphit_core::project_paths::ProjectPaths::at(
        &f.root,
        &graphit_core::project_paths::InstallationPaths::resolve(
            f.project.brand().clone(),
            &f.root,
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap(),
    )
    .unwrap();
    f.version =
        VersionContext::select(&paths, Checkout::NonGit, "feature-fenced", None, "").unwrap();
    dream!(
        f,
        ctx,
        issuer,
        authority,
        grant,
        read,
        write,
        _bearer,
        _credential,
        _admin
    );
    let inherited = NativeMemoryStore::get(ctx.snapshot(), &read, initial.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(inherited.generation.inherited_target.is_some());
    assert_eq!(inherited.memory, initial);
    let mut update = patch("named", 1);
    update.body = Some("Named isolated body".into());
    let changed = NativeMemoryStore::fenced_update(
        ctx.snapshot(),
        &write,
        initial.id,
        update,
        write_limits(),
    )
    .await
    .unwrap();
    assert_ne!(changed.updated_by, initial.updated_by);
    assert_eq!(changed.created_by, initial.created_by);
    assert_eq!(changed.attributes, initial.attributes);
    assert!(
        NativeMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .generation
            .inherited_target
            .is_none()
    );
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let store = graphit_data::lance_branch_store::LanceBranchStore::open(&uri, sdk::schema(), None)
        .await
        .unwrap();
    let default = store.capture_target(None).await.unwrap();
    assert_eq!(default.version(), default_pin.version());
    assert_eq!(sdk::all(&default).await.num_rows(), 3);
    let other = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    dream!(
        other,
        ctx,
        issuer_b,
        authority_b,
        grant_b,
        read_b,
        write_b,
        _bearer_b,
        _credential_b,
        _admin_b
    );
    assert_eq!(
        NativeMemoryStore::reobserve_fenced_mutation(
            ctx.snapshot(),
            &write_b,
            initial.id,
            FencedMemoryMutation::Delete(deletion("absent", 1)),
            write_limits()
        )
        .await
        .unwrap(),
        None
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read_b, initial.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert!(!other.project.runtime_root().join("memory").exists());
}

fn importance_attempt(key: &str, expected_revision: u64) -> ChangeMemoryImportance {
    ChangeMemoryImportance {
        expected_revision,
        idempotency_key: key.into(),
        reason: "Explicit relevance decision".into(),
    }
}
fn importance_operation(input: ChangeMemoryImportance, important: bool) -> FencedMemoryMutation {
    if important {
        FencedMemoryMutation::Promote(input)
    } else {
        FencedMemoryMutation::Demote(input)
    }
}
async fn set_importance(
    original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
    admission: &graphit_auth::ProjectWriteAdmission<'_>,
    id: MemoryId,
    input: ChangeMemoryImportance,
    important: bool,
) -> Result<Memory, MemoryStoreError> {
    if important {
        NativeMemoryStore::fenced_promote(original, admission, id, input, write_limits()).await
    } else {
        NativeMemoryStore::fenced_demote(original, admission, id, input, write_limits()).await
    }
}
fn assert_importance_delta(before: &Memory, after: &Memory, important: bool, actor: &str) {
    let mut expected = before.clone();
    expected.attributes.important = important;
    expected.revision += 1;
    expected.updated_by = actor.into();
    expected.updated_at = after.updated_at;
    assert!(after.updated_at >= before.updated_at);
    assert_eq!(after, &expected);
}
fn admitted_actor(write: &graphit_auth::ProjectWriteAdmission<'_>) -> String {
    let mut bytes = [0; 64];
    write.actor().unwrap().write_identifier(&mut bytes);
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fenced_importance_transitions_noop_cas_and_historical_identity() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(
        f, ctx, issuer, authority, grant, read, write, bearer, credential, admin
    );
    let actor = admitted_actor(&write);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let run2 = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let bearer2 = issuer
        .issue(
            &parent,
            &run2,
            &credential,
            &[
                Permission::Read(Module::Memory),
                Permission::WriteFencedMemory,
            ],
            &f.scope(),
        )
        .unwrap();
    let grant2 = issuer
        .verify(bearer2.expose_for_transport(), &credential, f.scope(), &ctx)
        .unwrap();
    let write2 = authority
        .admit_write(&grant2, &ctx, ProjectWriteOperation::FencedMemory)
        .unwrap();
    let actor2 = admitted_actor(&write2);
    assert_ne!(actor, actor2);
    for (i, (mandatory, important)) in [(false, false), (false, true), (true, false), (true, true)]
        .into_iter()
        .enumerate()
    {
        let mut authored = input(&format!("importance-seed-{i}"));
        authored.attributes = MemoryAttributes {
            classification: MemoryClass::Correction,
            mandatory,
            important,
        };
        authored.tags = vec!["literal".into(), "literal".into()];
        authored.references = Some(vec![reference()]);
        let seed =
            NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
                .await
                .unwrap();
        assert_eq!(seed.created_by, actor);
        let first_input = importance_attempt(&format!("first-{i}"), 1);
        let first = set_importance(
            ctx.snapshot(),
            &write,
            seed.id,
            first_input.clone(),
            !important,
        )
        .await
        .unwrap();
        assert_importance_delta(&seed, &first, !important, &actor);
        let second_input = importance_attempt(&format!("second-{i}"), 2);
        let second = set_importance(ctx.snapshot(), &write, seed.id, second_input, important)
            .await
            .unwrap();
        assert_importance_delta(&first, &second, important, &actor);
        let h = NativeMemoryStore::history(ctx.snapshot(), &read, seed.id, read_limits())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(h.revisions.len(), 2);
        assert_eq!(h.revisions[0].before, seed);
        assert_eq!(h.revisions[0].after, first);
        assert_eq!(h.revisions[1].after, second);
        assert!(matches!(h.revisions[0].operation, MemoryMutation::Promote(_)) == !important);
        assert!(matches!(h.revisions[1].operation, MemoryMutation::Demote(_)) == !important);
        let physical = sdk::all(&sdk::pin(&f).await).await;
        let tags: Vec<serde_json::Value> = (0..physical.num_rows())
            .filter(|r| {
                sdk::string(&physical, 1, *r) == "revision"
                    && sdk::string(&physical, 2, *r) == seed.id.to_string()
            })
            .map(|r| serde_json::from_str(sdk::string(&physical, 17, r)).unwrap())
            .collect();
        assert_eq!(tags.len(), 2);
        assert!(tags.iter().any(|v| v["operation"].get("Promote").is_some()));
        assert!(tags.iter().any(|v| v["operation"].get("Demote").is_some()));
        let noop = importance_attempt(&format!("noop-{i}"), 3);
        assert_eq!(
            set_importance(ctx.snapshot(), &write, seed.id, noop.clone(), important)
                .await
                .unwrap(),
            second
        );
        assert!(
            NativeMemoryStore::reobserve_fenced_mutation(
                ctx.snapshot(),
                &write,
                seed.id,
                importance_operation(noop.clone(), important),
                write_limits()
            )
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(sdk::pin(&f).await.version(), h.generation.sdk_version);
        let after_noop = NativeMemoryStore::history(ctx.snapshot(), &read, seed.id, read_limits())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after_noop, h);
        let mut stale = noop.clone();
        stale.expected_revision = 2;
        assert!(matches!(
            set_importance(ctx.snapshot(), &write, seed.id, stale, important).await,
            Err(MemoryStoreError::RevisionConflict {
                expected: 2,
                actual: 3
            })
        ));
        let a = importance_attempt(&format!("race-a-{i}"), 3);
        let b = importance_attempt(&format!("race-b-{i}"), 3);
        let (ra, rb) = tokio::join!(
            set_importance(ctx.snapshot(), &write, seed.id, a, !important),
            set_importance(ctx.snapshot(), &write2, seed.id, b, !important)
        );
        let (winner, loser, winner_actor) = if let Ok(v) = ra {
            (v, rb, &actor)
        } else {
            (rb.unwrap(), ra, &actor2)
        };
        assert!(matches!(
            loser,
            Err(MemoryStoreError::RevisionConflict {
                expected: 3,
                actual: 4
            })
        ));
        assert_importance_delta(&second, &winner, !important, winner_actor);
        let version = sdk::pin(&f).await.version();
        assert_eq!(
            set_importance(
                ctx.snapshot(),
                &write,
                seed.id,
                first_input.clone(),
                !important
            )
            .await
            .unwrap(),
            first
        );
        assert!(matches!(
            set_importance(
                ctx.snapshot(),
                &write,
                seed.id,
                first_input.clone(),
                important
            )
            .await,
            Err(MemoryStoreError::IdempotencyConflict)
        ));
        let mut changed = first_input.clone();
        changed.reason = "Changed reason".into();
        assert!(matches!(
            set_importance(ctx.snapshot(), &write, seed.id, changed, !important).await,
            Err(MemoryStoreError::IdempotencyConflict)
        ));
        let mut changed = first_input.clone();
        changed.expected_revision = 4;
        assert!(matches!(
            set_importance(ctx.snapshot(), &write, seed.id, changed, !important).await,
            Err(MemoryStoreError::IdempotencyConflict)
        ));
        let fresh_cancel = Cancellation::default();
        let fresh = request(&fresh_cancel);
        let fresh_grant = issuer
            .verify(
                bearer.expose_for_transport(),
                &credential,
                f.scope(),
                &fresh,
            )
            .unwrap();
        let fresh_write = authority
            .admit_write(&fresh_grant, &fresh, ProjectWriteOperation::FencedMemory)
            .unwrap();
        let observed = NativeMemoryStore::reobserve_fenced_mutation(
            fresh.snapshot(),
            &fresh_write,
            seed.id,
            importance_operation(first_input, !important),
            write_limits(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(observed, h.revisions[0]);
        assert_eq!(sdk::pin(&f).await.version(), version);
        // A prior no-op created no receipt: its key is available for a new real transition.
        let mut reused = noop;
        reused.expected_revision = 4;
        let final_state = set_importance(ctx.snapshot(), &write, seed.id, reused, important)
            .await
            .unwrap();
        assert_importance_delta(&winner, &final_state, important, &actor);
        let final_history =
            NativeMemoryStore::history(ctx.snapshot(), &read, seed.id, read_limits())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(final_history.revisions.len(), 4);
        assert_eq!(final_history.current, final_state);
        assert_eq!(final_history.creation.memory, seed);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fenced_importance_named_noop_preserves_inheritance_and_project_isolation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    let seed = {
        dream!(
            f,
            ctx,
            issuer,
            authority,
            grant,
            read,
            write,
            _bearer,
            _credential,
            _admin
        );
        let mut authored = input("named-importance");
        authored.attributes.mandatory = true;
        authored.attributes.important = true;
        authored.references = Some(vec![reference()]);
        let memory =
            NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
                .await
                .unwrap();
        assert_eq!(
            NativeMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
                .await
                .unwrap()
                .unwrap()
                .memory,
            memory
        );
        memory
    };
    let default_version = sdk::pin(&f).await.version();
    let paths = graphit_core::project_paths::ProjectPaths::at(
        &f.root,
        &graphit_core::project_paths::InstallationPaths::resolve(
            f.project.brand().clone(),
            &f.root,
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap(),
    )
    .unwrap();
    f.version =
        VersionContext::select(&paths, Checkout::NonGit, "importance-branch", None, "").unwrap();
    dream!(
        f,
        ctx,
        issuer,
        authority,
        grant,
        read,
        write,
        _bearer,
        _credential,
        _admin
    );
    let inherited = NativeMemoryStore::history(ctx.snapshot(), &read, seed.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        inherited.generation.inherited_target.as_deref(),
        Some("importance-branch")
    );
    let noop = importance_attempt("named-noop", 1);
    assert_eq!(
        set_importance(ctx.snapshot(), &write, seed.id, noop.clone(), true)
            .await
            .unwrap(),
        seed
    );
    assert!(
        NativeMemoryStore::reobserve_fenced_mutation(
            ctx.snapshot(),
            &write,
            seed.id,
            FencedMemoryMutation::Promote(noop),
            write_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
    let still_inherited = NativeMemoryStore::history(ctx.snapshot(), &read, seed.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(still_inherited, inherited);
    assert!(matches!(
        set_importance(
            ctx.snapshot(),
            &write,
            seed.id,
            importance_attempt("named-stale", 2),
            true
        )
        .await,
        Err(MemoryStoreError::RevisionConflict {
            expected: 2,
            actual: 1
        })
    ));
    let changed = set_importance(
        ctx.snapshot(),
        &write,
        seed.id,
        importance_attempt("named-demote", 1),
        false,
    )
    .await
    .unwrap();
    assert_importance_delta(&seed, &changed, false, &admitted_actor(&write));
    let named = NativeMemoryStore::history(ctx.snapshot(), &read, seed.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(named.generation.inherited_target.is_none());
    assert_eq!(named.revisions.len(), 1);
    assert_eq!(named.revisions[0].operation_version, "importance-branch");
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let store = LanceBranchStore::open(&uri, sdk::schema(), None)
        .await
        .unwrap();
    let default = store.capture_target(None).await.unwrap();
    assert_eq!(default.version(), default_version);
    assert_eq!(sdk::all(&default).await.num_rows(), 3);
    let foreign = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    dream!(
        foreign,
        ctx,
        foreign_issuer,
        foreign_authority,
        foreign_grant,
        foreign_read,
        foreign_write,
        _foreign_bearer,
        _foreign_credential,
        _foreign_admin
    );
    assert!(matches!(
        set_importance(
            ctx.snapshot(),
            &foreign_write,
            seed.id,
            importance_attempt("foreign", 1),
            true
        )
        .await,
        Err(MemoryStoreError::NotFound)
    ));
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &foreign_read, seed.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !foreign
            .project
            .runtime_root()
            .join("memory/index.lance")
            .exists()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fenced_importance_denies_wrong_admission_original_cancel_deadline_and_revoke() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(
        f, ctx, issuer, authority, grant, read, write, bearer, credential, admin
    );
    let mut authored = input("authority-importance");
    authored.attributes.important = true;
    let seed = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let version = sdk::pin(&f).await.version();
    let (ordinary_bearer, ordinary_credential, _run, _admin) = f.issue(&issuer, &ctx);
    let ordinary_grant = issuer
        .verify(
            ordinary_bearer.expose_for_transport(),
            &ordinary_credential,
            f.scope(),
            &ctx,
        )
        .unwrap();
    let ordinary = authority
        .admit_write(&ordinary_grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let task = authority
        .admit_write(&ordinary_grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    for desired in [false, true] {
        for wrong in [&ordinary, &task] {
            assert!(matches!(
                set_importance(
                    ctx.snapshot(),
                    wrong,
                    seed.id,
                    importance_attempt("wrong", 1),
                    desired
                )
                .await,
                Err(MemoryStoreError::WrongAdmission)
            ));
            assert!(matches!(
                NativeMemoryStore::reobserve_fenced_mutation(
                    ctx.snapshot(),
                    wrong,
                    seed.id,
                    importance_operation(importance_attempt("wrong", 1), desired),
                    write_limits()
                )
                .await,
                Err(MemoryStoreError::WrongAdmission)
            ));
        }
    }
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    for desired in [false, true] {
        assert!(matches!(
            set_importance(
                other.snapshot(),
                &write,
                seed.id,
                importance_attempt("foreign-original", 1),
                desired
            )
            .await,
            Err(MemoryStoreError::DifferentRequest)
        ));
    }
    let other_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &other,
        )
        .unwrap();
    let other_write = authority
        .admit_write(&other_grant, &other, ProjectWriteOperation::FencedMemory)
        .unwrap();
    other_cancel.cancel();
    for desired in [false, true] {
        assert!(matches!(
            set_importance(
                other.snapshot(),
                &other_write,
                seed.id,
                importance_attempt("cancel", 1),
                desired
            )
            .await,
            Err(MemoryStoreError::Request(_))
        ));
    }
    let deadline_cancel = Cancellation::default();
    let deadline = request_until(&deadline_cancel, Instant::now() + Duration::from_secs(1));
    let deadline_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &deadline,
        )
        .unwrap();
    let deadline_write = authority
        .admit_write(
            &deadline_grant,
            &deadline,
            ProjectWriteOperation::FencedMemory,
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    for desired in [false, true] {
        assert!(matches!(
            set_importance(
                deadline.snapshot(),
                &deadline_write,
                seed.id,
                importance_attempt("deadline", 1),
                desired
            )
            .await,
            Err(MemoryStoreError::Request(_))
        ));
    }
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    for desired in [false, true] {
        assert!(matches!(
            set_importance(
                ctx.snapshot(),
                &write,
                seed.id,
                importance_attempt("revoke", 1),
                desired
            )
            .await,
            Err(MemoryStoreError::Auth(_))
        ));
    }
    assert_eq!(sdk::pin(&f).await.version(), version);
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, seed.id, read_limits())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fenced_importance_durable_event_rejects_preserved_field_noop_and_codec_corruption() {
    for mode in 0..5 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        dream!(
            f,
            ctx,
            issuer,
            authority,
            grant,
            read,
            write,
            _bearer,
            _credential,
            _admin
        );
        let mut authored = input("corrupt-importance");
        authored.attributes.mandatory = true;
        authored.references = Some(vec![reference()]);
        let seed =
            NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
                .await
                .unwrap();
        NativeMemoryStore::fenced_promote(
            ctx.snapshot(),
            &write,
            seed.id,
            importance_attempt("real-promote", 1),
            write_limits(),
        )
        .await
        .unwrap();
        let pin = sdk::pin(&f).await;
        let all = sdk::all(&pin).await;
        let event = sdk::select(&all, "revision");
        let mut payload: serde_json::Value =
            serde_json::from_str(sdk::string(&event, 17, 0)).unwrap();
        match mode {
            0 => payload["before"]["attributes"]["mandatory"] = false.into(),
            1 => payload["before"]["body"] = "Changed valid predecessor".into(),
            2 => payload["before"]["references"] = serde_json::json!([]),
            3 => payload["before"]["attributes"]["important"] = true.into(),
            _ => payload["operation"]["Promote"]["mandatory"] = false.into(),
        }
        let corrupt = sdk::text_column(&event, 17, vec![serde_json::to_string(&payload).unwrap()]);
        sdk::replace(&pin, corrupt).await;
        assert!(
            NativeMemoryStore::history(ctx.snapshot(), &read, seed.id, read_limits())
                .await
                .is_err()
        );
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, seed.id, read_limits())
                .await
                .is_err()
        );
        assert!(
            NativeMemoryStore::reobserve_fenced_mutation(
                ctx.snapshot(),
                &write,
                seed.id,
                FencedMemoryMutation::Promote(importance_attempt("real-promote", 1)),
                write_limits()
            )
            .await
            .is_err()
        );
    }
}
