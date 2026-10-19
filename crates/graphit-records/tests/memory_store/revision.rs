use super::*;

fn patch(key: &str, revision: u64) -> UpdateMemory {
    UpdateMemory {
        expected_revision: revision,
        idempotency_key: key.into(),
        reason: " Explicit reason β \n".into(),
        title: None,
        body: None,
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    }
}
fn deletion(key: &str, revision: u64) -> DeleteMemory {
    DeleteMemory {
        expected_revision: revision,
        idempotency_key: key.into(),
        reason: "Explicit deletion".into(),
    }
}
fn reference() -> MemoryReference {
    MemoryReference {
        target_kind: ReferenceTargetKind::Task,
        scope: MemoryReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap()),
        id: "logical-task".into(),
        context: None,
        relation: "supports".into(),
        field: Some("body".into()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mutation_apis_deny_foreign_original_module_fence_and_revoke() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, credential, admin
    );
    let initial =
        NativeMemoryStore::create(ctx.snapshot(), &write, input("authority"), write_limits())
            .await
            .unwrap();
    let mut p = patch("attempt", 1);
    p.title = Some("New title".into());
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        NativeMemoryStore::update(
            other.snapshot(),
            &write,
            initial.id,
            p.clone(),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    assert!(matches!(
        NativeMemoryStore::history(other.snapshot(), &read, initial.id, read_limits()).await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    let task_write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let fenced = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
        .unwrap();
    for wrong in [&task_write, &fenced] {
        assert!(matches!(
            NativeMemoryStore::update(ctx.snapshot(), wrong, initial.id, p.clone(), write_limits())
                .await,
            Err(MemoryStoreError::WrongAdmission)
        ));
        assert!(matches!(
            NativeMemoryStore::delete(
                ctx.snapshot(),
                wrong,
                initial.id,
                deletion("denied", 1),
                write_limits()
            )
            .await,
            Err(MemoryStoreError::WrongAdmission)
        ));
        assert!(matches!(
            NativeMemoryStore::reobserve_mutation(
                ctx.snapshot(),
                wrong,
                initial.id,
                MemoryMutation::Update(p.clone()),
                write_limits()
            )
            .await,
            Err(MemoryStoreError::WrongAdmission)
        ));
    }
    let task_read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::history(ctx.snapshot(), &task_read, initial.id, read_limits()).await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let version = sdk::pin(&f).await.version();
    let fresh = issuer
        .verify(
            bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &other,
        )
        .unwrap();
    let fw = authority
        .admit_write(&fresh, &other, ProjectWriteOperation::Memory)
        .unwrap();
    other_cancel.cancel();
    assert!(
        NativeMemoryStore::update(other.snapshot(), &fw, initial.id, p.clone(), write_limits())
            .await
            .is_err()
    );
    let project_cancel = Cancellation::default();
    let project_ctx = request(&project_cancel);
    let project_b = Fixture::new_for(&project_ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    admissions!(
        project_b,
        project_ctx,
        issuer_b,
        authority_b,
        grant_b,
        read_b,
        write_b,
        _bearer_b,
        _credential_b,
        _admin_b
    );
    assert!(matches!(
        NativeMemoryStore::update(
            project_ctx.snapshot(),
            &write_b,
            initial.id,
            p.clone(),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::NotFound)
    ));
    assert!(
        NativeMemoryStore::history(project_ctx.snapshot(), &read_b, initial.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert!(!project_b.project.runtime_root().join("memory").exists());
    let deadline_cancel = Cancellation::default();
    let deadline = request_until(
        &deadline_cancel,
        Instant::now() + Duration::from_millis(200),
    );
    let gd = issuer
        .verify(
            bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &deadline,
        )
        .unwrap();
    let wd = authority
        .admit_write(&gd, &deadline, ProjectWriteOperation::Memory)
        .unwrap();
    let rd = authority
        .admit_read(&gd, &deadline, ProjectReadModule::Memory)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(220)).await;
    assert!(
        NativeMemoryStore::update(
            deadline.snapshot(),
            &wd,
            initial.id,
            p.clone(),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeMemoryStore::history(deadline.snapshot(), &rd, initial.id, read_limits())
            .await
            .is_err()
    );
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(
        NativeMemoryStore::delete(
            ctx.snapshot(),
            &write,
            initial.id,
            deletion("denied", 1),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeMemoryStore::reobserve_mutation(
            ctx.snapshot(),
            &write,
            initial.id,
            MemoryMutation::Update(p),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
            .await
            .is_err()
    );
    assert_eq!(sdk::pin(&f).await.version(), version);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inherited_revision_changes_only_named_target_and_retains_origin_versions() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (a, ar, _, _admin) = f.issue(&issuer, &ctx);
    let authority = ProjectRecordAuthority::new(&issuer);
    let initial = {
        let g = issuer
            .verify(a.expose_for_transport(), &ar, f.scope(), &ctx)
            .unwrap();
        let w = authority
            .admit_write(&g, &ctx, ProjectWriteOperation::Memory)
            .unwrap();
        NativeMemoryStore::create(ctx.snapshot(), &w, input("inherited"), write_limits())
            .await
            .unwrap()
    };
    let paths = graphit_core::project_paths::ProjectPaths::at(
        &f.root,
        &graphit_core::project_paths::InstallationPaths::resolve(
            graphit_core::brand::Brand::default(),
            &f.root,
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap(),
    )
    .unwrap();
    f.version =
        VersionContext::select(&paths, Checkout::NonGit, "revision-feature", None, "").unwrap();
    let (b, br, _, _admin2) = f.issue_for(&issuer, &ctx, f.scope());
    {
        let g = issuer
            .verify(b.expose_for_transport(), &br, f.scope(), &ctx)
            .unwrap();
        let r = authority
            .admit_read(&g, &ctx, ProjectReadModule::Memory)
            .unwrap();
        let w = authority
            .admit_write(&g, &ctx, ProjectWriteOperation::Memory)
            .unwrap();
        let before = sdk::pin(&f).await;
        assert!(before.inherited_target().is_some());
        assert!(
            NativeMemoryStore::reobserve_mutation(
                ctx.snapshot(),
                &w,
                initial.id,
                MemoryMutation::Update(patch("unknown", 1)),
                write_limits()
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(matches!(
            NativeMemoryStore::update(
                ctx.snapshot(),
                &w,
                initial.id,
                patch("noop", 1),
                write_limits()
            )
            .await,
            Err(MemoryStoreError::NoChange)
        ));
        assert!(sdk::pin(&f).await.inherited_target().is_some());
        let mut p = patch("named-update", 1);
        p.title = Some("Named current title".into());
        let updated = NativeMemoryStore::update(ctx.snapshot(), &w, initial.id, p, write_limits())
            .await
            .unwrap();
        assert_ne!(updated.updated_by, initial.created_by);
        let h = NativeMemoryStore::history(ctx.snapshot(), &r, initial.id, read_limits())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(h.creation.operation_version, "main");
        assert_eq!(h.revisions[0].operation_version, "revision-feature");
        assert_eq!(h.revisions[0].before, initial);
        assert_eq!(h.current, updated);
        assert!(h.generation.inherited_target.is_none());
        assert_eq!(h.revisions[0].actor, updated.updated_by);
        NativeMemoryStore::delete(
            ctx.snapshot(),
            &w,
            initial.id,
            deletion("named-delete", 2),
            write_limits(),
        )
        .await
        .unwrap();
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &r, initial.id, read_limits())
                .await
                .unwrap()
                .is_none()
        );
    }
    f.version = VersionContext::select(&paths, Checkout::NonGit, "main", None, "").unwrap();
    let g = issuer
        .verify(a.expose_for_transport(), &ar, f.scope(), &ctx)
        .unwrap();
    let r = authority
        .admit_read(&g, &ctx, ProjectReadModule::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &r, initial.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        initial
    );
    assert!(
        NativeMemoryStore::history(ctx.snapshot(), &r, initial.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .revisions
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn complete_chain_rejects_actual_missing_duplicate_and_valid_transition_fork() {
    for mode in 0..3 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
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
        let m = NativeMemoryStore::create(ctx.snapshot(), &write, input("chain"), write_limits())
            .await
            .unwrap();
        let mut p = patch("revision", 1);
        p.body = Some("New body".into());
        NativeMemoryStore::update(ctx.snapshot(), &write, m.id, p, write_limits())
            .await
            .unwrap();
        let pin = sdk::pin(&f).await;
        let batch = sdk::all(&pin).await;
        let event = sdk::select(&batch, "revision");
        let replacement = match mode {
            0 => vec![sdk::select(&batch, "head"), sdk::select(&batch, "creation")],
            1 => vec![batch.clone(), event],
            _ => {
                let mut payload: serde_json::Value =
                    serde_json::from_str(sdk::string(&event, 17, 0)).unwrap();
                payload["before"]["body"] = "Different nonblank predecessor".into();
                let changed =
                    sdk::text_column(&event, 17, vec![serde_json::to_string(&payload).unwrap()]);
                sdk::replace(&pin, changed).await;
                vec![]
            }
        };
        if !replacement.is_empty() {
            let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
                .unwrap()
                .to_string();
            let store =
                graphit_data::lance_module_store::LanceModuleStore::open(&uri, sdk::schema())
                    .await
                    .unwrap();
            store
                .snapshot()
                .prepare_replace(replacement)
                .await
                .unwrap()
                .commit()
                .await
                .unwrap();
        }
        let error = NativeMemoryStore::history(ctx.snapshot(), &read, m.id, read_limits())
            .await
            .unwrap_err();
        if mode == 1 {
            assert!(matches!(
                error,
                MemoryStoreError::Invalid("duplicate Memory row key")
            ));
        } else if mode == 2 {
            assert!(matches!(
                error,
                MemoryStoreError::Invalid("Memory history fork")
            ));
        }
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, m.id, read_limits())
                .await
                .is_err()
        );
        assert!(
            NativeMemoryStore::create(ctx.snapshot(), &write, input("chain"), write_limits())
                .await
                .is_err()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revisions_preserve_omission_flags_refs_chain_and_original_attempt_outcomes() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, credential, _admin
    );
    let mut source = input("creation");
    source.attributes.important = true;
    source.references = Some(vec![reference()]);
    let initial = NativeMemoryStore::create(ctx.snapshot(), &write, source.clone(), write_limits())
        .await
        .unwrap();
    let mut p = patch(" first ' β\n", 1);
    p.classification = Some(MemoryClass::Convention);
    p.references = Some(initial.references.clone());
    p.important = Some(false);
    let second = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        initial.id,
        p.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(second.body, initial.body);
    assert_eq!(second.title, initial.title);
    assert_eq!(second.tags, initial.tags);
    assert_eq!(second.references, initial.references);
    assert_eq!(second.reference_generation, 1);
    assert!(second.attributes.mandatory);
    assert!(!second.attributes.important);
    let mut q = patch("second", 2);
    q.body = Some(" Changed body \nβ".into());
    q.tags = Some(vec![]);
    q.references = Some(vec![]);
    q.mandatory = Some(false);
    q.important = Some(true);
    let third = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        initial.id,
        q.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(third.revision, 3);
    assert_eq!(third.reference_generation, 2);
    assert!(third.references.is_empty());
    assert!(third.tags.is_empty());
    assert_eq!(third.created_by, initial.created_by);
    assert_eq!(third.created_at, initial.created_at);
    assert!(!third.attributes.mandatory);
    assert!(third.attributes.important);
    let MemoryListRead::Published { memories, .. } =
        NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
            .await
            .unwrap()
    else {
        panic!("published catalogue")
    };
    assert_eq!(memories, vec![third.clone()]);
    let detail = NativeMemoryStore::get(ctx.snapshot(), &read, initial.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.memory, third);
    assert_eq!(detail.creation.memory, initial);
    let history = NativeMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(!history.deleted);
    assert_eq!(history.revisions.len(), 2);
    assert_eq!(history.revisions[0].before, initial);
    assert_eq!(history.revisions[0].after, second);
    assert_eq!(
        history.revisions[0].operation,
        MemoryMutation::Update(p.clone())
    );
    assert_eq!(history.revisions[1].before, second);
    assert_eq!(history.revisions[1].after, third);
    let version = sdk::pin(&f).await.version();
    assert_eq!(
        NativeMemoryStore::update(
            ctx.snapshot(),
            &write,
            initial.id,
            p.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        second
    );
    assert_eq!(
        NativeMemoryStore::create(ctx.snapshot(), &write, source.clone(), write_limits())
            .await
            .unwrap(),
        initial
    );
    assert_eq!(
        NativeMemoryStore::reobserve_create(ctx.snapshot(), &write, source, write_limits())
            .await
            .unwrap(),
        MemoryCreateObservation::Found(initial.clone())
    );
    let fresh = request(&cancel);
    let fg = issuer
        .verify(
            bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &fresh,
        )
        .unwrap();
    let fw = authority
        .admit_write(&fg, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::reobserve_mutation(
            fresh.snapshot(),
            &fw,
            initial.id,
            MemoryMutation::Update(p.clone()),
            write_limits()
        )
        .await
        .unwrap(),
        Some(history.revisions[0].clone())
    );
    let mut changed = p.clone();
    changed.reason = "different valid reason".into();
    assert!(matches!(
        NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, changed, write_limits())
            .await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    let mut collision = patch("creation", 3);
    collision.title = Some("Another title".into());
    assert!(matches!(
        NativeMemoryStore::update(
            ctx.snapshot(),
            &write,
            initial.id,
            collision,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    assert!(matches!(
        NativeMemoryStore::create(
            ctx.snapshot(),
            &write,
            input(&p.idempotency_key),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    assert!(matches!(
        NativeMemoryStore::update(
            ctx.snapshot(),
            &write,
            initial.id,
            patch("noop", 3),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::NoChange)
    ));
    let mut invalid = patch("invalid", 3);
    invalid.body = Some(" \n".into());
    assert!(matches!(
        NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, invalid, write_limits())
            .await,
        Err(MemoryStoreError::Invalid(_))
    ));
    let mut stale = patch("stale", 1);
    stale.body = Some("Valid body".into());
    assert!(matches!(
        NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, stale, write_limits()).await,
        Err(MemoryStoreError::RevisionConflict {
            expected: 1,
            actual: 3
        })
    ));
    let mut limit = write_limits();
    limit.max_row_bytes = NonZeroUsize::new(1).unwrap();
    let mut failed = patch("archive-budget", 3);
    failed.title = Some("Valid new title".into());
    assert!(matches!(
        NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, failed, limit).await,
        Err(MemoryStoreError::Limit(_))
    ));
    assert_eq!(sdk::pin(&f).await.version(), version);
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, initial.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        third
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn protected_explicit_delete_retains_history_and_new_attempts_never_resurrect() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
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
    let mut source = input("created");
    source.attributes.important = true;
    source.references = Some(vec![reference()]);
    let initial = NativeMemoryStore::create(ctx.snapshot(), &write, source.clone(), write_limits())
        .await
        .unwrap();
    let mut p = patch("updated", 1);
    p.body = Some("Final full body".into());
    let second = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        initial.id,
        p.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    let d = deletion("deleted", 2);
    let predelete_version = sdk::pin(&f).await.version();
    let mut archive_limit = write_limits();
    archive_limit.max_row_bytes = NonZeroUsize::new(1).unwrap();
    assert!(matches!(
        NativeMemoryStore::delete(ctx.snapshot(), &write, initial.id, d.clone(), archive_limit)
            .await,
        Err(MemoryStoreError::Limit(_))
    ));
    assert_eq!(sdk::pin(&f).await.version(), predelete_version);
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, initial.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        second
    );
    let archived = NativeMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(!archived.deleted);
    assert_eq!(archived.revisions.len(), 1);

    let final_state = NativeMemoryStore::delete(
        ctx.snapshot(),
        &write,
        initial.id,
        d.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(final_state.revision, 3);
    assert_eq!(final_state.body, second.body);
    assert_eq!(final_state.attributes, second.attributes);
    assert!(final_state.attributes.mandatory && final_state.attributes.important);
    assert!(final_state.references.is_empty());
    assert_eq!(final_state.reference_generation, 2);
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, initial.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        published(
            NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
                .await
                .unwrap()
        )
        .1
        .is_empty()
    );
    let MemoryListRead::Published { memories, .. } =
        NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
            .await
            .unwrap()
    else {
        panic!("published")
    };
    assert!(memories.is_empty());
    let h = NativeMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(h.deleted);
    assert_eq!(h.current, final_state);
    assert_eq!(h.creation.memory, initial);
    assert_eq!(h.revisions[1].before, second);
    assert_eq!(h.revisions[1].after, final_state);
    let version = sdk::pin(&f).await.version();
    assert_eq!(
        NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, p, write_limits())
            .await
            .unwrap(),
        second
    );
    assert_eq!(
        NativeMemoryStore::delete(
            ctx.snapshot(),
            &write,
            initial.id,
            d.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        final_state
    );
    assert_eq!(
        NativeMemoryStore::reobserve_mutation(
            ctx.snapshot(),
            &write,
            initial.id,
            MemoryMutation::Delete(d),
            write_limits()
        )
        .await
        .unwrap(),
        Some(h.revisions[1].clone())
    );
    assert_eq!(
        NativeMemoryStore::create(ctx.snapshot(), &write, source.clone(), write_limits())
            .await
            .unwrap(),
        initial
    );
    assert_eq!(
        NativeMemoryStore::reobserve_create(ctx.snapshot(), &write, source, write_limits())
            .await
            .unwrap(),
        MemoryCreateObservation::Found(initial.clone())
    );
    for revision in [1, 3] {
        let mut next = patch(&format!("new-update-{revision}"), revision);
        next.body = Some("Never published".into());
        assert!(matches!(
            NativeMemoryStore::update(ctx.snapshot(), &write, initial.id, next, write_limits())
                .await,
            Err(MemoryStoreError::NotFound)
        ));
        assert!(matches!(
            NativeMemoryStore::delete(
                ctx.snapshot(),
                &write,
                initial.id,
                deletion(&format!("new-delete-{revision}"), revision),
                write_limits()
            )
            .await,
            Err(MemoryStoreError::NotFound)
        ));
    }
    assert_eq!(sdk::pin(&f).await.version(), version);
    let new = NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("new-identity"),
        write_limits(),
    )
    .await
    .unwrap();
    assert_ne!(new.id, initial.id);
    let h2 = NativeMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h2.current, h.current);
    assert_eq!(h2.revisions, h.revisions);
    assert_eq!(h2.creation, h.creation);
    assert!(h2.deleted);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revision_cas_two_actual_actors_has_one_winner_and_no_fork() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, credential, _admin
    );
    let m = NativeMemoryStore::create(ctx.snapshot(), &write, input("race"), write_limits())
        .await
        .unwrap();
    let (b, _run) = f.issue_registered_for(&issuer, &ctx, f.scope(), &credential);
    let bg = issuer
        .verify(b.expose_for_transport(), &credential, f.scope(), &ctx)
        .unwrap();
    let bw = authority
        .admit_write(&bg, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let mut p = patch("actor-a", 1);
    p.title = Some("A".into());
    let mut q = patch("actor-b", 1);
    q.title = Some("B".into());
    let (a, b) = tokio::join!(
        NativeMemoryStore::update(ctx.snapshot(), &write, m.id, p, write_limits()),
        NativeMemoryStore::update(ctx.snapshot(), &bw, m.id, q, write_limits())
    );
    let winner = match (a, b) {
        (
            Ok(v),
            Err(MemoryStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }),
        )
        | (
            Err(MemoryStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }),
            Ok(v),
        ) => v,
        pair => panic!("one CAS winner: {pair:?}"),
    };
    let h = NativeMemoryStore::history(ctx.snapshot(), &read, m.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.current, winner);
    assert_eq!(h.revisions.len(), 1);
    assert_eq!(h.revisions[0].before, m);
    assert_eq!(sdk::all(&sdk::pin(&f).await).await.num_rows(), 5);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_new_mutations_and_reobservation_do_not_create_a_corpus() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
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
    let id = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
    let mut p = patch("missing", 1);
    p.title = Some("New".into());
    assert!(matches!(
        NativeMemoryStore::update(ctx.snapshot(), &write, id, p.clone(), write_limits()).await,
        Err(MemoryStoreError::NotFound)
    ));
    assert!(matches!(
        NativeMemoryStore::delete(
            ctx.snapshot(),
            &write,
            id,
            deletion("missing-delete", 1),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::NotFound)
    ));
    assert!(
        NativeMemoryStore::reobserve_mutation(
            ctx.snapshot(),
            &write,
            id,
            MemoryMutation::Update(p),
            write_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        NativeMemoryStore::history(ctx.snapshot(), &read, id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
    assert!(!f.project.runtime_root().join("memory").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_missing_event_or_header_cannot_hide_behind_tombstone() {
    for column in [1, 7, 16, 17] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
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
        let m = NativeMemoryStore::create(ctx.snapshot(), &write, input("corrupt"), write_limits())
            .await
            .unwrap();
        let d = deletion("delete", 1);
        NativeMemoryStore::delete(ctx.snapshot(), &write, m.id, d.clone(), write_limits())
            .await
            .unwrap();
        let pin = sdk::pin(&f).await;
        let batch = sdk::all(&pin).await;
        let revision = sdk::select(&batch, "revision");
        let corrupt = sdk::text_column(
            &revision,
            column,
            vec![if column == 1 {
                "missing-event".into()
            } else if column == 16 {
                "[] changed".into()
            } else {
                "corrupt".into()
            }],
        );
        sdk::replace(&pin, corrupt).await;
        assert!(
            NativeMemoryStore::history(ctx.snapshot(), &read, m.id, read_limits())
                .await
                .is_err()
        );
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, m.id, read_limits())
                .await
                .is_err()
        );
        assert!(
            NativeMemoryStore::reobserve_mutation(
                ctx.snapshot(),
                &write,
                m.id,
                MemoryMutation::Delete(d),
                write_limits()
            )
            .await
            .is_err()
        );
    }
}
