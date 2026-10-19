use super::*;
use crate::{
    broker::{
        BrokerError, BrokerRequestContext, BrokerVerifier, oauth::web::AuthenticatedWebCaller,
        project_access::BrokerProjectCaller,
    },
    store::LocalObservation,
};

/// Read authority for Hub artifacts physically installed in one installation.
/// Artifact metadata is selection data; it grants no USER or PROJECT authority.
pub struct InstalledHubReadAdmission<'a> {
    issuer: &'a RoleIssuer,
    caller: &'a AuthenticatedWebCaller<'a>,
    verifier: &'a BrokerVerifier,
    original: &'a RequestContext<'a>,
    provider: &'a str,
    installation: ScopeCapture<'a>,
    observation: Mutex<LocalObservation>,
}

fn broker_error(error: BrokerError) -> AuthError {
    match error {
        BrokerError::Cancelled | BrokerError::Deadline => AuthError::Request,
        BrokerError::InvalidToken => AuthError::Expired,
        _ => AuthError::Denied,
    }
}

impl RoleIssuer {
    fn web_installed_hub_policy(&self, provider: &str) -> Result<(), AuthError> {
        self.policy
            .web_installed_hub
            .iter()
            .any(|configured| configured == provider)
            .then_some(())
            .ok_or(AuthError::Denied)
    }

    /// Admit an actual registered Web caller under explicit server-owned policy.
    pub fn admit_web_installed_hub_read<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        installation: ScopeCapture<'a>,
    ) -> Result<InstalledHubReadAdmission<'a>, AuthError> {
        original.remaining().map_err(|_| AuthError::Request)?;
        self.web_installed_hub_policy(provider)?;
        caller
            .check(verifier, BrokerRequestContext::new(original.snapshot()))
            .map_err(broker_error)?;
        self.local
            .recheck_installation(&self.realm, original.snapshot())
            .map_err(source::store_error)?;
        let (guard, snapshot) = self
            .local
            .load_locked(original)
            .map_err(source::store_error)?;
        let record = snapshot
            .document()
            .provider(provider)
            .ok_or(AuthError::Denied)?;
        if !BrokerProjectCaller::Web(caller).matches_provider(verifier, record) {
            return Err(AuthError::Denied);
        }
        let admission = InstalledHubReadAdmission {
            issuer: self,
            caller,
            verifier,
            original,
            provider,
            installation,
            observation: Mutex::new(snapshot.into_observation()),
        };
        admission.recheck()?;
        if let Some(guard) = &guard {
            guard.revalidate().map_err(|error| {
                source::store_error(crate::store::AuthStoreError::Core(error.into()))
            })?;
        }
        original.check().map_err(|_| AuthError::Request)?;
        Ok(admission)
    }
}

impl<'a> InstalledHubReadAdmission<'a> {
    pub fn request(&self) -> &'a RequestContext<'a> {
        self.original
    }

    pub fn installation(&self) -> ScopeCapture<'a> {
        self.installation
    }

    pub(super) fn check_service(&self, issuer: &RoleIssuer) -> Result<(), AuthError> {
        if !std::ptr::eq(self.issuer, issuer) {
            return Err(AuthError::Denied);
        }
        self.recheck()
    }

    pub(super) fn web_service_caller(
        &self,
    ) -> (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str) {
        (self.caller, self.verifier, self.provider)
    }

    /// Join an existing PROJECT Hub/read proof from this exact Web request.
    /// Neither admission converts into the other or grants additional authority.
    pub fn check_project_source(
        &self,
        project: &super::records::ProjectReadAdmission<'_>,
    ) -> Result<(), AuthError> {
        self.recheck()?;
        project.check_web_hub_source(
            self.issuer,
            self.caller,
            self.verifier,
            self.provider,
            self.original,
        )
    }

    /// Recheck the retained proof and physical captures at I/O/delivery boundaries.
    pub fn recheck(&self) -> Result<(), AuthError> {
        self.original.remaining().map_err(|_| AuthError::Request)?;
        self.issuer.web_installed_hub_policy(self.provider)?;
        self.caller
            .check(
                self.verifier,
                BrokerRequestContext::new(self.original.snapshot()),
            )
            .map_err(broker_error)?;
        self.issuer
            .local
            .recheck_installation(&self.issuer.realm, self.original.snapshot())
            .map_err(source::store_error)?;
        self.issuer
            .local
            .revalidate_observation(
                &mut *self.observation.lock().map_err(|_| AuthError::State)?,
                self.original.snapshot(),
            )
            .map_err(source::store_error)?;
        let result = self.issuer.validate_physical_scope(
            &PhysicalScopeInput {
                installation: self.installation,
                project: None,
                providers: &[],
            },
            self.original,
        );
        self.original.remaining().map_err(|_| AuthError::Request)?;
        result
    }
}
