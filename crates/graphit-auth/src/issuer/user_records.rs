use super::*;
use crate::broker::{AuthenticatedBrokerCaller, BrokerError, BrokerRequestContext, BrokerVerifier};
use crate::broker::{oauth::web::AuthenticatedWebCaller, project_access::BrokerProjectCaller};
use crate::profile::ProviderKind;
use graphit_core::{artifact_store::UserIdentity, module_settings::Module};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserMemoryWriteOperation {
    Memory,
    FencedMemory,
}
impl UserMemoryWriteOperation {
    fn permission(self) -> Permission {
        match self {
            Self::Memory => Permission::WriteMemory,
            Self::FencedMemory => Permission::WriteFencedMemory,
        }
    }
}

enum UserProof<'a> {
    Web {
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
    },
    Native(&'a AuthenticatedCaller<'a>),
    LocalMemory(&'a AuthenticatedCaller<'a>),
    FencedParent {
        parent: &'a AdminParent<'a>,
        local: bool,
    },
    Incoming {
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
    },
}
struct Admission<'a> {
    issuer: &'a RoleIssuer,
    original: &'a RequestContext<'a>,
    subject: Option<VerifiedUserSubject<'a>>,
    proof: UserProof<'a>,
    identity: UserIdentity,
    user: &'a PublicationScope,
    permission: Permission,
    user_permission: Permission,
}

/// Live USER Memory read authority, separate from literal identity and PROJECT.
/// ```compile_fail
/// use graphit_auth::UserReadAdmission;
/// fn clone<T: Clone>() {} clone::<UserReadAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::UserReadAdmission;
/// fn restore<T: serde::de::DeserializeOwned>() {} restore::<UserReadAdmission<'static>>();
/// ```
pub struct UserReadAdmission<'a>(Admission<'a>);
/// Live USER Memory mutation authority. The store must enforce its obligation.
/// ```compile_fail
/// use graphit_auth::UserWriteAdmission;
/// fn debug<T: std::fmt::Debug>() {} debug::<UserWriteAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::{Actor, UserWriteAdmission};
/// fn grant<'a>(actor: Actor) -> UserWriteAdmission<'a> { actor.into() }
/// ```
pub struct UserWriteAdmission<'a> {
    admission: Admission<'a>,
    operation: UserMemoryWriteOperation,
}

/// USER Knowledge read authority for a verified Web or Incoming MCP principal.
/// Memory authority cannot substitute for this domain.
/// ```compile_fail
/// use graphit_auth::{UserReadAdmission, UserKnowledgeReadAdmission};
/// fn substitute<'a>(memory: UserReadAdmission<'a>) -> UserKnowledgeReadAdmission<'a> {
///     memory.into()
/// }
/// ```
/// ```compile_fail
/// use graphit_auth::UserKnowledgeReadAdmission;
/// fn clone<T: Clone>() {} clone::<UserKnowledgeReadAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::UserKnowledgeReadAdmission;
/// fn restore<T: serde::de::DeserializeOwned>() {} restore::<UserKnowledgeReadAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::{RoleIssuer, AuthenticatedCaller, PublicationScope};
/// use graphit_auth::broker::BrokerVerifier;
/// use graphit_hub::policy::RequestContext;
/// fn reject<'a>(issuer: &'a RoleIssuer, caller: &'a AuthenticatedCaller<'a>, verifier: &'a BrokerVerifier,
///     original: &'a RequestContext<'a>, user: &'a PublicationScope) {
///     issuer.admit_incoming_user_knowledge_read(caller, verifier, original, "server-provider", user);
/// }
/// ```
pub struct UserKnowledgeReadAdmission<'a>(Admission<'a>);

/// USER Knowledge mutation authority, independent of read authority and PROJECT.
/// ```compile_fail
/// use graphit_auth::{UserWriteAdmission, UserKnowledgeWriteAdmission};
/// fn substitute<'a>(memory: UserWriteAdmission<'a>) -> UserKnowledgeWriteAdmission<'a> {
///     memory.into()
/// }
/// ```
/// ```compile_fail
/// use graphit_auth::UserKnowledgeWriteAdmission;
/// fn debug<T: std::fmt::Debug>() {} debug::<UserKnowledgeWriteAdmission<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::UserKnowledgeWriteAdmission;
/// fn serialize<T: serde::Serialize>() {} serialize::<UserKnowledgeWriteAdmission<'static>>();
/// ```
pub struct UserKnowledgeWriteAdmission<'a>(Admission<'a>);

fn broker_error(error: BrokerError) -> AuthError {
    match error {
        BrokerError::Cancelled | BrokerError::Deadline => AuthError::Request,
        BrokerError::InvalidToken => AuthError::Expired,
        _ => AuthError::Denied,
    }
}
fn subject_error(error: ServiceUseError) -> AuthError {
    match error {
        ServiceUseError::Auth(error) => error,
        ServiceUseError::Store(error) => source::store_error(error),
        ServiceUseError::Broker(error) => broker_error(error),
        ServiceUseError::Profile(
            crate::profile::ProfileError::Cancelled | crate::profile::ProfileError::Deadline,
        ) => AuthError::Request,
        ServiceUseError::Profile(crate::profile::ProfileError::Limit) => AuthError::Limit,
        ServiceUseError::Profile(_)
        | ServiceUseError::Disabled
        | ServiceUseError::MissingCredential
        | ServiceUseError::LoginRequired
        | ServiceUseError::NeedsRefresh
        | ServiceUseError::WrongMode => AuthError::Denied,
    }
}
fn identity(subject: &VerifiedUserSubject<'_>) -> Result<UserIdentity, AuthError> {
    let identity = subject.identity().map_err(subject_error)?;
    UserIdentity::new(identity.issuer.to_owned(), identity.subject.to_owned())
        .map_err(|_| AuthError::Invalid)
}

fn local_memory_identity() -> Result<UserIdentity, AuthError> {
    UserIdentity::new("graphit:local-user".into(), "anonymous".into())
        .map_err(|_| AuthError::Invalid)
}

impl AuthenticatedCaller<'_> {
    /// The installation owner can use private USER Memory in local mode.
    /// An active Broker profile requires its own verified Native phase.
    pub fn uses_local_user_memory(&self) -> Result<bool, AuthError> {
        self.check()?;
        if self.native.is_some() {
            return Ok(false);
        }
        match self.source.origin() {
            Origin::Local => Ok(true),
            Origin::Runtime => {
                let snapshot = self
                    .issuer
                    .local
                    .load(self.request)
                    .map_err(source::store_error)?;
                match snapshot.document().active(self.request) {
                    Ok((provider, _)) => {
                        Ok(matches!(provider.config().kind, ProviderKind::Local { .. }))
                    }
                    Err(crate::profile::ProfileError::NoActive) => Ok(true),
                    Err(error) => Err(subject_error(error.into())),
                }
            }
        }
    }

    /// Resolve the installation-private scope. Record operations still require admission.
    pub fn user_memory_scope(&self) -> Result<PublicationScope, AuthError> {
        let identity = if self.uses_local_user_memory()? {
            local_memory_identity()?
        } else {
            identity(
                &self
                    .issuer
                    .user_from_authenticated(self)
                    .map_err(subject_error)?,
            )?
        };
        let root = self.issuer.realm.owner_root();
        let paths = graphit_core::project_paths::InstallationPaths::resolve(
            self.issuer.realm.brand().clone(),
            root,
            None,
            Some(root),
        )
        .map_err(|_| AuthError::Invalid)?;
        PublicationScope::user(&paths, self.issuer.installation.clone(), identity)
            .map_err(|_| AuthError::Invalid)
    }
}

impl<'a> AdminParent<'a> {
    /// Admit only this Native Runtime principal's administrative USER fenced write.
    /// The ordinary caller must permit MutateUser; this live parent must permit
    /// WriteFencedMemory. Neither permission is added to the caller.
    pub fn admit_native_user_memory_fenced_write(
        &'a self,
        original: &'a RequestContext<'a>,
        user: &'a PublicationScope,
    ) -> Result<UserWriteAdmission<'a>, AuthError> {
        self.admit_fenced_user_memory(original, user, false)
    }

    /// Admit the installation's private local corpus to the same fenced operation.
    pub fn admit_local_user_memory_fenced_write(
        &'a self,
        original: &'a RequestContext<'a>,
        user: &'a PublicationScope,
    ) -> Result<UserWriteAdmission<'a>, AuthError> {
        self.admit_fenced_user_memory(original, user, true)
    }

    fn admit_fenced_user_memory(
        &'a self,
        original: &'a RequestContext<'a>,
        user: &'a PublicationScope,
        local: bool,
    ) -> Result<UserWriteAdmission<'a>, AuthError> {
        if !std::ptr::eq(original, self.request) || self.caller.source.origin() != Origin::Runtime {
            return Err(AuthError::Denied);
        }
        self.caller.authorize(Permission::MutateUser)?;
        self.authorize(Permission::WriteFencedMemory)?;
        let (identity, subject) = if local {
            (local_memory_identity()?, None)
        } else {
            let subject = self
                .issuer
                .user_from_authenticated(self.caller)
                .map_err(subject_error)?;
            (identity(&subject)?, Some(subject))
        };
        let admission = Admission {
            issuer: self.issuer,
            original,
            identity,
            subject,
            proof: UserProof::FencedParent {
                parent: self,
                local,
            },
            user,
            permission: Permission::WriteFencedMemory,
            user_permission: Permission::MutateUser,
        };
        admission.check()?;
        Ok(UserWriteAdmission {
            admission,
            operation: UserMemoryWriteOperation::FencedMemory,
        })
    }
}

impl RoleIssuer {
    fn incoming_user_permissions(
        &self,
        provider: &str,
        user: Permission,
        memory: Permission,
    ) -> Result<(), AuthError> {
        let rule = self
            .policy
            .incoming_user
            .iter()
            .find(|rule| rule.provider == provider)
            .ok_or(AuthError::Denied)?;
        if !rule.permissions.contains(&user) || !rule.permissions.contains(&memory) {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    fn authenticated_user<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        original: &'a RequestContext<'a>,
        user: &'a PublicationScope,
        user_permission: Permission,
        permission: Permission,
    ) -> Result<Admission<'a>, AuthError> {
        if !std::ptr::eq(self, caller.issuer) || !std::ptr::eq(original, caller.request) {
            return Err(AuthError::Denied);
        }
        caller.authorize(user_permission)?;
        caller.authorize(permission)?;
        let (identity, subject, proof) = if caller.uses_local_user_memory()? {
            (
                local_memory_identity()?,
                None,
                UserProof::LocalMemory(caller),
            )
        } else {
            let subject = self
                .user_from_authenticated(caller)
                .map_err(subject_error)?;
            (
                identity(&subject)?,
                Some(subject),
                UserProof::Native(caller),
            )
        };
        let admission = Admission {
            issuer: self,
            original,
            identity,
            subject,
            proof,
            user,
            permission,
            user_permission,
        };
        admission.check()?;
        Ok(admission)
    }
    #[allow(clippy::too_many_arguments)]
    fn incoming_user<'a>(
        &'a self,
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
        user_permission: Permission,
        permission: Permission,
    ) -> Result<Admission<'a>, AuthError> {
        self.incoming_user_permissions(provider, user_permission, permission)?;
        caller
            .check(verifier, BrokerRequestContext::new(original.snapshot()))
            .map_err(broker_error)?;

        let (_guard, snapshot) = self
            .local
            .load_locked(original)
            .map_err(source::store_error)?;
        let record = snapshot
            .document()
            .provider(provider)
            .ok_or(AuthError::Denied)?;
        let ProviderKind::Broker(binding) = &record.config().kind else {
            return Err(AuthError::Denied);
        };
        if binding.mcp_resource.is_none()
            || caller.issuer() != binding.endpoint
            || caller.provider_revision() != record.revision()
            || !verifier.matches_service_binding(
                &binding.endpoint,
                binding.mcp_resource.as_deref(),
                record.revision(),
            )
        {
            return Err(AuthError::Denied);
        }
        let subject = self
            .user_from_incoming(caller, verifier, original)
            .map_err(subject_error)?;
        let admission = Admission {
            issuer: self,
            original,
            identity: identity(&subject)?,
            subject: Some(subject),
            proof: UserProof::Incoming {
                caller,
                verifier,
                provider,
            },
            user,
            permission,
            user_permission,
        };
        admission.check()?;

        original.check().map_err(|_| AuthError::Request)?;
        Ok(admission)
    }
    fn web_user<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
        permissions: (Permission, Permission),
    ) -> Result<Admission<'a>, AuthError> {
        let (user_permission, permission) = permissions;
        self.incoming_user_permissions(provider, user_permission, permission)?;
        caller
            .check(verifier, BrokerRequestContext::new(original.snapshot()))
            .map_err(broker_error)?;

        let (_guard, snapshot) = self
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
        let subject = self
            .user_from_web(caller, verifier, original)
            .map_err(subject_error)?;
        let admission = Admission {
            issuer: self,
            original,
            identity: identity(&subject)?,
            subject: Some(subject),
            proof: UserProof::Web {
                caller,
                verifier,
                provider,
            },
            user,
            permission,
            user_permission,
        };
        admission.check()?;

        original.check().map_err(|_| AuthError::Request)?;
        Ok(admission)
    }
    /// Admit own USER Memory read from the actual registered Web access proof.
    pub fn admit_web_user_memory_read<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
    ) -> Result<UserReadAdmission<'a>, AuthError> {
        self.web_user(
            caller,
            verifier,
            original,
            provider,
            user,
            (Permission::ReadUser, Permission::Read(Module::Memory)),
        )
        .map(UserReadAdmission)
    }
    /// Admit own USER Memory mutation only under explicit server provider policy.
    pub fn admit_web_user_memory_write<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
        operation: UserMemoryWriteOperation,
    ) -> Result<UserWriteAdmission<'a>, AuthError> {
        Ok(UserWriteAdmission {
            admission: self.web_user(
                caller,
                verifier,
                original,
                provider,
                user,
                (Permission::MutateUser, operation.permission()),
            )?,
            operation,
        })
    }
    /// Admit own USER Knowledge read from the registered Web access proof.
    pub fn admit_web_user_knowledge_read<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
    ) -> Result<UserKnowledgeReadAdmission<'a>, AuthError> {
        self.web_user(
            caller,
            verifier,
            original,
            provider,
            user,
            (Permission::ReadUser, Permission::Read(Module::Knowledge)),
        )
        .map(UserKnowledgeReadAdmission)
    }
    /// Admit own USER Knowledge mutation under the explicit provider policy.
    /// This grants neither read access nor native execution.
    pub fn admit_web_user_knowledge_write<'a>(
        &'a self,
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
    ) -> Result<UserKnowledgeWriteAdmission<'a>, AuthError> {
        self.web_user(
            caller,
            verifier,
            original,
            provider,
            user,
            (
                Permission::MutateUser,
                Permission::MutateModule(Module::Knowledge),
            ),
        )
        .map(UserKnowledgeWriteAdmission)
    }
    pub fn admit_authenticated_user_memory_read<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        original: &'a RequestContext<'a>,
        user: &'a PublicationScope,
    ) -> Result<UserReadAdmission<'a>, AuthError> {
        self.authenticated_user(
            caller,
            original,
            user,
            Permission::ReadUser,
            Permission::Read(Module::Memory),
        )
        .map(UserReadAdmission)
    }
    pub fn admit_authenticated_user_memory_write<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
        original: &'a RequestContext<'a>,
        user: &'a PublicationScope,
        operation: UserMemoryWriteOperation,
    ) -> Result<UserWriteAdmission<'a>, AuthError> {
        Ok(UserWriteAdmission {
            admission: self.authenticated_user(
                caller,
                original,
                user,
                Permission::MutateUser,
                operation.permission(),
            )?,
            operation,
        })
    }
    /// Admit only Incoming MCP Knowledge READ under the configured USER policy.
    pub fn admit_incoming_user_knowledge_read<'a>(
        &'a self,
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
    ) -> Result<UserKnowledgeReadAdmission<'a>, AuthError> {
        self.incoming_user(
            caller,
            verifier,
            original,
            provider,
            user,
            Permission::ReadUser,
            Permission::Read(Module::Knowledge),
        )
        .map(UserKnowledgeReadAdmission)
    }
    /// Admit only Incoming MCP Knowledge mutation; this does not grant read access.
    pub fn admit_incoming_user_knowledge_write<'a>(
        &'a self,
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
    ) -> Result<UserKnowledgeWriteAdmission<'a>, AuthError> {
        self.incoming_user(
            caller,
            verifier,
            original,
            provider,
            user,
            Permission::MutateUser,
            Permission::MutateModule(Module::Knowledge),
        )
        .map(UserKnowledgeWriteAdmission)
    }
    pub fn admit_incoming_user_memory_read<'a>(
        &'a self,
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
    ) -> Result<UserReadAdmission<'a>, AuthError> {
        self.incoming_user(
            caller,
            verifier,
            original,
            provider,
            user,
            Permission::ReadUser,
            Permission::Read(Module::Memory),
        )
        .map(UserReadAdmission)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn admit_incoming_user_memory_write<'a>(
        &'a self,
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
        provider: &'a str,
        user: &'a PublicationScope,
        operation: UserMemoryWriteOperation,
    ) -> Result<UserWriteAdmission<'a>, AuthError> {
        Ok(UserWriteAdmission {
            admission: self.incoming_user(
                caller,
                verifier,
                original,
                provider,
                user,
                Permission::MutateUser,
                operation.permission(),
            )?,
            operation,
        })
    }
}

impl Admission<'_> {
    fn check(&self) -> Result<(), AuthError> {
        self.original.remaining().map_err(|_| AuthError::Request)?;
        match &self.proof {
            UserProof::Web {
                caller,
                verifier,
                provider,
            } => {
                if !matches!(
                    (self.user_permission, self.permission),
                    (
                        Permission::ReadUser,
                        Permission::Read(Module::Memory | Module::Knowledge)
                    ) | (
                        Permission::MutateUser,
                        Permission::WriteMemory
                            | Permission::WriteFencedMemory
                            | Permission::MutateModule(Module::Knowledge)
                    )
                ) {
                    return Err(AuthError::Denied);
                }
                self.issuer.incoming_user_permissions(
                    provider,
                    self.user_permission,
                    self.permission,
                )?;
                caller
                    .check(
                        verifier,
                        BrokerRequestContext::new(self.original.snapshot()),
                    )
                    .map_err(broker_error)?;

                self.issuer.check_broker_provider(
                    provider,
                    crate::broker::project_access::BrokerProjectCaller::Web(caller),
                    verifier,
                    self.original,
                )?;
            }
            UserProof::Native(caller) | UserProof::LocalMemory(caller) => {
                if !std::ptr::eq(caller.issuer, self.issuer)
                    || !std::ptr::eq(caller.request, self.original)
                {
                    return Err(AuthError::Denied);
                }
                caller.authorize(self.user_permission)?;
                caller.authorize(self.permission)?;
                if matches!(self.proof, UserProof::LocalMemory(_))
                    && !caller.uses_local_user_memory()?
                {
                    return Err(AuthError::Denied);
                }
            }
            UserProof::FencedParent { parent, local } => {
                if !std::ptr::eq(parent.issuer, self.issuer)
                    || !std::ptr::eq(parent.request, self.original)
                    || self.user_permission != Permission::MutateUser
                    || self.permission != Permission::WriteFencedMemory
                {
                    return Err(AuthError::Denied);
                }
                parent.caller.authorize(Permission::MutateUser)?;
                parent.authorize(Permission::WriteFencedMemory)?;
                if *local && !parent.caller.uses_local_user_memory()? {
                    return Err(AuthError::Denied);
                }
            }
            UserProof::Incoming {
                caller,
                verifier,
                provider,
            } => {
                self.issuer.incoming_user_permissions(
                    provider,
                    self.user_permission,
                    self.permission,
                )?;
                caller
                    .check(
                        verifier,
                        BrokerRequestContext::new(self.original.snapshot()),
                    )
                    .map_err(broker_error)?;

                self.issuer.check_broker_provider(
                    provider,
                    crate::broker::project_access::BrokerProjectCaller::Incoming(caller),
                    verifier,
                    self.original,
                )?;
            }
        }
        if let Some(subject) = &self.subject {
            let identity = subject.identity().map_err(subject_error)?;
            if identity.issuer != self.identity.issuer()
                || identity.subject != self.identity.subject()
            {
                return Err(AuthError::Denied);
            }
        }
        let StoreOwner::User {
            installation_id,
            identity,
        } = self.user.owner()
        else {
            return Err(AuthError::Denied);
        };
        if installation_id != &self.issuer.installation
            || identity != &self.identity
            || self.user.brand().key() != self.issuer.brand
            || self.user.owner_root() != self.issuer.realm.owner_root()
        {
            return Err(AuthError::Denied);
        }
        self.original
            .remaining()
            .map(|_| ())
            .map_err(|_| AuthError::Request)
    }
}
impl<'a> UserReadAdmission<'a> {
    pub fn recheck(&self) -> Result<(), AuthError> {
        self.0.check()
    }
    pub fn user(&self) -> Result<&'a PublicationScope, AuthError> {
        self.recheck()?;
        Ok(self.0.user)
    }
    pub fn identity(&self) -> Result<&UserIdentity, AuthError> {
        self.recheck()?;
        Ok(&self.0.identity)
    }
    pub fn request(&self) -> Result<&'a RequestContext<'a>, AuthError> {
        self.recheck()?;
        Ok(self.0.original)
    }
}
impl<'a> UserWriteAdmission<'a> {
    pub fn recheck(&self) -> Result<Obligation, AuthError> {
        self.admission.check()?;
        Ok(match self.operation {
            UserMemoryWriteOperation::Memory => Obligation::Unconditional,
            UserMemoryWriteOperation::FencedMemory => Obligation::MemoryFenceRequired,
        })
    }
    pub fn operation(&self) -> Result<UserMemoryWriteOperation, AuthError> {
        self.recheck()?;
        Ok(self.operation)
    }
    pub fn user(&self) -> Result<&'a PublicationScope, AuthError> {
        self.recheck()?;
        Ok(self.admission.user)
    }
    pub fn identity(&self) -> Result<&UserIdentity, AuthError> {
        self.recheck()?;
        Ok(&self.admission.identity)
    }
    pub fn request(&self) -> Result<&'a RequestContext<'a>, AuthError> {
        self.recheck()?;
        Ok(self.admission.original)
    }
    pub fn actor(&self) -> Result<Actor, AuthError> {
        self.recheck()?;
        match self.admission.proof {
            UserProof::Native(caller) | UserProof::LocalMemory(caller) => caller.actor(),
            UserProof::FencedParent { parent, .. } => parent.caller.actor(),
            UserProof::Web { caller, .. } => {
                let actor = records::web_actor(self.admission.issuer, caller);
                self.admission.check()?;
                Ok(actor)
            }
            UserProof::Incoming { .. } => {
                let mut hash = Sha256::new();
                hash.update(b"graphit-user-incoming-actor-v1\0");
                for part in [
                    self.admission.issuer.installation.as_str(),
                    self.admission.identity.issuer(),
                    self.admission.identity.subject(),
                ] {
                    hash.update((part.len() as u64).to_be_bytes());
                    hash.update(part.as_bytes());
                }
                let actor = Actor::from_digest(hash.finalize().into());
                self.admission.check()?;
                Ok(actor)
            }
        }
    }
}

impl<'a> UserKnowledgeReadAdmission<'a> {
    pub fn recheck(&self) -> Result<(), AuthError> {
        self.0.check()
    }
    pub fn user(&self) -> Result<&'a PublicationScope, AuthError> {
        self.recheck()?;
        Ok(self.0.user)
    }
    pub fn identity(&self) -> Result<&UserIdentity, AuthError> {
        self.recheck()?;
        Ok(&self.0.identity)
    }
    pub fn request(&self) -> Result<&'a RequestContext<'a>, AuthError> {
        self.recheck()?;
        Ok(self.0.original)
    }
}
impl<'a> UserKnowledgeWriteAdmission<'a> {
    pub fn recheck(&self) -> Result<Obligation, AuthError> {
        self.0.check()?;
        Ok(Obligation::Unconditional)
    }
    pub fn user(&self) -> Result<&'a PublicationScope, AuthError> {
        self.recheck()?;
        Ok(self.0.user)
    }
    pub fn identity(&self) -> Result<&UserIdentity, AuthError> {
        self.recheck()?;
        Ok(&self.0.identity)
    }
    pub fn request(&self) -> Result<&'a RequestContext<'a>, AuthError> {
        self.recheck()?;
        Ok(self.0.original)
    }
}

impl<'a> Admission<'a> {
    fn storage_principal(&self) -> super::record_storage::RecordStoragePrincipal<'a> {
        use super::record_storage::RecordStoragePrincipal as P;
        match self.proof {
            UserProof::LocalMemory(c) => P::LocalUserMemory(c),
            UserProof::Native(c) => P::Caller(c),
            UserProof::FencedParent { parent, local } => {
                if local {
                    P::LocalUserMemory(parent.caller)
                } else {
                    P::Caller(parent.caller)
                }
            }
            UserProof::Web {
                caller,
                verifier,
                provider,
                ..
            } => P::Web {
                caller,
                verifier,
                provider,
            },
            UserProof::Incoming {
                caller,
                verifier,
                provider,
                ..
            } => P::Incoming {
                caller,
                verifier,
                provider,
                original: self.original,
            },
        }
    }
}
impl<'a> UserReadAdmission<'a> {
    pub(super) fn storage_issuer(&self) -> &'a RoleIssuer {
        self.0.issuer
    }
    pub(super) fn storage_original(&self) -> &'a RequestContext<'a> {
        self.0.original
    }
    pub(super) fn storage_principal(&self) -> super::record_storage::RecordStoragePrincipal<'a> {
        self.0.storage_principal()
    }
}
impl<'a> UserWriteAdmission<'a> {
    pub(super) fn storage_issuer(&self) -> &'a RoleIssuer {
        self.admission.issuer
    }
    pub(super) fn storage_original(&self) -> &'a RequestContext<'a> {
        self.admission.original
    }
    pub(super) fn storage_principal(&self) -> super::record_storage::RecordStoragePrincipal<'a> {
        self.admission.storage_principal()
    }
}
