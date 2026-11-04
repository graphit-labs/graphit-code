use super::*;
use graphit_auth::Permission;
use graphit_core::module_settings::Module;
use graphit_hub::native::Role;
use std::num::NonZeroU32;

fn listed(value: MemoryListRead) -> (MemoryGeneration, Vec<Memory>) {
    match value {
        MemoryListRead::Published {
            generation,
            memories,
        } => (generation, memories),
        MemoryListRead::NoCorpus => panic!("expected published catalogue"),
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalogue_full_content_flags_filters_and_current_order() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let mut values = vec![];
    for (i, (mandatory, important)) in [
        (true, true),
        (true, false),
        (false, true),
        (false, false),
        (false, false),
        (false, true),
    ]
    .into_iter()
    .enumerate()
    {
        let mut v = input(&format!("catalogue-{i}"));
        v.attributes = MemoryAttributes {
            classification: if i == 5 {
                MemoryClass::Skill
            } else {
                MemoryClass::Fact
            },
            mandatory,
            important,
        };
        v.tags = vec![" Tag'β ".into()];
        v.body = format!("Full body {i}: {}", "content".repeat(1024));
        values.push(
            NativeMemoryStore::create(ctx.snapshot(), &write, v, write_limits())
                .await
                .unwrap(),
        );
    }
    // Current-only catalogue accepts a coherent revised head without scanning audits.
    // Full chain validation remains the get/history responsibility.
    let pin = sdk::pin(&f).await;
    let batch = sdk::all(&pin).await;
    let base = values.iter().map(|m| m.created_at).max().unwrap();
    let offsets = [2, 8, 999, 10_000, 10_000, 10];
    for (i, m) in values.iter_mut().enumerate() {
        m.revision = 2;
        m.updated_at = base + offsets[i];
        m.updated_by = "c".repeat(64);
    }
    let row_memory = |r: usize| {
        values
            .iter()
            .find(|m| m.id.to_string() == sdk::string(&batch, 2, r))
            .unwrap()
    };
    let text = (0..batch.num_rows())
        .map(|r| {
            if sdk::string(&batch, 1, r) == "head" {
                serde_json::to_string(row_memory(r)).unwrap()
            } else {
                "unread historical payload".into()
            }
        })
        .collect();
    let actors = (0..batch.num_rows())
        .map(|r| {
            if sdk::string(&batch, 1, r) == "head" {
                row_memory(r).updated_by.clone()
            } else {
                sdk::string(&batch, 12, r).into()
            }
        })
        .collect();
    let revisions = (0..batch.num_rows())
        .map(|r| {
            if sdk::string(&batch, 1, r) == "head" {
                2
            } else {
                1
            }
        })
        .collect();
    let times = (0..batch.num_rows())
        .map(|r| {
            if sdk::string(&batch, 1, r) == "head" {
                row_memory(r).updated_at
            } else {
                row_memory(r).created_at
            }
        })
        .collect();
    let updated = sdk::text_column(&batch, 17, text);
    let updated = sdk::text_column(&updated, 12, actors);
    let updated = sdk::numbers(&updated, 8, revisions);
    let updated = sdk::numbers(&updated, 10, times);
    sdk::replace(&pin, updated).await;
    let version = sdk::pin(&f).await.version();
    let (generation, found) = listed(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter::default(),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let mut ties = [values[3].id, values[4].id];
    ties.sort();
    assert_eq!(
        found.iter().map(|m| m.id).collect::<Vec<_>>(),
        [
            values[1].id,
            values[0].id,
            values[2].id,
            values[5].id,
            ties[0],
            ties[1]
        ]
    );
    for m in &found {
        assert_eq!(m, values.iter().find(|v| v.id == m.id).unwrap());
    }
    let (_, mandatory) = listed(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter {
                mandatory: Some(true),
                ..Default::default()
            },
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(mandatory, found[..2]);
    assert!(mandatory.iter().all(|m| m.body.len() > 7000));
    let (_, contextual) = listed(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter {
                exclude_mandatory: true,
                ..Default::default()
            },
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(contextual, found[2..]);
    let (_, matching) = listed(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter {
                classification: Some(MemoryClass::Fact),
                tag: Some(" Tag'β ".into()),
                mandatory: Some(false),
                important: Some(true),
                exclude_mandatory: true,
            },
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(matching, vec![values[2].clone()]);
    for filter in [
        MemoryListFilter {
            mandatory: Some(true),
            exclude_mandatory: true,
            ..Default::default()
        },
        MemoryListFilter {
            tag: Some(" tag'β ".into()),
            ..Default::default()
        },
        MemoryListFilter {
            classification: Some(MemoryClass::Correction),
            ..Default::default()
        },
    ] {
        assert!(
            listed(
                NativeMemoryStore::list(ctx.snapshot(), &read, filter, read_limits())
                    .await
                    .unwrap()
            )
            .1
            .is_empty()
        );
    }
    assert_eq!(generation.sdk_version, version);
    assert_eq!(sdk::pin(&f).await.version(), version);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalogue_complete_input_caps_and_late_invalid_rows_are_errors() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    for i in 0..3 {
        NativeMemoryStore::create(
            ctx.snapshot(),
            &write,
            input(&format!("caps-{i}")),
            write_limits(),
        )
        .await
        .unwrap();
    }
    let pin = sdk::pin(&f).await;
    let batch = sdk::all(&pin).await;
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::new(1).unwrap();
    let filter = MemoryListFilter {
        exclude_mandatory: true,
        ..Default::default()
    };
    assert!(
        listed(
            NativeMemoryStore::list(ctx.snapshot(), &read, filter.clone(), limits)
                .await
                .unwrap()
        )
        .1
        .is_empty()
    );
    // Valid prefix must not conceal a later corrupt current row excluded by filter.
    let rows = (0..batch.num_rows())
        .filter(|r| sdk::string(&batch, 1, *r) == "head")
        .collect::<Vec<_>>();
    let bad = batch.slice(*rows.last().unwrap(), 1);
    sdk::replace(
        &pin,
        sdk::text_column(&bad, 17, vec!["invalid current payload".into()]),
    )
    .await;
    assert!(matches!(
        NativeMemoryStore::list(ctx.snapshot(), &read, filter, read_limits()).await,
        Err(MemoryStoreError::Json(_))
    ));
    // Write canonical duplicate keys through the actual SDK replacement adapter;
    // merge insertion would deliberately deduplicate that fixture.
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    let store = graphit_data::lance_module_store::LanceModuleStore::open(&uri, sdk::schema())
        .await
        .unwrap();
    store
        .snapshot()
        .prepare_replace(vec![batch.clone(), batch.slice(rows[0], 1)])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::new(1).unwrap();
    assert!(matches!(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter {
                exclude_mandatory: true,
                ..Default::default()
            },
            limits
        )
        .await,
        Err(MemoryStoreError::Invalid("duplicate Memory row key"))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalogue_original_module_project_cancel_revoke_and_deadline() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, admin
    );
    NativeMemoryStore::create(ctx.snapshot(), &write, input("boundary"), write_limits())
        .await
        .unwrap();
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        NativeMemoryStore::list(other.snapshot(), &read, Default::default(), read_limits()).await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    let task_read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &task_read,
            Default::default(),
            read_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let b = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    let bi = b.issuer(&ctx);
    let (bb, br, _, _ba) = b.issue(&bi, &ctx);
    let bg = bi
        .verify(bb.expose_for_transport(), &br, b.scope(), &ctx)
        .unwrap();
    let ba = ProjectRecordAuthority::new(&bi);
    let bread = ba.admit_read(&bg, &ctx, ProjectReadModule::Memory).unwrap();
    assert_eq!(
        NativeMemoryStore::list(ctx.snapshot(), &bread, Default::default(), read_limits())
            .await
            .unwrap(),
        MemoryListRead::NoCorpus
    );
    assert!(!b.project.runtime_root().join("memory").exists());
    let fresh = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &other)
        .unwrap();
    let freshread = authority
        .admit_read(&fresh, &other, ProjectReadModule::Memory)
        .unwrap();
    other_cancel.cancel();
    assert!(
        NativeMemoryStore::list(
            other.snapshot(),
            &freshread,
            Default::default(),
            read_limits()
        )
        .await
        .is_err()
    );
    let dc = Cancellation::default();
    let deadline = request_until(&dc, Instant::now() + Duration::from_millis(500));
    let dg = issuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &deadline,
        )
        .unwrap();
    let dr = authority
        .admit_read(&dg, &deadline, ProjectReadModule::Memory)
        .unwrap();
    tokio::time::sleep_until(deadline.snapshot().deadline().unwrap().into()).await;
    assert!(
        NativeMemoryStore::list(deadline.snapshot(), &dr, Default::default(), read_limits())
            .await
            .is_err()
    );
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(
        NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalogue_named_inheritance_is_observational() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _, _admin) = f.issue(&issuer, &ctx);
    let authority = ProjectRecordAuthority::new(&issuer);
    let initial = {
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let write = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
            .unwrap();
        NativeMemoryStore::create(ctx.snapshot(), &write, input("inherited"), write_limits())
            .await
            .unwrap()
    };
    let version = sdk::pin(&f).await.version();
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
        VersionContext::select(&paths, Checkout::NonGit, "catalogue-feature", None, "").unwrap();
    let (b, r, _, _a) = f.issue_for(&issuer, &ctx, f.scope());
    let g = issuer
        .verify(b.expose_for_transport(), &r, f.scope(), &ctx)
        .unwrap();
    let read = authority
        .admit_read(&g, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let (generation, memories) = listed(
        NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
            .await
            .unwrap(),
    );
    assert_eq!(memories, vec![initial]);
    assert!(generation.inherited_target.is_some());
    assert_eq!(generation.project_version, "catalogue-feature");
    assert_eq!(generation.sdk_version, version);
    let pin = sdk::pin(&f).await;
    assert!(pin.inherited_target().is_some());
    assert_eq!(pin.version(), version);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalogue_absence_empty_incomplete_hidden_rows_and_invalid_filter() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    assert!(matches!(
        NativeMemoryStore::list(
            ctx.snapshot(),
            &read,
            MemoryListFilter {
                tag: Some(" \n".into()),
                ..Default::default()
            },
            read_limits()
        )
        .await,
        Err(MemoryStoreError::Invalid(_))
    ));
    assert_eq!(
        NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
            .await
            .unwrap(),
        MemoryListRead::NoCorpus
    );
    assert!(!f.project.runtime_root().join("memory").exists());
    sdk::empty_with_schema(&f, &ctx, sdk::schema()).await;
    let (empty, records) = listed(
        NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
            .await
            .unwrap(),
    );
    assert!(records.is_empty());
    assert_eq!(sdk::pin(&f).await.version(), empty.sdk_version);
    NativeMemoryStore::create(ctx.snapshot(), &write, input("hidden"), write_limits())
        .await
        .unwrap();
    let pin = sdk::pin(&f).await;
    let batch = sdk::all(&pin).await;
    let head = sdk::select(&batch, "head");
    let hidden = sdk::flags(&head, 14, vec![true]);
    let hidden = sdk::text_column(&hidden, 17, vec!["excluded tombstone body".into()]);
    sdk::replace(&pin, hidden).await;
    assert!(
        listed(
            NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
                .await
                .unwrap()
        )
        .1
        .is_empty()
    );
    let incomplete = Fixture::new(&ctx, "main");
    let ii = incomplete.issuer(&ctx);
    let (b, r, _, _a) = incomplete.issue(&ii, &ctx);
    let g = ii
        .verify(b.expose_for_transport(), &r, incomplete.scope(), &ctx)
        .unwrap();
    let ia = ProjectRecordAuthority::new(&ii);
    let ir = ia.admit_read(&g, &ctx, ProjectReadModule::Memory).unwrap();
    {
        let _writer = sdk::directory_only(&incomplete, &ctx).await;
    }
    assert!(matches!(
        NativeMemoryStore::list(ctx.snapshot(), &ir, Default::default(), read_limits()).await,
        Err(MemoryStoreError::IncompleteCorpus)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalogue_real_role_reads_and_denied_tracker_or_missing_memory() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, _read, write, _bearer, _reference, _admin
    );
    let mandatory =
        NativeMemoryStore::create(ctx.snapshot(), &write, input("mandatory"), write_limits())
            .await
            .unwrap();
    let mut plain = input("plain");
    plain.attributes.mandatory = false;
    let plain = NativeMemoryStore::create(ctx.snapshot(), &write, plain, write_limits())
        .await
        .unwrap();
    for role in [Role::Scout, Role::Scribe, Role::Dream] {
        let (b, r, _, _a) = f.issue_role_for(
            &issuer,
            &ctx,
            f.scope(),
            role,
            &[Permission::Read(Module::Memory)],
        );
        let g = issuer
            .verify(b.expose_for_transport(), &r, f.scope(), &ctx)
            .unwrap();
        let read = authority
            .admit_read(&g, &ctx, ProjectReadModule::Memory)
            .unwrap();
        let all = listed(
            NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
                .await
                .unwrap(),
        )
        .1;
        assert_eq!(all, vec![mandatory.clone(), plain.clone()]);
        assert_eq!(
            listed(
                NativeMemoryStore::list(
                    ctx.snapshot(),
                    &read,
                    MemoryListFilter {
                        exclude_mandatory: true,
                        ..Default::default()
                    },
                    read_limits()
                )
                .await
                .unwrap()
            )
            .1,
            vec![plain.clone()]
        );
    }
    for role in [Role::Tracker, Role::Coordinator] {
        let (b, r, _, _a) = f.issue_role_for(
            &issuer,
            &ctx,
            f.scope(),
            role,
            &[Permission::Read(Module::Task)],
        );
        let g = issuer
            .verify(b.expose_for_transport(), &r, f.scope(), &ctx)
            .unwrap();
        assert!(
            authority
                .admit_read(&g, &ctx, ProjectReadModule::Memory)
                .is_err()
        );
    }
}
