//! Physical provider-service revocation shared by Models and Memory fixtures.
use graphit_auth::{profile::AiService, store};
use graphit_core::auth_document::publication::AuthPublicationEffect;
use graphit_hub::policy::RequestContext;

/// Runtime Local service authority observes the provider binding, independently
/// of Local profile credentials and the retained PROJECT record admission.
#[allow(
    dead_code,
    reason = "shared fixture; service revocation is exercised by model and project memory tests"
)]
pub fn revoke_service(store: &store::AuthStore, ctx: &RequestContext<'_>) {
    let result = store
        .mutate(store.load(ctx).unwrap(), ctx, |doc, ctx| {
            let provider = doc.provider("service").unwrap();
            let mut config = provider.config().clone();
            config.embedding = AiService::Disabled;
            doc.update_provider(
                "service",
                config,
                doc.revision(),
                provider.revision(),
                provider.updated_at() + 1,
                ctx,
            )
        })
        .unwrap();
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert!(result.failure().is_none());
    assert!(result.cleanup_failure().is_none());
}
