use super::fenced::{deletion, fenced, patch, reference, version};
use super::*;
use graphit_core::file_lock::Cancellation;
use graphit_records::memory_attributes::{MemoryAttributes, MemoryClass};
use std::num::NonZeroUsize;
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
        body: "durable combined facts".into(),
        reason: "literal consolidation β".into(),
    }
}
fn ref_query() -> UserMemoryReferencesQuery {
    let r = reference();
    UserMemoryReferencesQuery {
        outgoing: None,
        incoming: Some(MemoryReferenceTarget {
            target_kind: r.target_kind,
            scope: r.scope,
            id: r.id,
            context: r.context,
        }),
        page_size: NonZeroUsize::new(100).unwrap(),
        outgoing_cursor: None,
        incoming_cursor: None,
    }
}
fn ref_limits() -> MemoryStoreReadLimits {
    read_limits()
}

#[tokio::test]
async fn automatic_user_fold_one_delta_preserves_audits_markers_and_whole_group_history() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    fenced!(f, ctx, verifier, proof, read, _ordinary, write);
    let mut records = vec![];
    for (i, attributes) in [
        MemoryAttributes {
            classification: MemoryClass::Fact,
            mandatory: false,
            important: false,
        },
        MemoryAttributes {
            classification: MemoryClass::Skill,
            mandatory: false,
            important: true,
        },
        MemoryAttributes {
            classification: MemoryClass::Correction,
            mandatory: true,
            important: false,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let mut authored = input(&format!("fold-source-{i}"));
        authored.body = format!("original facts {i}");
        authored.attributes = attributes;
        authored.references = Some(vec![reference()]);
        records.push(
            NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
                .await
                .unwrap(),
        );
    }
    let mut proposal = fold(&records[0], &records[1], "atomic-fold");
    proposal.kind = MemoryFoldKind::Conflict;
    proposal
        .members
        .push(selected(&records[2], "atomic-fold/second-member"));
    let before = version(&f).await;
    let receipt = NativeUserMemoryStore::automatic_fold(
        ctx.snapshot(),
        &read,
        &write,
        proposal.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(version(&f).await, before + 1);
    assert_eq!(receipt.survivor.sdk_version, Some(before + 1));
    assert_eq!(receipt.removed.len(), 2);
    assert!(
        receipt
            .removed
            .iter()
            .all(|e| e.sdk_version == Some(before + 1))
    );
    let current = NativeUserMemoryStore::get(ctx.snapshot(), &read, records[0].id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .memory;
    assert_eq!(
        current.attributes,
        MemoryAttributes {
            classification: MemoryClass::Correction,
            mandatory: true,
            important: true
        }
    );
    assert_eq!(current.body, proposal.body);
    assert_eq!(current.created_at, records[0].created_at);
    assert_eq!(current.created_by, records[0].created_by);
    assert_eq!(current.references, records[0].references);
    assert_eq!(current.reference_generation, 1);
    let history = NativeUserMemoryStore::history(ctx.snapshot(), &read, current.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        history.revisions[0].operation,
        MemoryMutation::FoldSurvivor(_)
    ));
    for old in &records[1..] {
        assert!(
            NativeUserMemoryStore::get(ctx.snapshot(), &read, old.id, read_limits())
                .await
                .unwrap()
                .is_none()
        );
        let history = NativeUserMemoryStore::history(ctx.snapshot(), &read, old.id, read_limits())
            .await
            .unwrap()
            .unwrap();
        assert!(history.deleted);
        assert_eq!(history.revisions[0].before, *old);
        assert_eq!(history.current.body, old.body);
        assert!(history.current.references.is_empty());
        assert_eq!(history.current.reference_generation, 2);
        assert!(matches!(
            history.revisions[0].operation,
            MemoryMutation::FoldDelete(_)
        ));
        assert_eq!(history.generation.sdk_version, before + 1);
    }
    let UserMemoryReferencesRead::Published {
        incoming: Some(incoming),
        coverage,
        warnings,
        ..
    } = NativeUserMemoryStore::references(ctx.snapshot(), &read, ref_query(), ref_limits())
        .await
        .unwrap()
    else {
        panic!("own published incoming")
    };
    assert_eq!(incoming.total, 1);
    assert_eq!(incoming.results[0].source_revision, 2);
    assert!(coverage.memory_user_source_complete && warnings.is_empty());
    let observed = NativeUserMemoryStore::reobserve_automatic_fold(
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
    assert!(observed.removed.iter().all(|e| e.sdk_version.is_none()));
    assert_eq!(
        NativeUserMemoryStore::automatic_fold(
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
    assert_eq!(version(&f).await, before + 1);
    let mut later = patch("later-survivor", 2);
    later.body = Some("independent later facts".into());
    NativeUserMemoryStore::fenced_update(ctx.snapshot(), &write, current.id, later, write_limits())
        .await
        .unwrap();
    let after = version(&f).await;
    assert_eq!(
        NativeUserMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            proposal.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        Some(observed.clone())
    );
    assert_eq!(
        NativeUserMemoryStore::automatic_fold(
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
    assert_eq!(version(&f).await, after);
    for variant in 0..4 {
        let mut changed = proposal.clone();
        match variant {
            0 => changed.reason.push_str("changed"),
            1 => changed.kind = MemoryFoldKind::Merge,
            2 => changed.members.reverse(),
            3 => changed.members[0].idempotency_key.push_str("changed"),
            _ => unreachable!(),
        };
        assert!(matches!(
            NativeUserMemoryStore::automatic_fold(
                ctx.snapshot(),
                &read,
                &write,
                changed,
                write_limits()
            )
            .await,
            Err(MemoryStoreError::IdempotencyConflict)
        ));
        assert_eq!(version(&f).await, after);
    }
}

#[tokio::test]
async fn automatic_user_merge_transitively_retains_importance_and_validates_every_member_before_commit()
 {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    fenced!(f, ctx, verifier, proof, read, _ordinary, write);
    let mut a = input("merge-a");
    a.attributes = MemoryAttributes {
        classification: MemoryClass::Fact,
        mandatory: false,
        important: false,
    };
    let a = NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, a, write_limits())
        .await
        .unwrap();
    let mut b = input("merge-b");
    b.attributes = MemoryAttributes {
        classification: MemoryClass::Correction,
        mandatory: false,
        important: true,
    };
    let b = NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, b, write_limits())
        .await
        .unwrap();
    let c = NativeUserMemoryStore::fenced_create(
        ctx.snapshot(),
        &write,
        input("merge-c"),
        write_limits(),
    )
    .await
    .unwrap();
    let proposal = fold(&a, &b, "merge-first");
    let before = version(&f).await;
    for variant in 0..9 {
        let mut invalid = proposal.clone();
        match variant {
            0 => invalid.members[0].expected_revision = 2,
            1 => invalid.survivor.expected_revision = 2,
            2 => invalid.members[0].id = invalid.survivor.id,
            3 => invalid.members[0].idempotency_key = invalid.survivor.idempotency_key.clone(),
            4 => invalid.members.clear(),
            5 => invalid.members[0].expected_revision = 0,
            6 => invalid.body = " \n ".into(),
            7 => invalid.title = Some(" \n".into()),
            8 => invalid.members[0].idempotency_key = "merge-a".into(),
            _ => unreachable!(),
        };
        assert!(
            NativeUserMemoryStore::automatic_fold(
                ctx.snapshot(),
                &read,
                &write,
                invalid,
                write_limits()
            )
            .await
            .is_err(),
            "invalid group {variant}"
        );
        assert_eq!(version(&f).await, before);
    }
    NativeUserMemoryStore::automatic_fold(ctx.snapshot(), &read, &write, proposal, write_limits())
        .await
        .unwrap();
    let first = NativeUserMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .memory;
    assert!(first.attributes.important);
    assert!(!first.attributes.mandatory);
    assert_eq!(first.attributes.classification, MemoryClass::Correction);
    let second_proposal = fold(&first, &c, "merge-transitive");
    let before_second = version(&f).await;
    let receipt = NativeUserMemoryStore::automatic_fold(
        ctx.snapshot(),
        &read,
        &write,
        second_proposal,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(receipt.survivor.sdk_version, Some(before_second + 1));
    let second = NativeUserMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .memory;
    assert_eq!(
        second.attributes,
        MemoryAttributes {
            classification: MemoryClass::Correction,
            important: true,
            mandatory: true
        }
    );
    assert_eq!(second.revision, 3);
}

#[tokio::test]
async fn automatic_user_delete_protection_refusal_does_not_consume_attempt_and_retry_is_historical()
{
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    fenced!(f, ctx, verifier, proof, read, ordinary, write);
    let mut authored = input("protected-importance");
    authored.attributes.mandatory = false;
    authored.attributes.important = true;
    authored.references = Some(vec![reference()]);
    let important =
        NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
            .await
            .unwrap();
    let mandatory = NativeUserMemoryStore::fenced_create(
        ctx.snapshot(),
        &write,
        input("protected-mandatory"),
        write_limits(),
    )
    .await
    .unwrap();
    let before = version(&f).await;
    for memory in [&important, &mandatory] {
        assert_eq!(
            NativeUserMemoryStore::automatic_delete(
                ctx.snapshot(),
                &read,
                &write,
                memory.id,
                deletion("refused", memory.revision),
                write_limits()
            )
            .await
            .unwrap(),
            AutomaticDeleteOutcome::Refused(AutomaticDeleteRefusal::Protected)
        );
        assert_eq!(version(&f).await, before);
        assert_eq!(
            NativeUserMemoryStore::reobserve_automatic_delete(
                ctx.snapshot(),
                &read,
                &write,
                memory.id,
                deletion("refused", memory.revision),
                write_limits()
            )
            .await
            .unwrap(),
            None
        );
    }
    let mut unprotect = update("unprotect", 1);
    unprotect.body = None;
    unprotect.important = Some(false);
    let unprotected = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &ordinary,
        important.id,
        unprotect,
        write_limits(),
    )
    .await
    .unwrap();
    let before_delete = version(&f).await;
    let attempt = deletion("refused", unprotected.revision);
    let AutomaticDeleteOutcome::Deleted(effect) = NativeUserMemoryStore::automatic_delete(
        ctx.snapshot(),
        &read,
        &write,
        important.id,
        attempt.clone(),
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("valid removal")
    };
    assert_eq!(effect.sdk_version, Some(before_delete + 1));
    assert_eq!(effect.after_revision, 3);
    let after = version(&f).await;
    let observed = NativeUserMemoryStore::reobserve_automatic_delete(
        ctx.snapshot(),
        &read,
        &write,
        important.id,
        attempt.clone(),
        write_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(observed.sdk_version, None);
    assert_eq!(observed.after_revision, effect.after_revision);
    assert_eq!(
        NativeUserMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            important.id,
            attempt.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        AutomaticDeleteOutcome::Deleted(observed)
    );
    assert_eq!(version(&f).await, after);
    let history =
        NativeUserMemoryStore::history(ctx.snapshot(), &read, important.id, read_limits())
            .await
            .unwrap()
            .unwrap();
    assert!(history.deleted);
    assert!(history.current.references.is_empty());
    assert_eq!(history.current.reference_generation, 2);
    assert!(matches!(
        history.revisions.last().unwrap().operation,
        MemoryMutation::AutomaticDelete(_)
    ));
    let mut changed = attempt;
    changed.reason.push_str(" changed");
    assert!(matches!(
        NativeUserMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            important.id,
            changed,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    assert_eq!(version(&f).await, after);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_automatic_deletes_keep_the_actual_last_live_record() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    fenced!(f, ctx, verifier, proof, read, _ordinary, write);
    let mut authored = input("last-a");
    authored.attributes.mandatory = false;
    let a = NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let mut authored = input("last-b");
    authored.attributes.mandatory = false;
    let b = NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let before = version(&f).await;
    let (left, right) = tokio::join!(
        NativeUserMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            a.id,
            deletion("delete-a", 1),
            write_limits()
        ),
        NativeUserMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            b.id,
            deletion("delete-b", 1),
            write_limits()
        )
    );
    let (removed, remaining) = match (left.unwrap(), right.unwrap()) {
        (
            AutomaticDeleteOutcome::Deleted(effect),
            AutomaticDeleteOutcome::Refused(AutomaticDeleteRefusal::LastRecord),
        ) => (effect, b),
        (
            AutomaticDeleteOutcome::Refused(AutomaticDeleteRefusal::LastRecord),
            AutomaticDeleteOutcome::Deleted(effect),
        ) => (effect, a),
        other => panic!("one removal and one real last-record refusal: {other:?}"),
    };
    assert_eq!(removed.sdk_version, Some(before + 1));
    assert_eq!(version(&f).await, before + 1);
    let UserMemoryListRead::Published { memories, .. } = NativeUserMemoryStore::list(
        ctx.snapshot(),
        &read,
        MemoryListFilter::default(),
        read_limits(),
    )
    .await
    .unwrap() else {
        panic!("published")
    };
    assert_eq!(memories, vec![remaining.clone()]);
    let v = version(&f).await;
    assert_eq!(
        NativeUserMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            remaining.id,
            deletion("again-last", 1),
            write_limits()
        )
        .await
        .unwrap(),
        AutomaticDeleteOutcome::Refused(AutomaticDeleteRefusal::LastRecord)
    );
    assert_eq!(version(&f).await, v);
}

#[tokio::test]
async fn automatic_reobserve_absence_ordinary_write_and_foreign_read_original_never_birth() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    fenced!(f, ctx, verifier, proof, read, ordinary, write);
    let id = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
    let member = "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
    let proposal = FoldMemories {
        kind: MemoryFoldKind::Merge,
        survivor: MemoryFoldMember {
            id,
            expected_revision: 1,
            idempotency_key: "absent-survivor".into(),
        },
        members: vec![MemoryFoldMember {
            id: member,
            expected_revision: 1,
            idempotency_key: "absent-member".into(),
        }],
        title: None,
        body: "valid absent fold data".into(),
        reason: "valid reason".into(),
    };
    assert_eq!(
        NativeUserMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            proposal.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        None
    );
    assert_eq!(
        NativeUserMemoryStore::reobserve_automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            id,
            deletion("absent", 1),
            write_limits()
        )
        .await
        .unwrap(),
        None
    );
    assert!(
        NativeUserMemoryStore::automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            proposal.clone(),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeUserMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &write,
            id,
            deletion("absent", 1),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(!f.user.runtime_root().exists());
    assert!(
        NativeUserMemoryStore::automatic_fold(
            ctx.snapshot(),
            &read,
            &ordinary,
            proposal.clone(),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeUserMemoryStore::automatic_delete(
            ctx.snapshot(),
            &read,
            &ordinary,
            id,
            deletion("ordinary", 1),
            write_limits()
        )
        .await
        .is_err()
    );
    let foreign = request(&cancel);
    assert!(
        NativeUserMemoryStore::reobserve_automatic_fold(
            foreign.snapshot(),
            &read,
            &write,
            proposal,
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(!f.user.runtime_root().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_fold_and_ordinary_member_edit_have_one_serial_winner_without_partial_groups() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    fenced!(f, ctx, verifier, proof, read, ordinary, write);
    let mut authored = input("race-survivor");
    authored.references = Some(vec![reference()]);
    let a = NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let mut authored = input("race-member");
    authored.references = Some(vec![reference()]);
    let b = NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let proposal = fold(&a, &b, "race-fold");
    let edit = update("race-edit", 1);
    let before = version(&f).await;
    let (folded, edited) = tokio::join!(
        NativeUserMemoryStore::automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            proposal.clone(),
            write_limits()
        ),
        NativeUserMemoryStore::update(ctx.snapshot(), &ordinary, b.id, edit, write_limits())
    );
    let expected_inbound = match (folded, edited) {
        (Ok(receipt), Err(MemoryStoreError::NotFound)) => {
            assert_eq!(receipt.survivor.sdk_version, Some(before + 1));
            assert!(
                NativeUserMemoryStore::get(ctx.snapshot(), &read, b.id, read_limits())
                    .await
                    .unwrap()
                    .is_none()
            );
            let removed =
                NativeUserMemoryStore::history(ctx.snapshot(), &read, b.id, read_limits())
                    .await
                    .unwrap()
                    .unwrap();
            assert!(removed.deleted);
            assert!(removed.current.references.is_empty());
            let surviving = NativeUserMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
                .await
                .unwrap()
                .unwrap()
                .memory;
            assert_eq!(surviving.revision, 2);
            assert_eq!(surviving.body, proposal.body);
            1
        }
        (
            Err(MemoryStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }),
            Ok(changed),
        ) => {
            assert_eq!(changed.revision, 2);
            assert_eq!(
                NativeUserMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
                    .await
                    .unwrap()
                    .unwrap()
                    .memory,
                a
            );
            assert_eq!(
                NativeUserMemoryStore::get(ctx.snapshot(), &read, b.id, read_limits())
                    .await
                    .unwrap()
                    .unwrap()
                    .memory,
                changed
            );
            assert!(
                NativeUserMemoryStore::history(ctx.snapshot(), &read, a.id, read_limits())
                    .await
                    .unwrap()
                    .unwrap()
                    .revisions
                    .is_empty()
            );
            2
        }
        other => panic!("actual serial fold/member writer outcome: {other:?}"),
    };
    assert_eq!(version(&f).await, before + 1);
    let UserMemoryReferencesRead::Published {
        incoming: Some(incoming),
        coverage,
        warnings,
        ..
    } = NativeUserMemoryStore::references(ctx.snapshot(), &read, ref_query(), ref_limits())
        .await
        .unwrap()
    else {
        panic!("published refs")
    };
    assert_eq!(incoming.total, expected_inbound);
    assert!(coverage.memory_user_source_complete && warnings.is_empty());
}

#[tokio::test]
async fn paired_automatic_authority_config_path_and_cancellation_deny_without_publication() {
    for ending in 0..3 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::fenced(&ctx).await;
        fenced!(f, ctx, verifier, proof, read, _ordinary, write);
        let mut authored = input("guarded-member");
        authored.attributes.mandatory = false;
        let target =
            NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
                .await
                .unwrap();
        NativeUserMemoryStore::fenced_create(
            ctx.snapshot(),
            &write,
            input("guarded-support"),
            write_limits(),
        )
        .await
        .unwrap();
        let before = version(&f).await;
        match ending {
            0 => {
                let result = f
                    .store
                    .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                        let p = doc.provider("service").unwrap();
                        let revision = p.revision();
                        let at = p.updated_at() + 1;
                        let config = p.config().clone();
                        doc.update_provider("service", config, doc.revision(), revision, at, ctx)
                    })
                    .unwrap();
                assert!(result.failure().is_none());
            }
            1 => {
                let path = f.user.user_root().unwrap();
                std::fs::rename(path, path.with_extension("replaced")).unwrap();
                std::fs::create_dir(path).unwrap();
            }
            2 => cancel.cancel(),
            _ => unreachable!(),
        }
        assert!(
            NativeUserMemoryStore::automatic_delete(
                ctx.snapshot(),
                &read,
                &write,
                target.id,
                deletion("guarded-delete", 1),
                write_limits()
            )
            .await
            .is_err()
        );
        assert!(
            NativeUserMemoryStore::reobserve_automatic_delete(
                ctx.snapshot(),
                &read,
                &write,
                target.id,
                deletion("guarded-delete", 1),
                write_limits()
            )
            .await
            .is_err()
        );
        if ending == 1 {
            assert!(!f.user.runtime_root().join("memory").exists());
        } else {
            assert_eq!(version(&f).await, before);
        }
        assert_eq!(f.server.count("/userinfo"), 0);
        assert_eq!(f.server.count("/token"), 0);
    }
}

#[tokio::test]
async fn automatic_user_head_counter_consumes_eof_and_enforces_exact_metadata_caps_without_other_bodies()
 {
    use arrow_array::{Array, UInt64Array};
    use std::num::NonZeroU32;
    use std::sync::Arc;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    fenced!(f, ctx, verifier, proof, read, _ordinary, write);
    let mut authored = input("counter-target");
    authored.attributes.mandatory = false;
    authored.body = "small target body".into();
    let target =
        NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, authored, write_limits())
            .await
            .unwrap();
    for index in 0..3 {
        let mut other = input(&format!("counter-other-{index}"));
        other.title = format!("other {index} {}", "β".repeat(2048));
        other.body = "other authored body must not be retained by counter ".repeat(512);
        NativeUserMemoryStore::fenced_create(ctx.snapshot(), &write, other, write_limits())
            .await
            .unwrap();
    }
    let good = integrity::physical_rows(&f).await;
    let heads = good
        .iter()
        .filter(|b| integrity::text(b, "kind") == "head")
        .collect::<Vec<_>>();
    assert_eq!(heads.len(), 4);
    let other = heads
        .iter()
        .find(|b| integrity::text(b, "record_id") != target.id.to_string())
        .unwrap();
    let wrong_kind = integrity::replace_text(other, "kind", "wrong-kind-canonical-head");
    let foreign = integrity::replace_text(other, "user_subject", "foreign literal USER");
    let late_actor = integrity::replace_text(other, "updated_by", "invalid actor");
    let mut columns = other.columns().to_vec();
    columns[other.schema().index_of("revision").unwrap()] = Arc::new(UInt64Array::from(vec![0]));
    let malformed = arrow_array::RecordBatch::try_new(other.schema(), columns).unwrap();
    let mut duplicate = good.clone();
    duplicate.push((*other).clone());
    let mut corrupted = vec![duplicate];
    for bad in [wrong_kind, foreign, late_actor, malformed] {
        corrupted.push(
            good.iter()
                .map(|b| {
                    if integrity::text(b, "row_key") == integrity::text(other, "row_key") {
                        bad.clone()
                    } else {
                        b.clone()
                    }
                })
                .collect(),
        );
    }
    for (index, batches) in corrupted.into_iter().enumerate() {
        integrity::replace(&f, batches).await;
        let before = version(&f).await;
        let mut bounded = write_limits();
        bounded.read.batch_rows = NonZeroU32::new(1).unwrap();
        assert!(
            matches!(
                NativeUserMemoryStore::automatic_delete(
                    ctx.snapshot(),
                    &read,
                    &write,
                    target.id,
                    deletion(&format!("corrupt-counter-{index}"), 1),
                    bounded
                )
                .await,
                Err(MemoryStoreError::Invalid(_))
            ),
            "metadata corruption {index}"
        );
        assert_eq!(version(&f).await, before);
    }
    integrity::replace(&f, good.clone()).await;
    let before = version(&f).await;
    let columns = [
        "row_key",
        "kind",
        "record_id",
        "installation_id",
        "user_issuer",
        "user_subject",
        "title",
        "created_by",
        "updated_by",
    ];
    let metadata_bytes = heads
        .iter()
        .map(|b| {
            columns
                .iter()
                .map(|name| integrity::text(b, name).len())
                .sum::<usize>()
        })
        .sum::<usize>();
    let target_bytes = good
        .iter()
        .filter(|b| integrity::text(b, "record_id") == target.id.to_string())
        .map(|b| {
            b.columns()
                .iter()
                .filter_map(|a| a.as_any().downcast_ref::<arrow_array::StringArray>())
                .map(|a| if a.is_null(0) { 0 } else { a.value(0).len() })
                .sum::<usize>()
        })
        .sum::<usize>();
    assert!(
        metadata_bytes > target_bytes,
        "counter text boundary must exceed target-chain bytes"
    );
    assert!(good.iter().any(|b| integrity::text(b, "kind") == "head"
        && integrity::text(b, "payload").len() > metadata_bytes));
    let mut exact = write_limits();
    exact.read.batch_rows = NonZeroU32::new(1).unwrap();
    exact.read.max_query_bytes = NonZeroUsize::MIN;
    let AutomaticDeleteOutcome::Deleted(effect) = NativeUserMemoryStore::automatic_delete(
        ctx.snapshot(),
        &read,
        &write,
        target.id,
        deletion("counter-exact", 1),
        exact,
    )
    .await
    .unwrap() else {
        panic!("metadata-only counter permits actual bounded removal")
    };
    assert_eq!(effect.sdk_version, Some(before + 1));
    assert_eq!(version(&f).await, before + 1);
    let UserMemoryListRead::Published { memories, .. } = NativeUserMemoryStore::list(
        ctx.snapshot(),
        &read,
        MemoryListFilter::default(),
        read_limits(),
    )
    .await
    .unwrap() else {
        panic!("remaining heads")
    };
    assert_eq!(memories.len(), 3);
    assert!(memories.iter().all(|m| m.id != target.id));
}
