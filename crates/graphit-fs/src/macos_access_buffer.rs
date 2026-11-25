//! Checked decoding of the SDK's packed attribute response, without pointer casts.
use std::io;

const EXTENDED_SECURITY: u32 = 0x0040_0000;
const RETURNED_ATTRS: u32 = 0x8000_0000;
const REQUIRED: u32 = RETURNED_ATTRS;
// uint32 length, attribute_set_t, then the sole attrreference_t (Apple getattrlist(2)).
const FRAME: usize = 32;
const MAX_RESPONSE: usize = 8192;

// Qualify the packed envelope against the pinned SDK bindings when compiling Mac.
#[cfg(target_os = "macos")]
const _: () = {
    assert!(EXTENDED_SECURITY == libc::ATTR_CMN_EXTENDED_SECURITY);
    assert!(RETURNED_ATTRS == libc::ATTR_CMN_RETURNED_ATTRS);
    assert!(std::mem::size_of::<libc::attribute_set_t>() == 20);
    assert!(std::mem::size_of::<libc::attrreference_t>() == 8);
    assert!(
        FRAME
            == 4 + std::mem::size_of::<libc::attribute_set_t>()
                + std::mem::size_of::<libc::attrreference_t>()
    );
};

/// Bounded ACL data used only to verify private objects.
pub(crate) struct MacAccessSnapshot {
    acl: Option<Vec<u8>>,
}

impl MacAccessSnapshot {
    // Private objects may have no ACL or an empty native ACL.
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
    let mut query = [0_u8; 4];
    read(&mut query)?;
    let length = u32::from_ne_bytes(query) as usize;
    if length < FRAME {
        return Err(conflict());
    }
    // Clamp before checked addition; even usize::MAX never grows this allocation.
    let ceiling = limit
        .min(MAX_RESPONSE)
        .checked_add(FRAME)
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
    // For these fixed attributes XNU packs the single ACL variable at byte32.
    // kauth_filesec/ACE widths are multiples of four, so no trailing padding is
    // needed or adopted. ACL rights, flags and principals remain opaque.
    if start != FRAME || end != bytes.len() || !length.is_multiple_of(4) || present != (length != 0)
    {
        return Err(conflict());
    }
    if length > limit {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let acl = if present {
        bytes.drain(..FRAME);
        Some(bytes)
    } else {
        None
    };
    Ok(MacAccessSnapshot { acl })
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

    // Apple ABI: length + returned attribute_set_t + one attrreference_t.
    const HEADER_BYTES: usize = 32;

    fn set(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }
    fn fixture(payload: usize) -> Vec<u8> {
        let mut bytes = vec![0; HEADER_BYTES + payload];
        set(&mut bytes, 0, (HEADER_BYTES + payload) as u32);
        set(
            &mut bytes,
            4,
            REQUIRED | if payload == 0 { 0 } else { EXTENDED_SECURITY },
        );
        set(&mut bytes, 24, (HEADER_BYTES - 24) as u32);
        set(&mut bytes, 28, payload as u32);
        bytes
    }
    fn observe(bytes: &[u8], limit: usize) -> io::Result<MacAccessSnapshot> {
        snapshot(limit, |buffer| {
            buffer.copy_from_slice(&bytes[..buffer.len()]);
            Ok(())
        })
    }

    #[test]
    fn private_predicate_accepts_absence_or_exact_empty_native_acl() {
        let absent = observe(&fixture(0), 0).unwrap();
        assert!(absent.verify_no_aces().is_ok());
        let mut native = fixture(44);
        set(&mut native, HEADER_BYTES, 0x012c_c16d);
        let empty = observe(&native, 44).unwrap();
        assert!(empty.verify_no_aces().is_ok());
        for flags in [0, 0xffff, 1 << 17, u32::MAX] {
            set(&mut native, HEADER_BYTES + 40, flags);
            let current = observe(&native, 44).unwrap();
            assert!(current.verify_no_aces().is_ok());
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
            set(&mut bytes, HEADER_BYTES, magic);
            set(&mut bytes, HEADER_BYTES + 36, count);
            let value = observe(&bytes, length).unwrap();
            assert_eq!(value.verify_no_aces().unwrap_err().kind(), kind);
        }
    }

    #[test]
    fn acl_payload_quota_excludes_fixed_envelope() {
        assert!(observe(&fixture(0), 0).unwrap().acl.is_none());
        assert!(observe(&fixture(68), 68).unwrap().acl.is_some());
        for (payload, limit) in [(68, 67), (68, 0), (8164, usize::MAX)] {
            assert_eq!(
                observe(&fixture(payload), limit).err().unwrap().kind(),
                io::ErrorKind::FileTooLarge
            );
        }
        assert!(observe(&fixture(68), usize::MAX).is_ok());
        assert!(observe(&fixture(8160), 8160).is_ok());
    }
    #[test]
    fn query_admission_precedes_allocation_and_second_read() {
        for (limit, reported, calls, kind) in [
            (0, 36_u32, 1, io::ErrorKind::FileTooLarge),
            (usize::MAX, 8196, 1, io::ErrorKind::FileTooLarge),
            (usize::MAX, 31, 1, io::ErrorKind::InvalidData),
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
        for second in [32, 104, u32::MAX] {
            let mut calls = 0;
            let result = snapshot(68, |buffer| {
                calls += 1;
                if calls == 1 {
                    buffer.copy_from_slice(&100_u32.to_ne_bytes());
                } else {
                    assert_eq!(buffer.len(), 100);
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
            let result = snapshot(0, |buffer| {
                calls += 1;
                if calls == failing {
                    return Err(io::Error::from_raw_os_error(13));
                }
                buffer.copy_from_slice(&32_u32.to_ne_bytes());
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
        let mut bytes = fixture(0);
        set(&mut bytes, 4, 0);
        assert_eq!(
            decode(bytes, 0).err().unwrap().kind(),
            io::ErrorKind::Unsupported
        );
        for (offset, value) in [
            (4, REQUIRED | 1),
            (4, REQUIRED | 0x0080_0000),
            (4, REQUIRED | 0x0100_0000),
            (8, 1),
            (12, 1),
            (16, 1),
            (20, 1),
        ] {
            let mut bytes = fixture(0);
            set(&mut bytes, offset, value);
            assert_eq!(
                decode(bytes, 0).err().unwrap().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
    #[test]
    fn references_reject_truncation_overlap_overflow_and_padding() {
        for length in [0, 3, 23, 31, 63] {
            assert!(decode(fixture(68)[..length].to_vec(), 68).is_err());
        }
        for (offset, value) in [
            (24, u32::MAX),
            (24, i32::MAX as u32),
            (24, 0),
            (24, 7),
            (24, 9),
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
        set(&mut trailing, 0, 101);
        assert!(decode(trailing, 69).is_err());
        // Misaligned variable widths are never adopted as padded ACL data.
        assert!(decode(fixture(67), 67).is_err());
    }
    #[test]
    fn acl_presence_and_returned_bit_must_agree() {
        let mut absent = fixture(0);
        set(&mut absent, 4, REQUIRED | EXTENDED_SECURITY);
        let mut present = fixture(68);
        set(&mut present, 4, REQUIRED);
        for bytes in [absent, present] {
            assert_eq!(
                decode(bytes, 68).err().unwrap().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
}
