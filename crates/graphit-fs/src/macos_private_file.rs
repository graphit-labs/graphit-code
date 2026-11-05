//! Owned SDK ACL/filesec values, applied at creation before any content writes.
use crate::macos_private_sdk as sdk;
use std::{
    ffi::{CStr, CString},
    fs::{File, OpenOptions},
    io,
    os::{
        fd::{FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::Path,
    ptr,
};

struct Acl {
    pointer: sdk::acl_t,
}
impl Drop for Acl {
    fn drop(&mut self) {
        free_acl(self.pointer);
    }
}
struct FileSecurity {
    pointer: sdk::filesec_t,
}
impl Drop for FileSecurity {
    fn drop(&mut self) {
        free_filesec(self.pointer);
    }
}

pub(super) fn new_file(path: &Path) -> io::Result<File> {
    let path = native_path(path)?;
    let security = private_security(0o600)?;
    let file = create_file(&path, &security)?;
    crate::verify_private_file(&file)?;
    Ok(file)
}
pub(super) fn new_directory(path: &Path) -> io::Result<File> {
    let native = native_path(path)?;
    let security = private_security(0o700)?;
    create_directory(&native, &security)?;
    // mkdirx returns status, not a birth handle. Namespace provenance and any
    // residue after failure belong to the captured-parent caller.
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    crate::verify_private_directory(&directory)?;
    Ok(directory)
}
fn native_path(path: &Path) -> io::Result<CString> {
    let bytes = path.as_os_str().as_bytes();
    if bytes.len() > 64 * 1024 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    CString::new(bytes).map_err(|_| io::ErrorKind::InvalidInput.into())
}
fn private_security(mode: libc::mode_t) -> io::Result<FileSecurity> {
    let acl = allocate_acl()?;
    configure_acl(&acl)?;
    let security = allocate_filesec()?;
    configure_filesec(&security, &acl, mode)?;
    // FILESEC_ACL copies the native ACL, so security owns its separate copy.
    Ok(security)
}
fn status(result: libc::c_int) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[allow(unsafe_code)]
fn allocate_acl() -> io::Result<Acl> {
    // SAFETY: SDK allocation of an empty ACL; success is owned until acl_free.
    let pointer = unsafe { sdk::acl_init(0) };
    if pointer.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(Acl { pointer })
}
#[allow(unsafe_code)]
fn configure_acl(acl: &Acl) -> io::Result<()> {
    let mut flags = ptr::null_mut();
    // SAFETY: live SDK ACL and initialized out-pointer; flagset borrows its ACL.
    let result = unsafe { sdk::acl_get_flagset_np(acl.pointer.cast(), &mut flags) };
    status(result)?;
    if flags.is_null() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    // SAFETY: borrowed flagset remains live; SDK flag suppresses birth inheritance.
    let result = unsafe { sdk::acl_add_flag_np(flags, sdk::acl_flag_t_ACL_FLAG_NO_INHERIT) };
    status(result)
}
#[allow(unsafe_code)]
fn free_acl(pointer: sdk::acl_t) {
    // SAFETY: exactly one release of the successful SDK allocation owned by Acl.
    let _ = unsafe { sdk::acl_free(pointer.cast()) };
}
#[allow(unsafe_code)]
fn allocate_filesec() -> io::Result<FileSecurity> {
    // SAFETY: SDK allocation; successful value is owned until filesec_free.
    let pointer = unsafe { sdk::filesec_init() };
    if pointer.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(FileSecurity { pointer })
}
#[allow(unsafe_code)]
fn configure_filesec(security: &FileSecurity, acl: &Acl, mode: libc::mode_t) -> io::Result<()> {
    let owner: libc::uid_t = rustix::process::geteuid().as_raw();
    // SAFETY: filesec live; property copies the correctly typed UID value.
    let result = unsafe {
        sdk::filesec_set_property(
            security.pointer,
            sdk::filesec_property_t_FILESEC_OWNER,
            (&owner as *const libc::uid_t).cast(),
        )
    };
    status(result)?;
    // SAFETY: property copies a live mode_t value, not an integer-sized substitute.
    let result = unsafe {
        sdk::filesec_set_property(
            security.pointer,
            sdk::filesec_property_t_FILESEC_MODE,
            (&mode as *const libc::mode_t).cast(),
        )
    };
    status(result)?;
    // SAFETY: ACL property requires pointer-to-acl_t and copies its native bytes.
    let result = unsafe {
        sdk::filesec_set_property(
            security.pointer,
            sdk::filesec_property_t_FILESEC_ACL,
            (&acl.pointer as *const sdk::acl_t).cast(),
        )
    };
    status(result)
}
#[allow(unsafe_code)]
fn free_filesec(pointer: sdk::filesec_t) {
    // SAFETY: exactly one release of the SDK allocation owned by FileSecurity.
    unsafe { sdk::filesec_free(pointer) }
}
#[allow(unsafe_code)]
fn create_file(path: &CStr, security: &FileSecurity) -> io::Result<File> {
    // SAFETY: terminated lossless path and owned descriptor live through the call.
    let descriptor = unsafe {
        sdk::openx_np(
            path.as_ptr(),
            libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_RDWR,
            security.pointer,
        )
    };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful descriptor is owned, adopted once before any fallible IO.
    let descriptor = unsafe { OwnedFd::from_raw_fd(descriptor) };
    Ok(File::from(descriptor))
}
#[allow(unsafe_code)]
fn create_directory(path: &CStr, security: &FileSecurity) -> io::Result<()> {
    // SAFETY: terminated path and filesec live; no content is written by mkdirx.
    let result = unsafe { sdk::mkdirx_np(path.as_ptr(), security.pointer) };
    status(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsStr, fs, process::Command};

    #[test]
    fn inherited_parent_acl_does_not_enter_private_children() {
        let root = tempfile::tempdir().unwrap();
        let status = Command::new("chmod")
            .arg("+a")
            .arg("everyone allow read,write,file_inherit,directory_inherit")
            .arg(root.path())
            .status()
            .unwrap();
        assert!(status.success());
        let directory = new_directory(&root.path().join("private")).unwrap();
        let file = new_file(&root.path().join("stage")).unwrap();
        crate::verify_private_directory(&directory).unwrap();
        crate::verify_private_file(&file).unwrap();
        assert_eq!(file.metadata().unwrap().len(), 0);
    }
    #[test]
    fn extra_acl_grant_is_rejected_even_with_same_private_mode() {
        use std::os::unix::fs::MetadataExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("stage");
        let file = new_file(&path).unwrap();
        let before = file.metadata().unwrap();
        assert!(
            Command::new("chmod")
                .args(["+a", "everyone allow read"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let after = file.metadata().unwrap();
        assert_eq!(before.ino(), after.ino());
        assert_eq!(before.mode(), after.mode());
        assert_eq!(before.uid(), after.uid());
        assert_eq!(before.nlink(), after.nlink());
        assert_eq!(
            crate::verify_private_file(&file).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
    #[test]
    fn paths_are_lossless_bounded_and_exclusive() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("stage-é 🚀");
        let opaque = Path::new(OsStr::from_bytes(b"stage-\xff"));
        assert_eq!(native_path(opaque).unwrap().as_bytes(), b"stage-\xff");
        let file = new_file(&path).unwrap();
        assert_eq!(file.metadata().unwrap().len(), 0);
        assert_eq!(
            new_file(&path).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            new_file(Path::new("invalid\0path")).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            native_path(Path::new(&"x".repeat(65537)))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(fs::read(path).unwrap(), b"");
    }
    #[test]
    fn restrictive_umask_fails_before_content_and_exposes_empty_residue() {
        let root = tempfile::tempdir().unwrap();
        let status = Command::new("sh")
            .args([
                "-c",
                "umask 0777; exec \"$1\" --exact macos_private_file::tests::restricted_birth_child",
            ])
            .arg("graphit-mac-private-birth")
            .arg(std::env::current_exe().unwrap())
            .env("GRAPHIT_MAC_PRIVATE_BIRTH_FIXTURE", root.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::metadata(root.path().join("stage")).unwrap().len(), 0);
        assert!(fs::metadata(root.path().join("control")).unwrap().is_dir());
    }
    // Only the restrictive-umask parent explicitly executes this helper process.
    #[test]
    fn restricted_birth_child() {
        let Some(root) = std::env::var_os("GRAPHIT_MAC_PRIVATE_BIRTH_FIXTURE") else {
            return;
        };
        let root = Path::new(&root);
        assert_eq!(
            new_file(&root.join("stage")).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            new_directory(&root.join("control")).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
