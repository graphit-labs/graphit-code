//! SDK calls for descriptor-bound macOS access observation and reapplication.
use crate::macos_access_buffer::{self, MacAccessSnapshot};
use std::{fs::File, io, os::fd::AsRawFd};

/// Observe actual owner/group GUIDs and the opaque extended-security payload.
/// `limit` bounds logical GUID/payload bytes; the POSIX metadata quota belongs to
/// the caller. The fixed response overhead is bounded separately. This does not
/// limit allocation inside the kernel, qualify private birth or grant ownership.
pub fn macos_access_snapshot(file: &File, limit: usize) -> io::Result<MacAccessSnapshot> {
    extended_security_support(file)?;
    macos_access_buffer::snapshot(limit, |buffer| read_attributes(file, buffer))
}

/// Reapply observed access to a regular, singly linked file through its held FD.
/// `max_buffer_bytes` bounds the full setter packet before allocation. Readback
/// uses the exact logical GUID/ACL size of `access`, independently of that cap.
/// A setter/readback error may leave metadata changed. This grants no stage,
/// publication or cleanup authority; the caller must establish those separately.
pub fn macos_apply_access(
    file: &File,
    access: &MacAccessSnapshot,
    mode: u32,
    uid: u32,
    gid: u32,
    max_buffer_bytes: usize,
) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let mut prepared = access.prepare_apply(mode, uid, gid, max_buffer_bytes)?;
    let before = file.metadata()?;
    if !before.is_file() || before.nlink() != 1 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    extended_security_support(file)?;
    write_attributes(file, &mut prepared.bytes)?;
    let after = file.metadata()?;
    let observed = macos_access_snapshot(file, prepared.readback_limit)?;
    let final_metadata = file.metadata()?;
    if [&after, &final_metadata].into_iter().any(|metadata| {
        !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.mode() & 0o7777 != mode
            || metadata.uid() != uid
            || metadata.gid() != gid
    }) || observed != *access
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(())
}

#[allow(unsafe_code)]
fn write_attributes(file: &File, buffer: &mut [u8]) -> io::Result<()> {
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_OWNERID
            | libc::ATTR_CMN_GRPID
            | libc::ATTR_CMN_ACCESSMASK
            | libc::ATTR_CMN_EXTENDED_SECURITY
            | libc::ATTR_CMN_UUID
            | libc::ATTR_CMN_GRPUUID,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    // SAFETY: live File, initialized SDK attribute list and admitted initialized
    // packet remain exclusively borrowed throughout this synchronous copyin.
    let result = unsafe {
        libc::fsetattrlist(
            file.as_raw_fd(),
            (&mut attributes as *mut libc::attrlist).cast(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            0,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[allow(unsafe_code)]
fn extended_security_support(file: &File) -> io::Result<()> {
    errno::set_errno(errno::Errno(0));
    // SAFETY: the File keeps this valid descriptor alive. The SDK selector has
    // no pointer arguments; errno is captured immediately on this same thread.
    let value = unsafe { libc::fpathconf(file.as_raw_fd(), libc::_PC_EXTENDED_SECURITY_NP) };
    let error = errno::errno().0;
    macos_access_buffer::qualify_support(value, error)
}

#[allow(unsafe_code)]
fn read_attributes(file: &File, buffer: &mut [u8]) -> io::Result<()> {
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_EXTENDED_SECURITY
            | libc::ATTR_CMN_UUID
            | libc::ATTR_CMN_GRPUUID
            | libc::ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    // SAFETY: File, initialized SDK attrlist and exclusively borrowed initialized
    // byte buffer all outlive this synchronous call. Sizes match their storage.
    // The kernel copies packed bytes, not Rust references; decoding uses slices.
    let result = unsafe {
        libc::fgetattrlist(
            file.as_raw_fd(),
            (&mut attributes as *mut libc::attrlist).cast(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            libc::FSOPT_REPORT_FULLSIZE | libc::FSOPT_PACK_INVAL_ATTRS,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Observe access by held-directory-relative name without a leaf descriptor.
pub(crate) fn macos_access_snapshot_relative(
    parent: &File,
    child: &std::ffi::CStr,
    limit: usize,
) -> io::Result<MacAccessSnapshot> {
    extended_security_support(parent)?;
    macos_access_buffer::snapshot(limit, |buffer| {
        read_attributes_relative(parent, child, buffer)
    })
}

#[allow(unsafe_code)]
fn read_attributes_relative(
    parent: &File,
    child: &std::ffi::CStr,
    buffer: &mut [u8],
) -> io::Result<()> {
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_EXTENDED_SECURITY
            | libc::ATTR_CMN_UUID
            | libc::ATTR_CMN_GRPUUID
            | libc::ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    // SAFETY: live parent, NUL-terminated single component, initialized SDK
    // attrlist and exclusively borrowed initialized buffer outlive this call.
    // NOFOLLOW prevents the leaf from redirecting the access observation.
    let result = unsafe {
        libc::getattrlistat(
            parent.as_raw_fd(),
            child.as_ptr(),
            (&mut attributes as *mut libc::attrlist).cast(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            (libc::FSOPT_REPORT_FULLSIZE | libc::FSOPT_PACK_INVAL_ATTRS | libc::FSOPT_NOFOLLOW)
                .into(),
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        process::Command,
    };

    fn source(path: &std::path::Path) -> (File, MacAccessSnapshot) {
        fs::write(path, b"fixture").unwrap();
        let file = File::open(path).unwrap();
        let access = macos_access_snapshot(&file, 8192).unwrap();
        (file, access)
    }
    #[test]
    fn exact_observed_acl_guid_and_posix_access_is_reapplied_to_distinct_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source");
        fs::write(&path, b"fixture").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(
            Command::new("chmod")
                .args(["+a", "everyone allow read"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let (original, access) = source(&path);
        let original_metadata = original.metadata().unwrap();
        let destination = crate::create_private_file(&root.path().join("stage")).unwrap();
        assert_ne!(
            original_metadata.ino(),
            destination.metadata().unwrap().ino()
        );
        macos_apply_access(
            &destination,
            &access,
            original_metadata.mode() & 0o7777,
            original_metadata.uid(),
            original_metadata.gid(),
            8192,
        )
        .unwrap();
        assert!(macos_access_snapshot(&destination, 8192).unwrap() == access);
    }
    #[test]
    fn observed_absence_removes_destination_acl_without_adopting_empty_acl() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source");
        fs::write(&path, b"fixture").unwrap();
        assert!(
            Command::new("chmod")
                .arg("-N")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let (original, access) = source(&path);
        assert!(macos_access_snapshot(&original, 32).is_ok());
        let stage = root.path().join("stage");
        let destination = crate::create_private_file(&stage).unwrap();
        assert!(
            Command::new("chmod")
                .args(["+a", "everyone allow read"])
                .arg(&stage)
                .status()
                .unwrap()
                .success()
        );
        let metadata = original.metadata().unwrap();
        macos_apply_access(
            &destination,
            &access,
            metadata.mode() & 0o7777,
            metadata.uid(),
            metadata.gid(),
            96,
        )
        .unwrap();
        assert!(macos_access_snapshot(&destination, 32).unwrap() == access);
    }
    #[test]
    fn rejected_packet_cap_and_nonregular_target_change_no_access() {
        let root = tempfile::tempdir().unwrap();
        let (original, access) = source(&root.path().join("source"));
        let destination = crate::create_private_file(&root.path().join("stage")).unwrap();
        let metadata = original.metadata().unwrap();
        let before = macos_access_snapshot(&destination, 8192).unwrap();
        assert_eq!(
            macos_apply_access(
                &destination,
                &access,
                0o644,
                metadata.uid(),
                metadata.gid(),
                95
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::FileTooLarge
        );
        assert!(macos_access_snapshot(&destination, 8192).unwrap() == before);
        let directory = crate::create_private_directory(&root.path().join("control")).unwrap();
        assert_eq!(
            macos_apply_access(
                &directory,
                &access,
                0o644,
                metadata.uid(),
                metadata.gid(),
                8192
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidInput
        );
        crate::verify_private_directory(&directory).unwrap();
    }
}
