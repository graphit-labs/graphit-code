//! Discard the current managed stage; never promote or interpret its contents.
use super::{
    publication::{LOCK, PendingCleanup, STAGE},
    *,
};
use crate::{local_files::regular_metadata, native_document_lock::NativeDocumentLock};
use std::fs;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthCleanupEffect {
    Absent,
    Applied,
    NotApplied,
    Unknown,
}
#[must_use]
pub struct AuthStageRecovery {
    effect: AuthCleanupEffect,
    failure: Option<AuthDocumentError>,
    pending: Option<PendingCleanup>,
}
impl AuthStageRecovery {
    pub fn effect(&self) -> AuthCleanupEffect {
        self.effect
    }
    pub fn failure(&self) -> Option<AuthDocumentError> {
        self.failure
    }
    pub fn into_pending_cleanup(self) -> Option<PendingCleanup> {
        self.pending
    }
    fn failed(error: impl Into<AuthDocumentError>, pending: Option<PendingCleanup>) -> Self {
        Self {
            effect: AuthCleanupEffect::NotApplied,
            failure: Some(error.into()),
            pending,
        }
    }
}

impl AuthDocumentStore {
    /// The caller's current profile-management permission includes discarding any
    /// private regular content in this exact reserved namespace. No birth claim
    /// is made about files left by a prior process.
    pub fn recover_stage(&self, context: &SnapshotContext) -> AuthStageRecovery {
        self.discard_stage(None, context)
    }
    /// Retry bounded cleanup using an actual creation proof retained in process.
    /// A new authorized request supplies its own original deadline and budget.
    pub fn cleanup_pending(
        &self,
        pending: PendingCleanup,
        context: &SnapshotContext,
    ) -> AuthStageRecovery {
        if !Arc::ptr_eq(&self.identity, &pending.store_identity) {
            return AuthStageRecovery::failed(AuthDocumentError::Conflict, Some(pending));
        }
        self.discard_stage(Some(pending), context)
    }
    fn discard_stage(
        &self,
        pending: Option<PendingCleanup>,
        context: &SnapshotContext,
    ) -> AuthStageRecovery {
        let prepared: io::Result<AuthStageRecovery> = (|| {
            self.check_io(context)?;
            let path = self.directory.path(OsStr::new(STAGE))?;
            if pending
                .as_ref()
                .is_some_and(|proof| proof.scratch.path() != path)
            {
                return Err(private_document::conflict());
            }
            let lock = NativeDocumentLock::acquire(&self.directory, OsStr::new(LOCK), context)?;
            let check = || {
                self.check_io(context)?;
                lock.revalidate()
            };
            let access = self.access(context, &check);
            let current = private_document::capture(&path, &access)?;
            if let Some((file, observed)) = &current {
                if let Some(proof) = &pending {
                    proof.scratch.verify(file, &check)?;
                }
                phase("captured");
                if private_document::observe(&path, file, &access)? != *observed {
                    return Err(private_document::conflict());
                }
            }
            // Close Windows' non-delete-sharing observation handle before unlink.
            let observation = current.map(|(file, observed)| {
                drop(file);
                observed
            });
            access.check()?;
            phase("before_remove");
            let Some(observation) = observation else {
                if regular_metadata(&path)?.is_some() {
                    return Err(private_document::conflict());
                }
                access.check()?;
                return Ok(AuthStageRecovery {
                    effect: AuthCleanupEffect::Absent,
                    failure: None,
                    pending: None,
                });
            };
            let Some((file, latest)) = private_document::capture(&path, &access)? else {
                return Err(private_document::conflict());
            };
            if latest != observation {
                return Err(private_document::conflict());
            }
            if let Some(proof) = &pending {
                proof.scratch.verify(&file, &check)?;
            }
            drop(file);
            let identity = observation.identity;
            let metadata = regular_metadata(&path)?.ok_or_else(private_document::conflict)?;
            if crate::native_file_identity::physical_id(&path, &metadata)? != identity {
                return Err(private_document::conflict());
            }
            access.check()?;
            let native = native_remove(&path);
            phase("after_remove");
            let observed = (|| {
                access.check()?;
                regular_metadata(&path)
            })();
            let failure = native
                .as_ref()
                .err()
                .map(|error| AuthDocumentError::from(io::Error::from(error.kind())));
            let result = match (native.is_ok(), observed) {
                (_, Ok(None)) => AuthStageRecovery {
                    effect: AuthCleanupEffect::Applied,
                    failure,
                    pending: None,
                },
                (true, Err(error)) => AuthStageRecovery {
                    effect: AuthCleanupEffect::Applied,
                    failure: Some(error.into()),
                    pending: None,
                },
                (true, Ok(Some(_))) => AuthStageRecovery {
                    effect: AuthCleanupEffect::Applied,
                    failure: Some(AuthDocumentError::Conflict),
                    pending: None,
                },
                (false, Ok(Some(metadata))) => {
                    match crate::native_file_identity::physical_id(&path, &metadata) {
                        Ok(current) if current == identity => AuthStageRecovery {
                            effect: AuthCleanupEffect::NotApplied,
                            failure,
                            pending: None,
                        },
                        Ok(_) => AuthStageRecovery {
                            effect: AuthCleanupEffect::Unknown,
                            failure: Some(AuthDocumentError::Conflict),
                            pending: None,
                        },
                        Err(error) => AuthStageRecovery {
                            effect: AuthCleanupEffect::Unknown,
                            failure: Some(error.into()),
                            pending: None,
                        },
                    }
                }
                (false, Err(error)) => AuthStageRecovery {
                    effect: AuthCleanupEffect::Unknown,
                    failure: Some(error.into()),
                    pending: None,
                },
            };
            Ok(result)
        })();
        match prepared {
            Ok(mut result) => {
                if matches!(
                    result.effect,
                    AuthCleanupEffect::NotApplied | AuthCleanupEffect::Unknown
                ) {
                    result.pending = pending;
                }
                result
            }
            Err(error) => AuthStageRecovery::failed(error, pending),
        }
    }
}

fn native_remove(path: &std::path::Path) -> io::Result<()> {
    #[cfg(test)]
    NATIVE_REMOVE_CALLS.with(|calls| calls.set(calls.get() + 1));
    #[cfg(test)]
    if let Some(after) = REMOVE_FAULT.with(|slot| slot.borrow_mut().take()) {
        if after {
            fs::remove_file(path)?;
        }
        return Err(io::ErrorKind::Other.into());
    }
    fs::remove_file(path)
}
fn phase(name: &'static str) {
    #[cfg(test)]
    HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().as_mut() {
            hook(name);
        }
    });
    #[cfg(not(test))]
    let _ = name;
}
#[cfg(test)]
type Hook = Box<dyn FnMut(&'static str)>;
#[cfg(test)]
thread_local! {
    pub(super) static HOOK: std::cell::RefCell<Option<Hook>> = const { std::cell::RefCell::new(None) };
    pub(super) static REMOVE_FAULT: std::cell::RefCell<Option<bool>> = const { std::cell::RefCell::new(None) };
    pub(super) static NATIVE_REMOVE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
