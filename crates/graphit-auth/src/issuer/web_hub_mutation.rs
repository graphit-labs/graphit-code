//! Web Hub effects use installation policy and the actual registered principal.
use super::*;
use crate::{
    ProjectSelection,
    broker::{
        BrokerError, BrokerRequestContext, BrokerVerifier,
        oauth::web::AuthenticatedWebCaller,
        project_access::{
            BrokerProjectAccess, BrokerProjectCaller, BrokerProjectError, BrokerProjectScope,
            ProjectAccessAction,
        },
    },
    profile::WebHubScope,
};

struct WebHubAuthority<'a> {
    issuer: &'a RoleIssuer,
    caller: &'a AuthenticatedWebCaller<'a>,
    verifier: &'a BrokerVerifier,
    original: &'a RequestContext<'a>,
    provider: &'a str,
    installation: &'a PublicationScope,
    scope: WebHubScope,
}

fn broker_error(error: BrokerError) -> AuthError {
    match error {
        BrokerError::Cancelled | BrokerError::Deadline => AuthError::Request,
        BrokerError::InvalidToken => AuthError::Expired,
        _ => AuthError::Denied,
    }
}

impl RoleIssuer {
    fn web_hub_authority<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        installation: &'a PublicationScope,
        scope: WebHubScope,
    ) -> Result<WebHubAuthority<'a>, AuthError> {
        original.remaining().map_err(|_| AuthError::Request)?;
        caller
            .check(verifier, BrokerRequestContext::new(original.snapshot()))
            .map_err(broker_error)?;

        let (_guard, snapshot) = self
            .local
            .load_locked(original)
            .map_err(source::store_error)?;
        let document = snapshot.document();
        let record = document.provider(provider).ok_or(AuthError::Denied)?;
        if !BrokerProjectCaller::Web(caller).matches_provider(verifier, record)
            || !document.web_hub_administrators().iter().any(|entry| {
                entry.provider == provider
                    && entry.issuer == caller.issuer()
                    && entry.subject == caller.subject()
                    && entry.scopes.contains(&scope)
            })
        {
            return Err(AuthError::Denied);
        }
        let proof = WebHubAuthority {
            issuer: self,
            caller,
            verifier,
            original,
            provider,
            installation,
            scope,
        };
        proof.recheck()?;

        original.check().map_err(|_| AuthError::Request)?;
        Ok(proof)
    }

    /// Admit local FILE effects with private PROJECT administration and current Hub/read.
    pub async fn admit_web_hub_project_mutation<'a>(
        &'a self,
        input: WebHubProjectMutationInput<'a>,
    ) -> Result<WebHubMutationAdmission<'a>, BrokerProjectError> {
        let WebHubProjectMutationInput {
            caller,
            verifier,
            original,
            provider,
            scope,
            backend,
        } = input;
        let authority = self.web_hub_authority(
            caller,
            verifier,
            original,
            provider,
            scope.installation,
            WebHubScope::Project,
        )?;
        let admission = WebHubMutationAdmission {
            authority,
            project: Some((scope, backend)),
        };
        admission.recheck_for_effect().await?;
        Ok(admission)
    }

    /// Global authority has its own policy entry and never borrows a PROJECT.
    pub fn admit_web_hub_global_mutation<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        installation: &'a PublicationScope,
    ) -> Result<WebHubMutationAdmission<'a>, AuthError> {
        let admission = WebHubMutationAdmission {
            authority: self.web_hub_authority(
                caller,
                verifier,
                original,
                provider,
                installation,
                WebHubScope::Global,
            )?,
            project: None,
        };
        admission.recheck()?;
        Ok(admission)
    }

    /// Publishing reads its PROJECT source; it does not mutate that source.
    pub fn admit_web_hub_publication<'a>(
        &'a self,
        input: WebHubPublicationInput<'a>,
    ) -> Result<WebHubPublicationAdmission<'a>, AuthError> {
        let WebHubPublicationInput {
            caller,
            verifier,
            original,
            provider,
            installation,
            source,
        } = input;
        source.check_web_hub_source(self, caller, verifier, provider, original)?;
        let admission = WebHubPublicationAdmission {
            authority: self.web_hub_authority(
                caller,
                verifier,
                original,
                provider,
                installation,
                WebHubScope::Project,
            )?,
            source,
        };
        admission.recheck()?;
        Ok(admission)
    }
}

impl WebHubAuthority<'_> {
    fn recheck(&self) -> Result<(), AuthError> {
        self.original.remaining().map_err(|_| AuthError::Request)?;
        self.caller
            .check(
                self.verifier,
                BrokerRequestContext::new(self.original.snapshot()),
            )
            .map_err(broker_error)?;

        let current = self
            .issuer
            .local
            .load(self.original)
            .map_err(source::store_error)?;
        let document = current.document();
        let provider = document.provider(self.provider).ok_or(AuthError::Denied)?;
        if !BrokerProjectCaller::Web(self.caller).matches_provider(self.verifier, provider)
            || !document.web_hub_administrators().iter().any(|entry| {
                entry.provider == self.provider
                    && entry.issuer == self.caller.issuer()
                    && entry.subject == self.caller.subject()
                    && entry.scopes.contains(&self.scope)
            })
        {
            return Err(AuthError::Denied);
        }
        self.issuer
            .validate_scope_owners(self.installation, None, &[], self.original)?;
        self.original.remaining().map_err(|_| AuthError::Request)?;
        Ok(())
    }
}

pub struct WebHubProjectMutationInput<'a> {
    pub caller: &'a AuthenticatedWebCaller<'a>,
    pub verifier: &'a BrokerVerifier,
    pub original: &'a RequestContext<'a>,
    pub provider: &'a str,
    pub scope: BrokerProjectScope<'a>,
    pub backend: &'a BrokerProjectAccess,
}

/// Borrowed FILE mutation authority. A DTO cannot construct this proof.
/// ```compile_fail
/// use graphit_auth::WebHubMutationAdmission;
/// let _: WebHubMutationAdmission<'static> = serde_json::from_str("{}").unwrap();
/// ```
pub struct WebHubMutationAdmission<'a> {
    authority: WebHubAuthority<'a>,
    project: Option<(BrokerProjectScope<'a>, &'a BrokerProjectAccess)>,
}
impl<'a> WebHubMutationAdmission<'a> {
    pub fn request(&self) -> &'a RequestContext<'a> {
        self.authority.original
    }
    pub fn installation(&self) -> &'a PublicationScope {
        self.authority.installation
    }
    pub fn project(&self) -> Result<Option<ProjectSelection<'a>>, AuthError> {
        self.recheck()?;
        Ok(self.project.map(|(scope, _)| scope.project))
    }
    /// Local revalidation does not turn a prior SQL decision into a lease.
    pub fn recheck(&self) -> Result<(), AuthError> {
        self.authority.recheck()?;
        if let Some((scope, _)) = self.project {
            self.authority.issuer.validate_scope_owners(
                scope.installation,
                Some(scope.project),
                scope.providers,
                self.request(),
            )?;
        }
        self.authority.recheck()
    }
    /// Call immediately before an effect, including each later lifecycle effect.
    pub async fn recheck_for_effect(&self) -> Result<(), BrokerProjectError> {
        self.recheck()?;
        if let Some((scope, backend)) = self.project {
            backend
                .authorize(
                    BrokerProjectCaller::Web(self.authority.caller),
                    self.authority.verifier,
                    scope.project_id()?,
                    ProjectReadModule::Hub,
                    ProjectAccessAction::Read,
                    self.request(),
                )
                .await?;
        }
        self.recheck()?;
        Ok(())
    }
}

pub struct WebHubPublicationInput<'a> {
    pub caller: &'a AuthenticatedWebCaller<'a>,
    pub verifier: &'a BrokerVerifier,
    pub original: &'a RequestContext<'a>,
    pub provider: &'a str,
    pub installation: &'a PublicationScope,
    pub source: &'a ProjectReadAdmission<'a>,
}

/// Publication authority retains PROJECT Hub/read independently of HubWrite.
/// ```compile_fail
/// use graphit_auth::WebHubPublicationAdmission;
/// fn duplicate<T: Clone>() {} duplicate::<WebHubPublicationAdmission<'static>>();
/// ```
pub struct WebHubPublicationAdmission<'a> {
    authority: WebHubAuthority<'a>,
    source: &'a ProjectReadAdmission<'a>,
}
impl<'a> WebHubPublicationAdmission<'a> {
    pub fn request(&self) -> &'a RequestContext<'a> {
        self.authority.original
    }
    pub fn installation(&self) -> &'a PublicationScope {
        self.authority.installation
    }
    pub fn source(&self) -> Result<&'a ProjectReadAdmission<'a>, AuthError> {
        self.recheck()?;
        Ok(self.source)
    }
    pub fn recheck(&self) -> Result<(), AuthError> {
        self.authority.recheck()?;
        self.source.check_web_hub_source(
            self.authority.issuer,
            self.authority.caller,
            self.authority.verifier,
            self.authority.provider,
            self.request(),
        )?;
        self.authority.recheck()
    }
    pub(super) fn check_service(&self, issuer: &RoleIssuer) -> Result<(), AuthError> {
        if !std::ptr::eq(self.authority.issuer, issuer) {
            return Err(AuthError::Denied);
        }
        self.recheck()
    }
    pub(super) fn web_service_caller(
        &self,
    ) -> (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str) {
        (
            self.authority.caller,
            self.authority.verifier,
            self.authority.provider,
        )
    }
}
