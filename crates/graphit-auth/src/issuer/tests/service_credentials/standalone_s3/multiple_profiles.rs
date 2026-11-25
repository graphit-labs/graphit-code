use super::*;

#[test]
fn standalone_policy_keeps_anonymous_and_profile_admissions_distinct() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let fixture = Fixture::new(&ctx);
    let store = native_store(&fixture, &ctx);
    put(&store, local_config(), profile(None), false, &ctx);
    let anonymous = ServiceRuleInput::StandaloneS3 {
        purpose: ServicePurpose::HubRead,
        origin: ServiceOrigin::Runtime,
        provider: "service",
        profile: None,
    };
    let issuer = issuer(&fixture, store, &ctx, &[anonymous, rule()]);
    let caller = issuer
        .authenticate_runtime(&fixture.secret(), &ctx)
        .unwrap();
    for profile in [None, Some("transport")] {
        let mut selected = issuer
            .capture_standalone_s3_service(&caller, "service", profile)
            .unwrap();
        let lease = selected.credential().unwrap();
        match (profile, lease.standalone_s3().unwrap().credential) {
            (None, S3CredentialView::Anonymous)
            | (Some("transport"), S3CredentialView::Local(_)) => {}
            _ => panic!("each explicitly admitted profile must retain its own credential kind"),
        }
    }
}
