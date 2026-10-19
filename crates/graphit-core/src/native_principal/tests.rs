use super::*;
use crate::file_lock::Cancellation;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn proof(ctx: &SnapshotContext) -> NativeProcessPrincipal {
    NativeProcessPrincipal::capture(ctx).expect("native SDK capture")
}

#[test]
fn real_capture_recheck_and_thread_transfer_use_original_request_without_io() {
    let ctx = Arc::new(context());
    let p = proof(&ctx);
    p.recheck(&ctx).unwrap();
    let other = Arc::clone(&ctx);
    std::thread::spawn(move || p.recheck(&other))
        .join()
        .unwrap()
        .unwrap();
}
#[test]
fn matching_deadline_and_cancellation_do_not_create_same_request() {
    let ctx = context();
    let p = proof(&ctx);
    let fresh = SnapshotContext::new(ctx.deadline().unwrap(), ctx.cancellation().clone());
    assert_eq!(
        p.recheck_using(&fresh, |_| panic!("SDK on wrong request")),
        Err(PrincipalError::WrongRequest)
    );
}
#[test]
fn rechecking_the_original_request_preserves_identity() {
    let ctx = context();
    let p = proof(&ctx);
    p.recheck(&ctx).unwrap();
    let rechecked = proof(&ctx);
    rechecked.recheck(&ctx).unwrap();
    drop(rechecked);
    drop(p);
}
#[test]
fn cancelled_original_cannot_be_rescued_by_fresh_context() {
    let ctx = context();
    let p = proof(&ctx);
    ctx.cancellation().cancel();
    assert_eq!(
        p.recheck_using(&ctx, |_| panic!("SDK after cancellation")),
        Err(PrincipalError::Cancelled)
    );
    assert!(matches!(
        NativeProcessPrincipal::capture_using(&ctx, || panic!("SDK after cancellation")),
        Err(PrincipalError::Cancelled)
    ));
    assert_eq!(p.recheck(&context()), Err(PrincipalError::WrongRequest));
}
#[test]
fn expired_original_denies_before_sdk() {
    let ctx = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert!(matches!(
        NativeProcessPrincipal::capture_using(&ctx, || panic!("SDK after deadline")),
        Err(PrincipalError::Deadline)
    ));
}
#[test]
fn cancellation_after_sdk_capture_discards_observation() {
    let ctx = context();
    assert!(matches!(
        NativeProcessPrincipal::capture_using(&ctx, || {
            let observation = NativePrincipalObservation::capture()?;
            ctx.cancellation().cancel();
            Ok(observation)
        }),
        Err(PrincipalError::Cancelled)
    ));
}
#[test]
fn recheck_checks_original_liveness_after_sdk() {
    let ctx = context();
    let p = proof(&ctx);
    assert_eq!(
        p.recheck_using(&ctx, |observation| {
            observation.recheck()?;
            ctx.cancellation().cancel();
            Ok(())
        }),
        Err(PrincipalError::Cancelled)
    );
}
#[test]
fn closed_native_errors_remain_static_and_are_not_rebased() {
    let ctx = context();
    let p = proof(&ctx);
    for (native, expected) in [
        (
            NativePrincipalError::Unavailable,
            PrincipalError::Unavailable,
        ),
        (
            NativePrincipalError::InvalidObservation,
            PrincipalError::InvalidObservation,
        ),
        (NativePrincipalError::Changed, PrincipalError::Changed),
        (
            NativePrincipalError::Unsupported,
            PrincipalError::Unsupported,
        ),
    ] {
        assert_eq!(p.recheck_using(&ctx, |_| Err(native)), Err(expected));
        assert_eq!(
            format!("{expected}"),
            format!("native principal: {expected:?}")
        );
    }
    p.recheck(&ctx).unwrap();
}
#[test]
fn proof_is_automatically_send_sync() {
    fn check<T: Send + Sync>() {}
    check::<NativeProcessPrincipal>();
}

#[test]
fn cold_native_test_binary_requires_no_authentication_state() {
    const CHILD: &str = "GRAPHIT_PRINCIPAL_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let ctx = context();
        proof(&ctx).recheck(&ctx).unwrap();

        println!("principal-cold-native-ok");
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native_principal::tests::cold_native_test_binary_requires_no_authentication_state",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "cold native capture failed");
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("principal-cold-native-ok")
    );
}

#[test]
fn original_cancellation_is_checked_even_when_sdk_fails() {
    let ctx = context();
    assert!(matches!(
        NativeProcessPrincipal::capture_using(&ctx, || {
            ctx.cancellation().cancel();
            Err(NativePrincipalError::Unavailable)
        }),
        Err(PrincipalError::Cancelled)
    ));
    let ctx = context();
    let p = proof(&ctx);
    assert_eq!(
        p.recheck_using(&ctx, |_| {
            ctx.cancellation().cancel();
            Err(NativePrincipalError::Changed)
        }),
        Err(PrincipalError::Cancelled)
    );
}

#[test]
fn recheck_preserves_absolute_deadline_before_and_after_sdk() {
    let ctx = SnapshotContext::new(
        Instant::now() + Duration::from_millis(150),
        Cancellation::default(),
    );
    let p = proof(&ctx);
    assert_eq!(
        p.recheck_using(&ctx, |observation| {
            observation.recheck()?;
            std::thread::sleep(
                ctx.deadline()
                    .unwrap()
                    .saturating_duration_since(Instant::now())
                    + Duration::from_millis(1),
            );
            Ok(())
        }),
        Err(PrincipalError::Deadline)
    );
    assert_eq!(
        p.recheck_using(&ctx, |_| panic!("SDK after deadline")),
        Err(PrincipalError::Deadline)
    );
}
