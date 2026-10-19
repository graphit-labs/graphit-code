//! Exact text reads after the caller admits the FILE origin and verifies its cache/stage.
use super::FileTransport;
use graphit_core::native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits};
use serde_json::{Value, json};
use std::{
    io,
    path::{Component, Path},
};
impl FileTransport<'_, '_> {
    pub(crate) fn read_text_file(
        &self,
        root: &Path,
        relative: &str,
        limits: NativeDocumentFileLimits,
        max_output: usize,
    ) -> io::Result<Value> {
        let relative = Path::new(relative);
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            || relative.to_string_lossy().contains('\\')
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.check()?;
        let original = self.lease.original().map_err(io::Error::other)?;
        original.check()?;
        let path = root.join(relative);
        let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let directory = CapturedDocumentDirectory::capture(parent, limits, original.snapshot())?;
        if directory.canonical_path() != parent || !directory.canonical_path().starts_with(root) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let observation = directory.read(
            path.file_name().ok_or(io::ErrorKind::InvalidInput)?,
            original.snapshot(),
        )?;
        let bytes = observation.bytes().ok_or(io::ErrorKind::NotFound)?;
        let value = value(relative, bytes, max_output)?;
        directory.revalidate(original.snapshot())?;
        original.check()?;
        self.check()?;
        Ok(value)
    }
}

/// Shared FILE text/output codec; callers retain their own captured IO authority.
pub(crate) fn value(relative: &Path, bytes: &[u8], max_output: usize) -> io::Result<Value> {
    let text = std::str::from_utf8(bytes)
        .map_err(|cause| io::Error::new(io::ErrorKind::InvalidData, cause))?;
    let value = json!({"path":relative,"content":text});
    if serde_json::to_vec(&value).map_err(io::Error::other)?.len() > max_output {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(value)
}
