use super::*;
use graphit_auth::Permission;
use graphit_core::module_settings::Module;
use graphit_hub::native::Role;

macro_rules! dream {
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$grant:ident,$read:ident,$write:ident) => {
        dream!(
            $f,
            $ctx,
            $issuer,
            $authority,
            $grant,
            $read,
            $write,
            _bearer,
            _credential,
            _run,
            _admin
        );
    };
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$grant:ident,$read:ident,$write:ident,$bearer:ident,$credential:ident,$run:ident,$admin:ident) => {
        let $issuer = $f.issuer(&$ctx);
        let ($bearer, $credential, $run, $admin) = $f.issue_role_for(
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
    };
}
fn selected(memory: &Memory, key: &str) -> MemoryFoldMember {
    MemoryFoldMember {
        id: memory.id,
        expected_revision: memory.revision,
        idempotency_key: key.into(),
    }
}
fn fold(a: &Memory, b: &Memory, key: &str) -> FoldMemories {
    FoldMemories {
        kind: MemoryFoldKind::Merge,
        survivor: selected(a, &format!("{key}/survivor")),
        members: vec![selected(b, &format!("{key}/member"))],
        title: None,
        body: a.body.clone(),
        reason: "Consolidate selected observed facts".into(),
    }
}
fn deletion(memory: &Memory, key: &str) -> DeleteMemory {
    DeleteMemory {
        expected_revision: memory.revision,
        idempotency_key: key.into(),
        reason: "Automatic selected removal".into(),
    }
}
fn reference() -> MemoryReference {
    MemoryReference {
        target_kind: ReferenceTargetKind::Task,
        scope: MemoryReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap()),
        id: "unresolved-source-task".into(),
        context: None,
        relation: "supports".into(),
        field: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_fold_one_manifest_derives_attributes_and_archives_every_member() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(f, ctx, issuer, authority, grant, read, write);
    let mut records = Vec::new();
    for (i, class) in [
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
        let mut authored = input(&format!("source-{i}"));
        authored.attributes = MemoryAttributes {
            classification: class,
            important: i == 1,
            mandatory: i == 2,
        };
        authored.body = format!("Original factual content {i}");
        authored.references = Some(vec![reference()]);
        records.push(
            NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
                .await
                .unwrap(),
        );
    }
    let mut proposal = fold(&records[0], &records[1], "group");
    proposal.kind = MemoryFoldKind::Conflict;
    proposal.members = records[1..]
        .iter()
        .enumerate()
        .map(|(i, m)| selected(m, &format!("group/remove-{i}")))
        .collect();
    proposal.body = records
        .iter()
        .map(|m| m.body.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let before = sdk::pin(&f).await.version();
    let receipt = NativeMemoryStore::automatic_fold(
        ctx.snapshot(),
        &read,
        &write,
        proposal.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(sdk::pin(&f).await.version(), before + 1);
    let mut reordered = proposal.clone();
    reordered.members.reverse();
    assert!(matches!(
        NativeMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            reordered,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    assert_eq!(receipt.survivor.sdk_version, Some(before + 1));
    assert_eq!(receipt.removed.len(), 5);
    assert!(
        receipt
            .removed
            .iter()
            .all(|m| m.sdk_version == Some(before + 1))
    );
    let result = NativeMemoryStore::get(ctx.snapshot(), &read, records[0].id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .memory;
    assert_eq!(
        result.attributes,
        MemoryAttributes {
            classification: MemoryClass::Correction,
            important: true,
            mandatory: true
        }
    );
    assert_eq!(result.body, proposal.body);
    assert_eq!(result.title, records[0].title);
    assert_eq!(result.tags, records[0].tags);
    assert_eq!(result.references, records[0].references);
    assert_eq!(result.reference_generation, records[0].reference_generation);
    let (_, outgoing, incoming, coverage, warnings) = super::references::pages(
        NativeMemoryStore::references(
            ctx.snapshot(),
            &read,
            super::references::query(
                Some(result.id),
                Some(super::references::target(&reference())),
            ),
            super::references::limits(),
        )
        .await
        .unwrap(),
    );
    let outgoing = outgoing.unwrap();
    assert_eq!(outgoing.results[0].source_revision, result.revision);
    assert_eq!(
        outgoing.results[0].reference_generation,
        records[0].reference_generation
    );
    assert_eq!(incoming.unwrap().total, 1);
    assert!(coverage.memory_project_source_complete && warnings.is_empty());
    assert_eq!(result.created_by, records[0].created_by);
    assert_eq!(result.created_at, records[0].created_at);
    let h = NativeMemoryStore::history(ctx.snapshot(), &read, result.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        h.revisions[0].operation,
        MemoryMutation::FoldSurvivor(_)
    ));
    for old in &records[1..] {
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, old.id, read_limits())
                .await
                .unwrap()
                .is_none()
        );
        let h = NativeMemoryStore::history(ctx.snapshot(), &read, old.id, read_limits())
            .await
            .unwrap()
            .unwrap();
        assert!(h.deleted);
        assert_eq!(h.revisions[0].before, *old);
        assert_eq!(h.current.body, old.body);
        assert!(h.current.references.is_empty());
        assert_eq!(h.current.reference_generation, old.reference_generation + 1);
        assert!(matches!(
            h.revisions[0].operation,
            MemoryMutation::FoldDelete(_)
        ));
    }
    let observed = NativeMemoryStore::reobserve_automatic_fold(
        ctx.snapshot(),
        &read,
        &write,
        proposal.clone(),
        write_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(observed.survivor.after_revision, 2);
    assert!(observed.survivor.sdk_version.is_none());
    assert!(observed.removed.iter().all(|m| m.sdk_version.is_none()));
    assert_eq!(
        NativeMemoryStore::automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            proposal.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        observed
    );
    assert_eq!(sdk::pin(&f).await.version(), before + 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_fold_validates_all_before_commit_and_replays_unchanged_anchor() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(f, ctx, issuer, authority, grant, read, write);
    let a = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let b = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, input("b"), write_limits())
        .await
        .unwrap();
    let proposal = fold(&a, &b, "same");
    let initial = sdk::pin(&f).await.version();
    for invalid in 0..9 {
        let mut p = proposal.clone();
        match invalid {
            0 => p.body = " \n ".into(),
            1 => p.members[0].id = p.survivor.id,
            2 => p.members[0].idempotency_key = p.survivor.idempotency_key.clone(),
            3 => p.members[0].expected_revision = 2,
            4 => p.members[0].idempotency_key = "a".into(),
            5 => p.survivor.expected_revision = 2,
            6 => p.members[0].expected_revision = 0,
            7 => p.members.clear(),
            _ => p.title = Some(" \n".into()),
        }
        assert!(
            NativeMemoryStore::automatic_fold(ctx.snapshot(), &read, &write, p, write_limits())
                .await
                .is_err()
        );
        assert_eq!(sdk::pin(&f).await.version(), initial);
    }
    let receipt = NativeMemoryStore::automatic_fold(
        ctx.snapshot(),
        &read,
        &write,
        proposal.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(receipt.survivor.after_revision, 2);
    let current = NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .memory;
    assert_eq!(current.body, a.body);
    assert_eq!(current.title, a.title);
    assert_eq!(current.attributes, a.attributes);
    let patch = FencedUpdateMemory {
        expected_revision: 2,
        idempotency_key: "later".into(),
        reason: "Later independent revision".into(),
        title: None,
        body: Some("Later content".into()),
        references: None,
    };
    NativeMemoryStore::fenced_update(ctx.snapshot(), &write, a.id, patch, write_limits())
        .await
        .unwrap();
    let version = sdk::pin(&f).await.version();
    let mut colliding = proposal.clone();
    colliding.survivor.idempotency_key = "new-anchor".into();
    colliding.members[0].idempotency_key = "later".into();
    assert!(matches!(
        NativeMemoryStore::automatic_fold(ctx.snapshot(), &read, &write, colliding, write_limits())
            .await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    let historic = NativeMemoryStore::automatic_fold(
        ctx.snapshot(),
        &read,
        &write,
        proposal.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(historic.survivor.after_revision, 2);
    assert!(historic.survivor.sdk_version.is_none());
    assert_eq!(sdk::pin(&f).await.version(), version);
    for variant in 0..4 {
        let mut changed = proposal.clone();
        match variant {
            0 => changed.reason.push_str(" changed"),
            1 => changed.body.push_str(" changed"),
            2 => changed.members[0].idempotency_key.push_str(" changed"),
            _ => changed.kind = MemoryFoldKind::Conflict,
        }
        assert!(matches!(
            NativeMemoryStore::automatic_fold(
                ctx.snapshot(),
                &read,
                &write,
                changed,
                write_limits()
            )
            .await,
            Err(MemoryStoreError::IdempotencyConflict)
        ));
        assert_eq!(sdk::pin(&f).await.version(), version);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_delete_protection_refusal_key_and_concurrent_last_record() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(
        f, ctx, issuer, authority, grant, read, write, _bearer, credential, run, admin
    );
    let mut authored = input("a");
    authored.attributes.mandatory = false;
    authored.attributes.important = true;
    let a = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let mut authored = input("b");
    authored.attributes.mandatory = false;
    let b = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let base = sdk::pin(&f).await.version();
    assert_eq!(
        NativeMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            a.id,
            deletion(&a, "unused"),
            write_limits()
        )
        .await
        .unwrap(),
        AutomaticDeleteOutcome::Refused(AutomaticDeleteRefusal::Protected)
    );
    assert!(
        NativeMemoryStore::reobserve_automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            a.id,
            deletion(&a, "unused"),
            write_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(sdk::pin(&f).await.version(), base);
    let a = NativeMemoryStore::fenced_demote(
        ctx.snapshot(),
        &write,
        a.id,
        ChangeMemoryImportance {
            expected_revision: 1,
            idempotency_key: "demote".into(),
            reason: "Explicit importance removal".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    let second_cancel = Cancellation::default();
    let second = request(&second_cancel);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let second_bearer = issuer
        .issue(
            &parent,
            &run,
            &credential,
            &[
                Permission::Read(Module::Memory),
                Permission::WriteFencedMemory,
            ],
            &f.scope(),
        )
        .unwrap();
    let second_grant = issuer
        .verify(
            second_bearer.expose_for_transport(),
            &credential,
            f.scope(),
            &second,
        )
        .unwrap();
    let second_read = authority
        .admit_read(&second_grant, &second, ProjectReadModule::Memory)
        .unwrap();
    let second_write = authority
        .admit_write(&second_grant, &second, ProjectWriteOperation::FencedMemory)
        .unwrap();
    let (x, y) = tokio::join!(
        NativeMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            a.id,
            deletion(&a, "unused"),
            write_limits()
        ),
        NativeMemoryStore::automatic_delete(
            second.snapshot(),
            &second_read,
            &second_write,
            b.id,
            deletion(&b, "remove-b"),
            write_limits()
        )
    );
    let (x, y) = (x.unwrap(), y.unwrap());
    assert_eq!(
        [&x, &y]
            .iter()
            .filter(|r| matches!(r, AutomaticDeleteOutcome::Deleted(_)))
            .count(),
        1
    );
    assert_eq!(
        [&x, &y]
            .iter()
            .filter(|r| matches!(
                r,
                AutomaticDeleteOutcome::Refused(AutomaticDeleteRefusal::LastRecord)
            ))
            .count(),
        1
    );
    let (_, heads) = published(
        NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
    );
    assert_eq!(heads.len(), 1);
    for (memory, key, result) in [(&a, "unused", x), (&b, "remove-b", y)] {
        let historic = NativeMemoryStore::reobserve_automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            memory.id,
            deletion(memory, key),
            write_limits(),
        )
        .await
        .unwrap();
        if let AutomaticDeleteOutcome::Deleted(effect) = result {
            assert_eq!(historic.unwrap().after_revision, effect.after_revision);
            let retry = NativeMemoryStore::automatic_delete(
                ctx.snapshot(),
                &read,
                &write,
                memory.id,
                deletion(memory, key),
                write_limits(),
            )
            .await
            .unwrap();
            assert!(matches!(
                retry,
                AutomaticDeleteOutcome::Deleted(AutomaticMemoryEffect {
                    sdk_version: None,
                    ..
                })
            ));
        } else {
            assert!(historic.is_none());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_observation_no_birth_wrong_admission_and_cancel() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(f, ctx, issuer, authority, grant, read, write);
    let id = MemoryId::new(std::num::NonZeroU128::new(11).unwrap());
    let unknown = DeleteMemory {
        expected_revision: 1,
        idempotency_key: "unknown".into(),
        reason: "Observe only".into(),
    };
    assert!(
        NativeMemoryStore::reobserve_automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            id,
            unknown.clone(),
            write_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(!f.project.runtime_root().join("memory").exists());
    let mut authored = input("a");
    authored.attributes.mandatory = false;
    let a = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let base = sdk::pin(&f).await.version();
    let ordinary_issuer = f.issuer(&ctx);
    let (bearer, credential, _run, _admin) = f.issue(&ordinary_issuer, &ctx);
    let ordinary_grant = ordinary_issuer
        .verify(bearer.expose_for_transport(), &credential, f.scope(), &ctx)
        .unwrap();
    let ordinary_authority = ProjectRecordAuthority::new(&ordinary_issuer);
    let ordinary = ordinary_authority
        .admit_write(&ordinary_grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &ordinary,
            a.id,
            deletion(&a, "wrong"),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let foreign_cancel = Cancellation::default();
    let foreign = request(&foreign_cancel);
    assert!(
        NativeMemoryStore::automatic_delete(
            foreign.snapshot(),
            &read,
            &write,
            a.id,
            deletion(&a, "foreign"),
            write_limits()
        )
        .await
        .is_err()
    );
    cancel.cancel();
    assert!(
        NativeMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            a.id,
            deletion(&a, "cancel"),
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(sdk::pin(&f).await.version(), base);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_group_rejects_local_corruption_and_missing_other_member_without_repair() {
    for damage in 0..8 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        dream!(f, ctx, issuer, authority, grant, read, write);
        let a =
            NativeMemoryStore::fenced_create(ctx.snapshot(), &write, input("a"), write_limits())
                .await
                .unwrap();
        let b =
            NativeMemoryStore::fenced_create(ctx.snapshot(), &write, input("b"), write_limits())
                .await
                .unwrap();
        let proposal = fold(&a, &b, "corruption");
        NativeMemoryStore::automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            proposal.clone(),
            write_limits(),
        )
        .await
        .unwrap();
        let pin = sdk::pin(&f).await;
        if damage == 0 || damage == 7 {
            let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
                .unwrap()
                .to_string();
            let mut dataset = lance::Dataset::open(&uri).await.unwrap();
            let filter = if damage == 0 {
                format!("row_key = 'revision/{}/2'", b.id)
            } else {
                format!("record_id = '{}'", a.id)
            };
            dataset.delete(&filter).await.unwrap();
            // Ordinary reads validate their own chain, not unrelated removed members.
            if damage == 0 {
                assert!(
                    NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
                        .await
                        .unwrap()
                        .is_some()
                );
            }
        } else {
            let batch = sdk::all(&pin).await;
            let target = if damage == 5 { b.id } else { a.id };
            let row = (0..batch.num_rows())
                .find(|r| sdk::string(&batch, 0, *r) == format!("revision/{target}/2"))
                .unwrap();
            let mut payload: serde_json::Value =
                serde_json::from_str(sdk::string(&batch, 17, row)).unwrap();
            match damage {
                1 => {
                    payload["operation"]["FoldSurvivor"]["observations"][0]["attributes"]["important"] =
                        true.into()
                }
                2 => payload["before"]["body"] = "fabricated before".into(),
                3 => payload["after"]["body"] = "fabricated after".into(),
                4 => payload["operation"]["FoldSurvivor"]["unexpected"] = true.into(),
                5 => payload["operation"]["FoldDelete"]["survivor_key"] = "different anchor".into(),
                _ => {
                    payload["operation"]["FoldSurvivor"]["observations"][1]["attributes"]["mandatory"] =
                        false.into()
                }
            }
            sdk::replace(
                &pin,
                sdk::text_column(&batch.slice(row, 1), 17, vec![payload.to_string()]),
            )
            .await;
            if damage <= 4 {
                assert!(
                    NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
                        .await
                        .is_err(),
                    "local get damage {damage}"
                );
                assert!(
                    NativeMemoryStore::history(ctx.snapshot(), &read, a.id, read_limits())
                        .await
                        .is_err(),
                    "local history damage {damage}"
                );
            } else {
                // The mutated source binding/other observation can still derive a valid local chain.
                assert!(
                    NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
                        .await
                        .unwrap()
                        .is_some()
                );
            }
        }
        let damaged = sdk::pin(&f).await.version();
        assert!(
            NativeMemoryStore::reobserve_automatic_fold(
                ctx.snapshot(),
                &read,
                &write,
                proposal.clone(),
                write_limits()
            )
            .await
            .is_err(),
            "group damage {damage}"
        );
        assert!(
            NativeMemoryStore::automatic_fold(
                ctx.snapshot(),
                &read,
                &write,
                proposal,
                write_limits()
            )
            .await
            .is_err(),
            "retry damage {damage}"
        );
        assert_eq!(sdk::pin(&f).await.version(), damaged);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_delete_metadata_eof_rejects_foreign_duplicate_and_malformed() {
    for damage in 0..4 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        dream!(f, ctx, issuer, authority, grant, read, write);
        let mut authored = input("a");
        authored.attributes.mandatory = false;
        let a = NativeMemoryStore::fenced_create(
            ctx.snapshot(),
            &write,
            authored.clone(),
            write_limits(),
        )
        .await
        .unwrap();
        authored.idempotency_key = "b".into();
        let b = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
            .await
            .unwrap();
        let mut third = input("c");
        third.attributes.mandatory = false;
        let c = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, third, write_limits())
            .await
            .unwrap();
        let pin = sdk::pin(&f).await;
        let batch = sdk::all(&pin).await;
        let row = (0..batch.num_rows())
            .find(|r| sdk::string(&batch, 0, *r) == format!("head/{}", b.id))
            .unwrap();
        let head = batch.slice(row, 1);
        match damage {
            0 => {
                sdk::replace(
                    &pin,
                    sdk::text_column(&head, 3, vec!["01KSH1CRFFG8Z74B5ZS78WW809".into()]),
                )
                .await
            }
            1 => sdk::replace(&pin, sdk::text_column(&head, 2, vec![c.id.to_string()])).await,
            2 => sdk::replace(&pin, sdk::text_column(&head, 7, vec![" \n".into()])).await,
            3 => sdk::replace(&pin, sdk::numbers(&head, 8, vec![0])).await,
            _ => (),
        }
        let before = sdk::pin(&f).await.version();
        let mut limits = write_limits();
        limits.read.batch_rows = std::num::NonZeroU32::new(1).unwrap();
        assert!(
            NativeMemoryStore::automatic_delete(
                ctx.snapshot(),
                &read,
                &write,
                a.id,
                deletion(&a, "guarded"),
                limits
            )
            .await
            .is_err(),
            "metadata damage {damage}"
        );
        assert_eq!(sdk::pin(&f).await.version(), before);
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
                .await
                .unwrap()
                .is_some()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_named_inheritance_isolation_last_record_and_direct_protected_delete() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let permissions = [
        Permission::Read(Module::Memory),
        Permission::WriteFencedMemory,
    ];
    let (bearer, credential, _, admin) =
        f.issue_role_for(&issuer, &ctx, f.scope(), Role::Dream, &permissions);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let (a, b, c, proposal) = {
        let grant = issuer
            .verify(bearer.expose_for_transport(), &credential, f.scope(), &ctx)
            .unwrap();
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Memory)
            .unwrap();
        let write = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
            .unwrap();
        let mut authored = input("a");
        authored.attributes.mandatory = false;
        let a = NativeMemoryStore::fenced_create(
            ctx.snapshot(),
            &write,
            authored.clone(),
            write_limits(),
        )
        .await
        .unwrap();
        authored.idempotency_key = "b".into();
        let b = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
            .await
            .unwrap();
        let c = NativeMemoryStore::fenced_create(
            ctx.snapshot(),
            &write,
            input("protected"),
            write_limits(),
        )
        .await
        .unwrap();
        assert_eq!(
            NativeMemoryStore::automatic_delete(
                ctx.snapshot(),
                &read,
                &write,
                c.id,
                deletion(&c, "protected-auto"),
                write_limits()
            )
            .await
            .unwrap(),
            AutomaticDeleteOutcome::Refused(AutomaticDeleteRefusal::Protected)
        );
        NativeMemoryStore::fenced_delete(
            ctx.snapshot(),
            &write,
            c.id,
            deletion(&c, "explicit-protected"),
            write_limits(),
        )
        .await
        .unwrap();
        let proposal = fold(&a, &b, "default-group");
        (a, b, c, proposal)
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
    f.version = VersionContext::select(&paths, Checkout::NonGit, "feature", None, "").unwrap();
    let named_run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let named = issuer
        .issue(&parent, &named_run, &credential, &permissions, &f.scope())
        .unwrap();
    {
        let grant = issuer
            .verify(named.expose_for_transport(), &credential, f.scope(), &ctx)
            .unwrap();
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Memory)
            .unwrap();
        let write = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
            .unwrap();
        let inherited = sdk::pin(&f).await;
        assert!(inherited.inherited_target().is_some());
        assert!(
            NativeMemoryStore::reobserve_automatic_fold(
                ctx.snapshot(),
                &read,
                &write,
                proposal.clone(),
                write_limits()
            )
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(
            sdk::pin(&f).await.inherited_target(),
            inherited.inherited_target()
        );
        let receipt = NativeMemoryStore::automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            proposal.clone(),
            write_limits(),
        )
        .await
        .unwrap();
        let current = NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory;
        assert_eq!(current.revision, 2);
        assert_eq!(
            NativeMemoryStore::automatic_delete(
                ctx.snapshot(),
                &read,
                &write,
                a.id,
                deletion(&current, "last"),
                write_limits()
            )
            .await
            .unwrap(),
            AutomaticDeleteOutcome::Refused(AutomaticDeleteRefusal::LastRecord)
        );
        assert_eq!(
            sdk::pin(&f).await.version(),
            receipt.survivor.sdk_version.unwrap()
        );
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, b.id, read_limits())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            NativeMemoryStore::get(ctx.snapshot(), &read, c.id, read_limits())
                .await
                .unwrap()
                .is_none()
        );
    }
    f.version = VersionContext::select(&paths, Checkout::NonGit, "main", None, "").unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &credential, f.scope(), &ctx)
        .unwrap();
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
        .unwrap();
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory
            .revision,
        1
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, b.id, read_limits())
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        NativeMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            proposal,
            write_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_authority_other_module_project_actor_deadline_and_fold_no_birth() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    dream!(f, ctx, issuer, authority, grant, read, write);
    let missing_a = MemoryId::new(std::num::NonZeroU128::new(41).unwrap());
    let missing_b = MemoryId::new(std::num::NonZeroU128::new(42).unwrap());
    let absent = FoldMemories {
        kind: MemoryFoldKind::Merge,
        survivor: MemoryFoldMember {
            id: missing_a,
            expected_revision: 1,
            idempotency_key: "absent-anchor".into(),
        },
        members: vec![MemoryFoldMember {
            id: missing_b,
            expected_revision: 1,
            idempotency_key: "absent-member".into(),
        }],
        title: None,
        body: "Proposed content".into(),
        reason: "Read absence".into(),
    };
    assert!(
        NativeMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            absent.clone(),
            write_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(matches!(
        NativeMemoryStore::automatic_fold(ctx.snapshot(), &read, &write, absent, write_limits())
            .await,
        Err(MemoryStoreError::NotFound)
    ));
    assert!(!f.project.runtime_root().join("memory").exists());
    let a = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let b = NativeMemoryStore::fenced_create(ctx.snapshot(), &write, input("b"), write_limits())
        .await
        .unwrap();
    let proposal = fold(&a, &b, "actor");
    NativeMemoryStore::automatic_fold(
        ctx.snapshot(),
        &read,
        &write,
        proposal.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    let version = sdk::pin(&f).await.version();
    let issuer2 = f.issuer(&ctx);
    let (bearer2, credential2, _, _) = f.issue(&issuer2, &ctx);
    let grant2 = issuer2
        .verify(
            bearer2.expose_for_transport(),
            &credential2,
            f.scope(),
            &ctx,
        )
        .unwrap();
    let authority2 = ProjectRecordAuthority::new(&issuer2);
    let task_read = authority2
        .admit_read(&grant2, &ctx, ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &task_read,
            &write,
            proposal.clone(),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let actor_read = authority2
        .admit_read(&grant2, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let actor_write = authority2
        .admit_write(&grant2, &ctx, ProjectWriteOperation::FencedMemory)
        .unwrap();
    assert!(
        NativeMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &actor_read,
            &actor_write,
            proposal.clone(),
            write_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
    let f2 = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    dream!(
        f2,
        ctx,
        other_issuer,
        other_authority,
        other_grant,
        other_read,
        _other_write
    );
    assert!(matches!(
        NativeMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &other_read,
            &write,
            proposal.clone(),
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let timed_cancel = Cancellation::default();
    let timed = request_until(&timed_cancel, Instant::now() + Duration::from_secs(2));
    let timed_grant = issuer2
        .verify(
            bearer2.expose_for_transport(),
            &credential2,
            f.scope(),
            &timed,
        )
        .unwrap();
    let timed_read = authority2
        .admit_read(&timed_grant, &timed, ProjectReadModule::Memory)
        .unwrap();
    let timed_write = authority2
        .admit_write(&timed_grant, &timed, ProjectWriteOperation::FencedMemory)
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        NativeMemoryStore::reobserve_automatic_fold(
            timed.snapshot(),
            &timed_read,
            &timed_write,
            proposal,
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(sdk::pin(&f).await.version(), version);
}
