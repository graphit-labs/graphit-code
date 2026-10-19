//! Storage selection stays tied to the admitted record operation and its issuer.
use super::service_credentials::{Authority, ProfileSelection};
use super::*;
use crate::broker::{
    AuthenticatedBrokerCaller, BrokerVerifier, oauth::web::AuthenticatedWebCaller,
};
use crate::profile::{ProfileError, ProviderKind};
use graphit_core::{installation_identity::InstallationId, project_identity::ProjectId};
use std::{future::Future, io, pin::Pin};

#[derive(Clone)]
pub struct RemoteRecordStorage {
    /// Stable physical destination fingerprint; excludes renewable credentials.
    pub corpus_key: String,
    pub uri: String,
    pub credentials: object_store::aws::AwsCredentialProvider,
    pub storage_options: Arc<std::collections::HashMap<String, String>>,
    pub expires_at: Option<Instant>,
}
impl RemoteRecordStorage {
    pub fn check(&self) -> io::Result<()> {
        if self
            .expires_at
            .is_some_and(|expiry| Instant::now() >= expiry)
        {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "record storage credentials expired",
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy)]
pub enum RecordStorageAdmission<'a> {
    ProjectRead(&'a ProjectReadAdmission<'a>),
    ProjectWrite(&'a ProjectWriteAdmission<'a>),
    UserRead(&'a UserReadAdmission<'a>),
    UserWrite(&'a UserWriteAdmission<'a>),
}
#[derive(Clone, Copy)]
pub enum RecordStorageScope<'a> {
    Project {
        project_id: ProjectId,
        module: &'static str,
    },
    User {
        installation_id: &'a InstallationId,
    },
}
/// The application supplies discovery/STS transport; it cannot mint an admission.
pub trait RecordStorageResolver: Send + Sync {
    /// Optional demand renewal before Native USER authentication. No admission is minted.
    fn prepare_native<'a>(
        &'a self,
        mut selection: ServiceSelection<'a>,
        request: &'a RequestContext<'a>,
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let original = selection.original().map_err(io::Error::other)?;
            if !Arc::ptr_eq(original.snapshot(), request.snapshot()) {
                return Err(io::Error::other(AuthError::Request));
            }
            Ok(())
        })
    }
    fn resolve<'a>(
        &'a self,
        admission: RecordStorageAdmission<'a>,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<RemoteRecordStorage>>> + Send + 'a>>;
}

pub(super) enum RecordStoragePrincipal<'a> {
    Caller(&'a AuthenticatedCaller<'a>),
    Role(&'a VerifiedRoleGrant<'a>),
    Web {
        caller: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
    },
    Incoming {
        caller: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        provider: &'a str,
        original: &'a RequestContext<'a>,
    },
}
impl<'a> RecordStoragePrincipal<'a> {
    pub(super) fn authority(&self) -> Option<Authority<'a>> {
        match self {
            Self::Caller(c) => Some(Authority::Caller(c)),
            Self::Role(r) => Some(Authority::Role(r)),
            Self::Incoming {
                caller,
                verifier,
                original,
                ..
            } => Some(Authority::Incoming {
                caller,
                verifier,
                original,
            }),
            Self::Web { .. } => None,
        }
    }
    pub(super) fn origin(&self) -> ServiceOrigin {
        match self {
            Self::Web { .. } => ServiceOrigin::Web,
            Self::Incoming { .. } => ServiceOrigin::BrokerIncoming,
            _ => self.authority().expect("native principal").origin(),
        }
    }
    pub(super) fn hub_web_caller(
        &self,
    ) -> Option<(&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str)> {
        match self {
            Self::Web {
                caller,
                verifier,
                provider,
                ..
            } => Some((caller, verifier, provider)),
            _ => None,
        }
    }
    pub(super) fn provider(&self) -> Option<&'a str> {
        match self {
            Self::Web { provider, .. } | Self::Incoming { provider, .. } => Some(provider),
            _ => None,
        }
    }
}
impl<'a> RecordStorageAdmission<'a> {
    pub fn recheck(self) -> Result<(), AuthError> {
        match self {
            Self::ProjectRead(a) => a.recheck(),
            Self::ProjectWrite(a) => a.recheck().map(|_| ()),
            Self::UserRead(a) => a.recheck(),
            Self::UserWrite(a) => a.recheck().map(|_| ()),
        }
    }
    pub fn request(self) -> Result<&'a RequestContext<'a>, AuthError> {
        self.recheck()?;
        Ok(self.original())
    }
    pub(super) fn original(self) -> &'a RequestContext<'a> {
        match self {
            Self::ProjectRead(a) => a.storage_original(),
            Self::ProjectWrite(a) => a.storage_original(),
            Self::UserRead(a) => a.storage_original(),
            Self::UserWrite(a) => a.storage_original(),
        }
    }
    pub(super) fn issuer(self) -> &'a RoleIssuer {
        match self {
            Self::ProjectRead(a) => a.storage_issuer(),
            Self::ProjectWrite(a) => a.storage_issuer(),
            Self::UserRead(a) => a.storage_issuer(),
            Self::UserWrite(a) => a.storage_issuer(),
        }
    }
    pub(super) fn principal(self) -> RecordStoragePrincipal<'a> {
        match self {
            Self::ProjectRead(a) => a.storage_principal(),
            Self::ProjectWrite(a) => a.storage_principal(),
            Self::UserRead(a) => a.storage_principal(),
            Self::UserWrite(a) => a.storage_principal(),
        }
    }
    pub fn user_identity(
        self,
    ) -> Result<Option<&'a graphit_core::artifact_store::UserIdentity>, AuthError> {
        match self {
            Self::UserRead(a) => a.identity().map(Some),
            Self::UserWrite(a) => a.identity().map(Some),
            _ => Ok(None),
        }
    }
    pub fn scope(self) -> Result<RecordStorageScope<'a>, AuthError> {
        self.recheck()?;
        let project = match self {
            Self::ProjectRead(a) => Some((
                a.project()?,
                match a.module() {
                    ProjectReadModule::Memory => "memory",
                    ProjectReadModule::Task => "task",
                    _ => return Err(AuthError::Denied),
                },
            )),
            Self::ProjectWrite(a) => Some((
                a.project()?,
                match a.operation() {
                    ProjectWriteOperation::Memory | ProjectWriteOperation::FencedMemory => "memory",
                    ProjectWriteOperation::OwnTask
                    | ProjectWriteOperation::Task
                    | ProjectWriteOperation::CoordinateTask => "task",
                    _ => return Err(AuthError::Denied),
                },
            )),
            Self::UserRead(_) | Self::UserWrite(_) => None,
        };
        if let Some((project, module)) = project {
            let StoreOwner::Project { project_id } = project.scope.live.owner() else {
                return Err(AuthError::Denied);
            };
            Ok(RecordStorageScope::Project {
                project_id: *project_id,
                module,
            })
        } else {
            Ok(RecordStorageScope::User {
                installation_id: &self.issuer().installation,
            })
        }
    }
    /// The effective provider is selected only after the concrete record permission.
    /// A missing local profile or a local provider without S3 is genuinely local.
    pub fn service(self) -> Result<Option<ServiceSelection<'a>>, ServiceUseError> {
        self.recheck()?;
        self.scope()?;
        let issuer = self.issuer();
        let principal = self.principal();
        let (guard, mut snapshot) = issuer.local.load_locked(self.original())?;
        let document = snapshot.document();
        let (provider, profile) = if let Some(name) = principal.provider() {
            (document.provider(name).ok_or(ProfileError::Missing)?, None)
        } else {
            match document.active(self.original()) {
                Ok((provider, profile)) => (provider, Some(profile)),
                Err(ProfileError::NoActive) => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        };
        let name = provider.name().to_owned();
        let standalone = matches!(provider.config().kind, ProviderKind::Local { .. })
            && provider.config().s3.bucket.is_some();
        let remote = standalone || matches!(provider.config().kind, ProviderKind::Broker(_));
        let profile_name = profile.map(|p| p.name().to_owned());
        issuer.local.revalidate(&mut snapshot, self.original())?;
        if let Some(guard) = guard {
            guard
                .revalidate()
                .map_err(|e| crate::store::AuthStoreError::Core(e.into()))?;
        }
        self.recheck()?;
        if !remote {
            return Ok(None);
        }
        issuer
            .capture_service_authority_selected(
                Authority::Records(self),
                ServicePurpose::RecordStorage,
                Some(&name),
                if standalone {
                    ProfileSelection::Standalone(profile_name.as_deref())
                } else {
                    ProfileSelection::Active
                },
            )
            .map(Some)
    }
    async fn resolve(self) -> io::Result<Option<RemoteRecordStorage>> {
        self.recheck().map_err(io::Error::other)?;
        let result = if let Some(resolver) = self.issuer().record_storage.get() {
            resolver.resolve(self).await?
        } else if self.service().map_err(io::Error::other)?.is_none() {
            None
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "remote record storage resolver is not configured",
            ));
        };
        self.recheck().map_err(io::Error::other)?;
        if let Some(remote) = &result {
            remote.check()?;
        }
        Ok(result)
    }
}
impl RoleIssuer {
    /// Prepare only this installation's selected Native Records credential. Incoming/Web
    /// callers keep their own tokens; this step never grants USER or PROJECT permission.
    pub async fn prepare_native_record_access<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
    ) -> io::Result<()> {
        caller.native_provider().map_err(io::Error::other)?;
        let Some(resolver) = self.record_storage.get() else {
            return Ok(());
        };
        let snapshot = self.local.load(caller.request).map_err(io::Error::other)?;
        let (provider, _) = snapshot
            .document()
            .active(caller.request)
            .map_err(io::Error::other)?;
        let selection = self
            .capture_service_authority_selected(
                Authority::Caller(caller),
                ServicePurpose::RecordStorage,
                Some(provider.name()),
                ProfileSelection::Active,
            )
            .map_err(io::Error::other)?;
        resolver.prepare_native(selection, caller.request).await
    }
    pub fn set_record_storage_resolver(
        &self,
        resolver: Arc<dyn RecordStorageResolver>,
    ) -> Result<(), AuthError> {
        self.record_storage
            .set(resolver)
            .map_err(|_| AuthError::State)
    }
}
macro_rules! record_storage_method {
    ($ty:ident,$variant:ident) => {
        impl $ty<'_> {
            pub async fn record_storage(&self) -> io::Result<Option<RemoteRecordStorage>> {
                RecordStorageAdmission::$variant(self).resolve().await
            }
        }
    };
}
record_storage_method!(ProjectReadAdmission, ProjectRead);
record_storage_method!(ProjectWriteAdmission, ProjectWrite);
record_storage_method!(UserReadAdmission, UserRead);
record_storage_method!(UserWriteAdmission, UserWrite);
