use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    io::ErrorKind,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn shared_context_preserves_identity_and_equal_inputs_do_not() {
    let deadline = Instant::now() + Duration::from_secs(30);
    let signal = Cancellation::default();
    let original = Arc::new(SnapshotContext::new(deadline, signal.clone()));
    let shared = Arc::clone(&original);
    let fresh = SnapshotContext::new(deadline, signal);
    assert!(original.same_request(&shared));
    assert!(!original.same_request(&fresh));
    assert_eq!(shared.deadline(), Some(deadline));
    shared.check().unwrap();
}

#[test]
fn shared_signal_cancels_original_context_and_its_borrows() {
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    let shared = Arc::clone(&original);
    original.cancellation().cancel();
    assert!(shared.cancellation().is_cancelled());
    assert_eq!(shared.check().unwrap_err().kind(), ErrorKind::Interrupted);
}

#[test]
fn expired_deadline_is_not_extended_and_cancellation_keeps_precedence() {
    let deadline = Instant::now();
    let context = SnapshotContext::new(deadline, Cancellation::default());
    assert_eq!(context.deadline(), Some(deadline));
    assert_eq!(context.check().unwrap_err().kind(), ErrorKind::TimedOut);
    context.cancellation().cancel();
    assert_eq!(context.check().unwrap_err().kind(), ErrorKind::Interrupted);
}
