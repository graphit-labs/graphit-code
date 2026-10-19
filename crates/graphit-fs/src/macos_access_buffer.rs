//! Checked decoding of the SDK's packed attribute response, without pointer casts.
use std::io;

const EXTENDED_SECURITY: u32 = 0x0040_0000;
const UUID: u32 = 0x0080_0000;
const GRPUUID: u32 = 0x0100_0000;
const RETURNED_ATTRS: u32 = 0x8000_0000;
const REQUIRED: u32 = UUID | GRPUUID | RETURNED_ATTRS;
const FRAME: usize = 64;
const GUID_BYTES: usize = 32;
const OVERHEAD: usize = FRAME - GUID_BYTES;
const MAX_RESPONSE: usize = 8192;

// Qualify the packed envelope against the pinned SDK bindings when compiling Mac.
#[cfg(target_os = "macos")]
const _: () = {
    assert!(EXTENDED_SECURITY == libc::ATTR_CMN_EXTENDED_SECURITY);
    assert!(UUID == libc::ATTR_CMN_UUID);
    assert!(GRPUUID == libc::ATTR_CMN_GRPUUID);
    assert!(RETURNED_ATTRS == libc::ATTR_CMN_RETURNED_ATTRS);
    assert!(std::mem::size_of::<libc::attribute_set_t>() == 20);
    assert!(std::mem::size_of::<libc::attrreference_t>() == 8);
};

/// Read-only native data. Equality grants no publication or cleanup authority.
#[derive(PartialEq, Eq)]
pub struct MacAccessSnapshot {
    owner: [u8; 16],
    group: [u8; 16],
    acl: Option<Vec<u8>>,
}

pub(super) struct MacAccessApplication {
    pub(super) bytes: Vec<u8>,
    pub(super) readback_limit: usize,
}

impl MacAccessSnapshot {
    /// Digest exact GUID/opaque ACL data without exposing it or granting authority.
    /// This does not normalize absent, empty or native NOACL representations.
    pub fn fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        digest.update(b"graphit/native-access/macos\0");
        digest.update(self.owner);
        digest.update(self.group);
        match &self.acl {
            None => digest.update([0]),
            Some(acl) => {
                digest.update([1]);
                digest.update(acl);
            }
        }
        digest.finalize().into()
    }

    pub(super) fn prepare_apply(
        &self,
        mode: u32,
        uid: u32,
        gid: u32,
        max_buffer_bytes: usize,
    ) -> io::Result<MacAccessApplication> {
        if mode & !0o7777 != 0 || max_buffer_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let payload_length = if let Some(acl) = self.acl.as_deref() {
            44 + checked_acl_entries(acl)?.len()
        } else {
            44
        };
        let length = 52_usize
            .checked_add(payload_length)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if length > max_buffer_bytes.min(MAX_RESPONSE) {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        bytes.resize(length, 0);
        // Setter wire order: numeric ownership/mode, reference, GUIDs, ACL.
        // Unlike the getter, it has no length word or returned-attribute bitmap.
        bytes[0..4].copy_from_slice(&uid.to_ne_bytes());
        bytes[4..8].copy_from_slice(&gid.to_ne_bytes());
        bytes[8..12].copy_from_slice(&mode.to_ne_bytes());
        bytes[12..16].copy_from_slice(&40_i32.to_ne_bytes());
        bytes[16..20].copy_from_slice(&(payload_length as u32).to_ne_bytes());
        bytes[20..36].copy_from_slice(&self.owner);
        bytes[36..52].copy_from_slice(&self.group);
        if let Some(acl) = self.acl.as_deref() {
            bytes[52..].copy_from_slice(acl);
        } else {
            bytes[52..56].copy_from_slice(&0x012c_c16d_u32.to_ne_bytes());
            bytes[88..92].copy_from_slice(&u32::MAX.to_ne_bytes());
        }
        // This is the logical observation budget, separate from the setter's
        // physical packet cap. A normalization/growth cannot enlarge this read.
        let readback_limit = GUID_BYTES
            .checked_add(self.acl.as_ref().map_or(0, Vec::len))
            .ok_or(io::ErrorKind::FileTooLarge)?;
        Ok(MacAccessApplication {
            bytes,
            readback_limit,
        })
    }

    /// A public parent is admissible when this birth would inherit no ACEs.
    /// XNU kauth_acl_inherit selects DIRECTORY_INHERIT or FILE_INHERIT by kind;
    /// rights, principals and unrelated native flags are not interpreted here.
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn verify_no_inherited_aces(&self, directory: bool) -> io::Result<()> {
        let Some(acl) = self.acl.as_deref() else {
            return Ok(());
        };
        let entries = checked_acl_entries(acl)?;
        let inherited = if directory { 1 << 6 } else { 1 << 5 };
        for entry in entries.as_chunks::<24>().0 {
            if word(entry, 16)? & inherited != 0 {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        Ok(())
    }

    // Only private-object verification normalizes absence and an empty ACL.
    // Read-only observation equality continues to retain every native byte.
    pub(super) fn verify_no_aces(&self) -> io::Result<()> {
        let Some(acl) = self.acl.as_deref() else {
            return Ok(());
        };
        if acl.len() < 44 || word(acl, 0)? != 0x012c_c16d {
            return Err(conflict());
        }
        let count = word(acl, 36)?;
        if count != 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if acl.len() != 44 {
            return Err(conflict());
        }
        // Flags, including filesystem-private bits, stay opaque. XNU can drop
        // the initial NO_INHERIT flag while constructing the resulting ACL.
        Ok(())
    }
}

pub(super) fn qualify_support(value: i64, error: i32) -> io::Result<()> {
    if value == -1 && error != 0 {
        return Err(io::Error::from_raw_os_error(error));
    }
    if value <= 0 {
        return Err(io::ErrorKind::Unsupported.into());
    }
    Ok(())
}

pub(super) fn snapshot(
    limit: usize,
    mut read: impl FnMut(&mut [u8]) -> io::Result<()>,
) -> io::Result<MacAccessSnapshot> {
    if limit < GUID_BYTES {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut query = [0_u8; 4];
    read(&mut query)?;
    let length = u32::from_ne_bytes(query) as usize;
    if length < FRAME {
        return Err(conflict());
    }
    // Clamp before checked addition; even usize::MAX never grows this allocation.
    let ceiling = limit
        .min(MAX_RESPONSE)
        .checked_add(OVERHEAD)
        .ok_or(io::ErrorKind::FileTooLarge)?
        .min(MAX_RESPONSE);
    if length > ceiling {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    bytes.resize(length, 0);
    read(&mut bytes)?;
    decode(bytes, limit)
}

fn decode(mut bytes: Vec<u8>, limit: usize) -> io::Result<MacAccessSnapshot> {
    if bytes.len() < FRAME || word(&bytes, 0)? as usize != bytes.len() {
        return Err(conflict());
    }
    let common = word(&bytes, 4)?;
    if common & REQUIRED != REQUIRED {
        return Err(io::ErrorKind::Unsupported.into());
    }
    if common & !(REQUIRED | EXTENDED_SECURITY) != 0
        || [8, 12, 16, 20]
            .into_iter()
            .any(|offset| word(&bytes, offset).is_ok_and(|value| value != 0))
    {
        return Err(conflict());
    }
    let offset = i32::from_ne_bytes(bytes[24..28].try_into().map_err(|_| conflict())?);
    let start = 24_usize
        .checked_add(usize::try_from(offset).map_err(|_| conflict())?)
        .ok_or_else(conflict)?;
    let length = word(&bytes, 28)? as usize;
    let end = start.checked_add(length).ok_or_else(conflict)?;
    let present = common & EXTENDED_SECURITY != 0;
    // For these fixed attributes XNU packs the single ACL variable at byte64.
    // kauth_filesec/ACE widths are multiples of four, so no trailing padding is
    // needed or adopted. ACL rights, flags and principals remain opaque.
    if start != FRAME || end != bytes.len() || !length.is_multiple_of(4) || present != (length != 0)
    {
        return Err(conflict());
    }
    if GUID_BYTES
        .checked_add(length)
        .ok_or(io::ErrorKind::FileTooLarge)?
        > limit
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let owner = bytes[32..48].try_into().map_err(|_| conflict())?;
    let group = bytes[48..64].try_into().map_err(|_| conflict())?;
    let acl = if present {
        bytes.drain(..FRAME);
        Some(bytes)
    } else {
        None
    };
    Ok(MacAccessSnapshot { owner, group, acl })
}

// Same bounded native filesec/ACE envelope used by access reapplication.
fn checked_acl_entries(acl: &[u8]) -> io::Result<&[u8]> {
    if acl.len() < 44 || word(acl, 0)? != 0x012c_c16d {
        return Err(conflict());
    }
    let count = word(acl, 36)?;
    if count == u32::MAX || count > 128 {
        return Err(io::ErrorKind::Unsupported.into());
    }
    let expected = (count as usize)
        .checked_mul(24)
        .and_then(|size| size.checked_add(44))
        .ok_or_else(conflict)?;
    if acl.len() != expected {
        return Err(conflict());
    }
    Ok(&acl[44..])
}

fn word(bytes: &[u8], offset: usize) -> io::Result<u32> {
    let end = offset.checked_add(4).ok_or_else(conflict)?;
    Ok(u32::from_ne_bytes(
        bytes
            .get(offset..end)
            .ok_or_else(conflict)?
            .try_into()
            .map_err(|_| conflict())?,
    ))
}
fn conflict() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_keeps_guid_acl_presence_flags_and_opaque_bytes_distinct() {
        let base = || MacAccessSnapshot {
            owner: [1; 16],
            group: [2; 16],
            acl: None,
        };
        let expected = base().fingerprint();
        assert_eq!(base().fingerprint(), expected);
        for which in 0..2 {
            let mut value = base();
            if which == 0 {
                value.owner[15] ^= 1;
            } else {
                value.group[0] ^= 1;
            }
            assert_ne!(value.fingerprint(), expected);
        }
        let mut value = base();
        value.acl = Some(Vec::new());
        let empty = value.fingerprint();
        assert_ne!(empty, expected);
        value.acl = Some(vec![0; 44]);
        let zero = value.fingerprint();
        assert_ne!(zero, empty);
        value.acl.as_mut().unwrap()[40] = 1;
        assert_ne!(value.fingerprint(), zero);
        let flags = value.fingerprint();
        value.acl.as_mut().unwrap()[0] = 1;
        assert_ne!(value.fingerprint(), flags);
        // An observed native NOACL marker remains Some opaque data.
        let mut bytes = vec![0; 44];
        bytes[36..40].copy_from_slice(&u32::MAX.to_ne_bytes());
        value.acl = Some(bytes);
        assert_ne!(value.fingerprint(), expected);
    }

    fn set(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }
    fn fixture(payload: usize) -> Vec<u8> {
        let mut bytes = vec![0; FRAME + payload];
        set(&mut bytes, 0, (FRAME + payload) as u32);
        set(
            &mut bytes,
            4,
            REQUIRED | if payload == 0 { 0 } else { EXTENDED_SECURITY },
        );
        set(&mut bytes, 24, (FRAME - 24) as u32);
        set(&mut bytes, 28, payload as u32);
        bytes
    }
    fn observe(bytes: &[u8], limit: usize) -> io::Result<MacAccessSnapshot> {
        snapshot(limit, |buffer| {
            buffer.copy_from_slice(&bytes[..buffer.len()]);
            Ok(())
        })
    }

    fn native_acl(count: u32) -> Vec<u8> {
        let length = if count <= 128 {
            44 + 24 * count as usize
        } else {
            44
        };
        let mut bytes = fixture(length);
        set(&mut bytes, FRAME, 0x012c_c16d);
        set(&mut bytes, FRAME + 36, count);
        bytes
    }
    #[test]
    fn apply_packet_none_is_bounded_delete_command_not_private_acl() {
        let access = observe(&fixture(0), 32).unwrap();
        let packet = access.prepare_apply(0o640, 42, 43, 96).unwrap();
        assert_eq!(packet.bytes.len(), 96);
        assert_eq!(packet.readback_limit, 32);
        for (offset, value) in [
            (0, 42),
            (4, 43),
            (8, 0o640),
            (12, 40),
            (16, 44),
            (52, 0x012c_c16d),
            (88, u32::MAX),
        ] {
            assert_eq!(word(&packet.bytes, offset).unwrap(), value);
        }
        let mut delete = native_acl(u32::MAX);
        delete[FRAME..].copy_from_slice(&packet.bytes[52..]);
        assert!(observe(&delete, 76).unwrap().verify_no_aces().is_err());
        assert_eq!(
            access
                .prepare_apply(0o640, 42, 43, 95)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
        assert_eq!(
            access.prepare_apply(0o640, 42, 43, 0).err().unwrap().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[test]
    fn apply_packet_preserves_all_guid_and_acl_bytes_and_special_mode_bits() {
        let mut bytes = native_acl(1);
        bytes[32..64].fill(0xaa);
        set(&mut bytes, FRAME + 40, 0x8000_ffff);
        bytes[FRAME + 44..].fill(0x55);
        let access = observe(&bytes, 100).unwrap();
        let packet = access
            .prepare_apply(0o7777, u32::MAX, u32::MAX, 120)
            .unwrap();
        assert_eq!(&packet.bytes[20..52], &bytes[32..64]);
        assert_eq!(&packet.bytes[52..], &bytes[FRAME..]);
        assert_eq!(word(&packet.bytes, 8).unwrap(), 0o7777);
        assert_eq!(word(&packet.bytes, 0).unwrap(), u32::MAX);
        assert_eq!(packet.readback_limit, 100);
        assert_eq!(
            access
                .prepare_apply(0o10000, 0, 0, 8192)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[test]
    fn apply_packet_zero_entry_and_absence_remain_distinct() {
        let absent = observe(&fixture(0), 32).unwrap();
        let empty = observe(&native_acl(0), 76).unwrap();
        assert!(empty != absent);
        let empty_packet = empty.prepare_apply(0o600, 1, 2, 96).unwrap();
        assert_eq!(word(&empty_packet.bytes, 88).unwrap(), 0);
        assert_eq!(empty_packet.readback_limit, 76);
        assert_eq!(
            absent
                .prepare_apply(0o600, 1, 2, 96)
                .unwrap()
                .readback_limit,
            32
        );
    }
    #[test]
    fn apply_packet_rejects_sentinel_bad_magic_lengths_and_excess_count() {
        for (bytes, expected) in [
            (native_acl(u32::MAX), io::ErrorKind::Unsupported),
            (native_acl(129), io::ErrorKind::Unsupported),
            (fixture(44), io::ErrorKind::InvalidData),
            (
                native_acl(2)[..FRAME + 44].to_vec(),
                io::ErrorKind::InvalidData,
            ),
        ] {
            let mut bytes = bytes;
            let length = bytes.len();
            set(&mut bytes, 0, length as u32);
            set(&mut bytes, 28, (length - FRAME) as u32);
            let observed = observe(&bytes, 8192).unwrap();
            assert_eq!(
                observed
                    .prepare_apply(0o600, 1, 2, 8192)
                    .err()
                    .unwrap()
                    .kind(),
                expected
            );
        }
    }
    #[test]
    fn apply_packet_maximum_acl_has_exact_admission_and_finite_allocation() {
        let access = observe(&native_acl(128), 3148).unwrap();
        for cap in [3168, usize::MAX] {
            let packet = access.prepare_apply(0o600, 1, 2, cap).unwrap();
            assert_eq!(packet.bytes.len(), 3168);
            assert_eq!(packet.readback_limit, 3148);
        }
        assert_eq!(
            access
                .prepare_apply(0o600, 1, 2, 3167)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
    }

    #[test]
    fn private_predicate_accepts_absence_or_exact_empty_native_acl() {
        let absent = observe(&fixture(0), 32).unwrap();
        assert!(absent.verify_no_aces().is_ok());
        let mut native = fixture(44);
        set(&mut native, FRAME, 0x012c_c16d);
        let empty = observe(&native, 76).unwrap();
        assert!(empty.verify_no_aces().is_ok());
        assert!(absent != empty);
        for flags in [0, 0xffff, 1 << 17, u32::MAX] {
            set(&mut native, FRAME + 40, flags);
            let current = observe(&native, 76).unwrap();
            assert!(current.verify_no_aces().is_ok());
            if flags != 0 {
                assert!(current != empty);
            }
        }
    }

    #[test]
    fn private_predicate_rejects_noacl_sentinel_entries_and_malformed_blobs() {
        for (length, magic, count, kind) in [
            (40, 0x012c_c16d, 0, io::ErrorKind::InvalidData),
            (44, 0, 0, io::ErrorKind::InvalidData),
            (48, 0x012c_c16d, 0, io::ErrorKind::InvalidData),
            (44, 0x012c_c16d, u32::MAX, io::ErrorKind::PermissionDenied),
            (68, 0x012c_c16d, 1, io::ErrorKind::PermissionDenied),
        ] {
            let mut bytes = fixture(length);
            set(&mut bytes, FRAME, magic);
            set(&mut bytes, FRAME + 36, count);
            let value = observe(&bytes, 32 + length).unwrap();
            assert_eq!(value.verify_no_aces().unwrap_err().kind(), kind);
        }
    }

    #[test]
    fn logical_guid_and_payload_quota_excludes_fixed_envelope() {
        assert!(observe(&fixture(0), 32).unwrap().acl.is_none());
        assert!(observe(&fixture(68), 100).unwrap().acl.is_some());
        for (payload, limit) in [(0, 31), (68, 99), (68, 32)] {
            assert_eq!(
                observe(&fixture(payload), limit).err().unwrap().kind(),
                io::ErrorKind::FileTooLarge
            );
        }
        assert!(observe(&fixture(68), usize::MAX).is_ok());
    }
    #[test]
    fn query_admission_precedes_allocation_and_second_read() {
        for (limit, reported, calls, kind) in [
            (31, 64_u32, 0, io::ErrorKind::FileTooLarge),
            (32, 68, 1, io::ErrorKind::FileTooLarge),
            (usize::MAX, 8196, 1, io::ErrorKind::FileTooLarge),
            (usize::MAX, 63, 1, io::ErrorKind::InvalidData),
        ] {
            let mut actual = 0;
            let result = snapshot(limit, |buffer| {
                actual += 1;
                assert_eq!(buffer.len(), 4);
                buffer.copy_from_slice(&reported.to_ne_bytes());
                Ok(())
            });
            assert_eq!(result.err().unwrap().kind(), kind);
            assert_eq!(actual, calls);
        }
    }
    #[test]
    fn changed_second_size_conflicts_without_retry() {
        for second in [64, 136, u32::MAX] {
            let mut calls = 0;
            let result = snapshot(100, |buffer| {
                calls += 1;
                if calls == 1 {
                    buffer.copy_from_slice(&132_u32.to_ne_bytes());
                } else {
                    assert_eq!(buffer.len(), 132);
                    buffer.copy_from_slice(&fixture(68));
                    set(buffer, 0, second);
                }
                Ok(())
            });
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::InvalidData);
            assert_eq!(calls, 2);
        }
    }
    #[test]
    fn native_errors_are_preserved_in_query_and_second_call() {
        for failing in [1, 2] {
            let mut calls = 0;
            let result = snapshot(32, |buffer| {
                calls += 1;
                if calls == failing {
                    return Err(io::Error::from_raw_os_error(13));
                }
                buffer.copy_from_slice(&64_u32.to_ne_bytes());
                Ok(())
            });
            assert_eq!(result.err().unwrap().raw_os_error(), Some(13));
            assert_eq!(calls, failing);
        }
    }
    #[test]
    fn supported_absence_is_distinct_from_unknown_or_failed_support() {
        for value in [1, 2, i64::MAX] {
            assert!(qualify_support(value, 0).is_ok());
        }
        for value in [-1, 0, -2] {
            assert_eq!(
                qualify_support(value, 0).unwrap_err().kind(),
                io::ErrorKind::Unsupported
            );
        }
        assert_eq!(
            qualify_support(-1, 13).unwrap_err().raw_os_error(),
            Some(13)
        );
    }
    #[test]
    fn required_bitmap_rejects_missing_or_unrequested_attributes() {
        for missing in [UUID, GRPUUID, RETURNED_ATTRS] {
            let mut bytes = fixture(0);
            set(&mut bytes, 4, REQUIRED & !missing);
            assert_eq!(
                decode(bytes, 32).err().unwrap().kind(),
                io::ErrorKind::Unsupported
            );
        }
        for (offset, value) in [(4, REQUIRED | 1), (8, 1), (12, 1), (16, 1), (20, 1)] {
            let mut bytes = fixture(0);
            set(&mut bytes, offset, value);
            assert_eq!(
                decode(bytes, 32).err().unwrap().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
    #[test]
    fn references_reject_truncation_overlap_overflow_and_padding() {
        for length in [0, 3, 23, 31, 63] {
            assert!(decode(fixture(68)[..length].to_vec(), 100).is_err());
        }
        for (offset, value) in [
            (24, u32::MAX),
            (24, i32::MAX as u32),
            (24, 0),
            (24, 39),
            (24, 41),
            (28, u32::MAX),
            (28, 64),
            (28, 69),
        ] {
            let mut bytes = fixture(68);
            set(&mut bytes, offset, value);
            assert_eq!(
                decode(bytes, usize::MAX).err().unwrap().kind(),
                io::ErrorKind::InvalidData
            );
        }
        let mut trailing = fixture(68);
        trailing.push(0);
        set(&mut trailing, 0, 133);
        assert!(decode(trailing, 101).is_err());
        // Misaligned variable widths are never adopted as padded ACL data.
        assert!(decode(fixture(67), 99).is_err());
    }
    #[test]
    fn acl_presence_and_returned_bit_must_agree() {
        let mut absent = fixture(0);
        set(&mut absent, 4, REQUIRED | EXTENDED_SECURITY);
        let mut present = fixture(68);
        set(&mut present, 4, REQUIRED);
        for bytes in [absent, present] {
            assert_eq!(
                decode(bytes, 100).err().unwrap().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
    #[test]
    fn equality_preserves_null_and_actual_guids_and_all_payload_bits() {
        let original = fixture(68);
        let observed = observe(&original, 100).unwrap();
        assert!(observed.owner == [0; 16] && observed.group == [0; 16]);
        assert!(observed == observe(&original, 100).unwrap());
        for position in [32, 47, 48, 63, 64, 104, 131] {
            let mut changed = original.clone();
            changed[position] = 255;
            assert!(observed != observe(&changed, 100).unwrap());
        }
        assert!(observed != observe(&fixture(0), 32).unwrap());
    }
    #[test]
    fn parent_admission_uses_child_inheritance_flags_without_requiring_private_parent() {
        assert!(
            observe(&fixture(0), 32)
                .unwrap()
                .verify_no_inherited_aces(true)
                .is_ok()
        );
        for flags in [
            0,
            1 << 4,
            1 << 7,
            1 << 8,
            1 << 5,
            1 << 6,
            (1 << 5) | (1 << 6),
        ] {
            let mut bytes = fixture(68);
            set(&mut bytes, FRAME, 0x012c_c16d);
            set(&mut bytes, FRAME + 36, 1);
            set(&mut bytes, FRAME + 44 + 16, flags | 1);
            let parent = observe(&bytes, 100).unwrap();
            assert_eq!(
                parent.verify_no_inherited_aces(true).is_err(),
                flags & (1 << 6) != 0
            );
            assert_eq!(
                parent.verify_no_inherited_aces(false).is_err(),
                flags & (1 << 5) != 0
            );
            assert!(parent.verify_no_aces().is_err());
        }
        for (length, count) in [(44, 1), (68, 0), (44, u32::MAX), (44, 129)] {
            let mut bytes = fixture(length);
            set(&mut bytes, FRAME, 0x012c_c16d);
            set(&mut bytes, FRAME + 36, count);
            assert!(
                observe(&bytes, 32 + length)
                    .unwrap()
                    .verify_no_inherited_aces(true)
                    .is_err()
            );
        }
    }
}
