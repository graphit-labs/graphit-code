//! Bounded descriptor-bound PROJECT access data. Never a private-birth grant.
use sha2::{Digest, Sha256};
use std::{ffi::c_void, fs::File, io, ops::Range, os::windows::io::AsRawHandle, ptr};
use windows_sys::Win32::{
    Foundation::ERROR_INSUFFICIENT_BUFFER,
    Security::{
        ACE_HEADER, ACL, ACL_REVISION_INFORMATION, ACL_SIZE_INFORMATION, AclRevisionInformation,
        AclSizeInformation, DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetAce,
        GetAclInformation, GetKernelObjectSecurity, GetLengthSid, GetSecurityDescriptorControl,
        GetSecurityDescriptorDacl, GetSecurityDescriptorGroup, GetSecurityDescriptorOwner,
        GetSecurityDescriptorRMControl, IsValidAcl, IsValidSecurityDescriptor,
        OWNER_SECURITY_INFORMATION, SE_RM_CONTROL_VALID, SE_SELF_RELATIVE,
        SECURITY_DESCRIPTOR_RELATIVE,
    },
};

pub struct WindowsDocumentAccess {
    fingerprint: [u8; 32],
    #[cfg(test)]
    bytes: usize,
    #[cfg(test)]
    diagnostics: AccessDiagnostics,
}
#[cfg(test)]
#[derive(Default)]
struct AccessDiagnostics {
    revision: u32,
    control: u16,
    owner: [u8; 32],
    owner_present: bool,
    owner_defaulted: bool,
    group: [u8; 32],
    group_present: bool,
    group_defaulted: bool,
    dacl: [u8; 32],
    dacl_present: bool,
    dacl_null: bool,
    dacl_defaulted: bool,
    acl_revision: u32,
    ace_count: u32,
    rm_control: Option<u8>,
}
impl PartialEq for WindowsDocumentAccess {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}
impl Eq for WindowsDocumentAccess {}
impl WindowsDocumentAccess {
    /// Exact SDK components, independent of self-relative offsets and padding.
    /// Retains control/defaulted flags and complete ACEs in their native order.
    pub fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
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
    #[cfg(test)]
    let mut diagnostics = AccessDiagnostics::default();
    let fingerprint = component_fingerprint(
        &words,
        length,
        control,
        revision,
        before,
        #[cfg(test)]
        &mut diagnostics,
    )?;
    before()?;
    Ok(WindowsDocumentAccess {
        fingerprint,
        #[cfg(test)]
        bytes: length,
        #[cfg(test)]
        diagnostics,
    })
}

// Returned SDK component pointers must remain inside the already validated,
// bounded self-relative descriptor. No component is decoded by this module.
fn native_range(
    words: &[u32],
    length: usize,
    address: *const c_void,
    bytes: usize,
) -> io::Result<Range<usize>> {
    let start = address
        .addr()
        .checked_sub(words.as_ptr().addr())
        .ok_or(io::ErrorKind::InvalidData)?;
    let end = start.checked_add(bytes).ok_or(io::ErrorKind::InvalidData)?;
    if end > length || length > words.len().saturating_mul(4) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(start..end)
}
fn hash_native(
    digest: &mut Sha256,
    words: &[u32],
    length: usize,
    address: *const c_void,
    bytes: usize,
) -> io::Result<()> {
    let range = native_range(words, length, address, bytes)?;
    digest.update((bytes as u64).to_le_bytes());
    for (index, word) in words
        .iter()
        .enumerate()
        .take(range.end.div_ceil(4))
        .skip(range.start / 4)
    {
        let bytes = word.to_ne_bytes();
        let start = range.start.saturating_sub(index * 4);
        let end = (range.end - index * 4).min(4);
        digest.update(&bytes[start..end]);
    }
    Ok(())
}
#[allow(unsafe_code)]
fn hash_sid(
    digest: &mut Sha256,
    words: &[u32],
    length: usize,
    sid: *mut c_void,
    before: &mut impl FnMut() -> io::Result<()>,
    #[cfg(test)] comparison: &mut [u8; 32],
) -> io::Result<()> {
    digest.update([u8::from(!sid.is_null())]);
    if !sid.is_null() {
        before()?;
        // SAFETY: GetSecurityDescriptorOwner/Group returned this SID from the
        // SDK-validated live self-relative descriptor; it cannot outlive words.
        let bytes = unsafe { GetLengthSid(sid) } as usize;
        hash_native(digest, words, length, sid, bytes)?;
        #[cfg(test)]
        {
            let mut component = Sha256::new();
            hash_native(&mut component, words, length, sid, bytes)?;
            *comparison = component.finalize().into();
        }
    }
    Ok(())
}

#[allow(unsafe_code)]
fn component_fingerprint(
    words: &[u32],
    length: usize,
    control: u16,
    revision: u32,
    before: &mut impl FnMut() -> io::Result<()>,
    #[cfg(test)] diagnostics: &mut AccessDiagnostics,
) -> io::Result<[u8; 32]> {
    #[cfg(test)]
    {
        diagnostics.revision = revision;
        diagnostics.control = control;
    }
    let descriptor = words.as_ptr().cast_mut().cast();
    let mut digest = Sha256::new();
    digest.update(b"graphit/windows-document-access/components/v1\0");
    digest.update(revision.to_le_bytes());
    digest.update(control.to_le_bytes());
    let mut owner = ptr::null_mut();
    let mut owner_defaulted = 0;
    before()?;
    // SAFETY: the synchronous getter borrows the validated, aligned descriptor
    // and initialized outputs; returned components remain borrowed from words.
    if unsafe { GetSecurityDescriptorOwner(descriptor, &mut owner, &mut owner_defaulted) } == 0 {
        return Err(io::Error::last_os_error());
    }
    digest.update([u8::from(owner_defaulted != 0)]);
    #[cfg(test)]
    {
        diagnostics.owner_present = !owner.is_null();
        diagnostics.owner_defaulted = owner_defaulted != 0;
    }
    hash_sid(
        &mut digest,
        words,
        length,
        owner,
        before,
        #[cfg(test)]
        &mut diagnostics.owner,
    )?;
    let mut group = ptr::null_mut();
    let mut group_defaulted = 0;
    before()?;
    // SAFETY: same live validated descriptor and initialized component outputs.
    if unsafe { GetSecurityDescriptorGroup(descriptor, &mut group, &mut group_defaulted) } == 0 {
        return Err(io::Error::last_os_error());
    }
    digest.update([u8::from(group_defaulted != 0)]);
    #[cfg(test)]
    {
        diagnostics.group_present = !group.is_null();
        diagnostics.group_defaulted = group_defaulted != 0;
    }
    hash_sid(
        &mut digest,
        words,
        length,
        group,
        before,
        #[cfg(test)]
        &mut diagnostics.group,
    )?;
    let mut present = 0;
    let mut dacl = ptr::null_mut();
    let mut defaulted = 0;
    before()?;
    // SAFETY: exact validated descriptor; outputs distinguish absent and NULL
    // DACLs without constructing a Rust reference to a potentially null ACL.
    if unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }
        == 0
    {
        return Err(io::Error::last_os_error());
    }
    digest.update([
        u8::from(present != 0),
        u8::from(defaulted != 0),
        u8::from(!dacl.is_null()),
    ]);
    #[cfg(test)]
    {
        diagnostics.dacl_present = present != 0;
        diagnostics.dacl_null = dacl.is_null();
        diagnostics.dacl_defaulted = defaulted != 0;
    }
    #[cfg(test)]
    let mut acl_digest = Sha256::new();
    if !dacl.is_null() {
        native_range(words, length, dacl.cast(), std::mem::size_of::<ACL>())?;
        before()?;
        // SAFETY: nonnull SDK DACL lies in the live validated descriptor.
        if unsafe { IsValidAcl(dacl) } == 0 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut acl_revision = ACL_REVISION_INFORMATION::default();
        before()?;
        // SAFETY: validated DACL and a correctly sized initialized SDK output.
        if unsafe {
            GetAclInformation(
                dacl,
                ptr::from_mut(&mut acl_revision).cast(),
                std::mem::size_of::<ACL_REVISION_INFORMATION>() as u32,
                AclRevisionInformation,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut size = ACL_SIZE_INFORMATION::default();
        before()?;
        // SAFETY: validated DACL and a correctly sized initialized SDK output.
        if unsafe {
            GetAclInformation(
                dacl,
                ptr::from_mut(&mut size).cast(),
                std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        native_range(words, length, dacl.cast(), size.AclBytesInUse as usize)?;
        digest.update(acl_revision.AclRevision.to_le_bytes());
        digest.update(size.AceCount.to_le_bytes());
        #[cfg(test)]
        {
            diagnostics.acl_revision = acl_revision.AclRevision;
            diagnostics.ace_count = size.AceCount;
            acl_digest.update(acl_revision.AclRevision.to_le_bytes());
            acl_digest.update(size.AceCount.to_le_bytes());
        }
        // Capacity/free bytes are allocation data, not permissions. Each ACE
        // includes its header, flags, SID and any object/callback payload.
        for index in 0..size.AceCount {
            let mut entry = ptr::null_mut();
            before()?;
            // SAFETY: index is bounded by the count returned for this validated
            // DACL; the initialized pointer remains borrowed from its buffer.
            if unsafe { GetAce(dacl, index, &mut entry) } == 0 {
                return Err(io::Error::last_os_error());
            }
            native_range(words, length, entry, std::mem::size_of::<ACE_HEADER>())?;
            // SAFETY: SDK GetAce returns an aligned ACE; its whole header lies
            // within words, whose descriptor and ACL have already been validated.
            let header = unsafe { ptr::read(entry.cast::<ACE_HEADER>()) };
            if usize::from(header.AceSize) < std::mem::size_of::<ACE_HEADER>() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            hash_native(&mut digest, words, length, entry, header.AceSize.into())?;
            #[cfg(test)]
            hash_native(&mut acl_digest, words, length, entry, header.AceSize.into())?;
        }
    }
    #[cfg(test)]
    {
        diagnostics.dacl = acl_digest.finalize().into();
    }
    if control & SE_RM_CONTROL_VALID != 0 {
        let mut rm_control = 0_u8;
        before()?;
        // SAFETY: SE_RM_CONTROL_VALID was obtained from this validated descriptor;
        // the synchronous SDK call borrows it and the initialized byte output.
        let status = unsafe { GetSecurityDescriptorRMControl(descriptor, &mut rm_control) };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        digest.update([rm_control]);
        #[cfg(test)]
        {
            diagnostics.rm_control = Some(rm_control);
        }
    }
    Ok(digest.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    fn fixture(
        owner: &[u8],
        group: &[u8],
        aces: &[Vec<u8>],
        order: [usize; 3],
        padding: usize,
        free: usize,
    ) -> Vec<u32> {
        // SECURITY_DESCRIPTOR_RELATIVE, S-1-5-18 and S-1-5-32-544.
        let mut acl = vec![2, 0, 0, 0, aces.len() as u8, 0, 0, 0];
        for ace in aces {
            acl.extend_from_slice(ace);
        }
        acl.resize(acl.len() + free, 0);
        let size = (acl.len() as u16).to_le_bytes();
        acl[2..4].copy_from_slice(&size);
        let parts = [owner, group, acl.as_slice()];
        let mut data = vec![0_u8; 20 + padding];
        data[..4].copy_from_slice(&[1, 0, 4, 128]);
        for part in order {
            let offset = (data.len() as u32).to_le_bytes();
            let field = [4, 8, 16][part];
            data[field..field + 4].copy_from_slice(&offset);
            data.extend_from_slice(parts[part]);
        }
        data.as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(u32::from_ne_bytes)
            .collect()
    }
    fn owner() -> Vec<u8> {
        vec![1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0]
    }
    fn group() -> Vec<u8> {
        vec![1, 2, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0, 32, 2, 0, 0]
    }
    fn ace(mask: u32) -> Vec<u8> {
        let mut ace = vec![0, 0, 20, 0];
        ace.extend_from_slice(&mask.to_le_bytes());
        ace.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
        ace
    }
    fn fixture_fingerprint(words: &[u32]) -> [u8; 32] {
        component_fingerprint(
            words,
            words.len() * 4,
            (words[0] >> 16) as u16,
            1,
            &mut || Ok(()),
            &mut AccessDiagnostics::default(),
        )
        .unwrap()
    }
    #[test]
    fn sdk_components_ignore_layout_and_acl_capacity_but_preserve_security() {
        let aces = [ace(0x120089), ace(0x120116)];
        let original = fixture(&owner(), &group(), &aces, [0, 1, 2], 0, 0);
        let expected = fixture_fingerprint(&original);
        for order in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
            let moved = fixture(&owner(), &group(), &aces, order, 16, 12);
            assert_ne!(original, moved);
            assert_eq!(fixture_fingerprint(&moved), expected);
        }
        for different in [
            fixture(&group(), &group(), &aces, [0, 1, 2], 0, 0),
            fixture(&owner(), &owner(), &aces, [0, 1, 2], 0, 0),
            fixture(
                &owner(),
                &group(),
                &[aces[1].clone(), aces[0].clone()],
                [0, 1, 2],
                0,
                0,
            ),
            fixture(
                &owner(),
                &group(),
                &[ace(0x120088), aces[1].clone()],
                [0, 1, 2],
                0,
                0,
            ),
        ] {
            assert_ne!(fixture_fingerprint(&different), expected);
        }
        for flag in [0x0001, 0x0002, 0x0008, 0x0100, 0x0400, 0x1000, 0x2000] {
            let mut different = original.clone();
            different[0] |= flag << 16;
            assert_ne!(fixture_fingerprint(&different), expected);
        }
        let mut different = original.clone();
        // First ACE's inherited flag; ordering and raw flags are retained.
        let offset = original[4] as usize + 8 + 1;
        let mut bytes = different[offset / 4].to_ne_bytes();
        bytes[offset % 4] |= 0x10;
        different[offset / 4] = u32::from_ne_bytes(bytes);
        assert_ne!(fixture_fingerprint(&different), expected);
    }
    #[test]
    fn sdk_components_retain_callback_ace_payload_and_reject_out_of_bound_ranges() {
        let mut callback = ace(0x120089);
        callback[0] = 9; // ACCESS_ALLOWED_CALLBACK_ACE_TYPE.
        callback[2..4].copy_from_slice(&24_u16.to_le_bytes());
        callback.extend_from_slice(&[0x10, 0x20, 0x30, 0x40]);
        let original = fixture(&owner(), &group(), &[callback.clone()], [0, 1, 2], 0, 0);
        callback[23] ^= 1;
        let changed = fixture(&owner(), &group(), &[callback], [0, 1, 2], 0, 0);
        assert_ne!(
            fixture_fingerprint(&original),
            fixture_fingerprint(&changed)
        );
        let address = original.as_ptr().cast::<c_void>();
        let length = original.len() * 4;
        assert!(native_range(&original, length, address.wrapping_byte_sub(1), 1).is_err());
        assert!(native_range(&original, length, address.wrapping_byte_add(length), 1).is_err());
        assert!(native_range(&original, length + 1, address, length + 1).is_err());
    }
    #[test]
    fn sdk_components_distinguish_missing_null_empty_and_rm_control() {
        let empty = fixture(&owner(), &group(), &[], [0, 1, 2], 0, 0);
        let mut null = empty.clone();
        null[4] = 0;
        let mut absent = null.clone();
        absent[0] &= !(4 << 16);
        assert_ne!(fixture_fingerprint(&empty), fixture_fingerprint(&null));
        assert_ne!(fixture_fingerprint(&null), fixture_fingerprint(&absent));
        assert_ne!(fixture_fingerprint(&empty), fixture_fingerprint(&absent));
        let mut rm = empty.clone();
        rm[0] |= 0x4000 << 16;
        let first = fixture_fingerprint(&rm);
        rm[0] |= 1 << 8;
        assert_ne!(fixture_fingerprint(&rm), first);
        let mut without_owner = empty.clone();
        without_owner[1] = 0;
        assert_ne!(
            fixture_fingerprint(&without_owner),
            fixture_fingerprint(&empty)
        );
        let mut without_group = empty.clone();
        without_group[2] = 0;
        assert_ne!(
            fixture_fingerprint(&without_group),
            fixture_fingerprint(&empty)
        );
    }
    #[test]
    fn sdk_components_check_original_before_each_sdk_observation() {
        let words = fixture(&owner(), &group(), &[ace(0x120089)], [0, 1, 2], 0, 0);
        let mut calls = 0;
        component_fingerprint(
            &words,
            words.len() * 4,
            0x8004,
            1,
            &mut || {
                calls += 1;
                Ok(())
            },
            &mut AccessDiagnostics::default(),
        )
        .unwrap();
        assert!(calls > 4);
        for stop in 1..=calls {
            let mut observed = 0;
            let error = component_fingerprint(
                &words,
                words.len() * 4,
                0x8004,
                1,
                &mut || {
                    observed += 1;
                    if observed == stop {
                        Err(io::ErrorKind::Interrupted.into())
                    } else {
                        Ok(())
                    }
                },
                &mut AccessDiagnostics::default(),
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Interrupted);
            assert_eq!(observed, stop);
        }
    }
    fn compare_access(before: &AccessDiagnostics, after: &AccessDiagnostics) -> String {
        format!(
            "revision_match={}, control_before={:#06x}, control_after={:#06x}, owner_sid_match={}, owner_present_match={}, owner_defaulted_match={}, group_sid_match={}, group_present_match={}, group_defaulted_match={}, dacl_present_match={}, dacl_null_match={}, dacl_defaulted_match={}, acl_revision_before={}, acl_revision_after={}, ace_count_before={}, ace_count_after={}, ace_bytes_match={}, rm_match={}",
            before.revision == after.revision,
            before.control,
            after.control,
            before.owner == after.owner,
            before.owner_present == after.owner_present,
            before.owner_defaulted == after.owner_defaulted,
            before.group == after.group,
            before.group_present == after.group_present,
            before.group_defaulted == after.group_defaulted,
            before.dacl_present == after.dacl_present,
            before.dacl_null == after.dacl_null,
            before.dacl_defaulted == after.dacl_defaulted,
            before.acl_revision,
            after.acl_revision,
            before.ace_count,
            after.ace_count,
            before.dacl == after.dacl,
            before.rm_control == after.rm_control,
        )
    }
    #[test]
    fn native_replace_preserves_previous_access_components() {
        use std::io::Write;
        let temporary = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temporary.path()).unwrap();
        let mut differences = Vec::new();
        for private_previous in [false, true] {
            let destination = root.join(format!("existing-{private_previous}"));
            if private_previous {
                // The previous private document is born by the same first-save
                // stage and exclusive rename as Core's missing-file publication.
                let previous_directory_path = root.join(".graphit-doc-previous");
                let previous_directory =
                    crate::create_private_directory(&previous_directory_path).unwrap();
                let previous_stage = previous_directory_path.join("document");
                let mut file = crate::create_private_file(&previous_stage).unwrap();
                file.write_all(b"old").unwrap();
                file.sync_all().unwrap();
                drop(file);
                crate::rename_exclusive(&previous_stage, &destination).unwrap();
                drop(previous_directory);
                std::fs::remove_dir(previous_directory_path).unwrap();
            } else {
                std::fs::write(&destination, b"old").unwrap();
            }
            let previous =
                snapshot(&File::open(&destination).unwrap(), 65536, &mut || Ok(())).unwrap();
            let directory_path = root.join(format!(".graphit-doc-next-{private_previous}"));
            let directory = crate::create_private_directory(&directory_path).unwrap();
            let stage_path = directory_path.join("document");
            let mut stage = crate::create_private_file(&stage_path).unwrap();
            stage.write_all(b"new").unwrap();
            stage.sync_all().unwrap();
            let prepared = snapshot(&stage, 65536, &mut || Ok(())).unwrap();
            drop(stage);
            crate::replace_existing_file(&stage_path, &destination).unwrap();
            let actual =
                snapshot(&File::open(&destination).unwrap(), 65536, &mut || Ok(())).unwrap();
            assert!(!stage_path.exists());
            assert_eq!(std::fs::read(&destination).unwrap(), b"new");
            crate::verify_private_directory(&directory).unwrap();
            drop(directory);
            std::fs::remove_dir(directory_path).unwrap();
            if actual != previous {
                differences.push(format!(
                    "private_previous={private_previous}; previous_to_post: {}; stage_to_post: {}",
                    compare_access(&previous.diagnostics, &actual.diagnostics),
                    compare_access(&prepared.diagnostics, &actual.diagnostics),
                ));
            }
        }
        // Keep the strict publication contract and report only component
        // equality/flags/counts; never log SIDs, ACLs or raw descriptor data.
        assert!(differences.is_empty(), "{}", differences.join("\n"));
    }
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
        assert!(value.bytes > 0);
        assert!(
            value == windows_document_access_snapshot(&file, value.bytes, &mut || Ok(())).unwrap()
        );
        assert!(windows_document_access_snapshot(&file, value.bytes - 1, &mut || Ok(())).is_err());
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
