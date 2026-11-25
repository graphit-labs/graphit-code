//! Exact text reads after the caller admits the FILE origin and verifies its cache/stage.
use super::FileTransport;
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    path::{Component, Path},
};
impl FileTransport<'_, '_> {
    pub(crate) fn read_text_file(
        &self,
        root: &Path,
        relative: &str,
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
        if !std::fs::metadata(&path)?.is_file() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let maximum = original.content_limits().max_input_bytes;
        let file = std::fs::File::open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut bytes = Vec::new();
        file.take(
            u64::try_from(maximum)
                .map_err(|_| io::ErrorKind::FileTooLarge)?
                .checked_add(1)
                .ok_or(io::ErrorKind::FileTooLarge)?,
        )
        .read_to_end(&mut bytes)?;
        if bytes.len() > maximum {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let value = value(relative, &bytes, max_output)?;
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
