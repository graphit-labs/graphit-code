//! Checked publication of the fixed installation authentication document.
//! The application authorizes profile management before invoking this boundary.
use super::recovery::AuthCleanupEffect;
use super::*;
use crate::{
    local_files::regular_metadata,
    native_document_file::FileObservation,
    native_document_lock::NativeDocumentLock,
    private_document::{CreatedScratch, ExpectedEffect, Observed},
};
use std::{
    io::{Seek, Write},
    path::Path,
};

pub(super) const STAGE: &str = ".auth.json.stage";
pub(super) const LOCK: &str = ".auth.json.lock";
/// Runtime agent views must exclude these names in the installation owner root.
/// Privacy of a file does not authorize exposing it to an agent filesystem view.
pub const AUTH_PRIVATE_NAMES: [&str; 3] = ["auth.json", STAGE, LOCK];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthPublicationEffect {
    Applied,
    Unchanged,
    NotApplied,
    Unknown,
}

/// An original process-local creation proof; neither data nor a restored grant.
/// ```compile_fail
/// use graphit_core::auth_document::publication::PendingCleanup;
/// let _: PendingCleanup = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::auth_document::publication::PendingCleanup;
/// fn copy(value: &PendingCleanup) { let _ = Clone::clone(value); }
/// ```
/// ```compile_fail
/// use graphit_core::auth_document::publication::PendingCleanup;
/// fn expose(value: &PendingCleanup) { println!("{value:?}"); }
/// ```
pub struct PendingCleanup {
    pub(super) scratch: CreatedScratch,
    pub(super) store_identity: Arc<()>,
}

#[must_use]
pub struct AuthDocumentPublication {
    effect: AuthPublicationEffect,
    failure: Option<AuthDocumentError>,
    cleanup_failure: Option<AuthDocumentError>,
    cleanup_effect: AuthCleanupEffect,
    pending: Option<PendingCleanup>,
}
impl AuthDocumentPublication {
    pub fn effect(&self) -> AuthPublicationEffect {
        self.effect
    }
    pub fn failure(&self) -> Option<AuthDocumentError> {
        self.failure
    }
    pub fn cleanup_failure(&self) -> Option<AuthDocumentError> {
        self.cleanup_failure
    }
    pub fn cleanup_effect(&self) -> AuthCleanupEffect {
        self.cleanup_effect
    }
    pub fn into_pending_cleanup(self) -> Option<PendingCleanup> {
        self.pending
    }
    fn failed(error: impl Into<AuthDocumentError>) -> Self {
        Self {
            effect: AuthPublicationEffect::NotApplied,
            failure: Some(error.into()),
            cleanup_failure: None,
            cleanup_effect: AuthCleanupEffect::NotApplied,
            pending: None,
        }
    }
}

impl AuthDocumentStore {
    /// Consumes the old FD observation. The body is borrowed data, never authority.
    /// A stage left by a previous process must first be discarded by `recover_stage`.
    pub fn publish(
        &self,
        expected: AuthDocumentSnapshot,
        body: &[u8],
        context: &SnapshotContext,
    ) -> AuthDocumentPublication {
        self.publish_inner(expected, body, context)
    }

    fn publish_inner(
        &self,
        mut expected: AuthDocumentSnapshot,
        body: &[u8],
        context: &SnapshotContext,
    ) -> AuthDocumentPublication {
        if let Err(error) = self.check(context) {
            return AuthDocumentPublication::failed(error);
        }
        if body.len() > self.limits.bytes {
            return AuthDocumentPublication::failed(AuthDocumentError::Limit);
        }
        if !Arc::ptr_eq(&self.identity, &expected.store_identity)
            || !expected.request_identity.matches(context)
        {
            return AuthDocumentPublication::failed(AuthDocumentError::Conflict);
        }
        let destination = match self.directory.path(OsStr::new("auth.json")) {
            Ok(path) => path,
            Err(error) => return AuthDocumentPublication::failed(error),
        };
        let stage = match self.directory.path(OsStr::new(STAGE)) {
            Ok(path) => path,
            Err(error) => return AuthDocumentPublication::failed(error),
        };
        let lock = match NativeDocumentLock::acquire(&self.directory, OsStr::new(LOCK), context) {
            Ok(lock) => lock,
            Err(error) => return AuthDocumentPublication::failed(error),
        };
        if let Err(error) = self
            .revalidate(&mut expected, context)
            .and_then(|()| lock.revalidate().map_err(AuthDocumentError::from))
        {
            return AuthDocumentPublication::failed(error);
        }
        phase("validated");
        if expected.bytes() == Some(body) {
            return match self
                .check(context)
                .and_then(|()| lock.revalidate().map_err(AuthDocumentError::from))
            {
                Ok(()) => AuthDocumentPublication {
                    effect: AuthPublicationEffect::Unchanged,
                    failure: None,
                    cleanup_failure: None,
                    cleanup_effect: AuthCleanupEffect::NotApplied,
                    pending: None,
                },
                Err(error) => AuthDocumentPublication::failed(error),
            };
        }
        let check = || {
            self.check_io(context)?;
            lock.revalidate()
        };
        let access = self.access(context, &check);
        let prepared = (|| {
            access.check()?;
            if regular_metadata(&stage)?.is_some() {
                return Err(private_document::conflict());
            }
            context.check()?;
            CreatedScratch::create(stage, &check, create_stage)
        })();
        let (scratch, mut file) = match prepared {
            Ok(value) => value,
            Err(error) => return AuthDocumentPublication::failed(error),
        };
        phase("created");
        let written = (|| {
            for chunk in body.chunks(64 * 1024) {
                let mut remaining = chunk;
                while !remaining.is_empty() {
                    access.check()?;
                    scratch.verify(&file, &check)?;
                    let amount = file.write(remaining)?;
                    if amount == 0 {
                        return Err(io::ErrorKind::WriteZero.into());
                    }
                    remaining = &remaining[amount..];
                    phase("written");
                }
            }
            access.check()?;
            scratch.verify(&file, &check)?;
            file.sync_all()?;
            phase("synced");
            scratch.verify(&file, &check)?;
            file.rewind()?;
            private_document::compare_bytes(&mut file, body, &access)?;
            if private_document::observe(scratch.path(), &file, &access)?.length
                != body.len() as u64
            {
                return Err(private_document::conflict());
            }
            Ok(())
        })();
        // Windows' private birth handles exclude delete sharing. Preserve origin,
        // then close the stage and old handles before the final closed comparison.
        drop(file);
        let old = expected.file.take().map(|captured| {
            drop(captured.file);
            captured.value
        });
        if let Err(error) = written {
            return self.finish(
                scratch,
                AuthPublicationEffect::NotApplied,
                Some(error.into()),
                &check,
            );
        }
        let identity = match private_document::replacement_identity(
            scratch.identity(),
            old.as_ref().map(|value| &value.metadata.identity),
        ) {
            Ok(identity) => identity,
            Err(error) => {
                return self.finish(
                    scratch,
                    AuthPublicationEffect::NotApplied,
                    Some(error.into()),
                    &check,
                );
            }
        };
        let ready = (|| {
            phase("before_replace");
            #[cfg(feature = "auth-test-support")]
            super::test_support::trigger_before_replace(self, context);
            access.check()?;
            let stage_metadata = verify_stage(&scratch, body, &access)?;
            private_document::verify_closed(&destination, old.as_ref(), &access)?;
            phase("after_old_compare");
            verify_stage_metadata(&scratch, &stage_metadata, &access)?;
            access.check()
        })();
        if let Err(error) = ready {
            return self.finish(
                scratch,
                AuthPublicationEffect::NotApplied,
                Some(error.into()),
                &check,
            );
        }
        let native = native_replace(scratch.path(), &destination, old.is_some());
        phase("after_replace");
        let observation = private_document::observe_effect(
            &destination,
            ExpectedEffect {
                identity: &identity,
                bytes: body,
                old: old.as_ref(),
                native_failed: native.is_err(),
                retain: false,
            },
            &access,
        );
        let (effect, failure) = classify(native, observation, &identity);
        self.finish(scratch, effect, failure, &check)
    }

    fn finish(
        &self,
        scratch: CreatedScratch,
        effect: AuthPublicationEffect,
        failure: Option<AuthDocumentError>,
        check: &dyn Fn() -> io::Result<()>,
    ) -> AuthDocumentPublication {
        phase("before_cleanup");
        let (cleanup_effect, cleanup_failure) = match check() {
            Err(error) => (AuthCleanupEffect::NotApplied, Some(error.into())),
            Ok(()) => match regular_metadata(scratch.path()) {
                Ok(None) => (AuthCleanupEffect::Absent, None),
                Err(error) => (AuthCleanupEffect::NotApplied, Some(error.into())),
                Ok(Some(_)) => match scratch.cleanup(check) {
                    Ok(()) => (AuthCleanupEffect::Applied, None),
                    Err(error) => {
                        let observed = (|| {
                            check()?;
                            regular_metadata(scratch.path())
                        })();
                        let effect = match observed {
                            Ok(None) => AuthCleanupEffect::Applied,
                            Ok(Some(metadata))
                                if crate::native_file_identity::physical_id(
                                    scratch.path(),
                                    &metadata,
                                )
                                .is_ok_and(|id| id == *scratch.identity()) =>
                            {
                                AuthCleanupEffect::NotApplied
                            }
                            _ => AuthCleanupEffect::Unknown,
                        };
                        (effect, Some(error.into()))
                    }
                },
            },
        };
        let pending = cleanup_failure.map(|_| PendingCleanup {
            scratch,
            store_identity: Arc::clone(&self.identity),
        });
        AuthDocumentPublication {
            effect,
            failure,
            cleanup_failure,
            cleanup_effect,
            pending,
        }
    }
}

fn verify_stage(
    scratch: &CreatedScratch,
    body: &[u8],
    access: &Access<'_>,
) -> io::Result<FileObservation> {
    let Some((mut file, observed)) = private_document::capture(scratch.path(), access)? else {
        return Err(private_document::conflict());
    };
    if observed.identity != *scratch.identity() {
        return Err(private_document::conflict());
    }
    scratch.verify(&file, access.check)?;
    private_document::compare_bytes(&mut file, body, access)?;
    if private_document::observe(scratch.path(), &file, access)? != observed {
        return Err(private_document::conflict());
    }
    Ok(observed)
}

// The old document comparison can take multiple reads. Recheck actual stage
// origin, namespace and privacy afterward, closing the Windows handle before
// replacement. Metadata observation adds no content pass or IO reservation.
fn verify_stage_metadata(
    scratch: &CreatedScratch,
    expected: &FileObservation,
    access: &Access<'_>,
) -> io::Result<()> {
    let Some((file, observed)) = private_document::capture(scratch.path(), access)? else {
        return Err(private_document::conflict());
    };
    if observed != *expected || observed.identity != *scratch.identity() {
        return Err(private_document::conflict());
    }
    scratch.verify(&file, access.check)?;
    if private_document::observe(scratch.path(), &file, access)? != observed {
        return Err(private_document::conflict());
    }
    Ok(())
}

fn classify(
    native: io::Result<()>,
    observation: io::Result<Option<Observed>>,
    identity: &crate::native_file_identity::PhysicalId,
) -> (AuthPublicationEffect, Option<AuthDocumentError>) {
    let native_failure = native.err().map(AuthDocumentError::from);
    match observation {
        Ok(Some(value)) if value.metadata.identity == *identity => {
            (AuthPublicationEffect::Applied, native_failure)
        }
        Ok(_) if native_failure.is_some() => (AuthPublicationEffect::NotApplied, native_failure),
        Ok(_) => (
            AuthPublicationEffect::Applied,
            Some(AuthDocumentError::Conflict),
        ),
        Err(error) if native_failure.is_none() => {
            (AuthPublicationEffect::Applied, Some(error.into()))
        }
        Err(error) => (AuthPublicationEffect::Unknown, Some(error.into())),
    }
}

fn native_replace(stage: &Path, destination: &Path, exists: bool) -> io::Result<()> {
    #[cfg(test)]
    NATIVE_REPLACE_CALLS.with(|calls| calls.set(calls.get() + 1));
    #[cfg(test)]
    if let Some(fault) = NATIVE_FAULT.with(|slot| slot.borrow_mut().take()) {
        return match fault {
            NativeFault::Before => Err(io::ErrorKind::Other.into()),
            NativeFault::After => {
                private_document::replace(stage, destination, exists)?;
                Err(io::ErrorKind::Other.into())
            }
        };
    }
    private_document::replace(stage, destination, exists)
}
fn create_stage(path: &Path) -> io::Result<File> {
    let file = private_document::create_file(path)?;
    #[cfg(test)]
    if BIRTH_FAULT.with(|slot| slot.replace(false)) {
        drop(file);
        return Err(io::ErrorKind::Other.into());
    }
    Ok(file)
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
enum NativeFault {
    Before,
    After,
}
#[cfg(test)]
type Hook = Box<dyn FnMut(&'static str)>;
#[cfg(test)]
thread_local! {
    static HOOK: std::cell::RefCell<Option<Hook>> = const { std::cell::RefCell::new(None) };
    static NATIVE_FAULT: std::cell::RefCell<Option<NativeFault>> = const { std::cell::RefCell::new(None) };
    static BIRTH_FAULT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static NATIVE_REPLACE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
mod tests;
