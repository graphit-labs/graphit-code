//! Minimal audited generated SDK calls. No manual bindings or path normalization.
use super::NodeKind;
use std::{
    ffi::OsStr,
    fs::File,
    io,
    mem::{offset_of, size_of},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    ptr,
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
            FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
        },
    },
    Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE, RtlNtStatusToDosError, UNICODE_STRING},
        Storage::FileSystem::{
            FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
            FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, FileIdBothDirectoryInfo,
            GetFileInformationByHandleEx, SYNCHRONIZE,
        },
        System::IO::{IO_STATUS_BLOCK, IO_STATUS_BLOCK_0},
    },
};

pub(super) fn open_relative_readonly(
    parent: &File,
    name: &OsStr,
    kind: Option<NodeKind>,
    max_component_bytes: usize,
    internal_enumerator: bool,
    document_read: bool,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    open_relative(
        parent,
        name,
        kind,
        max_component_bytes,
        RelativeOperation::Read {
            internal_enumerator,
            document_read,
        },
        before,
    )
}

/// Exact held-parent private directory/lock operation. FILE_CREATE is exclusive;
/// FILE_OPEN only observes an existing entry and never adopts a replacement.
pub(super) fn open_private_relative(
    parent: &File,
    name: &OsStr,
    directory: bool,
    create: bool,
    max_component_bytes: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    open_relative(
        parent,
        name,
        Some(if directory {
            NodeKind::Directory
        } else {
            NodeKind::File
        }),
        max_component_bytes,
        RelativeOperation::Private { create },
        before,
    )
}

pub(super) fn open_private_database_relative(
    parent: &File,
    name: &OsStr,
    max_component_bytes: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    open_relative(
        parent,
        name,
        Some(NodeKind::File),
        max_component_bytes,
        RelativeOperation::DatabaseRead,
        before,
    )
}

enum RelativeOperation {
    Read {
        internal_enumerator: bool,
        document_read: bool,
    },
    Private {
        create: bool,
    },
    DatabaseRead,
}

#[allow(unsafe_code)]
fn open_relative(
    parent: &File,
    name: &OsStr,
    kind: Option<NodeKind>,
    max_component_bytes: usize,
    operation: RelativeOperation,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    let (internal_enumerator, document_read, private, create, database_read) = match operation {
        RelativeOperation::Read {
            internal_enumerator,
            document_read,
        } => (internal_enumerator, document_read, false, false, false),
        RelativeOperation::Private { create } => (false, true, true, create, false),
        RelativeOperation::DatabaseRead => (false, true, false, false, true),
    };
    let count = name.encode_wide().count();
    let byte_count = count.checked_mul(2).ok_or(io::ErrorKind::FileTooLarge)?;
    if count == 0
        || byte_count > max_component_bytes
        || byte_count > usize::from(u16::MAX - 1)
        || name
            .encode_wide()
            .any(|u| u == 0 || u == 47 || u == 92 || u == 58)
        || (name == OsStr::new(".") && !internal_enumerator)
        || name == OsStr::new("..")
        || (internal_enumerator && (name != OsStr::new(".") || kind != Some(NodeKind::Directory)))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // The caller admits this temporary logical metadata before invoking us.
    let mut wide = Vec::new();
    wide.try_reserve_exact(count)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    wide.extend(name.encode_wide());
    let length = u16::try_from(byte_count).map_err(|_| io::ErrorKind::FileTooLarge)?;
    let name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: wide.as_mut_ptr(),
    };
    before()?;
    let descriptor = if create {
        Some(crate::windows_private_file::private_descriptor()?)
    } else {
        None
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>())
            .map_err(|_| io::ErrorKind::FileTooLarge)?,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &name,
        Attributes: 0,
        SecurityDescriptor: descriptor
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr().cast()),
        SecurityQualityOfService: ptr::null(),
    };
    let mut status_block = IO_STATUS_BLOCK {
        Anonymous: IO_STATUS_BLOCK_0 { Status: 0 },
        Information: 0,
    };
    let mut raw = ptr::null_mut();
    let options = FILE_OPEN_REPARSE_POINT
        | FILE_SYNCHRONOUS_IO_NONALERT
        | match kind {
            Some(NodeKind::Directory) => FILE_DIRECTORY_FILE,
            Some(NodeKind::File) => FILE_NON_DIRECTORY_FILE,
            None => 0,
        };
    let mut access = if document_read {
        windows_sys::Win32::Storage::FileSystem::READ_CONTROL
    } else {
        0
    } | FILE_READ_ATTRIBUTES
        | SYNCHRONIZE
        | match kind {
            Some(NodeKind::Directory) => FILE_LIST_DIRECTORY | FILE_TRAVERSE,
            _ => FILE_READ_DATA,
        };
    if private && kind == Some(NodeKind::File) {
        access |= GENERIC_READ | GENERIC_WRITE;
    }
    let sharing = FILE_SHARE_READ
        | if private || database_read || kind == Some(NodeKind::Directory) {
            FILE_SHARE_WRITE
        } else {
            0
        };
    let disposition = if create { FILE_CREATE } else { FILE_OPEN };
    before()?;
    // SAFETY: parent and the optional owned private descriptor are borrowed/alive; generated structs, UTF16 buffer and output
    // locals remain valid for the synchronous call. FILE_CREATE is exclusive;
    // FILE_OPEN cannot create/truncate. SYNCHRONIZE/NONALERT is synchronous.
    // The name is exactly one component, with its checked byte length, no NUL.
    let status = unsafe {
        NtCreateFile(
            &mut raw,
            access,
            &attributes,
            &mut status_block,
            ptr::null(),
            0,
            sharing,
            disposition,
            options,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        // SAFETY: conversion consumes only the returned NTSTATUS value. It does
        // not borrow pointers or adopt a handle on the failure branch.
        let code = unsafe { RtlNtStatusToDosError(status) };
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    if raw.is_null() || status != 0 {
        // Synchronous successful opens return STATUS_SUCCESS. A nonzero status
        // with an owned handle still must close it, handled by the adoption below.
        if raw.is_null() {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
    // SAFETY: successful NtCreateFile transferred one non-null owned handle.
    // Exactly this site adopts it once; failure branches never adopt. OwnedHandle
    // closes it even when subsequent validation or the status fence fails.
    let owned = unsafe { OwnedHandle::from_raw_handle(raw) };
    let file = File::from(owned);
    if status != 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(file)
}

#[allow(unsafe_code)]
pub(super) fn query_directory(
    file: &File,
    buffer: &mut [u64],
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<bool> {
    let bytes = buffer
        .len()
        .checked_mul(size_of::<u64>())
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(io::ErrorKind::FileTooLarge)?;
    if usize::try_from(bytes).map_err(|_| io::ErrorKind::FileTooLarge)?
        < size_of::<FILE_ID_BOTH_DIR_INFO>()
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    buffer.fill(0);
    before()?;
    // SAFETY: the alive synchronous directory handle remains borrowed. Vec<u64>
    // supplies aligned initialized writable bytes for the exact checked size.
    // SDK writes bytes only during the call and retains no pointer. Parsing uses
    // SDK-derived offsets and checked byte reads, never a cast/deref to a record.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdBothDirectoryInfo,
            buffer.as_mut_ptr().cast(),
            bytes,
        )
    };
    if ok != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(18) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[allow(unsafe_code)]
pub(super) fn directory_buffer_bytes(buffer: &[u64]) -> io::Result<&[u8]> {
    let length = buffer
        .len()
        .checked_mul(size_of::<u64>())
        .ok_or(io::ErrorKind::FileTooLarge)?;
    // SAFETY: every u64 is initialized and has no padding. The borrowed byte
    // slice has the same lifetime and checked extent as its underlying buffer;
    // no mutation through another alias occurs while this immutable borrow lives.
    Ok(unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast(), length) })
}

#[cfg(test)]
pub(super) fn directory_name(
    buffer: &[u64],
    offset: usize,
    max_component_bytes: usize,
) -> io::Result<(std::ffi::OsString, Option<usize>)> {
    directory_record(buffer, offset, max_component_bytes).map(|(name, _, next)| (name, next))
}

pub(super) fn directory_record(
    buffer: &[u64],
    offset: usize,
    max_component_bytes: usize,
) -> io::Result<(std::ffi::OsString, u32, Option<usize>)> {
    use std::os::windows::ffi::OsStringExt;
    let bytes = directory_buffer_bytes(buffer)?;
    let name_offset = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
    let at = |field: usize| offset.checked_add(field).ok_or(io::ErrorKind::InvalidData);
    let read_u32 = |field: usize| -> io::Result<u32> {
        let start = at(field)?;
        let end = start.checked_add(4).ok_or(io::ErrorKind::InvalidData)?;
        let value: [u8; 4] = bytes
            .get(start..end)
            .ok_or(io::ErrorKind::InvalidData)?
            .try_into()
            .map_err(|_| io::ErrorKind::InvalidData)?;
        Ok(u32::from_ne_bytes(value))
    };
    let next = usize::try_from(read_u32(offset_of!(
        FILE_ID_BOTH_DIR_INFO,
        NextEntryOffset
    ))?)
    .map_err(|_| io::ErrorKind::InvalidData)?;
    let length = usize::try_from(read_u32(offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength))?)
        .map_err(|_| io::ErrorKind::InvalidData)?;
    if length == 0
        || length % 2 != 0
        || length > max_component_bytes
        || length > usize::from(u16::MAX - 1)
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let start = at(name_offset)?;
    let end = start
        .checked_add(length)
        .ok_or(io::ErrorKind::InvalidData)?;
    let name = bytes.get(start..end).ok_or(io::ErrorKind::InvalidData)?;
    let (units, _) = name.as_chunks::<2>();
    // A native entry is still one component. Reject aliases/NUL before owned
    // wide/name allocation; unpaired surrogate code units are preserved.
    if units.iter().any(|pair| {
        let unit = u16::from_ne_bytes(*pair);
        unit == 0 || unit == 47 || unit == 92 || unit == 58
    }) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let next_offset = if next == 0 {
        None
    } else {
        if next % 8 != 0
            || next
                < name_offset
                    .checked_add(length)
                    .ok_or(io::ErrorKind::InvalidData)?
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let next = offset.checked_add(next).ok_or(io::ErrorKind::InvalidData)?;
        if next
            .checked_add(name_offset)
            .is_none_or(|n| n > bytes.len())
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Some(next)
    };
    let attributes = read_u32(offset_of!(FILE_ID_BOTH_DIR_INFO, FileAttributes))?;
    let mut wide = Vec::new();
    wide.try_reserve_exact(length / 2)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    for value in units {
        wide.push(u16::from_ne_bytes(*value));
    }
    Ok((
        std::ffi::OsString::from_wide(&wide),
        attributes,
        next_offset,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    fn record(name: &[u16], length_override: Option<u32>, next: u32) -> Vec<u64> {
        let mut bytes = vec![0u8; 256];
        let length_at = offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength);
        let next_at = offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset);
        let name_at = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
        let length = length_override.unwrap_or((name.len() * 2) as u32);
        bytes[length_at..length_at + 4].copy_from_slice(&length.to_ne_bytes());
        bytes[next_at..next_at + 4].copy_from_slice(&next.to_ne_bytes());
        for (index, unit) in name.iter().enumerate() {
            let at = name_at + index * 2;
            bytes[at..at + 2].copy_from_slice(&unit.to_ne_bytes());
        }
        bytes
            .as_chunks::<8>()
            .0
            .iter()
            .copied()
            .map(u64::from_ne_bytes)
            .collect()
    }
    #[test]
    fn sdk_layout_parser_preserves_unpaired_utf16_before_native_qualification() {
        let units = [0xd800, u16::from(b'x')];
        let (name, next) = directory_name(&record(&units, None, 0), 0, 256).unwrap();
        assert_eq!(name.encode_wide().collect::<Vec<_>>(), units);
        assert_eq!(next, None);
        // This is a pure SDK-layout parser fixture, not native FS creation.
    }
    #[test]
    fn invalid_offsets_lengths_and_component_aliases_are_rejected() {
        for buffer in [
            record(&[u16::from(b'x')], Some(3), 0),
            record(&[u16::from(b'x')], Some(1000), 0),
            record(&[u16::from(b'x')], None, 8),
            record(&[0], None, 0),
            record(&[u16::from(b'/')], None, 0),
            record(&[u16::from(b':')], None, 0),
        ] {
            assert!(directory_name(&buffer, 0, 256).is_err());
        }
    }
}
