use std::{io, path::Path};

/// Move one prepared entry without replacing any existing destination.
///
/// Moving a symlink moves its entry, without following its target. Paths use
/// normal native resolution, including linked parents and mount points. Callers
/// provide any required synchronization and content checks.
///
/// Linux and macOS use the exclusive kernel rename operation. Windows uses
/// MoveFileExW without REPLACE_EXISTING or COPY_ALLOWED on 64-bit targets. Other
/// targets/filesystems return Unsupported/native errors without a copy fallback.
/// Conventional long Windows paths receive extended-length syntax through the
/// Windows path SDK; this performs no filesystem capture or parent inspection.
/// Synchronous native I/O is not preemptible and does not establish power-loss
/// durability or protection against hostile concurrent filesystem mutation.
pub fn rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    validate_path(source)?;
    validate_path(destination)?;
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
        // Adapt conventional long paths at this SDK boundary, without opening
        // their parents or changing paths already using verbatim semantics.
        let source = windows_rename_path(source)?;
        let destination = windows_rename_path(destination)?;
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

#[cfg(all(windows, target_pointer_width = "64"))]
fn windows_rename_path(path: &Path) -> io::Result<std::path::PathBuf> {
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
        path::{Component, Prefix},
    };
    use windows_sys::Win32::UI::Shell::PATHCCH_MAX_CCH;

    if matches!(path.components().next(), Some(Component::Prefix(prefix)) if prefix.kind().is_verbatim())
    {
        return Ok(path.to_owned());
    }
    // GetFullPathNameW via std: lexical native resolution, without filesystem I/O.
    let absolute = std::path::absolute(path)?;
    if !matches!(absolute.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::UNC(_, _)))
    {
        return Ok(path.to_owned());
    }
    let mut input: Vec<u16> = absolute.as_os_str().encode_wide().collect();
    if input.len() < 260 {
        return Ok(path.to_owned());
    }
    input.push(0);
    let mut output = vec![0u16; PATHCCH_MAX_CCH as usize];
    let result = extend_windows_rename_path(&input, &mut output);
    if result < 0 {
        return Err(io::Error::other(format!(
            "PathCchCanonicalizeEx failed with HRESULT {result:#010x}"
        )));
    }
    let end = output
        .iter()
        .position(|unit| *unit == 0)
        .ok_or(io::ErrorKind::InvalidData)?;
    Ok(OsString::from_wide(&output[..end]).into())
}

#[cfg(all(windows, target_pointer_width = "64"))]
#[allow(unsafe_code)]
fn extend_windows_rename_path(input: &[u16], output: &mut [u16]) -> i32 {
    use windows_sys::Win32::{
        Foundation::E_INVALIDARG,
        UI::Shell::{PATHCCH_ENSURE_IS_EXTENDED_LENGTH_PATH, PathCchCanonicalizeEx},
    };
    if input.last() != Some(&0) || output.is_empty() {
        return E_INVALIDARG;
    }
    // SAFETY: input is NUL-terminated and the output pointer has the declared
    // writable capacity. The slices cannot move or alias during this SDK call.
    // ENSURE requests extended syntax itself; combining it with ALLOW_LONG_PATHS
    // is invalid. Keep the requested native spelling without normalizing segments.
    unsafe {
        PathCchCanonicalizeEx(
            output.as_mut_ptr(),
            output.len(),
            input.as_ptr(),
            PATHCCH_ENSURE_IS_EXTENDED_LENGTH_PATH,
        )
    }
}

fn validate_path(path: &Path) -> io::Result<()> {
    // The Windows dependency passes a UTF-16 pointer to the SDK. Reject embedded
    // NUL rather than allowing the native API to use a truncated path.
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
