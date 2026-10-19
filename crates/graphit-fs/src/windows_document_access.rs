//! Bounded descriptor-bound PROJECT access data. Never a private-birth grant.
use std::{fs::File, io, os::windows::io::AsRawHandle, ptr};
use windows_sys::Win32::{
    Foundation::ERROR_INSUFFICIENT_BUFFER,
    Security::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetKernelObjectSecurity,
        GetSecurityDescriptorControl, IsValidSecurityDescriptor, OWNER_SECURITY_INFORMATION,
        SE_SELF_RELATIVE, SECURITY_DESCRIPTOR_RELATIVE,
    },
};

#[derive(PartialEq, Eq)]
pub struct WindowsDocumentAccess {
    words: Vec<u32>,
    bytes: usize,
}
impl WindowsDocumentAccess {
    /// Bounded borrowed native data; never filesystem authority.
    pub fn words(&self) -> &[u32] {
        &self.words
    }
    pub fn byte_length(&self) -> usize {
        self.bytes
    }
}

/// Observe owner/group/DACL from the exact READ_CONTROL handle using caller-owned
/// aligned storage. Growth between query/read is a conflict, with no retry.
pub fn windows_document_access_snapshot(
    file: &File,
    limit: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<WindowsDocumentAccess> {
    snapshot(file, limit, before)
}

#[allow(unsafe_code)]
fn snapshot(
    file: &File,
    limit: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<WindowsDocumentAccess> {
    if limit == 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let information =
        OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut needed = 0_u32;
    before()?;
    // SAFETY: live borrowed File and initialized output length; zero-capacity
    // query has a null output buffer, which the generated SDK explicitly permits.
    let queried = unsafe {
        GetKernelObjectSecurity(
            file.as_raw_handle(),
            information,
            ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    let error = io::Error::last_os_error();
    if queried != 0 || error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
        return Err(if queried != 0 {
            io::ErrorKind::InvalidData.into()
        } else {
            error
        });
    }
    let length = usize::try_from(needed).map_err(|_| io::ErrorKind::FileTooLarge)?;
    if length < std::mem::size_of::<SECURITY_DESCRIPTOR_RELATIVE>() || length > limit {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let count = length.checked_add(3).ok_or(io::ErrorKind::FileTooLarge)? / 4;
    before()?;
    let mut words = Vec::<u32>::new();
    words
        .try_reserve_exact(count)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    words.resize(count, 0);
    before()?;
    let mut actual = 0_u32;
    // SAFETY: initialized u32 storage has SDK-required alignment and at least
    // needed bytes. File, storage and initialized out-length remain exclusively
    // borrowed for this synchronous call; no Rust references cross the SDK.
    let copied = unsafe {
        GetKernelObjectSecurity(
            file.as_raw_handle(),
            information,
            words.as_mut_ptr().cast(),
            needed,
            &mut actual,
        )
    };
    if copied == 0 {
        let error = io::Error::last_os_error();
        return Err(
            if error.raw_os_error() == Some(ERROR_INSUFFICIENT_BUFFER as i32) {
                io::ErrorKind::InvalidData.into()
            } else {
                error
            },
        );
    }
    if actual != needed {
        return Err(io::ErrorKind::InvalidData.into());
    }
    before()?;
    // SAFETY: the SDK just wrote a complete self-relative descriptor in this
    // initialized aligned live buffer; validation only borrows its native data.
    if unsafe { IsValidSecurityDescriptor(words.as_mut_ptr().cast()) } == 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut control = 0_u16;
    let mut revision = 0_u32;
    before()?;
    // SAFETY: validated live descriptor and initialized scalar output slots are
    // borrowed only for this synchronous generated SDK inspection.
    if unsafe {
        GetSecurityDescriptorControl(words.as_mut_ptr().cast(), &mut control, &mut revision)
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if control & SE_SELF_RELATIVE == 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(WindowsDocumentAccess {
        words,
        bytes: length,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    #[test]
    fn real_read_control_handle_returns_bounded_repeatable_access_data() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("document"), b"original").unwrap();
        let parent =
            crate::read_directory::open_root_readonly(temporary.path(), &mut || Ok(())).unwrap();
        let file = crate::read_directory::open_relative_document_read(
            &parent,
            OsStr::new("document"),
            4096,
            &mut || Ok(()),
        )
        .unwrap();
        let value = windows_document_access_snapshot(&file, 65536, &mut || Ok(())).unwrap();
        assert!(value.byte_length() > 0);
        assert!(value.words().len() * 4 >= value.byte_length());
        assert!(value.words().len() * 4 - value.byte_length() < 4);
        assert!(
            value
                == windows_document_access_snapshot(&file, value.byte_length(), &mut || Ok(()))
                    .unwrap()
        );
        assert!(
            windows_document_access_snapshot(&file, value.byte_length() - 1, &mut || Ok(()))
                .is_err()
        );
        assert_eq!(
            windows_document_access_snapshot(&file, 65536, &mut || Err(
                io::ErrorKind::Interrupted.into()
            ))
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::Interrupted
        );
    }
}
