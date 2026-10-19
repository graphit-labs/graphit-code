//! Darwin held-parent operations use public rustix APIs and the shared verifier.
//! Birth under a parent with an ACE inherited by this child is denied before IO.
use rustix::fs::{Mode, OFlags};
use std::{ffi::OsStr, fs::File, io};

fn open(
    parent: &File,
    child: &OsStr,
    flags: OFlags,
    mode: Mode,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    crate::unix_relative::relative_open(
        parent,
        child,
        flags | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        mode,
        before,
    )
}
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
fn admit_birth(
    parent: &File,
    directory: bool,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    before()?;
    crate::macos_access_snapshot(parent, 8192)?.verify_no_inherited_aces(directory)?;
    before()
}
pub fn create_private_directory_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    super::name(child, component_capacity, before)?;
    admit_birth(parent, true, before)?;
    rustix::fs::mkdirat(parent, child, Mode::RWXU)?;
    let file = open(
        parent,
        child,
        OFlags::RDONLY | OFlags::DIRECTORY,
        Mode::empty(),
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
    super::name(child, component_capacity, before)?;
    let file = open(
        parent,
        child,
        OFlags::RDONLY | OFlags::DIRECTORY,
        Mode::empty(),
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
    super::name(child, component_capacity, before)?;
    admit_birth(parent, false, before)?;
    let file = match open(
        parent,
        child,
        OFlags::RDWR | OFlags::CREATE | OFlags::EXCL,
        Mode::RUSR | Mode::WUSR,
        before,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            open(parent, child, OFlags::RDWR, Mode::empty(), before)?
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
    super::name(child, component_capacity, before)?;
    let file = open(parent, child, OFlags::RDWR, Mode::empty(), before)?;
    checked(file, false, before)
}
pub fn open_private_file_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    super::name(child, component_capacity, before)?;
    let file = open(parent, child, OFlags::RDONLY, Mode::empty(), before)?;
    checked(file, false, before)
}
pub fn inspect_private_directory_current(
    file: &File,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    before()?;
    crate::verify_private_directory(file)?;
    before()
}
