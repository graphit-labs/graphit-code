//! Fresh Knowledge history admission; no conversion between Runtime and USER.
use graphit_auth::{
    AuthError, AuthenticatedCaller, Permission, UserKnowledgeReadAdmission,
    UserKnowledgeWriteAdmission,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeContext, StoreOwner},
    module_settings::Module,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_hub::policy::RequestContext;
use std::{io, sync::Arc};

enum Authority<'a> {
    Service(Box<AuthenticatedCaller<'a>>),
    User {
        read: &'a UserKnowledgeReadAdmission<'a>,
        write: Option<&'a UserKnowledgeWriteAdmission<'a>>,
    },
}

pub(crate) struct Access<'a> {
    original: &'a RequestContext<'a>,
    scope: CapturedPublicationScope,
    authority: Authority<'a>,
}
impl<'a> Access<'a> {
    pub(crate) fn service(
        auth: &'a DaemonAuth,
        credential: &str,
        original: &'a RequestContext<'a>,
    ) -> io::Result<Self> {
        original.check()?;
        let caller = auth
            .issuer()
            .authenticate_runtime(credential, original)
            .map_err(auth_error)?;
        let scope = auth.installation().live;
        if !matches!(scope.owner(), StoreOwner::Installation { .. }) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let value = Self {
            original,
            scope: scope.clone(),
            authority: Authority::Service(Box::new(caller)),
        };
        value.read()?;
        Ok(value)
    }
    pub(crate) fn user(
        read: &'a UserKnowledgeReadAdmission<'a>,
        write: Option<&'a UserKnowledgeWriteAdmission<'a>>,
    ) -> io::Result<Self> {
        let original = read.request().map_err(auth_error)?;
        let captured = read.user().map_err(auth_error)?;
        if !matches!(captured.live.owner(), StoreOwner::User { .. }) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if let Some(write) = write {
            let other = write.request().map_err(auth_error)?;
            let write_scope = write.user().map_err(auth_error)?;
            if !Arc::ptr_eq(original.snapshot(), other.snapshot())
                || write_scope.live.owner() != captured.live.owner()
                || !write_scope.record.matches_scope_bounded(
                    captured.live,
                    &ScopeContext::from_snapshot(original.snapshot()),
                )?
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        let value = Self {
            original,
            scope: captured.live.clone(),
            authority: Authority::User { read, write },
        };
        value.read()?;
        Ok(value)
    }
    pub(crate) fn original(&self) -> &'a RequestContext<'a> {
        self.original
    }
    pub(crate) fn scope(&self) -> &CapturedPublicationScope {
        &self.scope
    }
    pub(crate) fn read(&self) -> io::Result<()> {
        self.original.check()?;
        match &self.authority {
            Authority::Service(caller) => {
                caller
                    .authorize(Permission::Read(Module::Knowledge))
                    .map_err(auth_error)?;
            }
            Authority::User { read, .. } => read.recheck().map_err(auth_error)?,
        }
        self.scope
            .revalidate_bounded(&ScopeContext::from_snapshot(self.original.snapshot()))
    }
    pub(crate) fn write(&self) -> io::Result<()> {
        self.read()?;
        match &self.authority {
            Authority::Service(caller) => {
                caller
                    .authorize(Permission::MutateModule(Module::Knowledge))
                    .map_err(auth_error)?;
            }
            Authority::User { write, .. } => {
                write
                    .ok_or(io::ErrorKind::PermissionDenied)?
                    .recheck()
                    .map_err(auth_error)?;
            }
        }
        self.original.check()
    }
}

fn auth_error(error: AuthError) -> io::Error {
    let kind = match error {
        AuthError::Denied | AuthError::Expired => io::ErrorKind::PermissionDenied,
        AuthError::Request => io::ErrorKind::Interrupted,
        AuthError::Invalid | AuthError::InvalidSetting(_) => io::ErrorKind::InvalidInput,
        AuthError::Limit => io::ErrorKind::FileTooLarge,
        AuthError::Entropy | AuthError::State | AuthError::Io => io::ErrorKind::Other,
    };
    io::Error::new(kind, error)
}
