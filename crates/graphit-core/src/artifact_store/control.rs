//! Current private runtime boundary, not evidence of catalog or stage origin.
use super::{
    ArtifactError, ArtifactResult, ArtifactStage, CapturedPublicationScope, ScopeContext, at,
};
use crate::{
    local_files::open_or_create_private_directory,
    native_document_file::open_access_directory,
    native_file_identity::{PhysicalId, handle_id, physical_id},
};
use std::{fs, fs::File, io};

pub(super) struct ControlDirectory {
    file: File,
    identity: PhysicalId,
}

/// Keep the operation's primary failure while reporting a failed final boundary check.
pub(super) fn checked<T>(
    result: ArtifactResult<T>,
    validation: io::Result<()>,
    stage: ArtifactStage,
) -> ArtifactResult<T> {
    match (result, validation) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(_), Err(validation)) => Err(at(stage, validation)),
        (Err(primary), Err(validation)) => Err(ArtifactError {
            secondary: primary.secondary.or(Some(validation.kind())),
            ..primary
        }),
    }
}

impl ControlDirectory {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(super) fn held_native_file(
        &self,
        chain: &super::native_source::NativeControlChain,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<&File> {
        chain.revalidate()?;
        graphit_fs::write_relative::inspect_private_directory_current(&self.file, &mut || {
            request.check()
        })?;
        request.check()?;
        if handle_id(&self.file)? != self.identity {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let parent = chain.file()?;
        request.check()?;
        if handle_id(&parent)? != self.identity {
            return Err(io::ErrorKind::InvalidData.into());
        }
        request.check()?;
        Ok(&self.file)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(super) fn from_native_chain(
        chain: &super::native_source::NativeControlChain,
        scope: &CapturedPublicationScope,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        chain.revalidate()?;
        scope.revalidate_native(request)?;
        let held = chain.file()?;
        graphit_fs::write_relative::inspect_private_directory_current(&held, &mut || {
            request.check()
        })?;
        request.check()?;
        let file = held.try_clone()?;
        request.check()?;
        let identity = handle_id(&file)?;
        identity.validate()?;
        drop(held);
        chain.revalidate()?;
        scope.revalidate_native(request)?;
        Ok(Self { file, identity })
    }

    pub(super) fn open(scope: &CapturedPublicationScope) -> io::Result<Self> {
        scope.check(None)?;
        let file = open_access_directory(scope.runtime_root())?;
        Self::capture(scope, file, None)
    }

    pub(super) fn create_or_open(
        scope: &CapturedPublicationScope,
        context: &ScopeContext,
    ) -> io::Result<Self> {
        scope.check(Some(context))?;
        let file = open_or_create_private_directory(scope.runtime_root())?;
        Self::capture(scope, file, Some(context))
    }

    fn capture(
        scope: &CapturedPublicationScope,
        file: File,
        context: Option<&ScopeContext>,
    ) -> io::Result<Self> {
        graphit_fs::verify_private_directory(&file)?;
        let identity = handle_id(&file)?;
        identity.validate()?;
        let captured = Self { file, identity };
        captured.revalidate(scope, context)?;
        Ok(captured)
    }

    pub(super) fn revalidate(
        &self,
        scope: &CapturedPublicationScope,
        context: Option<&ScopeContext>,
    ) -> io::Result<()> {
        scope.check(context)?;
        graphit_fs::verify_private_directory(&self.file)?;
        let path = scope.runtime_root();
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || handle_id(&self.file)? != self.identity
            || physical_id(path, &metadata)? != self.identity
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        scope.check(context)
    }
}

#[cfg(test)]
pub(super) mod test_hooks {
    use std::cell::RefCell;
    type Hook = RefCell<Option<Box<dyn FnOnce()>>>;
    thread_local! {
        pub(in crate::artifact_store) static BEFORE_LOCK: Hook = const { RefCell::new(None) };
        pub(in crate::artifact_store) static AFTER_READ: Hook = const { RefCell::new(None) };
        pub(in crate::artifact_store) static AFTER_SAVE: Hook = const { RefCell::new(None) };
    }
    pub(in crate::artifact_store) fn before_lock() {
        if let Some(hook) = BEFORE_LOCK.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }
    pub(in crate::artifact_store) fn after_read() {
        if let Some(hook) = AFTER_READ.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }
    pub(in crate::artifact_store) fn after_save() {
        if let Some(hook) = AFTER_SAVE.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }
}
