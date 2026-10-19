use std::{
    io,
    path::{Component, Path},
};

/// Move one prepared entry without replacing any existing destination.
///
/// Paths must be absolute, bounded native paths with no NUL, parent traversal,
/// trailing separator or final dot component. Moving a symlink moves its entry;
/// it does not follow its target. Callers provide ownership, parent validation,
/// coordination and journaling. This operation does not provide those contracts.
///
/// Linux and macOS use the exclusive kernel rename operation. Windows uses
/// MoveFileExW without REPLACE_EXISTING or COPY_ALLOWED on 64-bit targets. Other
/// targets/filesystems return Unsupported/native errors without a copy fallback.
/// Synchronous native I/O is not preemptible and does not establish power-loss
/// durability or protection against hostile concurrent filesystem mutation.
pub fn rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    validate_path(source)?;
    validate_path(destination)?;
    if source == destination {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            source,
            rustix::fs::CWD,
            destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )?;
        Ok(())
    }
    #[cfg(all(windows, target_pointer_width = "64"))]
    {
        // The pinned dependency uses lossless OsStr UTF-16 and MoveFileExW flags
        // zero. Its C declaration has the unified Windows 64-bit calling ABI.
        // Our NUL check prevents truncated native names; no heuristic fallback.
        renamore::rename_exclusive(source, destination)
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "macos",
        all(windows, target_pointer_width = "64")
    )))]
    {
        Err(io::ErrorKind::Unsupported.into())
    }
}

fn validate_path(path: &Path) -> io::Result<()> {
    let bytes = path.as_os_str().as_encoded_bytes();
    let final_component = bytes
        .rsplit(|byte| *byte == b'/' || (cfg!(windows) && *byte == b'\\'))
        .next()
        .unwrap_or_default();
    if !path.is_absolute()
        || bytes.len() > super::MAX_PATH_BYTES
        || bytes.contains(&0)
        || matches!(final_component, b"" | b"." | b"..")
        || !matches!(path.components().next_back(), Some(Component::Normal(_)))
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
