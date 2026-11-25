//! Typed authentication data over conventional private file I/O.
//!
//! This layer does not authenticate callers. Consumers must verify current
//! `ManageAuthProfiles` authority before publication.
use crate::profile::{AuthDocument, ProfileError, ProfileLimits};
use graphit_core::auth_document::{
    AuthDocumentError, AuthDocumentMutation, AuthDocumentSnapshot, AuthDocumentStore,
    publication::AuthPublicationEffect,
};
use graphit_hub::policy::RequestContext;
use secrecy::zeroize::Zeroizing;
use sha2::{Digest, Sha256};
use std::fmt;

#[cfg(test)]
thread_local! {
    pub(crate) static AFTER_AUTH_READ: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthStoreError {
    Core(AuthDocumentError),
    Profile(ProfileError),
}
impl fmt::Display for AuthStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => error.fmt(formatter),
            Self::Profile(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for AuthStoreError {}
impl From<AuthDocumentError> for AuthStoreError {
    fn from(error: AuthDocumentError) -> Self {
        Self::Core(error)
    }
}
impl From<ProfileError> for AuthStoreError {
    fn from(error: ProfileError) -> Self {
        Self::Profile(error)
    }
}

/// Typed data at the configured private authentication document path.
pub struct AuthStore {
    physical: AuthDocumentStore,
    limits: ProfileLimits,
}
impl AuthStore {
    #[cfg(test)]
    pub(crate) fn cancel_publication_before_replace(
        &self,
        context: &RequestContext<'_>,
    ) -> Result<
        graphit_core::auth_document::test_support::PublicationCancellation,
        graphit_core::auth_document::test_support::ArmError,
    > {
        graphit_core::auth_document::test_support::PublicationCancellation::before_replace(
            &self.physical,
            context.snapshot(),
        )
    }

    /// Validates positive schema limits without creating files or auth defaults.
    pub fn new(
        physical: AuthDocumentStore,
        limits: ProfileLimits,
        context: &RequestContext<'_>,
    ) -> Result<Self, AuthStoreError> {
        drop(AuthDocument::new(limits, context)?);
        Ok(Self { physical, limits })
    }

    pub(crate) fn profile_limits(&self) -> ProfileLimits {
        self.limits
    }

    pub(crate) fn try_profile_refresh_lock(
        &self,
        profile: &str,
        context: &RequestContext<'_>,
    ) -> Result<Option<graphit_core::file_lock::FileLock>, AuthStoreError> {
        use std::fmt::Write as _;
        crate::profile::valid_name(profile)?;
        let mut digest = Sha256::new();
        digest.update(b"graphit-native-auth-profile-refresh-lock\0");
        digest.update(profile.as_bytes());
        let mut name = String::new();
        name.try_reserve_exact(96)
            .map_err(|_| ProfileError::Limit)?;
        name.push_str(".auth-refresh-");
        for byte in digest.finalize() {
            write!(&mut name, "{byte:02x}").map_err(|_| ProfileError::Limit)?;
        }
        name.push_str(".lock");
        Ok(self
            .physical
            .try_profile_refresh_lock(std::ffi::OsStr::new(&name), context.snapshot())?)
    }

    /// Borrows disjoint original fields; no capture, parse, copy or grant.
    pub(crate) fn snapshot_parts<'s>(
        &'s self,
        snapshot: &'s mut AuthSnapshot,
    ) -> (AuthSnapshotRecheck<'s>, &'s AuthDocument) {
        (
            AuthSnapshotRecheck {
                store: self,
                physical: &mut snapshot.physical,
            },
            &snapshot.document,
        )
    }

    pub(crate) fn maximum_encoded_bytes(&self) -> usize {
        self.limits.bytes
    }

    /// Read and decode current bytes under the short shared publisher lock.
    pub(crate) fn load_locked(
        &self,
        context: &RequestContext<'_>,
    ) -> Result<(graphit_core::file_lock::FileLock, AuthSnapshot), AuthStoreError> {
        let (guard, physical) = self.physical.read_locked(context.snapshot())?;
        #[cfg(test)]
        AFTER_AUTH_READ.with(|slot| {
            let hook = slot.borrow_mut().take();
            if let Some(hook) = hook {
                hook();
            }
        });
        let was_present = physical.bytes().is_some();
        let document = match physical.bytes() {
            Some(bytes) => AuthDocument::decode(bytes, self.limits, context),
            None => AuthDocument::new(self.limits, context),
        };
        Ok((
            guard,
            AuthSnapshot {
                physical,
                document: document?,
                was_present,
            },
        ))
    }

    pub(crate) fn root(&self) -> &std::path::Path {
        self.physical.root()
    }

    /// Read current private bytes and decode them under the original request.
    pub fn load(&self, context: &RequestContext<'_>) -> Result<AuthSnapshot, AuthStoreError> {
        let physical = self.physical.read(context.snapshot())?;
        #[cfg(test)]
        AFTER_AUTH_READ.with(|slot| {
            let hook = slot.borrow_mut().take();
            if let Some(hook) = hook {
                hook();
            }
        });
        let was_present = physical.bytes().is_some();
        let document = match physical.bytes() {
            Some(bytes) => AuthDocument::decode(bytes, self.limits, context),
            None => AuthDocument::new(self.limits, context),
        };
        Ok(AuthSnapshot {
            physical,
            document: document?,
            was_present,
        })
    }

    /// Publish with byte CAS; only a successful atomic replacement returns Applied.
    pub fn publish(
        &self,
        snapshot: AuthSnapshot,
        context: &RequestContext<'_>,
    ) -> Result<AuthPublicationEffect, AuthStoreError> {
        self.mutate(snapshot, context, |_, _| Ok(()))
    }

    /// Apply typed data operations before the short content-CAS publication.
    /// The closure holds no publication lock. Remote producers must finish first.
    pub fn mutate(
        &self,
        snapshot: AuthSnapshot,
        context: &RequestContext<'_>,
        mutation: impl FnOnce(&mut AuthDocument, &RequestContext<'_>) -> Result<(), ProfileError>,
    ) -> Result<AuthPublicationEffect, AuthStoreError> {
        let prepared = self.prepare(snapshot, context, mutation)?;
        self.commit(prepared)
    }

    pub(crate) fn prepare<'a>(
        &'a self,
        mut snapshot: AuthSnapshot,
        context: &'a RequestContext<'a>,
        mutation: impl FnOnce(&mut AuthDocument, &RequestContext<'_>) -> Result<(), ProfileError>,
    ) -> Result<PreparedAuthPublication<'a>, AuthStoreError> {
        context
            .snapshot()
            .check()
            .map_err(AuthDocumentError::from)?;
        mutation(&mut snapshot.document, context)?;
        let AuthSnapshot {
            physical,
            document,
            was_present: _,
        } = snapshot;
        let body = document.encode(context)?;
        drop(document);
        Ok(PreparedAuthPublication {
            store: self,
            context,
            physical,
            body,
        })
    }

    pub(crate) fn commit(
        &self,
        prepared: PreparedAuthPublication<'_>,
    ) -> Result<AuthPublicationEffect, AuthStoreError> {
        if !std::ptr::eq(self, prepared.store) {
            return Err(AuthStoreError::Core(AuthDocumentError::Denied));
        }
        self.physical
            .publish(
                prepared.physical,
                &prepared.body,
                prepared.context.snapshot(),
            )
            .map_err(Into::into)
    }

    /// Decode and mutate the latest document while retaining its exact publisher
    /// lock. Callers validate original target expectations; no HTTP await is allowed.
    pub(crate) fn prepare_latest<'a>(
        &'a self,
        context: &'a RequestContext<'a>,
        mutation: impl FnOnce(&mut AuthDocument, &RequestContext<'_>) -> Result<(), ProfileError>,
    ) -> Result<PreparedLatestAuthPublication<'a>, AuthStoreError> {
        let physical = self.physical.lock_latest(context.snapshot())?;
        let mut document = match physical.bytes() {
            Some(bytes) => AuthDocument::decode(bytes, self.limits, context)?,
            None => return Err(ProfileError::Missing.into()),
        };
        mutation(&mut document, context)?;
        let body = document.encode(context)?;
        Ok(PreparedLatestAuthPublication {
            store: self,
            physical,
            body,
        })
    }

    pub(crate) fn commit_latest(
        &self,
        prepared: PreparedLatestAuthPublication<'_>,
    ) -> Result<AuthPublicationEffect, AuthStoreError> {
        if !std::ptr::eq(self, prepared.store) {
            return Err(AuthDocumentError::Denied.into());
        }
        prepared
            .physical
            .publish(&prepared.body)
            .map_err(Into::into)
    }
}

/// Encoded data bound to its actual store and original request, never authority.
pub(crate) struct PreparedAuthPublication<'a> {
    store: &'a AuthStore,
    context: &'a RequestContext<'a>,
    physical: AuthDocumentSnapshot,
    body: Zeroizing<Vec<u8>>,
}

pub(crate) struct PreparedLatestAuthPublication<'a> {
    store: &'a AuthStore,
    physical: AuthDocumentMutation<'a>,
    body: Zeroizing<Vec<u8>>,
}

/// Typed document data and the expected bytes for a later content CAS.
/// It carries no caller, role, or permission proof.
///
/// ```compile_fail
/// use graphit_auth::store::AuthSnapshot;
/// fn duplicate(value: &AuthSnapshot) -> AuthSnapshot { value.clone() }
/// ```
/// ```compile_fail
/// use graphit_auth::store::AuthSnapshot;
/// fn disclose(value: &AuthSnapshot) { println!("{value:?}"); }
/// ```
/// ```compile_fail
/// use graphit_auth::store::AuthSnapshot;
/// let _: AuthSnapshot = serde_json::from_str("{}").unwrap();
/// ```
pub struct AuthSnapshot {
    physical: AuthDocumentSnapshot,
    document: AuthDocument,
    was_present: bool,
}
impl AuthSnapshot {
    pub fn document(&self) -> &AuthDocument {
        &self.document
    }
    pub fn was_present(&self) -> bool {
        self.was_present
    }
}

/// Borrowed expected bytes; freshness checks read current content, never directory proofs.
pub(crate) struct AuthSnapshotRecheck<'a> {
    store: &'a AuthStore,
    physical: &'a mut AuthDocumentSnapshot,
}
impl AuthSnapshotRecheck<'_> {
    pub(crate) fn recheck(
        &mut self,
        original: &graphit_core::file_io::SnapshotContext,
    ) -> Result<(), AuthStoreError> {
        let current = self.store.physical.read(original)?;
        if current.bytes() != self.physical.bytes() {
            return Err(AuthDocumentError::Conflict.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
