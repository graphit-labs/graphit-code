//! Safe existing rustix primitives for strict relative read-only capture.
use crate::NodeKind;
use rustix::fs::{Mode, OFlags};
use std::os::unix::fs::MetadataExt;
use std::{ffi::OsStr, fs::File, io, path::Path};

pub(super) fn root(path: &Path, before: &mut impl FnMut() -> io::Result<()>) -> io::Result<File> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
    before()?;
    #[cfg(target_os = "linux")]
    let fd = {
        use rustix::fs::{CWD, ResolveFlags, openat2};
        openat2(
            CWD,
            path,
            flags,
            Mode::empty(),
            ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
        )
        .map_err(map_strict_error)?
    };
    #[cfg(target_os = "macos")]
    let fd = rustix::fs::open(path, flags, Mode::empty()).map_err(io::Error::from)?;
    Ok(File::from(fd))
}
pub(super) fn relative(
    parent: &File,
    name: &OsStr,
    kind: Option<NodeKind>,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    let flags = OFlags::RDONLY
        | OFlags::CLOEXEC
        | OFlags::NOFOLLOW
        | OFlags::NONBLOCK
        | if kind == Some(NodeKind::Directory) {
            OFlags::DIRECTORY
        } else {
            OFlags::empty()
        };
    relative_open(parent, name, flags, Mode::empty(), before)
}

pub(super) fn relative_open(
    parent: &File,
    name: &OsStr,
    flags: OFlags,
    mode: Mode,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    before()?;
    #[cfg(target_os = "linux")]
    let fd = {
        use rustix::fs::{ResolveFlags, openat2};
        openat2(
            parent,
            name,
            flags,
            mode,
            ResolveFlags::BENEATH
                | ResolveFlags::NO_SYMLINKS
                | ResolveFlags::NO_MAGICLINKS
                | ResolveFlags::NO_XDEV,
        )
        .map_err(map_strict_error)?
    };
    #[cfg(target_os = "macos")]
    let fd = rustix::fs::openat(parent, name, flags, mode).map_err(io::Error::from)?;
    let file = File::from(fd);
    #[cfg(target_os = "macos")]
    {
        before()?;
        let original = rustix::fs::fstatfs(parent).map_err(io::Error::from)?;
        before()?;
        let current = rustix::fs::fstatfs(&file).map_err(io::Error::from)?;
        // Existing libc extra_traits enables generated fsid_t equality. This is
        // observable filesystem identity, not Linux NO_XDEV equivalence.
        if original.f_fsid != current.f_fsid {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
    before()?;
    let parent_device = parent.metadata()?.dev();
    before()?;
    if parent_device != file.metadata()?.dev() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(file)
}
#[cfg(target_os = "linux")]
fn map_strict_error(error: rustix::io::Errno) -> io::Error {
    if error == rustix::io::Errno::NOSYS || error == rustix::io::Errno::OPNOTSUPP {
        io::ErrorKind::Unsupported.into()
    } else {
        io::Error::from(error)
    }
}
pub(super) fn enumeration_fd(
    parent: &File,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    // Internal ".", with strict flags and independent open-file description:
    // never Dir::read_from, a shared dup cursor, or a removed-dir empty fallback.
    let file = relative(parent, OsStr::new("."), Some(NodeKind::Directory), before)?;
    before()?;
    let original = parent.metadata()?;
    before()?;
    let observed = file.metadata()?;
    if original.dev() != observed.dev() || original.ino() != observed.ino() || observed.nlink() == 0
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        // Darwin can reopen an unlinked directory and report a nonzero link
        // count. Check the native name still refers to the held directory.
        before()?;
        let path = rustix::fs::getpath(&file).map_err(io::Error::from)?;
        before()?;
        let named = std::fs::symlink_metadata(Path::new(OsStr::from_bytes(path.as_bytes())))?;
        if !named.is_dir() || named.dev() != observed.dev() || named.ino() != observed.ino() {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
    Ok(file)
}
