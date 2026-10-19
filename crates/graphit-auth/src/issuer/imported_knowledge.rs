use super::*;
use crate::{
    broker::{
        BrokerError, BrokerRequestContext, BrokerVerifier, oauth::web::AuthenticatedWebCaller,
        project_access::BrokerProjectCaller,
    },
    store::LocalObservation,
};

/// Read authority for imported Knowledge in one captured installation.
/// Getters borrow selection data; consumers recheck at I/O and delivery boundaries.
/// This grants neither USER nor PROJECT access, and offers no mutation operation.
/// ```compile_fail
/// use graphit_auth::ImportedKnowledgeReadAdmission;
/// fn clone<T: Clone>() {} clone::<ImportedKnowledgeReadAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::{ImportedKnowledgeReadAdmission, UserReadAdmission};
/// fn user<'a>(read: ImportedKnowledgeReadAdmission<'a>) -> UserReadAdmission<'a> {
///     read.into()
/// }
/// ```
/// ```compile_fail
/// use graphit_auth::{ImportedKnowledgeReadAdmission, ProjectReadAdmission};
/// fn project<'a>(read: ImportedKnowledgeReadAdmission<'a>) -> ProjectReadAdmission<'a> {
///     read.into()
/// }
/// ```
pub struct ImportedKnowledgeReadAdmission<'a> {
    issuer: &'a RoleIssuer,
    caller: ReadCaller<'a>,
    original: &'a RequestContext<'a>,
    installation: ScopeCapture<'a>,
    observation: Mutex<LocalObservation>,
}

enum ReadCaller<'a> {
    Native(&'a AuthenticatedCaller<'a>),
    Web {
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
    },
}

fn native_read(issuer: &RoleIssuer, caller: &AuthenticatedCaller<'_>) -> Result<(), AuthError> {
    if !std::ptr::eq(caller.issuer, issuer)
        || caller.authorize(Permission::Read(
            graphit_core::module_settings::Module::Knowledge,
        ))? != Obligation::Unconditional
    {
        return Err(AuthError::Denied);
    }
    Ok(())
}

fn broker_error(error: BrokerError) -> AuthError {
    match error {
        BrokerError::Cancelled | BrokerError::Deadline => AuthError::Request,
        BrokerError::InvalidToken => AuthError::Expired,
        _ => AuthError::Denied,
    }
}

impl RoleIssuer {
    /// Admit an actual Runtime/Local Knowledge read in this installation.
    /// The caller supplies its own Original; no PROJECT or Web proof is fabricated.
    pub fn admit_authenticated_imported_knowledge_read<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        installation: ScopeCapture<'a>,
    ) -> Result<ImportedKnowledgeReadAdmission<'a>, AuthError> {
        let original = caller.request;
        original.remaining().map_err(|_| AuthError::Request)?;
        native_read(self, caller)?;
        self.local
            .recheck_installation(&self.realm, original.snapshot())
            .map_err(source::store_error)?;
        let (guard, snapshot) = self
            .local
            .load_locked(original)
            .map_err(source::store_error)?;
        let admission = ImportedKnowledgeReadAdmission {
            issuer: self,
            caller: ReadCaller::Native(caller),
            original,
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

    fn web_imported_knowledge_policy(&self, provider: &str) -> Result<(), AuthError> {
        if self
            .policy
            .web_imported_knowledge
            .iter()
            .any(|configured| configured == provider)
        {
            Ok(())
        } else {
            Err(AuthError::Denied)
        }
    }

    /// Admit a registered Web proof using an explicitly enabled server provider.
    /// Provider selection comes from server configuration, not the request body.
    pub fn admit_web_imported_knowledge_read<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        installation: ScopeCapture<'a>,
    ) -> Result<ImportedKnowledgeReadAdmission<'a>, AuthError> {
        original.remaining().map_err(|_| AuthError::Request)?;
        self.web_imported_knowledge_policy(provider)?;
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
        let admission = ImportedKnowledgeReadAdmission {
            issuer: self,
            caller: ReadCaller::Web {
                caller,
                verifier,
                provider,
            },
            original,
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

impl<'a> ImportedKnowledgeReadAdmission<'a> {
    pub(super) fn origin(&self) -> super::service_credentials::ServiceOrigin {
        use super::service_credentials::ServiceOrigin;
        match &self.caller {
            ReadCaller::Native(caller) => match caller.source.origin() {
                Origin::Runtime => ServiceOrigin::Runtime,
                Origin::Local => ServiceOrigin::Local,
            },
            ReadCaller::Web { .. } => ServiceOrigin::Web,
        }
    }

    pub(super) fn web_service_caller(
        &self,
    ) -> Option<(&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str)> {
        match self.caller {
            ReadCaller::Web {
                caller,
                verifier,
                provider,
            } => Some((caller, verifier, provider)),
            ReadCaller::Native(_) => None,
        }
    }

    pub(super) fn native_remote_permission(&self) -> Result<Obligation, AuthError> {
        self.recheck()?;
        match self.caller {
            ReadCaller::Native(caller) => {
                caller.authorize(Permission::External(crate::ExternalEffect::RemoteRequest))
            }
            ReadCaller::Web { .. } => Err(AuthError::Denied),
        }
    }

    pub(super) fn check_embedding_service(&self, issuer: &RoleIssuer) -> Result<(), AuthError> {
        if !std::ptr::eq(self.issuer, issuer) {
            return Err(AuthError::Denied);
        }
        self.recheck()
    }

    pub fn request(&self) -> &'a RequestContext<'a> {
        self.original
    }

    pub fn installation(&self) -> ScopeCapture<'a> {
        self.installation
    }

    pub fn recheck(&self) -> Result<(), AuthError> {
        self.original.remaining().map_err(|_| AuthError::Request)?;
        match &self.caller {
            ReadCaller::Native(caller) => native_read(self.issuer, caller)?,
            ReadCaller::Web {
                caller,
                verifier,
                provider,
            } => {
                self.issuer.web_imported_knowledge_policy(provider)?;
                caller
                    .check(
                        verifier,
                        BrokerRequestContext::new(self.original.snapshot()),
                    )
                    .map_err(broker_error)?;
            }
        }
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
        // Preserve cancellation/deadline if physical validation failed during I/O.
        self.original.remaining().map_err(|_| AuthError::Request)?;
        result
    }
}
