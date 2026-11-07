//! PROJECT-only held-root document reads. This type never supplies a writer path.
use super::*;
use crate::artifact_store::{CapturedPublicationScope, native_source::chain::CapturedChain};
use crate::file_io::native_name_bytes;
use graphit_fs::read_directory;
use std::sync::Arc;

/// Observed presence of one exact child directory; contains no retained handle.
/// The entry may change after the last validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryPresence {
    PresentDirectory,
    Absent,
}

/// Read-only capture of an existing PROJECT directory.
/// An empty relative path selects the already held PROJECT owner root.
/// Decoded records, names and receipt data cannot construct this capability.
///
/// ```compile_fail
/// use graphit_core::native_document_file::CapturedProjectDocumentDirectory;
/// let _: CapturedProjectDocumentDirectory = serde_json::from_str("{}").unwrap();
/// ```
pub struct CapturedProjectDocumentDirectory {
    limits: NativeDocumentFileLimits,
    parent_path_bytes: usize,
    chain: Arc<CapturedChain>,
}
impl CapturedProjectDocumentDirectory {
    pub fn capture(
        scope: &CapturedPublicationScope,
        relative_directory: &Path,
        limits: NativeDocumentFileLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<Self> {
        scope.require_project()?;
        Self::capture_record(scope, relative_directory, limits, request)
    }
    pub(crate) fn capture_record(
        scope: &CapturedPublicationScope,
        relative_directory: &Path,
        limits: NativeDocumentFileLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let parent_bytes = Self::validate_capture_path(scope, relative_directory, limits, request)?;

        let chain = CapturedChain::capture_record(scope, relative_directory, request)?;
        Ok(Self {
            limits,
            parent_path_bytes: parent_bytes,
            chain,
        })
    }
    pub(crate) fn validate_capture_path(
        scope: &CapturedPublicationScope,
        relative_directory: &Path,
        limits: NativeDocumentFileLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<usize> {
        scope.validate_record_selection(relative_directory)?;
        Self::validate_from_root(scope.owner_root(), relative_directory, limits, request)
    }
    pub(crate) fn validate_runtime_capture_path(
        scope: &CapturedPublicationScope,
        selection: &Path,
        limits: NativeDocumentFileLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<usize> {
        Self::validate_from_root(scope.runtime_anchor_root(), selection, limits, request)
    }
    fn validate_from_root(
        root: &Path,
        relative_directory: &Path,
        limits: NativeDocumentFileLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<usize> {
        crate::file_io::validate_relative(relative_directory, true)?;
        request.check()?;
        let count = relative_directory
            .components()
            .count()
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if count > limits.ancestors {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut parent_bytes = native_name_bytes(root.as_os_str())?;
        if parent_bytes > limits.path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        for component in relative_directory.components() {
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            request.check()?;
            parent_bytes = parent_bytes
                .checked_add(if cfg!(windows) { 2 } else { 1 })
                .and_then(|n| {
                    native_name_bytes(name)
                        .ok()
                        .and_then(|bytes| n.checked_add(bytes))
                })
                .ok_or(io::ErrorKind::FileTooLarge)?;
            if parent_bytes > limits.path_bytes {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
        }

        Ok(parent_bytes)
    }
    /// Read a directory within this PROJECT's captured global store.
    /// The relative selection cannot select checkout sources or another PROJECT.
    /// An empty selection names the existing PROJECT runtime root itself.
    pub fn capture_runtime(
        scope: &CapturedPublicationScope,
        relative_directory: &Path,
        limits: NativeDocumentFileLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        scope.require_project()?;
        let selection = scope.runtime_selection(relative_directory)?;
        let parent_path_bytes =
            Self::validate_runtime_capture_path(scope, &selection, limits, request)?;
        let chain = CapturedChain::capture_runtime(scope, relative_directory, request)?;
        Ok(Self {
            limits,
            parent_path_bytes,
            chain,
        })
    }
    /// Probe this PROJECT's global store without creating any missing component.
    /// Only a genuine NotFound after revalidation represents absence.
    /// An empty selection probes the PROJECT runtime root without creating it.
    pub fn probe_runtime(
        scope: &CapturedPublicationScope,
        relative_directory: &Path,
        limits: NativeDocumentFileLimits,
        request: &Arc<SnapshotContext>,
    ) -> io::Result<DirectoryPresence> {
        scope.require_project()?;
        let selection = scope.runtime_selection(relative_directory)?;
        Self::validate_runtime_capture_path(scope, &selection, limits, request)?;
        CapturedChain::probe_runtime(scope, relative_directory, request)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn from_captured_chain(
        chain: Arc<CapturedChain>,
        limits: NativeDocumentFileLimits,
        path_bytes: usize,
    ) -> Self {
        Self {
            limits,
            parent_path_bytes: path_bytes,
            chain,
        }
    }
    /// Revalidate the original request and captured directory chain.
    /// Child file contents and directory entries may change without invalidating it.
    ///
    /// ```
    /// use graphit_core::{file_io::SnapshotContext,
    ///     native_document_file::CapturedProjectDocumentDirectory};
    /// fn check(directory: &CapturedProjectDocumentDirectory,
    ///          current: &SnapshotContext) -> std::io::Result<()> {
    ///     directory.revalidate(current)
    /// }
    /// ```
    pub fn revalidate(&self, current: &SnapshotContext) -> io::Result<()> {
        self.check_context(current)?;
        self.chain.revalidate()
    }
    fn check_context(
        &self,
        context: &SnapshotContext,
    ) -> io::Result<&std::sync::Arc<crate::file_io::SnapshotContext>> {
        let request = self.chain.request();
        if !context.same_request(request) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        request.check()?;
        Ok(request)
    }
    fn open(
        &self,
        name: &OsStr,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<File> {
        let parent = self.chain.directory()?.lock()?;
        let file =
            read_directory::open_relative_document_read(&parent, name, 64 * 1024, &mut || {
                request.check()
            })?;
        Ok(file)
    }
    fn validate_child(&self, name: &OsStr) -> io::Result<()> {
        crate::file_io::validate_relative(Path::new(name), false)?;
        name::validate_name(name)?;
        let name_bytes = native_name_bytes(name)?;
        let child_size = self
            .parent_path_bytes
            .checked_add(if cfg!(windows) { 2 } else { 1 })
            .and_then(|n| n.checked_add(name_bytes))
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if child_size > self.limits.path_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }

        Ok(())
    }
    /// Inspect one exact child directory without creating or retaining it.
    /// Only a missing child after successful chain validation returns Absent.
    ///
    /// ```
    /// use graphit_core::{file_io::SnapshotContext,
    ///     native_document_file::{CapturedProjectDocumentDirectory, DirectoryPresence}};
    /// fn corpus(directory: &CapturedProjectDocumentDirectory, request: &SnapshotContext)
    ///     -> std::io::Result<DirectoryPresence> {
    ///     directory.probe_child_directory(std::ffi::OsStr::new("index.lance"), request)
    /// }
    /// ```
    pub fn probe_child_directory(
        &self,
        child: &OsStr,
        current: &SnapshotContext,
    ) -> io::Result<DirectoryPresence> {
        let request = self.check_context(current)?;
        self.validate_child(child)?;
        self.chain.revalidate()?;
        #[cfg(test)]
        test_hooks::run(&test_hooks::BEFORE_OPEN);
        let opened = {
            let directory = self.chain.directory()?.lock()?;
            read_directory::open_relative_readonly(
                &directory,
                child,
                Some(graphit_fs::NodeKind::Directory),
                64 * 1024,
                &mut || request.check(),
            )
            .map(drop)
        };
        self.chain.revalidate()?;
        match opened {
            Ok(()) => Ok(DirectoryPresence::PresentDirectory),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(DirectoryPresence::Absent),
            Err(error) => Err(error),
        }
    }
    pub fn read(
        &self,
        name: &OsStr,
        context: &SnapshotContext,
    ) -> io::Result<NativeDocumentObservation> {
        let request = self.check_context(context)?;
        self.validate_child(name)?;

        self.chain.revalidate()?;
        #[cfg(test)]
        test_hooks::run(&test_hooks::BEFORE_OPEN);
        let opened = match self.open(name, request) {
            Ok(opened) => Some(opened),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        self.chain.revalidate()?;
        let value = if let Some(mut opened) = opened {
            let before = observe(&opened, self.limits.access_bytes, request)?;
            self.chain.revalidate()?;
            let body = read::body(
                &mut opened,
                before.length,
                self.limits,
                self.limits.bytes,
                context,
            )?;
            {
                let after = observe(&opened, self.limits.access_bytes, request)?;
                if after != before {
                    return Err(conflict());
                }
            }
            {
                let fresh = self.open(name, request)?;
                let fresh_observation = observe(&fresh, self.limits.access_bytes, request)?;
                if fresh_observation != before {
                    return Err(conflict());
                }
            }
            self.chain.revalidate()?;
            request.check()?;
            let receipt = NativeDocumentReceipt {
                file_id: before.identity.clone(),
                length: before.length,
                content: body.digest,
                access: before.access.fingerprint(),
            };
            Some(ObservedFile {
                receipt,
                metadata: before,
                bytes: body.bytes,
            })
        } else {
            None
        };
        self.chain.revalidate()?;
        request.check()?;
        let name = name.to_owned();
        Ok(NativeDocumentObservation {
            request: context.request_identity(),
            directory: self.chain.identity()?.clone(),
            name,
            value,
            _project: Some(Arc::clone(&self.chain)),
        })
    }
    pub fn revalidate_observation(
        &self,
        observation: &NativeDocumentObservation,
        context: &SnapshotContext,
    ) -> io::Result<()> {
        self.check_context(context)?;
        if observation
            ._project
            .as_ref()
            .is_none_or(|retained| !Arc::ptr_eq(retained, &self.chain))
            || !observation.request.matches(context)
            || observation.directory != *self.chain.identity()?
        {
            return Err(conflict());
        }
        let fresh = self.read(&observation.name, context)?;
        if fresh.value != observation.value {
            return Err(conflict());
        }
        Ok(())
    }
}

fn observe(
    file: &File,
    limit: usize,
    request: &std::sync::Arc<crate::file_io::SnapshotContext>,
) -> io::Result<FileObservation> {
    request.check()?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    request.check()?;
    let identity = handle_id(file)?;
    identity.validate()?;
    #[cfg(target_os = "linux")]
    let access = {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(conflict());
        }
        Access::Linux(graphit_fs::linux_access_snapshot(file, limit)?)
    };
    #[cfg(all(unix, not(target_os = "linux")))]
    let access = {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(conflict());
        }
        let remaining = limit.checked_sub(12).ok_or(io::ErrorKind::FileTooLarge)?;
        #[cfg(not(target_os = "macos"))]
        let _ = remaining;
        Access::Unix {
            mode: metadata.mode() & 0o7777,
            uid: metadata.uid(),
            gid: metadata.gid(),
            #[cfg(target_os = "macos")]
            mac_access: graphit_fs::macos_access_snapshot(file, remaining)?,
        }
    };
    #[cfg(windows)]
    let access = {
        request.check()?;
        if winapi_util::file::information(file)?.number_of_links() != 1 {
            return Err(conflict());
        }
        Access::WindowsProject(graphit_fs::windows_document_access_snapshot(
            file,
            limit,
            &mut || request.check(),
        )?)
    };
    request.check()?;
    Ok(FileObservation {
        identity,
        access,
        length: metadata.len(),
        modified: metadata.modified()?,
    })
}

#[cfg(test)]
mod tests;
