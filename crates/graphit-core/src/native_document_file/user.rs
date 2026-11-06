use super::*;
use crate::artifact_store::{CapturedPublicationScope, native_source::chain::CapturedChain};
use crate::file_io::SnapshotContext;
use std::sync::Arc;

/// Held readonly directory confined to one captured literal USER namespace.
/// The wrapper shares native directory operations, never PROJECT authority.
pub struct CapturedUserDocumentDirectory {
    directory: CapturedProjectDocumentDirectory,
}
impl CapturedUserDocumentDirectory {
    pub fn capture(
        scope: &CapturedPublicationScope,
        relative_directory: &Path,
        limits: NativeDocumentFileLimits,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let selection = scope.user_selection(relative_directory)?;
        Ok(Self {
            directory: CapturedProjectDocumentDirectory::capture_record(
                scope, &selection, limits, original,
            )?,
        })
    }
    /// Observe only this USER's runtime/memory. READ never creates the namespace.
    pub fn probe_memory(
        scope: &CapturedPublicationScope,
        limits: NativeDocumentFileLimits,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<DirectoryPresence> {
        let selection = scope.user_selection(&Path::new("runtime").join("memory"))?;
        CapturedProjectDocumentDirectory::validate_capture_path(
            scope, &selection, limits, original,
        )?;
        CapturedChain::probe_user_memory(scope, &selection, original)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn from_record(directory: CapturedProjectDocumentDirectory) -> Self {
        Self { directory }
    }
    pub fn revalidate(&self, current: &SnapshotContext) -> io::Result<()> {
        self.directory.revalidate(current)
    }
    pub fn probe_child_directory(
        &self,
        child: &OsStr,
        current: &SnapshotContext,
    ) -> io::Result<DirectoryPresence> {
        self.directory.probe_child_directory(child, current)
    }
    pub fn read(
        &self,
        name: &OsStr,
        current: &SnapshotContext,
    ) -> io::Result<NativeDocumentObservation> {
        self.directory.read(name, current)
    }
    pub fn revalidate_observation(
        &self,
        observation: &NativeDocumentObservation,
        current: &SnapshotContext,
    ) -> io::Result<()> {
        self.directory.revalidate_observation(observation, current)
    }
}
