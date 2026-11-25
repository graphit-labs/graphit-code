//! SDK reads for macOS private-file checks.
use crate::macos_access_buffer::{self, MacAccessSnapshot};
use std::{fs::File, io, os::fd::AsRawFd};

/// Read the extended-security payload for private-file checks.
/// `limit` bounds the ACL payload; the response envelope is bounded separately.
/// This does not limit kernel allocation or grant ownership.
pub(crate) fn macos_access_snapshot(file: &File, limit: usize) -> io::Result<MacAccessSnapshot> {
    extended_security_support(file)?;
    macos_access_buffer::snapshot(limit, |buffer| read_attributes(file, buffer))
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
        commonattr: libc::ATTR_CMN_EXTENDED_SECURITY | libc::ATTR_CMN_RETURNED_ATTRS,
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

/// Observe current ACL privacy through its pathname without opening the leaf.
pub(crate) fn macos_access_snapshot_path(path: &std::path::Path) -> io::Result<MacAccessSnapshot> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path =
        CString::new(path.as_os_str().as_bytes()).map_err(|_| io::ErrorKind::InvalidInput)?;
    path_security_support(&path)?;
    macos_access_buffer::snapshot(8192, |buffer| read_path_attributes(&path, buffer))
}

#[allow(unsafe_code)]
fn path_security_support(path: &std::ffi::CStr) -> io::Result<()> {
    errno::set_errno(errno::Errno(0));
    // SAFETY: the terminated path remains live; pathconf performs metadata I/O
    // without adopting or closing a descriptor for the database leaf.
    let value = unsafe { libc::pathconf(path.as_ptr(), libc::_PC_EXTENDED_SECURITY_NP) };
    let error = errno::errno().0;
    macos_access_buffer::qualify_support(value, error)
}

#[allow(unsafe_code)]
fn read_path_attributes(path: &std::ffi::CStr, buffer: &mut [u8]) -> io::Result<()> {
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_EXTENDED_SECURITY | libc::ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    // SAFETY: terminated path, initialized SDK attrlist and borrowed initialized
    // storage remain valid for this metadata-only call. The packed response is
    // decoded from checked byte slices and never cast to borrowed Rust values.
    let result = unsafe {
        libc::getattrlist(
            path.as_ptr(),
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
