use super::*;
use graphit_auth::{Obligation, UserMemoryWriteOperation};
use graphit_core::file_lock::Cancellation;
use graphit_records::reference_projection::ReferenceTargetKind;
macro_rules! fenced {
    ($f:ident,$ctx:ident,$verifier:ident,$proof:ident,$read:ident,$ordinary:ident,$write:ident) => {
        incoming!($f, $ctx, $verifier, $proof, $read, $ordinary);
        let $write = $f
            .issuer
            .admit_incoming_user_memory_write(
                &$proof,
                &$verifier,
                &$ctx,
                "service",
                $f.scope(),
                graphit_auth::UserMemoryWriteOperation::FencedMemory,
            )
            .unwrap();
        assert_eq!(
            $write.operation().unwrap(),
            graphit_auth::UserMemoryWriteOperation::FencedMemory
        );
        assert_eq!(
            $write.recheck().unwrap(),
            graphit_auth::Obligation::MemoryFenceRequired
        );
    };
}
pub(super) use fenced;
pub(super) fn patch(key: &str, revision: u64) -> FencedUpdateMemory {
    FencedUpdateMemory {
        expected_revision: revision,
        idempotency_key: key.into(),
        reason: "observed literal content β".into(),
        title: None,
        body: None,
        references: None,
    }
}
pub(super) fn deletion(key: &str, revision: u64) -> DeleteMemory {
    DeleteMemory {
        expected_revision: revision,
        idempotency_key: key.into(),
        reason: "selected removal β".into(),
    }
}
pub(super) fn reference() -> MemoryReference {
    MemoryReference {
        target_kind: ReferenceTargetKind::Task,
        scope: MemoryReferenceScope::User("opaque other principal".into()),
        id: "literal-task-no-target-grant".into(),
        context: None,
        relation: "supports".into(),
        field: Some("body".into()),
    }
}
pub(super) async fn version(f: &Fixture) -> u64 {
    let uri = url::Url::from_file_path(f.user.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    graphit_data::lance_branch_store::LanceBranchStore::open(&uri, user_memory_schema(), None)
        .await
        .unwrap()
        .capture_target(None)
        .await
        .unwrap()
        .version()
}

#[tokio::test]
async fn fenced_user_lifecycle_preserves_attributes_refs_and_historical_attempts() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    fenced!(f, ctx, verifier, proof, read, _ordinary, write);
    let mut authored = input("fenced-create '\nβ");
    authored.body = "fenceduserquartz".into();
    authored.attributes.classification =
        graphit_records::memory_attributes::MemoryClass::Correction;
    authored.references = Some(vec![reference(), reference()]);
    assert_eq!(
        NativeUserMemoryStore::reobserve_fenced_create(
            ctx.snapshot(),
            &write,
            authored.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        MemoryCreateObservation::Unresolved
    );
    assert!(!f.user.runtime_root().exists());
    let initial = NativeUserMemoryStore::fenced_create(
        ctx.snapshot(),
        &write,
        authored.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    let UserMemoryLexicalSearchRead::Published {
        total_hits, hits, ..
    } = NativeUserMemoryStore::search_lexical(
        ctx.snapshot(),
        &read,
        MemoryLexicalQuery {
            text: authored.body.clone(),
            filter: MemoryListFilter::default(),
            include_deleted_history: false,
            top_k: std::num::NonZeroUsize::new(10).unwrap(),
        },
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("fenced USER create must initialize FTS");
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current.id, initial.id);
    assert_eq!(integrity::physical_rows(&f).await.len(), 3);
    let mut edit = patch("fenced-edit", 1);
    edit.title = Some("literal fenced title β".into());
    edit.body = Some("literal fenced body".into());
    let second = NativeUserMemoryStore::fenced_update(
        ctx.snapshot(),
        &write,
        initial.id,
        edit.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(second.attributes, initial.attributes);
    assert_eq!(second.tags, initial.tags);
    assert_eq!(second.created_at, initial.created_at);
    assert_eq!(second.created_by, initial.created_by);
    assert_eq!(second.references, initial.references);
    assert_eq!(second.reference_generation, 1);
    let promote = ChangeMemoryImportance {
        expected_revision: 2,
        idempotency_key: "fenced-promote".into(),
        reason: "importance only".into(),
    };
    let third = NativeUserMemoryStore::fenced_promote(
        ctx.snapshot(),
        &write,
        initial.id,
        promote.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(third.attributes.important && third.attributes.mandatory);
    let before = version(&f).await;
    let noop = ChangeMemoryImportance {
        expected_revision: 3,
        idempotency_key: "noop".into(),
        reason: "already important".into(),
    };
    assert_eq!(
        NativeUserMemoryStore::fenced_promote(
            ctx.snapshot(),
            &write,
            initial.id,
            noop,
            write_limits()
        )
        .await
        .unwrap(),
        third
    );
    assert_eq!(version(&f).await, before);
    let stale = ChangeMemoryImportance {
        expected_revision: 2,
        idempotency_key: "stale-noop".into(),
        reason: "stale".into(),
    };
    assert!(
        NativeUserMemoryStore::fenced_promote(
            ctx.snapshot(),
            &write,
            initial.id,
            stale,
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(version(&f).await, before);
    let demote = ChangeMemoryImportance {
        expected_revision: 3,
        idempotency_key: "fenced-demote".into(),
        reason: "importance only".into(),
    };
    let fourth = NativeUserMemoryStore::fenced_demote(
        ctx.snapshot(),
        &write,
        initial.id,
        demote.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(!fourth.attributes.important);
    assert!(fourth.attributes.mandatory);
    assert_eq!(fourth.reference_generation, 1);
    let mut clear = patch("fenced-clear", 4);
    clear.references = Some(vec![]);
    let fifth = NativeUserMemoryStore::fenced_update(
        ctx.snapshot(),
        &write,
        initial.id,
        clear,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(fifth.reference_generation, 2);
    let delete = deletion("fenced-delete", 5);
    let last = NativeUserMemoryStore::fenced_delete(
        ctx.snapshot(),
        &write,
        initial.id,
        delete.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(last.attributes, fifth.attributes);
    assert_eq!(last.reference_generation, 3);
    assert!(last.references.is_empty());
    let published = version(&f).await;
    assert_eq!(
        NativeUserMemoryStore::fenced_create(
            ctx.snapshot(),
            &write,
            authored.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        initial
    );
    assert_eq!(
        NativeUserMemoryStore::reobserve_fenced_create(
            ctx.snapshot(),
            &write,
            authored.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        MemoryCreateObservation::Found(initial.clone())
    );
    for (op, after) in [
        (FencedMemoryMutation::Update(edit.clone()), second.clone()),
        (
            FencedMemoryMutation::Promote(promote.clone()),
            third.clone(),
        ),
        (FencedMemoryMutation::Demote(demote.clone()), fourth.clone()),
        (FencedMemoryMutation::Delete(delete.clone()), last.clone()),
    ] {
        let event = NativeUserMemoryStore::reobserve_fenced_mutation(
            ctx.snapshot(),
            &write,
            initial.id,
            op,
            write_limits(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(event.after, after);
        assert_eq!(event.owner.identity.subject(), SUBJECT);
    }
    assert_eq!(
        NativeUserMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            edit,
            write_limits()
        )
        .await
        .unwrap(),
        second
    );
    assert_eq!(
        NativeUserMemoryStore::fenced_promote(
            ctx.snapshot(),
            &write,
            initial.id,
            promote,
            write_limits()
        )
        .await
        .unwrap(),
        third
    );
    assert_eq!(
        NativeUserMemoryStore::fenced_demote(
            ctx.snapshot(),
            &write,
            initial.id,
            demote,
            write_limits()
        )
        .await
        .unwrap(),
        fourth
    );
    assert_eq!(
        NativeUserMemoryStore::fenced_delete(
            ctx.snapshot(),
            &write,
            initial.id,
            delete,
            write_limits()
        )
        .await
        .unwrap(),
        last
    );
    assert_eq!(version(&f).await, published);
    let history = NativeUserMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(history.deleted);
    assert_eq!(history.revisions.len(), 5);
    assert_eq!(history.current, last);
    assert_eq!(history.creation.input, authored);
}

#[tokio::test]
async fn ordinary_permission_and_original_denial_never_birth_fenced_user_corpus() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, _read, ordinary);
    let id = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
    assert!(
        f.issuer
            .admit_incoming_user_memory_write(
                &proof,
                &verifier,
                &ctx,
                "service",
                f.scope(),
                UserMemoryWriteOperation::FencedMemory
            )
            .is_err()
    );
    assert!(
        NativeUserMemoryStore::fenced_create(
            ctx.snapshot(),
            &ordinary,
            input("deny"),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeUserMemoryStore::reobserve_fenced_create(
            ctx.snapshot(),
            &ordinary,
            input("deny"),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeUserMemoryStore::fenced_update(
            ctx.snapshot(),
            &ordinary,
            id,
            patch("deny", 1),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeUserMemoryStore::fenced_delete(
            ctx.snapshot(),
            &ordinary,
            id,
            deletion("deny", 1),
            write_limits()
        )
        .await
        .is_err()
    );
    let change = ChangeMemoryImportance {
        expected_revision: 1,
        idempotency_key: "deny".into(),
        reason: "deny".into(),
    };
    assert!(
        NativeUserMemoryStore::fenced_promote(
            ctx.snapshot(),
            &ordinary,
            id,
            change.clone(),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeUserMemoryStore::fenced_demote(ctx.snapshot(), &ordinary, id, change, write_limits())
            .await
            .is_err()
    );
    assert!(
        NativeUserMemoryStore::reobserve_fenced_mutation(
            ctx.snapshot(),
            &ordinary,
            id,
            FencedMemoryMutation::Delete(deletion("deny", 1)),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(!f.user.runtime_root().exists());
    let f = Fixture::fenced(&ctx).await;
    fenced!(f, ctx, verifier, proof, _read, _ordinary, write);
    let foreign = request(&cancel);
    assert!(
        NativeUserMemoryStore::fenced_create(
            foreign.snapshot(),
            &write,
            input("foreign-original"),
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(!f.user.runtime_root().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_fenced_concurrent_create_cas_and_changed_attempt_key_conflict() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::native_fenced(&ctx).await;
    f.install_native_profile(&ctx);
    let verifier = f.server.verifier(false, f.provider_revision(&ctx));
    let secret = f.secret();
    let caller = f
        .issuer
        .authenticate_runtime(&secret, &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let read = f
        .issuer
        .admit_authenticated_user_memory_read(&caller, &ctx, f.scope())
        .unwrap();
    let write = f
        .issuer
        .admit_authenticated_user_memory_write(
            &caller,
            &ctx,
            f.scope(),
            UserMemoryWriteOperation::FencedMemory,
        )
        .unwrap();
    assert_eq!(write.recheck().unwrap(), Obligation::MemoryFenceRequired);
    let authored = input("native-concurrent");
    let (a, b) = tokio::join!(
        NativeUserMemoryStore::fenced_create(
            ctx.snapshot(),
            &write,
            authored.clone(),
            write_limits()
        ),
        NativeUserMemoryStore::fenced_create(
            ctx.snapshot(),
            &write,
            authored.clone(),
            write_limits()
        )
    );
    let initial = a.unwrap();
    assert_eq!(b.unwrap(), initial);
    assert_eq!(integrity::physical_rows(&f).await.len(), 3);
    let initial_version = version(&f).await;
    let mut a = patch("winner-a", 1);
    a.body = Some("winner A".into());
    let mut b = patch("winner-b", 1);
    b.body = Some("winner B".into());
    let (left, right) = tokio::join!(
        NativeUserMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            a.clone(),
            write_limits()
        ),
        NativeUserMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            b.clone(),
            write_limits()
        )
    );
    let (winner, attempt) = match (left, right) {
        (
            Ok(m),
            Err(MemoryStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }),
        ) => (m, a),
        (
            Err(MemoryStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }),
            Ok(m),
        ) => (m, b),
        other => panic!("one actual CAS winner: {other:?}"),
    };
    assert_eq!(version(&f).await, initial_version + 1);
    let mut altered = attempt.clone();
    altered.body = Some("changed same key".into());
    assert!(matches!(
        NativeUserMemoryStore::fenced_update(
            ctx.snapshot(),
            &write,
            initial.id,
            altered,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    let mut later = patch("later", 2);
    later.body = Some("later content".into());
    NativeUserMemoryStore::fenced_update(ctx.snapshot(), &write, initial.id, later, write_limits())
        .await
        .unwrap();
    let v = version(&f).await;
    assert_eq!(
        NativeUserMemoryStore::fenced_update(
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
    assert_eq!(
        NativeUserMemoryStore::reobserve_fenced_mutation(
            ctx.snapshot(),
            &write,
            initial.id,
            FencedMemoryMutation::Update(attempt),
            write_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .after,
        winner
    );
    assert_eq!(version(&f).await, v);
    assert_eq!(
        NativeUserMemoryStore::history(ctx.snapshot(), &read, initial.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .revisions
            .len(),
        2
    );
    assert_eq!(f.server.count("/userinfo"), 0);
    assert_eq!(f.server.count("/token"), 0);
}
