//! Descriptor-bound Linux POSIX metadata and opaque access ACLs.
use rustix::{
    fs::{Gid, Mode, Uid, XattrFlags, fchmod, fchown, fgetxattr, fremovexattr, fsetxattr},
    io::Errno,
};
use std::{fs::File, io, os::unix::fs::MetadataExt};

const ACL_NAME: &str = "system.posix_acl_access";
const POSIX_BYTES: usize = 12;
const MAX_ACL_BYTES: usize = 64 * 1024;

#[derive(PartialEq, Eq)]
struct PosixAccess {
    mode: u32,
    uid: u32,
    gid: u32,
}

/// Observed access, without publication, adoption or cleanup authority.
/// Opaque ACL bytes and absence remain distinct. No serialization or raw access.
///
/// ```compile_fail
/// use graphit_fs::LinuxAccessSnapshot;
/// fn dump(access: LinuxAccessSnapshot) { println!("{access:?}"); }
/// ```
///
/// ```compile_fail
/// use graphit_fs::LinuxAccessSnapshot;
/// let _ = LinuxAccessSnapshot { posix: (), acl: None };
/// ```
#[derive(PartialEq, Eq)]
pub struct LinuxAccessSnapshot {
    posix: PosixAccess,
    acl: Option<Vec<u8>>,
}

impl LinuxAccessSnapshot {
    /// Logical retained ACL length; this exposes neither contents nor authority.
    pub fn retained_acl_bytes(&self) -> usize {
        self.acl.as_ref().map_or(0, Vec::len)
    }

    /// Digest exact observed access data; this grants no filesystem authority.
    /// Absence and a present ACL, including an empty one, remain distinct.
    pub fn fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        digest.update(b"graphit/native-access/linux\0");
        digest.update(self.posix.mode.to_le_bytes());
        digest.update(self.posix.uid.to_le_bytes());
        digest.update(self.posix.gid.to_le_bytes());
        match &self.acl {
            None => digest.update([0]),
            Some(acl) => {
                digest.update([1]);
                digest.update(acl);
            }
        }
        digest.finalize().into()
    }
}

/// Capture regular, singly linked file access through its held descriptor.
/// `max_access_bytes` bounds twelve POSIX bytes plus retained ACL bytes. The
/// kernel's work is not interruptible by the caller.
pub fn linux_access_snapshot(
    file: &File,
    max_access_bytes: usize,
) -> io::Result<LinuxAccessSnapshot> {
    linux_access_snapshot_checked(file, max_access_bytes, &mut || Ok(()))
}

/// Capture access while checking cancellation or a deadline around native calls.
/// The access limit bounds retained ACL bytes before allocation; changes between
/// the length query and read are rejected without retry or buffer growth.
pub fn linux_access_snapshot_checked(
    file: &File,
    max_access_bytes: usize,
    check: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<LinuxAccessSnapshot> {
    let limit = acl_limit(max_access_bytes)?;
    check()?;
    let posix = posix_access(file)?;
    let acl = read_acl_checked(file, limit, check)?;
    check()?;
    if posix_access(file).map_err(observed_change)? != posix {
        return Err(io::ErrorKind::InvalidData.into());
    }
    check()?;
    Ok(LinuxAccessSnapshot { posix, acl })
}

/// Maximum retained ACL storage for an already validated access limit.
/// This is a logical allocation bound, not an authority or kernel-work grant.
pub fn linux_access_storage_limit(max_access_bytes: usize) -> io::Result<usize> {
    acl_limit(max_access_bytes)
}

/// Reapply captured access through the held descriptor and verify exact readback.
/// Admission precedes setters. Ownership changes precede ACL changes; complete
/// mode bits are restored last because chown can clear special bits. Any setter
/// or readback error may leave metadata changed. There is no retry or rollback.
/// The caller must establish stage origin and publication authority separately.
pub fn linux_apply_access(
    file: &File,
    access: &LinuxAccessSnapshot,
    max_access_bytes: usize,
) -> io::Result<()> {
    let limit = acl_limit(max_access_bytes)?;
    let length = access.acl.as_ref().map_or(0, Vec::len);
    if length > limit {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    if access.posix.mode & !0o7777 != 0
        || access.posix.uid == u32::MAX
        || access.posix.gid == u32::MAX
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let before = posix_access(file)?;
    let uid = (before.uid != access.posix.uid).then(|| Uid::from_raw(access.posix.uid));
    let gid = (before.gid != access.posix.gid).then(|| Gid::from_raw(access.posix.gid));
    if uid.is_some() || gid.is_some() {
        fchown(file, uid, gid)?;
    }
    match &access.acl {
        Some(acl) => fsetxattr(file, ACL_NAME, acl, XattrFlags::empty())?,
        None => match fremovexattr(file, ACL_NAME) {
            Ok(()) | Err(Errno::NODATA) => {}
            Err(error) => return Err(error.into()),
        },
    }
    fchmod(file, Mode::from_raw_mode(access.posix.mode))?;
    #[cfg(test)]
    test_hooks::run(&test_hooks::AFTER_APPLY);
    if linux_access_snapshot(file, POSIX_BYTES + length).map_err(observed_change)? != *access {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(())
}

fn acl_limit(max_access_bytes: usize) -> io::Result<usize> {
    if max_access_bytes == 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    max_access_bytes
        .checked_sub(POSIX_BYTES)
        .map(|bytes| bytes.min(MAX_ACL_BYTES))
        .ok_or_else(|| io::ErrorKind::FileTooLarge.into())
}

fn posix_access(file: &File) -> io::Result<PosixAccess> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(PosixAccess {
        mode: metadata.mode() & 0o7777,
        uid: metadata.uid(),
        gid: metadata.gid(),
    })
}

fn observed_change(error: io::Error) -> io::Error {
    if error.kind() == io::ErrorKind::InvalidInput {
        io::ErrorKind::InvalidData.into()
    } else {
        error
    }
}

fn read_acl_checked(
    file: &File,
    limit: usize,
    check: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<Option<Vec<u8>>> {
    let mut query: [u8; 0] = [];
    check()?;
    let length = match fgetxattr(file, ACL_NAME, query.as_mut_slice()) {
        Ok(length) => length,
        Err(Errno::NODATA) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if length > limit {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    check()?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    bytes.resize(length, 0);
    #[cfg(test)]
    test_hooks::run(&test_hooks::AFTER_QUERY);
    check()?;
    match fgetxattr(file, ACL_NAME, bytes.as_mut_slice()) {
        Ok(read) => {
            check()?;
            if read == length {
                Ok(Some(bytes))
            } else {
                Err(io::ErrorKind::InvalidData.into())
            }
        }
        Err(Errno::NODATA | Errno::RANGE) => Err(io::ErrorKind::InvalidData.into()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod test_hooks {
    use std::{cell::RefCell, thread::LocalKey};
    type Hook = RefCell<Option<Box<dyn FnOnce()>>>;
    thread_local! {
        pub(super) static AFTER_QUERY: Hook = const { RefCell::new(None) };
        pub(super) static AFTER_APPLY: Hook = const { RefCell::new(None) };
    }
    pub(super) fn run(hook: &'static LocalKey<Hook>) {
        if let Some(action) = hook.with(|hook| hook.borrow_mut().take()) {
            action();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    fn fixture() -> (tempfile::TempDir, File, File) {
        let tmp = tempfile::tempdir().unwrap();
        let source = crate::create_private_file(&tmp.path().join("source")).unwrap();
        let target = crate::create_private_file(&tmp.path().join("target")).unwrap();
        (tmp, source, target)
    }

    fn named_acls(file: &File, principals: &[u32]) {
        // Test-only documented Linux ACL xattr ABI. Production never parses it.
        let mut bytes = 2u32.to_le_bytes().to_vec();
        let entries = [(1u16, 6u16, u32::MAX)]
            .into_iter()
            .chain(principals.iter().map(|&id| (2, 4, id)))
            .chain([(4, 4, u32::MAX), (16, 4, u32::MAX), (32, 0, u32::MAX)]);
        for (tag, permissions, id) in entries {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&permissions.to_le_bytes());
            bytes.extend_from_slice(&id.to_le_bytes());
        }
        fsetxattr(file, ACL_NAME, &bytes, XattrFlags::empty()).unwrap();
    }

    #[test]
    fn checked_snapshot_uses_the_same_observation_and_denies_each_prior_call() {
        let (_tmp, source, _target) = fixture();
        named_acls(&source, &[424242]);
        let expected = linux_access_snapshot(&source, 56).unwrap();
        let mut calls = 0;
        let observed = linux_access_snapshot_checked(&source, 56, &mut || {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert!(observed == expected);
        assert!(calls >= 6);
        for allowed in 0..calls {
            let mut seen = 0;
            let error = linux_access_snapshot_checked(&source, 56, &mut || {
                if seen == allowed {
                    return Err(io::ErrorKind::Interrupted.into());
                }
                seen += 1;
                Ok(())
            })
            .err()
            .unwrap();
            assert_eq!(error.kind(), io::ErrorKind::Interrupted);
            assert_eq!(seen, allowed);
            assert!(linux_access_snapshot(&source, 56).unwrap() == expected);
        }
        assert_eq!(linux_access_storage_limit(56).unwrap(), 44);
        assert_eq!(linux_access_storage_limit(usize::MAX).unwrap(), 65_536);
        assert!(linux_access_storage_limit(11).is_err());
    }

    #[test]
    fn checked_snapshot_preserves_acl_limit_and_absence() {
        let (_tmp, source, target) = fixture();
        named_acls(&source, &[424242]);
        let expected = linux_access_snapshot(&source, 56).unwrap();
        assert!(linux_access_snapshot_checked(&source, 56, &mut || Ok(())).unwrap() == expected);
        assert_eq!(
            linux_access_snapshot_checked(&source, 55, &mut || Ok(()))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
        assert_eq!(
            linux_access_snapshot_checked(&target, 12, &mut || Ok(()))
                .unwrap()
                .retained_acl_bytes(),
            0
        );
    }

    #[test]
    fn checked_short_acl_read_rejects_changed_length() {
        let (_tmp, source, _target) = fixture();
        named_acls(&source, &[424242, 424243]);
        let other = source.try_clone().unwrap();
        test_hooks::AFTER_QUERY.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || named_acls(&other, &[424242])));
        });
        let error = linux_access_snapshot_checked(&source, 64, &mut || Ok(()))
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn fingerprint_distinguishes_exact_opaque_data_without_normalization() {
        let base = || LinuxAccessSnapshot {
            posix: PosixAccess {
                mode: 0o640,
                uid: 42,
                gid: 43,
            },
            acl: None,
        };
        let expected = base().fingerprint();
        assert_eq!(base().fingerprint(), expected);
        let mut value = base();
        value.acl = Some(Vec::new());
        assert_ne!(value.fingerprint(), expected);
        let present_empty = value.fingerprint();
        value.acl = Some(vec![0]);
        assert_ne!(value.fingerprint(), present_empty);
        let present_zero = value.fingerprint();
        value.acl = Some(vec![1]);
        assert_ne!(value.fingerprint(), present_zero);
        for field in 0..3 {
            let mut value = base();
            match field {
                0 => value.posix.mode ^= 0o1000,
                1 => value.posix.uid += 1,
                _ => value.posix.gid += 1,
            }
            assert_ne!(value.fingerprint(), expected);
        }
    }

    #[test]
    fn fingerprint_tracks_actual_acl_principals_and_exact_applied_access() {
        let (_tmp, source, target) = fixture();
        named_acls(&source, &[424242]);
        let before = linux_access_snapshot(&source, 56).unwrap();
        linux_apply_access(&target, &before, 56).unwrap();
        assert_eq!(
            linux_access_snapshot(&target, 56).unwrap().fingerprint(),
            before.fingerprint()
        );
        let mode_before = source.metadata().unwrap().mode();
        named_acls(&source, &[424243]);
        assert_eq!(source.metadata().unwrap().mode(), mode_before);
        let after = linux_access_snapshot(&source, 56).unwrap();
        assert_ne!(after.fingerprint(), before.fingerprint());
        assert_eq!(
            linux_access_snapshot(&target, 56).unwrap().fingerprint(),
            before.fingerprint()
        );
    }

    #[test]
    fn observed_acl_and_special_mode_roundtrip_to_a_distinct_file() {
        let (_tmp, source, target) = fixture();
        named_acls(&source, &[424242, 424243]);
        source
            .set_permissions(fs::Permissions::from_mode(0o7640))
            .unwrap();
        let expected = linux_access_snapshot(&source, 64).unwrap();
        assert_eq!(expected.posix.mode, 0o7640);
        assert_ne!(
            source.metadata().unwrap().ino(),
            target.metadata().unwrap().ino()
        );
        linux_apply_access(&target, &expected, 64).unwrap();
        assert!(linux_access_snapshot(&target, 64).unwrap() == expected);
        assert_eq!(fs::read(_tmp.path().join("target")).unwrap(), b"");
    }

    #[test]
    fn absent_acl_removes_present_acl_and_preserves_absence() {
        let (_tmp, source, target) = fixture();
        let expected = linux_access_snapshot(&source, 12).unwrap();
        assert!(expected.acl.is_none());
        named_acls(&target, &[424242]);
        linux_apply_access(&target, &expected, 12).unwrap();
        assert!(linux_access_snapshot(&target, 12).unwrap() == expected);
        linux_apply_access(&target, &expected, 12).unwrap();
        assert!(linux_access_snapshot(&target, 12).unwrap() == expected);
    }

    #[test]
    fn quotas_reject_before_effect_and_exact_limits_pass() {
        let (_tmp, source, target) = fixture();
        named_acls(&source, &[424242]);
        let expected = linux_access_snapshot(&source, 56).unwrap();
        let before = linux_access_snapshot(&target, 12).unwrap();
        for (limit, kind) in [
            (55, io::ErrorKind::FileTooLarge),
            (11, io::ErrorKind::FileTooLarge),
            (0, io::ErrorKind::InvalidInput),
        ] {
            assert_eq!(
                linux_apply_access(&target, &expected, limit)
                    .unwrap_err()
                    .kind(),
                kind
            );
            assert!(linux_access_snapshot(&target, 12).unwrap() == before);
        }
        assert_eq!(
            linux_access_snapshot(&source, 55).err().unwrap().kind(),
            io::ErrorKind::FileTooLarge
        );
        linux_apply_access(&target, &expected, 56).unwrap();
        assert!(linux_access_snapshot(&target, 56).unwrap() == expected);
        assert_eq!(acl_limit(usize::MAX).unwrap(), MAX_ACL_BYTES);
    }

    #[test]
    fn descriptor_remains_the_target_after_path_replacement() {
        let (tmp, source, target) = fixture();
        named_acls(&source, &[424242]);
        let expected = linux_access_snapshot(&source, 56).unwrap();
        fs::rename(tmp.path().join("target"), tmp.path().join("retained")).unwrap();
        let replacement = crate::create_private_file(&tmp.path().join("target")).unwrap();
        let replacement_access = linux_access_snapshot(&replacement, 12).unwrap();
        linux_apply_access(&target, &expected, 56).unwrap();
        assert!(
            linux_access_snapshot(&File::open(tmp.path().join("retained")).unwrap(), 56).unwrap()
                == expected
        );
        assert!(linux_access_snapshot(&replacement, 12).unwrap() == replacement_access);
    }

    #[test]
    fn hardlinks_and_directories_are_rejected_before_setters() {
        let (tmp, source, target) = fixture();
        let expected = linux_access_snapshot(&source, 12).unwrap();
        fs::hard_link(tmp.path().join("target"), tmp.path().join("alias")).unwrap();
        assert_eq!(
            linux_apply_access(&target, &expected, 12)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(target.metadata().unwrap().mode() & 0o7777, 0o600);
        let directory = File::open(tmp.path()).unwrap();
        let before = directory.metadata().unwrap().mode();
        assert_eq!(
            linux_apply_access(&directory, &expected, 12)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(directory.metadata().unwrap().mode(), before);
    }

    #[test]
    fn growth_shrink_and_removal_between_query_and_read_conflict() {
        for change in 0..3 {
            let (_tmp, source, _target) = fixture();
            named_acls(
                &source,
                if change == 1 {
                    &[424242, 424243]
                } else {
                    &[424242]
                },
            );
            let other = source.try_clone().unwrap();
            test_hooks::AFTER_QUERY.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || match change {
                    0 => named_acls(&other, &[424242, 424243]),
                    1 => named_acls(&other, &[424242]),
                    _ => fremovexattr(&other, ACL_NAME).unwrap(),
                }))
            });
            assert_eq!(
                linux_access_snapshot(&source, 64).err().unwrap().kind(),
                io::ErrorKind::InvalidData
            );
            assert_eq!(source.metadata().unwrap().len(), 0);
        }
    }

    #[test]
    fn hardlink_created_during_query_is_a_concurrent_conflict() {
        let (tmp, source, _target) = fixture();
        named_acls(&source, &[424242]);
        let source_path = tmp.path().join("source");
        let alias = tmp.path().join("new-link");
        test_hooks::AFTER_QUERY.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || fs::hard_link(source_path, alias).unwrap()));
        });
        assert_eq!(
            linux_access_snapshot(&source, 56).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(source.metadata().unwrap().nlink(), 2);
    }

    #[test]
    fn mode_only_change_during_query_conflicts_with_unchanged_acl_bytes() {
        let (_tmp, source, _target) = fixture();
        named_acls(&source, &[424242]);
        let before = linux_access_snapshot(&source, 56).unwrap();
        let other = source.try_clone().unwrap();
        test_hooks::AFTER_QUERY.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fchmod(&other, Mode::from_raw_mode(0o1640)).unwrap();
            }));
        });
        assert_eq!(
            linux_access_snapshot(&source, 56).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        let after = linux_access_snapshot(&source, 56).unwrap();
        assert_eq!(after.acl, before.acl);
        assert_eq!(after.posix.mode, 0o1640);
        assert_ne!(after.posix.mode, before.posix.mode);
    }

    #[test]
    fn oversized_private_payload_is_rejected_before_metadata_effect() {
        let (_tmp, source, target) = fixture();
        let before = linux_access_snapshot(&target, 12).unwrap();
        let mut expected = linux_access_snapshot(&source, 12).unwrap();
        expected.acl = Some(vec![0; MAX_ACL_BYTES + 1]);
        assert_eq!(
            linux_apply_access(&target, &expected, usize::MAX)
                .unwrap_err()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
        assert!(linux_access_snapshot(&target, 12).unwrap() == before);
    }

    #[test]
    fn readback_mismatch_reports_error_with_metadata_effect_remaining() {
        let (_tmp, source, target) = fixture();
        named_acls(&source, &[424242]);
        let expected = linux_access_snapshot(&source, 56).unwrap();
        let other = target.try_clone().unwrap();
        test_hooks::AFTER_APPLY.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || named_acls(&other, &[424243])))
        });
        assert_eq!(
            linux_apply_access(&target, &expected, 56)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let after = linux_access_snapshot(&target, 56).unwrap();
        assert!(after != expected);
        assert!(after.acl.is_some());
    }

    #[test]
    fn path_only_descriptor_preserves_native_setter_error() {
        let (tmp, source, _target) = fixture();
        let expected = linux_access_snapshot(&source, 12).unwrap();
        let fd = rustix::fs::open(
            tmp.path().join("target"),
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::CLOEXEC,
            Mode::empty(),
        )
        .unwrap();
        let file = File::from(fd);
        assert!(file.metadata().unwrap().is_file());
        assert_eq!(
            linux_apply_access(&file, &expected, 12)
                .unwrap_err()
                .raw_os_error(),
            Some(Errno::BADF.raw_os_error())
        );
    }

    #[test]
    fn reserved_identity_and_invalid_mode_are_rejected_before_effect() {
        let (_tmp, source, target) = fixture();
        let before = linux_access_snapshot(&target, 12).unwrap();
        for field in 0..3 {
            let mut expected = linux_access_snapshot(&source, 12).unwrap();
            match field {
                0 => expected.posix.uid = u32::MAX,
                1 => expected.posix.gid = u32::MAX,
                _ => expected.posix.mode = 0o10000,
            }
            assert_eq!(
                linux_apply_access(&target, &expected, 12)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
            assert!(linux_access_snapshot(&target, 12).unwrap() == before);
        }
    }

    #[test]
    fn unsupported_filesystem_error_is_not_absence() {
        let file = File::open("/proc/self/status").unwrap();
        let direct = fgetxattr(&file, ACL_NAME, &mut [0u8; 0][..]).unwrap_err();
        assert_eq!(direct, Errno::OPNOTSUPP);
        assert_eq!(
            linux_access_snapshot(&file, 12)
                .err()
                .unwrap()
                .raw_os_error(),
            Some(direct.raw_os_error())
        );
    }
}
