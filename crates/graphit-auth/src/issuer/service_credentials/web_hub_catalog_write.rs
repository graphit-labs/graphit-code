//! Remote catalog effects retain Web authority without borrowing a local source.
use super::*;
use crate::{
    broker::project_access::BrokerProjectCaller,
    scope::{PhysicalScopeInput, ScopeCapture},
};

/// Private capture; the existing selection owns its AuthStore observation.
#[derive(Clone, Copy)]
pub(in crate::issuer) struct WebHubCatalogWrite<'a> {
    pub(super) issuer: &'a RoleIssuer,
    pub(super) caller: &'a AuthenticatedWebCaller<'a>,
    pub(super) verifier: &'a BrokerVerifier,
    pub(super) original: &'a RequestContext<'a>,
    pub(super) provider: &'a str,
    installation: ScopeCapture<'a>,
}

impl WebHubCatalogWrite<'_> {
    pub(super) fn check(self, issuer: &RoleIssuer) -> Result<(), ServiceUseError> {
        original_check(self.original)?;
        if !std::ptr::eq(self.issuer, issuer) {
            return Err(AuthError::Denied.into());
        }
        self.caller.check(
            self.verifier,
            BrokerRequestContext::new(self.original.snapshot()),
        )?;
        issuer
            .local
            .recheck_installation(&issuer.realm, self.original.snapshot())?;
        issuer.validate_physical_scope(
            &PhysicalScopeInput {
                installation: self.installation,
                project: None,
                providers: &[],
            },
            self.original,
        )?;
        original_check(self.original)
    }

    pub(super) fn check_provider(
        self,
        record: &crate::profile::ProviderRecord,
    ) -> Result<(), ServiceUseError> {
        if !BrokerProjectCaller::Web(self.caller).matches_provider(self.verifier, record) {
            return Err(AuthError::Denied.into());
        }
        Ok(())
    }
}

impl RoleIssuer {
    /// Select the session's exact Web/HubWrite/IncomingBearer service for a
    /// remote catalog effect. The Broker and current STS grants authorize the
    /// destination; this capture grants no local PROJECT or installation effects.
    pub fn capture_web_hub_catalog_write_service<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        installation: ScopeCapture<'a>,
    ) -> Result<ServiceSelection<'a>, ServiceUseError> {
        self.capture_service_authority(
            Authority::WebHubCatalogWrite(WebHubCatalogWrite {
                issuer: self,
                caller,
                verifier,
                original,
                provider,
                installation,
            }),
            ServicePurpose::HubWrite,
            Some(provider),
        )
    }
}
