//! Windows private operations reuse the exact SDK descriptor and handle verifier.
use std::{ffi::OsStr, fs::File, io};

fn checked(
    file: File,
    directory: bool,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    before()?;
    if directory {
        crate::verify_private_directory(&file)?;
    } else {
        crate::verify_private_file(&file)?;
    }
    before()?;
    Ok(file)
}

pub fn create_private_directory_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    let file = crate::windows_relative_sdk::open_private_relative(
        parent,
        child,
        true,
        true,
        component_capacity,
        before,
    )?;
    checked(file, true, before)
}

pub fn open_private_directory_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    let file = crate::windows_relative_sdk::open_private_relative(
        parent,
        child,
        true,
        false,
        component_capacity,
        before,
    )?;
    checked(file, true, before)
}

pub fn open_private_lock_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    let file = match crate::windows_relative_sdk::open_private_relative(
        parent,
        child,
        false,
        true,
        component_capacity,
        before,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            crate::windows_relative_sdk::open_private_relative(
                parent,
                child,
                false,
                false,
                component_capacity,
                before,
            )?
        }
        Err(error) => return Err(error),
    };
    checked(file, false, before)
}

pub fn open_existing_private_lock_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    let file = crate::windows_relative_sdk::open_private_relative(
        parent,
        child,
        false,
        false,
        component_capacity,
        before,
    )?;
    checked(file, false, before)
}

pub fn open_private_file_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    let file = crate::read_directory::open_relative_document_read(
        parent,
        child,
        component_capacity,
        before,
    )?;
    checked(file, false, before)
}

/// Existing private database observation, without create/truncate or write access.
/// The handle shares driver writes and excludes deletion; release sidecar observers
/// before driver cleanup. The held-parent and same-handle checks remain mandatory.
pub fn open_private_database_file_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    let file = crate::windows_relative_sdk::open_private_database_relative(
        parent,
        child,
        component_capacity,
        before,
    )?;
    before()?;
    crate::windows_private_file::verify_database_file(&file)?;
    before()?;
    Ok(file)
}

pub fn inspect_private_directory_current(
    file: &File,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    before()?;
    crate::verify_private_directory(file)?;
    before()
}
