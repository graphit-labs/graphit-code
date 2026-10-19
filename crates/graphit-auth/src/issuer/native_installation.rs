use super::service_credentials::{ServiceUseError, original_check};
use super::{AuthenticatedCaller, Origin};
use crate::{
    broker::{BrokerRequestContext, BrokerVerifier, VerifiedNativeLogin},
    policy::{AuthError, ExternalEffect, Obligation, Permission},
    profile::ProviderKind,
    store::LocalObservation,
};
use std::sync::Mutex;

pub(super) struct NativePhase<'a> {
    verifier: &'a BrokerVerifier,
    proof: VerifiedNativeLogin<'a>,
    device: bool,
    observation: Mutex<LocalObservation>,
}
impl NativePhase<'_> {
    pub(super) fn user_identity(
        &self,
        caller: &AuthenticatedCaller<'_>,
    ) -> Result<super::user_subject::SubjectIdentity<'_>, ServiceUseError> {
        self.check(caller)?;
        Ok(super::user_subject::SubjectIdentity {
            issuer: self.proof.issuer(),
            subject: self.proof.subject(),
            username: self.proof.username(),
            teams: self.proof.teams(),
            origin: super::VerifiedUserOrigin::Native,
        })
    }
    fn check_proof(&self, context: BrokerRequestContext<'_>) -> Result<(), ServiceUseError> {
        if self.device
            && self.proof.needs_service_refresh(
                self.verifier,
                context,
                std::time::Duration::ZERO,
            )?
        {
            return Err(ServiceUseError::LoginRequired);
        }
        self.proof.check(self.verifier, context)?;
        Ok(())
    }
    fn check(&self, caller: &AuthenticatedCaller<'_>) -> Result<(), ServiceUseError> {
        original_check(caller.request)?;
        self.check_proof(BrokerRequestContext::new(caller.request.snapshot()))?;
        let mut observation = self.observation.lock().map_err(|_| AuthError::State)?;
        original_check(caller.request)?;
        caller
            .issuer
            .local
            .revalidate_observation(&mut observation, caller.request.snapshot())?;
        drop(observation);
        self.check_proof(BrokerRequestContext::new(caller.request.snapshot()))?;
        original_check(caller.request)
    }
}
impl<'a> AuthenticatedCaller<'a> {
    /// Reads the effective Native provider configuration for this Runtime caller.
    ///
    /// This returns public endpoint/resource configuration and its actual local
    /// revision without administrative authority, JWT verification or network I/O.
    /// It does not authorize installation. Build the verifier from these values,
    /// then call authenticate_native, which captures and verifies the active
    /// profile independently. Provider changes may make that binding fail.
    pub fn native_provider(&self) -> Result<(crate::profile::BrokerService, u64), ServiceUseError> {
        original_check(self.request)?;
        self.check()?;
        if self.source.origin() != Origin::Runtime {
            return Err(AuthError::Denied.into());
        }
        self.issuer
            .local
            .recheck_installation(&self.issuer.realm, self.request.snapshot())?;
        let (guard, mut snapshot) = self.issuer.local.load_locked(self.request)?;
        let (provider, profile) = snapshot.document().active(self.request)?;
        let ProviderKind::Broker(binding) = &provider.config().kind else {
            return Err(ServiceUseError::WrongMode);
        };
        let credentials = profile
            .broker_credentials()
            .ok_or(ServiceUseError::WrongMode)?;
        let issuer = credentials.issuer;
        if issuer != binding.endpoint {
            return Err(AuthError::Denied.into());
        }
        let metadata = (binding.clone(), provider.revision());
        original_check(self.request)?;
        self.check()?;
        self.issuer.local.revalidate(&mut snapshot, self.request)?;
        if let Some(guard) = &guard {
            guard
                .revalidate()
                .map_err(|error| crate::store::AuthStoreError::Core(error.into()))?;
        }
        original_check(self.request)?;
        Ok(metadata)
    }

    /// Qualifies this Runtime caller against the issuer's current Native profile.
    ///
    /// The signed access JWT is verified once through the existing verifier/cache.
    /// No refresh or UserInfo is performed. The returned caller keeps its original
    /// Runtime proof and a private Native phase; ordinary permissions are unchanged.
    ///
    /// ```compile_fail
    /// use graphit_auth::{VerifiedRoleGrant, broker::BrokerVerifier};
    /// async fn role<'a>(role: VerifiedRoleGrant<'a>, verifier: &'a BrokerVerifier) {
    ///     role.authenticate_native(verifier).await;
    /// }
    /// ```
    /// ```compile_fail
    /// use graphit_auth::broker::{AuthenticatedBrokerCaller, BrokerVerifier};
    /// async fn incoming<'a>(caller: AuthenticatedBrokerCaller<'a>, verifier: &'a BrokerVerifier) {
    ///     caller.authenticate_native(verifier).await;
    /// }
    /// ```
    pub async fn authenticate_native(
        mut self,
        verifier: &'a BrokerVerifier,
    ) -> Result<Self, ServiceUseError> {
        original_check(self.request)?;
        self.check()?;
        if self.source.origin() != Origin::Runtime || self.native.is_some() {
            return Err(AuthError::Denied.into());
        }
        self.issuer
            .local
            .recheck_installation(&self.issuer.realm, self.request.snapshot())?;
        let (snapshot, (issuer, subject, provider_revision, device)) = {
            let (guard, mut snapshot) = self.issuer.local.load_locked(self.request)?;
            let (provider, profile) = snapshot.document().active(self.request)?;
            let ProviderKind::Broker(binding) = &provider.config().kind else {
                return Err(ServiceUseError::WrongMode);
            };
            let credentials = profile
                .broker_credentials()
                .ok_or(ServiceUseError::WrongMode)?;
            let issuer = credentials.issuer;
            let subject = credentials.subject;
            if issuer != binding.endpoint
                || !verifier.matches_service_binding(
                    &binding.endpoint,
                    binding.mcp_resource.as_deref(),
                    provider.revision(),
                )
            {
                return Err(AuthError::Denied.into());
            }
            if credentials.device
                && u64::try_from(credentials.expires_at)
                    .ok()
                    .and_then(|seconds| {
                        std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds))
                    })
                    .is_none_or(|expiry| expiry <= std::time::SystemTime::now())
            {
                return Err(ServiceUseError::LoginRequired);
            }
            let captured = (
                issuer.to_owned(),
                subject.to_owned(),
                provider.revision(),
                credentials.device,
            );
            self.issuer.local.revalidate(&mut snapshot, self.request)?;
            if let Some(guard) = &guard {
                guard
                    .revalidate()
                    .map_err(|error| crate::store::AuthStoreError::Core(error.into()))?;
            }
            original_check(self.request)?;
            self.check()?;
            (snapshot, captured)
        };
        let (_, profile) = snapshot.document().active(self.request)?;
        let credentials = profile
            .broker_credentials()
            .ok_or(ServiceUseError::WrongMode)?;
        let proof = verifier
            .verify_refresh(
                credentials.access_token.expose(),
                None,
                &subject,
                BrokerRequestContext::new(self.request.snapshot()),
            )
            .await?;
        if proof.issuer() != issuer
            || proof.subject() != subject
            || proof.provider_revision() != provider_revision
        {
            return Err(AuthError::Denied.into());
        }
        original_check(self.request)?;
        self.check()?;
        let phase = NativePhase {
            verifier,
            proof,
            device,
            observation: Mutex::new(snapshot.into_observation()),
        };
        phase.check(&self)?;
        self.native = Some(phase);
        Ok(self)
    }

    /// Checks Native installation authority before an installation effect.
    ///
    /// This is separate from ordinary Source/record/role permissions. Call it
    /// again at the operation's existing barriers using this same caller.
    /// Rechecks use local expiry and the original physical observation, without
    /// another JWT decode, HTTP request or replacement request context.
    pub fn authorize_native_installation(
        &self,
        permission: Permission,
    ) -> Result<Obligation, ServiceUseError> {
        original_check(self.request)?;
        self.check()?;
        if !matches!(
            permission,
            Permission::Configure
                | Permission::ManageCache
                | Permission::External(ExternalEffect::PublishedFilesystem)
        ) || !self.issuer.policy.native_installation.contains(&permission)
        {
            return Err(AuthError::Denied.into());
        }
        let phase = self.native.as_ref().ok_or(AuthError::Denied)?;
        phase.check(self)?;
        Ok(Obligation::Unconditional)
    }
}
