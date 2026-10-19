use super::*;
use crate::auth_document::test_support::{ArmError, PublicationCancellation};

fn pending(result: AuthDocumentPublication) -> PendingCleanup {
    assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
    assert_eq!(result.failure(), Some(AuthDocumentError::Cancelled));
    assert_eq!(result.cleanup_effect(), AuthCleanupEffect::NotApplied);
    assert_eq!(result.cleanup_failure(), Some(AuthDocumentError::Cancelled));
    result.into_pending_cleanup().unwrap()
}
fn assert_destination(fixture: &Fixture, old: Option<&[u8]>) {
    let path = fixture.root.join("auth.json");
    if let Some(old) = old {
        assert_eq!(fs::read(path).unwrap(), old);
    } else {
        assert!(!path.exists());
    }
}
fn real_cancelled_publication(old: Option<&[u8]>) {
    let fixture = Fixture::new();
    if let Some(old) = old {
        fixture.write("auth.json", old);
    }
    let ctx = context();
    let store = fixture.store(&ctx);
    let snapshot = store.read(&ctx).unwrap();

    let guard = PublicationCancellation::before_replace(&store, &ctx).unwrap();

    let calls = NATIVE_REPLACE_CALLS.with(|calls| calls.get());
    let proof = pending(store.publish(snapshot, b"new", &ctx));
    assert!(Arc::ptr_eq(&proof.store_identity, &store.identity));
    assert_eq!(proof.scratch.path(), fixture.root.join(STAGE));
    assert_eq!(NATIVE_REPLACE_CALLS.with(|calls| calls.get()), calls);
    assert_destination(&fixture, old);
    assert_eq!(fs::read(fixture.root.join(STAGE)).unwrap(), b"new");
    assert!(ctx.check().is_err());

    assert!(matches!(
        PublicationCancellation::before_replace(&store, &context()),
        Err(ArmError::Busy)
    ));
    drop(guard);

    assert!(fixture.root.join(STAGE).exists());
    assert!(ctx.check().is_err());
    let recovered = store.cleanup_pending(proof, &context());
    assert_eq!(recovered.effect(), AuthCleanupEffect::Applied);
    assert_eq!(recovered.failure(), None);
    assert!(recovered.into_pending_cleanup().is_none());
    assert!(!fixture.root.join(STAGE).exists());
    assert_destination(&fixture, old);
}
#[test]
fn real_before_replace_preserves_old_destination_and_original_pending() {
    real_cancelled_publication(Some(b"old"));
}
#[test]
fn real_before_replace_preserves_cold_start_absence_and_original_pending() {
    real_cancelled_publication(None);
}
#[test]
fn nested_busy_and_early_drop_keep_one_lease_without_cancellation() {
    let fixture = Fixture::new();
    let ctx = context();
    let store = fixture.store(&ctx);
    let guard = PublicationCancellation::before_replace(&store, &ctx).unwrap();
    assert!(matches!(
        PublicationCancellation::before_replace(&store, &ctx),
        Err(ArmError::Busy)
    ));
    drop(guard);
    assert!(ctx.check().is_ok());
    let result = store.publish(store.read(&ctx).unwrap(), b"ordinary", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert_eq!(result.failure(), None);
    let again = PublicationCancellation::before_replace(&store, &ctx).unwrap();
    drop(again);
}
#[test]
fn oversized_body_does_not_fire_and_drop_allows_valid_publication() {
    let fixture = Fixture::new();
    let ctx = context();
    let store = Fixture::store_at(&fixture.root, &ctx, 2);
    let guard = PublicationCancellation::before_replace(&store, &ctx).unwrap();
    let result = store.publish(store.read(&ctx).unwrap(), b"new", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::NotApplied);
    assert_eq!(result.failure(), Some(AuthDocumentError::Limit));
    assert!(result.into_pending_cleanup().is_none());
    assert!(!fixture.root.join(STAGE).exists());
    assert_destination(&fixture, None);
    assert!(ctx.check().is_ok());
    assert!(matches!(
        PublicationCancellation::before_replace(&store, &ctx),
        Err(ArmError::Busy)
    ));

    drop(guard);

    let result = store.publish(store.read(&ctx).unwrap(), b"ok", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert_eq!(result.failure(), None);
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"ok");
    assert!(ctx.check().is_ok());
}
#[test]
fn intentional_panic_unwind_drops_guard_and_next_publication_is_ordinary() {
    let fixture = Fixture::new();
    let ctx = context();
    let store = fixture.store(&ctx);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = PublicationCancellation::before_replace(&store, &ctx).unwrap();
        panic!("intentional fixture guard unwind");
    }));
    assert!(panic.is_err());
    assert!(ctx.check().is_ok());
    let result = store.publish(store.read(&ctx).unwrap(), b"ordinary", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    let guard = PublicationCancellation::before_replace(&store, &ctx).unwrap();
    drop(guard);
}
#[test]
fn foreign_store_with_equal_logical_owner_does_not_consume_lease_and_denies_proof() {
    let first = Fixture::new();
    let other = Fixture::new();
    let ctx = context();
    let store = first.store(&ctx);
    let foreign = other.store(&ctx);
    assert_eq!(store.scope.owner(), foreign.scope.owner());
    let guard = PublicationCancellation::before_replace(&store, &ctx).unwrap();
    let result = foreign.publish(foreign.read(&ctx).unwrap(), b"foreign", &ctx);
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert!(ctx.check().is_ok());
    let proof = pending(store.publish(store.read(&ctx).unwrap(), b"actual", &ctx));
    let identity = Arc::clone(&proof.store_identity);
    drop(guard);
    let denied = foreign.cleanup_pending(proof, &context());
    assert_eq!(denied.effect(), AuthCleanupEffect::NotApplied);
    assert_eq!(denied.failure(), Some(AuthDocumentError::Conflict));
    let proof = denied.into_pending_cleanup().unwrap();
    assert!(Arc::ptr_eq(&proof.store_identity, &identity));
    assert!(first.root.join(STAGE).exists());
    let cleaned = store.cleanup_pending(proof, &context());
    assert_eq!(cleaned.effect(), AuthCleanupEffect::Applied);
    assert!(!first.root.join(STAGE).exists());
    assert_eq!(fs::read(other.root.join("auth.json")).unwrap(), b"foreign");
}
#[test]
fn fresh_request_with_same_deadline_and_cancellation_does_not_consume_lease() {
    let fixture = Fixture::new();
    let ctx = context();
    let store = fixture.store(&ctx);
    let fresh = SnapshotContext::new(ctx.deadline().unwrap(), ctx.cancellation().clone());
    assert!(!ctx.same_request(&fresh));
    let guard = PublicationCancellation::before_replace(&store, &ctx).unwrap();
    let result = store.publish(store.read(&fresh).unwrap(), b"first", &fresh);
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert!(ctx.check().is_ok());
    let proof = pending(store.publish(store.read(&ctx).unwrap(), b"second", &ctx));
    assert_eq!(fs::read(fixture.root.join("auth.json")).unwrap(), b"first");
    drop(guard);
    assert_eq!(
        store.cleanup_pending(proof, &context()).effect(),
        AuthCleanupEffect::Applied
    );
}
#[test]
fn shared_request_clone_observes_original_cancellation() {
    let fixture = Fixture::new();
    let ctx = std::sync::Arc::new(context());
    let shared = std::sync::Arc::clone(&ctx);
    assert!(ctx.same_request(&shared));
    let store = fixture.store(&ctx);
    let guard = PublicationCancellation::before_replace(&store, &ctx).unwrap();
    let proof = pending(store.publish(store.read(&shared).unwrap(), b"new", &shared));
    assert!(ctx.check().is_err());
    assert!(shared.check().is_err());
    drop(guard);
    assert_eq!(
        store.cleanup_pending(proof, &context()).effect(),
        AuthCleanupEffect::Applied
    );
}
#[test]
fn same_store_and_request_on_other_thread_are_not_cancelled() {
    let fixture = Fixture::new();
    let ctx = context();
    let store = fixture.store(&ctx);
    let guard = PublicationCancellation::before_replace(&store, &ctx).unwrap();
    thread::scope(|scope| {
        scope
            .spawn(|| {
                let result = store.publish(store.read(&ctx).unwrap(), b"other-thread", &ctx);
                assert_eq!(result.effect(), AuthPublicationEffect::Applied);
                assert_eq!(result.failure(), None);
            })
            .join()
            .unwrap();
    });
    assert!(ctx.check().is_ok());
    let proof = pending(store.publish(store.read(&ctx).unwrap(), b"main-thread", &ctx));
    assert_eq!(
        fs::read(fixture.root.join("auth.json")).unwrap(),
        b"other-thread"
    );
    drop(guard);
    assert_eq!(
        store.cleanup_pending(proof, &context()).effect(),
        AuthCleanupEffect::Applied
    );
}
#[test]
fn invalid_context_has_static_error_zero_io_and_no_lease() {
    let fixture = Fixture::new();
    let alive = context();
    let store = fixture.store(&alive);
    let cancelled = context();
    cancelled.cancellation().cancel();
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    for invalid in [&cancelled, &expired] {
        assert!(matches!(
            PublicationCancellation::before_replace(&store, invalid),
            Err(ArmError::InvalidContext)
        ));
    }
    assert_eq!(
        ArmError::Busy.to_string(),
        "publication cancellation fixture already armed"
    );
    assert_eq!(
        ArmError::InvalidContext.to_string(),
        "publication cancellation fixture context is not live"
    );
    let guard = PublicationCancellation::before_replace(&store, &alive).unwrap();
    drop(guard);
}
