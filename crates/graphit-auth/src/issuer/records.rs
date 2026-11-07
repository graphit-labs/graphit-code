use super::{
    Actor, AuthError, AuthenticatedCaller, MutexGuard, Obligation, Permission, RoleIssuer, State,
    VerifiedRoleGrant, obligation, source,
};
use crate::broker::BrokerVerifier;
use crate::broker::project_access::{
    BrokerProjectCaller, BrokerProjectError, BrokerProjectScope, BrokerRecordRead,
    BrokerRecordWrite, ProjectAccessAction,
};
use crate::store::LocalObservation;
use crate::{ProjectCapture, ScopeInput, broker::oauth::web::AuthenticatedWebCaller};
use graphit_core::module_settings::Module;
use graphit_hub::policy::RequestContext;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

pub(super) fn web_actor(issuer: &RoleIssuer, caller: &AuthenticatedWebCaller<'_>) -> Actor {
    let mut hash = Sha256::new();
    hash.update(b"graphit-web-actor-v1\0");
    for part in [
        issuer.installation.as_str(),
        caller.issuer(),
        caller.subject(),
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    Actor::from_digest(hash.finalize().into())
}

/// The project record domains accepted by this authority boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectReadModule {
    Memory,
    Task,
    Knowledge,
    Ast,
    Hub,
}
impl ProjectReadModule {
    fn permission(self) -> Permission {
        Permission::Read(match self {
            Self::Memory => Module::Memory,
            Self::Task => Module::Task,
            Self::Knowledge => Module::Knowledge,
            Self::Ast => Module::Ast,
            Self::Hub => Module::Hub,
        })
    }
}

/// A classified operation; the record handler must enforce its returned obligation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectWriteOperation {
    Memory,
    OwnTask,
    FencedMemory,
    Task,
    CoordinateTask,
    Knowledge,
    Ast,
}
impl ProjectWriteOperation {
    fn permission(self) -> Permission {
        match self {
            Self::Memory => Permission::WriteMemory,
            Self::OwnTask => Permission::WriteOwnTask,
            Self::FencedMemory => Permission::WriteFencedMemory,
            Self::Task => Permission::WriteTask,
            Self::CoordinateTask => Permission::CoordinateTask,
            Self::Knowledge => Permission::MutateModule(Module::Knowledge),
            Self::Ast => Permission::MutateModule(Module::Ast),
        }
    }
}

/// Selects an existing issuer. Construction performs no authentication or I/O.
/// A store supplied by a caller cannot replace that issuer's store.
/// ```compile_fail
/// use graphit_auth::{ProjectRecordAuthority, RoleIssuer, store::AuthStore};
/// fn replace(issuer: &RoleIssuer, store: &AuthStore) {
///     let _ = ProjectRecordAuthority::new(issuer, store);
/// }
/// ```
pub struct ProjectRecordAuthority<'i> {
    issuer: &'i RoleIssuer,
}
impl<'i> ProjectRecordAuthority<'i> {
    pub fn new(issuer: &'i RoleIssuer) -> Self {
        Self { issuer }
    }

    /// Verifies one current PROJECT decision for an actual Broker caller.
    /// A later operation needs its own admission and fresh PROJECT request.
    pub async fn admit_broker_read<'a>(
        &'a self,
        input: BrokerRecordRead<'a>,
    ) -> Result<ProjectReadAdmission<'a>, BrokerProjectError> {
        let BrokerRecordRead {
            caller,
            verifier,
            backend,
            original,
            scope,
            provider,
            module,
        } = input;
        let proof = self.broker_proof(
            caller,
            verifier,
            original,
            scope,
            provider,
            module.permission(),
        )?;
        backend
            .authorize(
                caller,
                verifier,
                scope.project_id()?,
                module,
                ProjectAccessAction::Read,
                original,
            )
            .await?;
        proof.check(self.issuer)?;
        Ok(ProjectReadAdmission {
            issuer: self.issuer,
            proof: RecordProof::Broker(Box::new(proof)),
            module,
        })
    }

    /// Admit one Memory write using a fresh, exact PROJECT write decision.
    pub async fn admit_broker_write<'a>(
        &'a self,
        input: BrokerRecordWrite<'a>,
    ) -> Result<ProjectWriteAdmission<'a>, BrokerProjectError> {
        let BrokerRecordWrite {
            caller,
            verifier,
            backend,
            original,
            scope,
            provider,
            operation,
        } = input;
        if !matches!(
            operation,
            ProjectWriteOperation::Memory | ProjectWriteOperation::FencedMemory
        ) {
            return Err(AuthError::Denied.into());
        }
        let caller = BrokerProjectCaller::Web(caller);
        let proof = self.broker_proof(
            caller,
            verifier,
            original,
            scope,
            provider,
            operation.permission(),
        )?;
        backend
            .authorize(
                caller,
                verifier,
                scope.project_id()?,
                ProjectReadModule::Memory,
                ProjectAccessAction::Write,
                original,
            )
            .await?;
        proof.check(self.issuer)?;
        Ok(ProjectWriteAdmission {
            issuer: self.issuer,
            proof: RecordProof::Broker(Box::new(proof)),
            operation,
        })
    }

    fn broker_proof<'a>(
        &'a self,
        caller: BrokerProjectCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        scope: BrokerProjectScope<'a>,
        provider: &'a str,
        permission: Permission,
    ) -> Result<BrokerRecordProof<'a>, BrokerProjectError> {
        caller.check(verifier, original)?;
        self.issuer
            .validate_physical_scope(&scope.physical(), original)?;
        self.issuer
            .local
            .recheck_installation(&self.issuer.realm, original.snapshot())?;
        let (guard, snapshot) = self.issuer.local.load_locked(original)?;
        let selected = snapshot
            .document()
            .provider(provider)
            .ok_or(AuthError::Denied)?;
        if !caller.matches_provider(verifier, selected) {
            return Err(AuthError::Denied.into());
        }
        let proof = BrokerRecordProof {
            caller,
            verifier,
            original,
            scope,
            permission,
            provider,
            observation: Mutex::new(snapshot.into_observation()),
        };
        proof.check(self.issuer)?;
        if let Some(guard) = &guard {
            guard
                .revalidate()
                .map_err(|error| crate::store::AuthStoreError::Core(error.into()))?;
        }
        original.check().map_err(|_| AuthError::Request)?;
        Ok(proof)
    }

    pub fn admit_read<'a>(
        &'a self,
        grant: &'a VerifiedRoleGrant<'a>,
        original_request: &'a RequestContext<'a>,
        module: ProjectReadModule,
    ) -> Result<ProjectReadAdmission<'a>, AuthError> {
        lineage(self.issuer, grant, original_request)?;
        let admission = ProjectReadAdmission {
            issuer: self.issuer,
            proof: RecordProof::Role(grant),
            module,
        };
        admission.recheck()?;
        Ok(admission)
    }

    pub fn admit_write<'a>(
        &'a self,
        grant: &'a VerifiedRoleGrant<'a>,
        original_request: &'a RequestContext<'a>,
        operation: ProjectWriteOperation,
    ) -> Result<ProjectWriteAdmission<'a>, AuthError> {
        lineage(self.issuer, grant, original_request)?;
        let admission = ProjectWriteAdmission {
            issuer: self.issuer,
            proof: RecordProof::Role(grant),
            operation,
        };
        admission.recheck()?;
        Ok(admission)
    }

    /// Admits the actual Runtime or Local caller under its original request and project captures.
    pub fn admit_authenticated_read<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        original_request: &'a RequestContext<'a>,
        scope: ScopeInput<'a>,
        module: ProjectReadModule,
    ) -> Result<ProjectReadAdmission<'a>, AuthError> {
        authenticated_lineage(self.issuer, caller, original_request)?;
        let admission = ProjectReadAdmission {
            issuer: self.issuer,
            proof: RecordProof::Authenticated { caller, scope },
            module,
        };
        admission.recheck()?;
        Ok(admission)
    }

    /// Admits one classified operation without issuing a role, credential or run.
    pub fn admit_authenticated_write<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        original_request: &'a RequestContext<'a>,
        scope: ScopeInput<'a>,
        operation: ProjectWriteOperation,
    ) -> Result<ProjectWriteAdmission<'a>, AuthError> {
        authenticated_lineage(self.issuer, caller, original_request)?;
        let admission = ProjectWriteAdmission {
            issuer: self.issuer,
            proof: RecordProof::Authenticated { caller, scope },
            operation,
        };
        admission.recheck()?;
        Ok(admission)
    }
}

/// Borrows the original project proof and captures; access getters revalidate them.
/// ```compile_fail
/// use graphit_auth::ProjectReadAdmission;
/// fn clone<T: Clone>() {} clone::<ProjectReadAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectReadAdmission;
/// fn debug<T: std::fmt::Debug>() {} debug::<ProjectReadAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectReadAdmission;
/// fn serial<T: serde::Serialize>() {} serial::<ProjectReadAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectReadAdmission;
/// let _: ProjectReadAdmission<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectReadAdmission;
/// let admission = ProjectReadAdmission { issuer: (), grant: (), module: () };
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectReadModule;
/// let module = ProjectReadModule::User;
/// ```
/// ```compile_fail
/// use graphit_auth::{Permission, ProjectReadModule};
/// let module = ProjectReadModule::from(Permission::ReadUser);
/// ```
/// ```compile_fail
/// use graphit_auth::{ProjectReadAdmission, ProjectReadModule, ProjectRecordAuthority, RoleIssuer, VerifiedRoleGrant};
/// use graphit_hub::policy::RequestContext;
/// fn detach<'a>(issuer: &'a RoleIssuer, grant: &'a VerifiedRoleGrant<'a>, request: &'a RequestContext<'a>) -> ProjectReadAdmission<'static> {
///     let authority = ProjectRecordAuthority::new(issuer);
///     authority.admit_read(grant, request, ProjectReadModule::Memory).unwrap()
/// }
/// ```
pub struct ProjectReadAdmission<'a> {
    issuer: &'a RoleIssuer,
    proof: RecordProof<'a>,
    module: ProjectReadModule,
}
impl<'a> ProjectReadAdmission<'a> {
    pub(super) fn check_web_hub_source(
        &self,
        issuer: &RoleIssuer,
        caller: &AuthenticatedWebCaller<'_>,
        verifier: &BrokerVerifier,
        provider: &str,
        original: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        if self.module != ProjectReadModule::Hub
            || !std::ptr::eq(self.issuer, issuer)
            || !std::sync::Arc::ptr_eq(self.proof.request().snapshot(), original.snapshot())
        {
            return Err(AuthError::Denied);
        }
        match &self.proof {
            RecordProof::Broker(proof)
                if matches!(proof.caller, BrokerProjectCaller::Web(observed) if std::ptr::eq(observed, caller))
                    && std::ptr::eq(proof.verifier, verifier)
                    && proof.provider == provider =>
            {
                self.recheck()
            }
            _ => Err(AuthError::Denied),
        }
    }

    pub(super) fn check_web_embedding(
        &self,
        issuer: &RoleIssuer,
        caller: &AuthenticatedWebCaller<'_>,
        original: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        if !std::ptr::eq(self.issuer, issuer)
            || !matches!(
                self.module,
                ProjectReadModule::Ast | ProjectReadModule::Knowledge | ProjectReadModule::Task
            )
            || !std::sync::Arc::ptr_eq(self.proof.request().snapshot(), original.snapshot())
        {
            return Err(AuthError::Denied);
        }
        match &self.proof {
            RecordProof::Broker(proof) if matches!(proof.caller, BrokerProjectCaller::Web(observed) if std::ptr::eq(observed, caller)) => {
                self.recheck()
            }
            _ => Err(AuthError::Denied),
        }
    }

    /// Borrow the same admitted Broker transport; retain and recheck this admission.
    pub(super) fn web_service_caller(
        &self,
    ) -> Option<(&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str)> {
        match &self.proof {
            RecordProof::Broker(proof) => match proof.caller {
                BrokerProjectCaller::Web(caller) => Some((caller, proof.verifier, proof.provider)),
                BrokerProjectCaller::Incoming(_) => None,
            },
            _ => None,
        }
    }

    /// The class selected during admission, without I/O or a new authority check.
    /// Call `recheck` before using the original admission for access.
    pub const fn module(&self) -> ProjectReadModule {
        self.module
    }

    pub fn recheck(&self) -> Result<(), AuthError> {
        self.proof.recheck(self.issuer, self.module.permission())
    }
    pub fn project(&self) -> Result<&ProjectCapture<'a>, AuthError> {
        self.recheck()?;
        self.proof.project().ok_or(AuthError::Denied)
    }
    pub fn request(&self) -> Result<&'a RequestContext<'a>, AuthError> {
        self.recheck()?;
        Ok(self.proof.request())
    }

    /// Revalidate once, then borrow this admission's project and original request.
    /// The returned metadata grants no authority; recheck again at the next effect boundary.
    pub fn project_and_request(
        &self,
    ) -> Result<(&ProjectCapture<'a>, &'a RequestContext<'a>), AuthError> {
        self.recheck()?;
        Ok((
            self.proof.project().ok_or(AuthError::Denied)?,
            self.proof.request(),
        ))
    }
}

/// Authorization for one classified project operation, without any data effect.
/// ```compile_fail
/// use graphit_auth::ProjectWriteAdmission;
/// fn clone<T: Clone>() {} clone::<ProjectWriteAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectWriteAdmission;
/// fn debug<T: std::fmt::Debug>() {} debug::<ProjectWriteAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectWriteAdmission;
/// fn serial<T: serde::Serialize>() {} serial::<ProjectWriteAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectWriteAdmission;
/// let _: ProjectWriteAdmission<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectWriteAdmission;
/// let admission = ProjectWriteAdmission { issuer: (), grant: (), operation: () };
/// ```
/// ```compile_fail
/// use graphit_auth::ProjectWriteOperation;
/// let operation = ProjectWriteOperation::User;
/// ```
pub struct ProjectWriteAdmission<'a> {
    issuer: &'a RoleIssuer,
    proof: RecordProof<'a>,
    operation: ProjectWriteOperation,
}
impl<'a> ProjectWriteAdmission<'a> {
    /// The class selected during admission, without I/O or a new authority check.
    /// Call `recheck` before using the original admission for effects.
    pub const fn operation(&self) -> ProjectWriteOperation {
        self.operation
    }

    pub fn recheck(&self) -> Result<Obligation, AuthError> {
        let permission = self.operation.permission();
        self.proof.recheck(self.issuer, permission)?;
        Ok(obligation(permission))
    }
    pub fn actor(&self) -> Result<Actor, AuthError> {
        self.proof.actor(self.issuer, self.operation.permission())
    }

    /// Task participant identity under this current, rechecked PROJECT admission.
    /// Runtime identifies the installation; Local additionally identifies its
    /// authenticated canonical provider/profile. Neither includes a boot epoch
    /// or credential generation. Role participants remain specific to their run;
    /// this does not make managed credentials survive a restart.
    pub fn task_actor(&self) -> Result<Actor, AuthError> {
        if !matches!(
            self.operation,
            ProjectWriteOperation::OwnTask
                | ProjectWriteOperation::Task
                | ProjectWriteOperation::CoordinateTask
        ) {
            return Err(AuthError::Denied);
        }
        self.recheck()?;
        match &self.proof {
            RecordProof::Authenticated { caller, .. } => {
                let mut hash = Sha256::new();
                hash.update(b"graphit-task-principal-v1\0");
                let installation = self.issuer.installation.as_str();
                hash.update((installation.len() as u64).to_be_bytes());
                hash.update(installation.as_bytes());
                caller.source.update_task_principal(&mut hash);
                self.recheck()?;
                Ok(Actor::from_digest(hash.finalize().into()))
            }
            RecordProof::Role(_) => self.actor(),
            RecordProof::Broker(_) => Err(AuthError::Denied),
        }
    }
    pub fn project(&self) -> Result<&ProjectCapture<'a>, AuthError> {
        self.recheck()?;
        self.proof.project().ok_or(AuthError::Denied)
    }
    pub fn request(&self) -> Result<&'a RequestContext<'a>, AuthError> {
        self.recheck()?;
        Ok(self.proof.request())
    }

    /// Revalidate once, then borrow this admission's project and original request.
    /// The returned metadata grants no authority; retain the operation's effect fences.
    pub fn project_and_request(
        &self,
    ) -> Result<(&ProjectCapture<'a>, &'a RequestContext<'a>), AuthError> {
        self.recheck()?;
        Ok((
            self.proof.project().ok_or(AuthError::Denied)?,
            self.proof.request(),
        ))
    }
}

enum RecordProof<'a> {
    Role(&'a VerifiedRoleGrant<'a>),
    Authenticated {
        caller: &'a AuthenticatedCaller<'a>,
        scope: ScopeInput<'a>,
    },
    Broker(Box<BrokerRecordProof<'a>>),
}
struct BrokerRecordProof<'a> {
    caller: BrokerProjectCaller<'a>,
    verifier: &'a BrokerVerifier,
    original: &'a RequestContext<'a>,
    scope: BrokerProjectScope<'a>,
    permission: Permission,
    provider: &'a str,
    observation: Mutex<LocalObservation>,
}
impl BrokerRecordProof<'_> {
    fn check(&self, issuer: &RoleIssuer) -> Result<(), AuthError> {
        self.original.remaining().map_err(|_| AuthError::Request)?;
        self.caller
            .check(self.verifier, self.original)
            .map_err(|error| match error {
                crate::broker::BrokerError::Cancelled | crate::broker::BrokerError::Deadline => {
                    AuthError::Request
                }
                crate::broker::BrokerError::InvalidToken => AuthError::Expired,
                _ => AuthError::Denied,
            })?;
        issuer.validate_physical_scope(&self.scope.physical(), self.original)?;
        issuer
            .local
            .recheck_installation(&issuer.realm, self.original.snapshot())
            .map_err(source::store_error)?;
        issuer
            .local
            .revalidate_observation(
                &mut *self.observation.lock().map_err(|_| AuthError::State)?,
                self.original.snapshot(),
            )
            .map_err(source::store_error)?;
        self.original
            .remaining()
            .map(|_| ())
            .map_err(|_| AuthError::Request)
    }
}
impl<'a> RecordProof<'a> {
    fn request(&self) -> &'a RequestContext<'a> {
        match self {
            Self::Role(grant) => grant.request,
            Self::Authenticated { caller, .. } => caller.request,
            Self::Broker(proof) => proof.original,
        }
    }
    fn project(&self) -> Option<&ProjectCapture<'a>> {
        match self {
            Self::Role(grant) => grant.scope.project.as_ref(),
            Self::Authenticated { scope, .. } => scope.project.as_ref(),
            Self::Broker(proof) => Some(&proof.scope.project),
        }
    }
    fn recheck(&self, issuer: &RoleIssuer, permission: Permission) -> Result<(), AuthError> {
        match self {
            Self::Role(grant) => {
                drop(checked_state(issuer, grant, permission)?);
                Ok(())
            }
            Self::Authenticated { caller, scope } => {
                checked_authenticated(issuer, caller, scope, permission)
            }
            Self::Broker(proof) => {
                if proof.permission != permission {
                    return Err(AuthError::Denied);
                }
                proof.check(issuer)
            }
        }
    }
    fn actor(&self, issuer: &RoleIssuer, permission: Permission) -> Result<Actor, AuthError> {
        match self {
            Self::Role(grant) => {
                let state = checked_state(issuer, grant, permission)?;
                let actor = state
                    .credentials
                    .get(&grant.id)
                    .ok_or(AuthError::Denied)?
                    .actor
                    .clone();
                grant.request.remaining().map_err(|_| AuthError::Request)?;
                Ok(actor)
            }
            Self::Authenticated { caller, scope } => {
                checked_authenticated(issuer, caller, scope, permission)?;
                caller.actor()
            }
            Self::Broker(proof) => {
                self.recheck(issuer, permission)?;
                match proof.caller {
                    BrokerProjectCaller::Web(caller)
                        if matches!(
                            permission,
                            Permission::WriteMemory | Permission::WriteFencedMemory
                        ) =>
                    {
                        let actor = web_actor(issuer, caller);
                        proof.check(issuer)?;
                        Ok(actor)
                    }
                    _ => Err(AuthError::Denied),
                }
            }
        }
    }
}

fn authenticated_lineage(
    issuer: &RoleIssuer,
    caller: &AuthenticatedCaller<'_>,
    request: &RequestContext<'_>,
) -> Result<(), AuthError> {
    request.remaining().map_err(|_| AuthError::Request)?;
    if !std::ptr::eq(issuer, caller.issuer)
        || !std::ptr::eq(request, caller.request)
        || !request.snapshot().same_request(caller.request.snapshot())
    {
        return Err(AuthError::Denied);
    }
    Ok(())
}

fn checked_authenticated(
    issuer: &RoleIssuer,
    caller: &AuthenticatedCaller<'_>,
    scope: &ScopeInput<'_>,
    permission: Permission,
) -> Result<(), AuthError> {
    authenticated_lineage(issuer, caller, caller.request)?;
    if scope.project.is_none() {
        return Err(AuthError::Denied);
    }
    issuer
        .local
        .recheck_installation(&issuer.realm, caller.request.snapshot())
        .map_err(source::store_error)?;
    caller.authorize(permission)?;
    issuer.validate_scope(scope, caller.request)?;
    caller.authorize(permission)?;
    caller.request.remaining().map_err(|_| AuthError::Request)?;
    Ok(())
}

fn lineage(
    issuer: &RoleIssuer,
    grant: &VerifiedRoleGrant<'_>,
    request: &RequestContext<'_>,
) -> Result<(), AuthError> {
    request.remaining().map_err(|_| AuthError::Request)?;
    if !std::ptr::eq(issuer, grant.issuer)
        || !std::ptr::eq(request, grant.request)
        || !request.snapshot().same_request(grant.request.snapshot())
        || grant.scope.project.is_none()
    {
        return Err(AuthError::Denied);
    }
    Ok(())
}

fn checked_state<'i>(
    issuer: &RoleIssuer,
    grant: &'i VerifiedRoleGrant<'_>,
    permission: Permission,
) -> Result<MutexGuard<'i, State>, AuthError> {
    lineage(issuer, grant, grant.request)?;
    issuer
        .local
        .recheck_installation(&issuer.realm, grant.request.snapshot())
        .map_err(source::store_error)?;
    let state = grant.check()?;
    if !state
        .credentials
        .get(&grant.id)
        .ok_or(AuthError::Denied)?
        .permissions
        .contains(&permission)
    {
        return Err(AuthError::Denied);
    }
    grant.request.remaining().map_err(|_| AuthError::Request)?;
    Ok(state)
}

impl<'a> RecordProof<'a> {
    fn storage_principal(&self) -> super::record_storage::RecordStoragePrincipal<'a> {
        use super::record_storage::RecordStoragePrincipal as P;
        match self {
            Self::Role(r) => P::Role(r),
            Self::Authenticated { caller, .. } => P::Caller(caller),
            Self::Broker(p) => match p.caller {
                BrokerProjectCaller::Web(caller) => P::Web {
                    caller,
                    verifier: p.verifier,
                    provider: p.provider,
                },
                BrokerProjectCaller::Incoming(caller) => P::Incoming {
                    caller,
                    verifier: p.verifier,
                    provider: p.provider,
                    original: p.original,
                },
            },
        }
    }
}
impl<'a> ProjectReadAdmission<'a> {
    pub(super) fn storage_issuer(&self) -> &'a RoleIssuer {
        self.issuer
    }
    pub(super) fn storage_original(&self) -> &'a RequestContext<'a> {
        self.proof.request()
    }
    pub(super) fn storage_principal(&self) -> super::record_storage::RecordStoragePrincipal<'a> {
        self.proof.storage_principal()
    }
}
impl<'a> ProjectWriteAdmission<'a> {
    pub(super) fn storage_issuer(&self) -> &'a RoleIssuer {
        self.issuer
    }
    pub(super) fn storage_original(&self) -> &'a RequestContext<'a> {
        self.proof.request()
    }
    pub(super) fn storage_principal(&self) -> super::record_storage::RecordStoragePrincipal<'a> {
        self.proof.storage_principal()
    }
}
