//! Typed installation authentication data over actual private Core observations.
//!
//! This layer does not authenticate callers. Consumers must verify current
//! `ManageAuthProfiles` authority before publication, recovery, or cleanup.
use crate::profile::{AuthDocument, ProfileError, ProfileLimits};
use graphit_core::auth_document::{
    AuthDocumentError, AuthDocumentSnapshot, AuthDocumentStore,
    publication::{AuthDocumentPublication, PendingCleanup},
    recovery::AuthStageRecovery,
};
use graphit_hub::policy::RequestContext;
use secrecy::zeroize::Zeroizing;
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

/// Owns an actual captured installation store, never a reconstructed path proof.
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

    /// Decode one observation while retaining its existing publisher lock.
    /// An absent document uses Core's original absence observation without rereading.
    pub(crate) fn load_locked<'a>(
        &'a self,
        context: &'a RequestContext<'_>,
    ) -> Result<
        (
            Option<graphit_core::native_document_lock::NativeDocumentLock<'a>>,
            AuthSnapshot,
        ),
        AuthStoreError,
    > {
        let (guard, mut physical) = self.physical.read_locked(context.snapshot())?;
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
        self.physical
            .revalidate(&mut physical, context.snapshot())?;
        if let Some(guard) = &guard {
            guard
                .revalidate()
                .map_err(|error| AuthStoreError::Core(error.into()))?;
        }
        Ok((
            guard,
            AuthSnapshot {
                physical,
                document: document?,
                was_present,
            },
        ))
    }

    pub(crate) fn revalidate(
        &self,
        snapshot: &mut AuthSnapshot,
        context: &RequestContext<'_>,
    ) -> Result<(), AuthStoreError> {
        self.physical
            .revalidate(&mut snapshot.physical, context.snapshot())?;
        Ok(())
    }

    pub(crate) fn recheck_installation(
        &self,
        realm: &graphit_core::artifact_store::CapturedPublicationScope,
        context: &graphit_core::file_io::SnapshotContext,
    ) -> Result<(), AuthStoreError> {
        self.physical.recheck_installation(realm, context)?;
        Ok(())
    }
    pub(crate) fn revalidate_observation(
        &self,
        observation: &mut LocalObservation,
        context: &graphit_core::file_io::SnapshotContext,
    ) -> Result<(), AuthStoreError> {
        self.physical
            .revalidate(&mut observation.physical, context)?;
        Ok(())
    }

    /// Reads and parses once, then verifies the original physical observation.
    /// Present documents require two bounded physical content passes. The final
    /// physical/context check also runs when decoding fails and takes precedence.
    pub fn load(&self, context: &RequestContext<'_>) -> Result<AuthSnapshot, AuthStoreError> {
        let mut physical = self.physical.read(context.snapshot())?;
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
        self.physical
            .revalidate(&mut physical, context.snapshot())?;
        Ok(AuthSnapshot {
            physical,
            document: document?,
            was_present,
        })
    }

    /// Consumes the observation. Core effects and cleanup proofs remain intact.
    pub fn publish(
        &self,
        snapshot: AuthSnapshot,
        context: &RequestContext<'_>,
    ) -> Result<AuthDocumentPublication, AuthStoreError> {
        self.mutate(snapshot, context, |_, _| Ok(()))
    }

    /// Applies pure typed data operations after validating request/store identity.
    /// The closure holds no publication lock. Remote producers must finish first.
    pub fn mutate(
        &self,
        snapshot: AuthSnapshot,
        context: &RequestContext<'_>,
        mutation: impl FnOnce(&mut AuthDocument, &RequestContext<'_>) -> Result<(), ProfileError>,
    ) -> Result<AuthDocumentPublication, AuthStoreError> {
        let prepared = self.prepare(snapshot, context, mutation)?;
        self.commit(prepared)
    }

    pub(crate) fn prepare<'a>(
        &'a self,
        mut snapshot: AuthSnapshot,
        context: &'a RequestContext<'a>,
        mutation: impl FnOnce(&mut AuthDocument, &RequestContext<'_>) -> Result<(), ProfileError>,
    ) -> Result<PreparedAuthPublication<'a>, AuthStoreError> {
        self.physical
            .revalidate(&mut snapshot.physical, context.snapshot())?;
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
    ) -> Result<AuthDocumentPublication, AuthStoreError> {
        if !std::ptr::eq(self, prepared.store) {
            return Err(AuthStoreError::Core(AuthDocumentError::Denied));
        }
        Ok(self.physical.publish(
            prepared.physical,
            &prepared.body,
            prepared.context.snapshot(),
        ))
    }

    /// Explicitly discards the managed stage without parsing or promoting it.
    /// The consumer must first verify current `ManageAuthProfiles` authority.
    pub fn recover_stage(&self, context: &RequestContext<'_>) -> AuthStageRecovery {
        self.physical.recover_stage(context.snapshot())
    }

    /// Passes an actual retained Core cleanup proof to its original store.
    /// A newly authorized request supplies its own cancellation and deadline.
    pub fn cleanup_pending(
        &self,
        pending: PendingCleanup,
        context: &RequestContext<'_>,
    ) -> AuthStageRecovery {
        self.physical.cleanup_pending(pending, context.snapshot())
    }
}

/// Encoded data bound to its actual store and original request, never authority.
pub(crate) struct PreparedAuthPublication<'a> {
    store: &'a AuthStore,
    context: &'a RequestContext<'a>,
    physical: AuthDocumentSnapshot,
    body: Zeroizing<Vec<u8>>,
}

/// Typed data tied to a real opaque Core observation and original request.
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
    pub(crate) fn into_observation(self) -> LocalObservation {
        let Self {
            physical,
            document,
            was_present: _,
        } = self;
        drop(document);
        LocalObservation { physical }
    }
    pub fn document(&self) -> &AuthDocument {
        &self.document
    }
    pub fn was_present(&self) -> bool {
        self.was_present
    }
}

/// Borrowed original observation, private to this crate and never authority.
pub(crate) struct AuthSnapshotRecheck<'a> {
    store: &'a AuthStore,
    physical: &'a mut AuthDocumentSnapshot,
}
impl AuthSnapshotRecheck<'_> {
    pub(crate) fn recheck(
        &mut self,
        original: &graphit_core::file_io::SnapshotContext,
    ) -> Result<(), AuthStoreError> {
        self.store.physical.revalidate(self.physical, original)?;
        Ok(())
    }
}

/// Consumed original physical observation, never stored identity or authority.
/// ```compile_fail
/// use graphit_auth::store::LocalObservation;
/// ```
pub(crate) struct LocalObservation {
    physical: AuthDocumentSnapshot,
}

#[cfg(test)]
mod tests;
