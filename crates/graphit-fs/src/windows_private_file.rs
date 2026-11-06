//! Exact SDK interop: owned descriptors, lossless paths and private handle adoption.
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io,
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{FromRawHandle, OwnedHandle},
    },
    path::{Component, Path},
    ptr,
};
use windows_permissions::{
    Acl, LocalBox, SecurityDescriptor,
    constants::{AccessRights, AceFlags, AceType, SeObjectType, SecurityInformation},
    wrappers::{ConvertSecurityDescriptorToStringSecurityDescriptor, GetSecurityInfo, IsValidAcl},
};
use windows_sys::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
    Security::{
        ACE_HEADER, GetAce, GetSecurityDescriptorControl, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::{
        CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ, FILE_SHARE_WRITE, ReplaceFileW,
    },
};

struct WidePath {
    units: Vec<u16>,
}
impl WidePath {
    fn new(path: &Path) -> io::Result<Self> {
        use std::path::Prefix;
        if !path.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let prefix = match path.components().next() {
            Some(Component::Prefix(p)) => p.kind(),
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        };
        if !matches!(
            prefix,
            Prefix::Disk(_)
                | Prefix::VerbatimDisk(_)
                | Prefix::UNC(_, _)
                | Prefix::VerbatimUNC(_, _)
        ) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        for component in path.components() {
            if matches!(component, Component::ParentDir | Component::CurDir) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            if let Component::Normal(name) = component
                && name.encode_wide().any(|u| u == u16::from(b':'))
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        let count = path.as_os_str().encode_wide().count();
        let extra = match prefix {
            Prefix::Disk(_) => 4,
            Prefix::UNC(_, _) => 6,
            _ => 0,
        };
        let capacity = count
            .checked_add(extra)
            .and_then(|n| n.checked_add(1))
            .ok_or(io::ErrorKind::InvalidInput)?;
        if capacity
            .checked_mul(2)
            .is_none_or(|n| n > super::MAX_PATH_BYTES)
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut units = Vec::with_capacity(capacity);
        // Extended paths avoid MAX_PATH and preserve native UTF-16 without normalization.
        match prefix {
            Prefix::Disk(_) => {
                units.extend([92, 92, 63, 92]);
                units.extend(path.as_os_str().encode_wide());
            }
            Prefix::UNC(_, _) => {
                units.extend([92, 92, 63, 92, 85, 78, 67, 92]);
                units.extend(path.as_os_str().encode_wide().skip(2));
            }
            _ => units.extend(path.as_os_str().encode_wide()),
        }
        for unit in &mut units {
            if *unit == u16::from(b'/') {
                *unit = u16::from(b'\\');
            }
        }
        if units
            .split(|u| *u == u16::from(b'\\'))
            .any(|part| part == [46] || part == [46, 46])
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if units
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_mul(2))
            .is_none_or(|n| n > super::MAX_PATH_BYTES)
            || units.contains(&0)
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        units.push(0);
        Ok(Self { units })
    }
}

pub(super) fn private_descriptor() -> io::Result<LocalBox<SecurityDescriptor>> {
    private_descriptor_with_flags("")
}

fn private_descriptor_with_flags(flags: &str) -> io::Result<LocalBox<SecurityDescriptor>> {
    use winsafe::{HPROCESS, TokenInfo, co};
    let token = HPROCESS::GetCurrentProcess()
        .OpenProcessToken(co::TOKEN::QUERY)
        .map_err(|e| io::Error::from_raw_os_error(e.raw() as i32))?;
    let info = token
        .GetTokenInformation(co::TOKEN_INFORMATION_CLASS::User)
        .map_err(|e| io::Error::from_raw_os_error(e.raw() as i32))?;
    let TokenInfo::User(user) = info else {
        return Err(io::ErrorKind::InvalidData.into());
    };
    let sid = winsafe::ConvertSidToStringSid(user.User.Sid().ok_or(io::ErrorKind::InvalidData)?)
        .map_err(|e| io::Error::from_raw_os_error(e.raw() as i32))?;
    format!("O:{sid}D:P(A;{flags};FA;;;{sid})").parse()
}

fn attributes(descriptor: &LocalBox<SecurityDescriptor>) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.as_ptr().cast(),
        bInheritHandle: 0,
    }
}

pub(super) fn new_file(path: &Path) -> io::Result<File> {
    let path = WidePath::new(path)?;
    let descriptor = private_descriptor()?;
    let file = create_file(&path, &descriptor, FILE_SHARE_READ)?;
    verify(&file, false)?;
    Ok(file)
}

pub(super) fn new_lock_file(path: &Path) -> io::Result<File> {
    let path = WidePath::new(path)?;
    let descriptor = private_descriptor()?;
    let file = create_file(&path, &descriptor, FILE_SHARE_READ | FILE_SHARE_WRITE)?;
    verify(&file, false)?;
    Ok(file)
}

pub(super) fn new_database_file(path: &Path) -> io::Result<File> {
    let path = WidePath::new(path)?;
    let descriptor = private_descriptor()?;
    let file = create_file(&path, &descriptor, FILE_SHARE_READ | FILE_SHARE_WRITE)?;
    verify(&file, false)?;
    Ok(file)
}

pub(super) fn new_directory(path: &Path, database: bool) -> io::Result<File> {
    let wide = WidePath::new(path)?;
    let descriptor = private_descriptor_with_flags(if database { "OI" } else { "" })?;
    create_directory(&wide, &descriptor)?;
    let directory = OpenOptions::new()
        .access_mode(0x0002_0000)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if database {
        verify_database_directory(&directory)?;
    } else {
        verify(&directory, true)?;
    }
    Ok(directory)
}

/// Capture owner/group/DACL from the exact handle. Opaque data; no ownership authority.
pub fn access_snapshot(file: &File) -> io::Result<OsString> {
    let info = SecurityInformation::Owner | SecurityInformation::Group | SecurityInformation::Dacl;
    let descriptor = GetSecurityInfo(file, SeObjectType::SE_FILE_OBJECT, info)?;
    ConvertSecurityDescriptorToStringSecurityDescriptor(&descriptor, info)
}

fn private_object_descriptor(
    file: &File,
    directory: bool,
) -> io::Result<LocalBox<SecurityDescriptor>> {
    let m = file.metadata()?;
    let information = winapi_information(file)?;
    if m.is_dir() != directory
        || (!directory && (!m.is_file() || information.1 != 1 || m.permissions().readonly()))
        || information.0 & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    GetSecurityInfo(
        file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
}

fn owner_dacl(descriptor: &SecurityDescriptor) -> io::Result<OsString> {
    ConvertSecurityDescriptorToStringSecurityDescriptor(
        descriptor,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
}

pub(super) fn verify(file: &File, directory: bool) -> io::Result<()> {
    let descriptor = private_object_descriptor(file, directory)?;
    if !directory {
        return verify_private_file_descriptor(&descriptor);
    }
    let actual = owner_dacl(&descriptor)?;
    if actual == owner_dacl(private_descriptor()?.as_ref())?
        || (directory && actual == owner_dacl(private_descriptor_with_flags("OI")?.as_ref())?)
    {
        Ok(())
    } else {
        Err(io::ErrorKind::PermissionDenied.into())
    }
}

fn verify_private_file_descriptor(actual: &SecurityDescriptor) -> io::Result<()> {
    let expected = private_descriptor()?;
    let owner = expected.owner().ok_or(io::ErrorKind::InvalidData)?;
    if actual.owner() != Some(owner) || descriptor_control(actual)? & SE_DACL_PROTECTED == 0 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let dacl = actual
        .dacl()
        .filter(|dacl| IsValidAcl(dacl) && dacl.len() != 0)
        .ok_or(io::ErrorKind::PermissionDenied)?;
    // ReplaceFileW merges DACLs. Equivalent explicit owner ACEs and inheritance
    // bookkeeping need not have the creation descriptor's exact SDDL spelling.
    // Keep the access policy: a protected, nonempty DACL granting only this owner
    // full file access, with no inherited, conditional, denied or foreign ACE.
    for index in 0..dacl.len() {
        let header = descriptor_ace_header(dacl, index)?;
        if header.AceType != AceType::ACCESS_ALLOWED_ACE_TYPE as u8 || header.AceFlags != 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let ace = dacl.get_ace(index).ok_or(io::ErrorKind::PermissionDenied)?;
        if ace.mask() != AccessRights::FileAllAccess || ace.sid() != Some(owner) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
    }
    Ok(())
}

#[allow(unsafe_code)]
fn descriptor_control(descriptor: &SecurityDescriptor) -> io::Result<u16> {
    let mut control = 0;
    let mut revision = 0;
    // SAFETY: the SDK-owned descriptor remains alive and valid for this read-only
    // call; both output pointers reference initialized, correctly sized locals.
    let result = unsafe {
        GetSecurityDescriptorControl(
            std::ptr::from_ref(descriptor).cast_mut().cast(),
            &mut control,
            &mut revision,
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(control)
}

#[allow(unsafe_code)]
fn descriptor_ace_header(dacl: &Acl, index: u32) -> io::Result<ACE_HEADER> {
    let mut entry = ptr::null_mut();
    // SAFETY: the ACL is borrowed from a live SDK-owned valid descriptor; GetAce
    // validates the index and returns a pointer into that same retained ACL.
    let result = unsafe { GetAce(std::ptr::from_ref(dacl).cast(), index, &mut entry) };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    if entry.is_null() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    // SAFETY: successful GetAce supplies a complete aligned SDK ACE_HEADER.
    // Copy only that header while the descriptor remains borrowed. Inspect raw
    // type/flags before using SDK helpers that normalize unknown flag bits.
    Ok(unsafe { std::ptr::read(entry.cast::<ACE_HEADER>()) })
}

pub(super) fn verify_database_directory(file: &File) -> io::Result<()> {
    if owner_dacl(private_object_descriptor(file, true)?.as_ref())?
        != owner_dacl(private_descriptor_with_flags("OI")?.as_ref())?
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

pub(super) fn verify_database_file(file: &File) -> io::Result<()> {
    use winsafe::{HPROCESS, TokenInfo, co};
    let actual = private_object_descriptor(file, false)?;
    let private = private_descriptor()?;
    if owner_dacl(&actual)? == owner_dacl(&private)? {
        return Ok(());
    }
    // SQLite creates sidecars with the directory's inherited ACE and the
    // creator token's default owner, which need not be its user SID.
    let user = private.owner().ok_or(io::ErrorKind::InvalidData)?;
    let ace = actual
        .dacl()
        .filter(|dacl| dacl.len() == 1)
        .and_then(|dacl| dacl.get_ace(0))
        .ok_or(io::ErrorKind::PermissionDenied)?;
    if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE
        || ace.flags() != AceFlags::Inherited
        || ace.mask() != AccessRights::FileAllAccess
        || ace.sid() != Some(user)
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    if actual.owner() != Some(user) {
        let token = HPROCESS::GetCurrentProcess()
            .OpenProcessToken(co::TOKEN::QUERY)
            .map_err(|e| io::Error::from_raw_os_error(e.raw() as i32))?;
        let info = token
            .GetTokenInformation(co::TOKEN_INFORMATION_CLASS::Owner)
            .map_err(|e| io::Error::from_raw_os_error(e.raw() as i32))?;
        let TokenInfo::Owner(owner) = info else {
            return Err(io::ErrorKind::InvalidData.into());
        };
        let sid = winsafe::ConvertSidToStringSid(owner.Owner().ok_or(io::ErrorKind::InvalidData)?)
            .map_err(|e| io::Error::from_raw_os_error(e.raw() as i32))?;
        let owner: LocalBox<windows_permissions::Sid> = sid.parse()?;
        if actual.owner() != Some(&*owner) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
    }
    Ok(())
}

fn winapi_information(file: &File) -> io::Result<(u32, u64)> {
    let info = winapi_util::file::information(file)?;
    Ok((info.file_attributes() as u32, info.number_of_links()))
}

/// Existing-destination primitive, flags zero: preserve its DACL and creation time.
/// Callers close incompatible file handles first, persist origin proof, revalidate
/// parents/old bytes and observe the namespace after both success and error.
/// Error may include a partial native effect; this is not compare-and-swap.
pub fn replace_existing_file(stage: &Path, destination: &Path) -> io::Result<()> {
    replace_file(&WidePath::new(stage)?, &WidePath::new(destination)?)
}

#[allow(unsafe_code)]
fn create_directory(path: &WidePath, descriptor: &LocalBox<SecurityDescriptor>) -> io::Result<()> {
    let attributes = attributes(descriptor);
    // SAFETY: bounded NUL-terminated native units and SDK descriptor/attributes
    // stay alive for this synchronous call; the handle cannot be inherited.
    let result = unsafe { CreateDirectoryW(path.units.as_ptr(), &attributes) };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[allow(unsafe_code)]
fn create_file(
    path: &WidePath,
    descriptor: &LocalBox<SecurityDescriptor>,
    sharing: u32,
) -> io::Result<File> {
    let attributes = attributes(descriptor);
    // SAFETY: initialized generated SDK types and owned path/descriptor live
    // throughout the call; CREATE_NEW never truncates/adopts an existing entry.
    let handle = unsafe {
        CreateFileW(
            path.units.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            sharing,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a successful CREATE_NEW returns a unique valid owned handle. It
    // is transferred once into RAII, never exposed or manually closed afterward.
    let owned = unsafe { OwnedHandle::from_raw_handle(handle) };
    Ok(File::from(owned))
}

#[allow(unsafe_code)]
fn replace_file(stage: &WidePath, destination: &WidePath) -> io::Result<()> {
    // SAFETY: both bounded terminated native paths stay alive through the
    // synchronous SDK call; flags zero retain ACL-merge errors, null backups
    // and reserved parameters require no additional allocation or ownership.
    let result = unsafe {
        ReplaceFileW(
            destination.units.as_ptr(),
            stage.units.as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            ptr::null(),
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStringExt;

    #[test]
    fn private_file_acl_checks_access_instead_of_sddl_serialization() -> io::Result<()> {
        let private = private_descriptor()?;
        let owner = owner_dacl(&private)?;
        let owner = owner.to_str().ok_or(io::ErrorKind::InvalidData)?;
        let (owner, _) = owner.split_once("D:").ok_or(io::ErrorKind::InvalidData)?;
        let user = owner.strip_prefix("O:").ok_or(io::ErrorKind::InvalidData)?;
        for acl in [
            format!("{owner}D:PAI(A;;FA;;;{user})"),
            format!("{owner}D:PAI(A;;FA;;;{user})(A;;FA;;;{user})"),
        ] {
            let descriptor: LocalBox<SecurityDescriptor> = acl.parse()?;
            assert!(verify_private_file_descriptor(&descriptor).is_ok());
        }
        for acl in [
            format!("{owner}D:(A;;FA;;;{user})"),
            format!("{owner}D:P"),
            format!("{owner}D:P(A;;FA;;;{user})(A;;FR;;;WD)"),
            format!("{owner}D:P(A;;FR;;;{user})"),
            format!("{owner}D:P(A;ID;FA;;;{user})"),
            format!("{owner}D:P(A;OI;FA;;;{user})"),
            format!("{owner}D:P(A;CR;FA;;;{user})"),
            format!("{owner}D:P(D;;FR;;;WD)(A;;FA;;;{user})"),
            format!("O:WDD:P(A;;FA;;;{user})"),
        ] {
            let descriptor: LocalBox<SecurityDescriptor> = acl.parse()?;
            assert!(matches!(
                verify_private_file_descriptor(&descriptor),
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied
            ));
        }
        Ok(())
    }

    #[test]
    fn private_replacement_preserves_readers_and_protected_parent() -> io::Result<()> {
        use std::io::{Read, Write};
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_DELETE;
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("private");
        drop(new_directory(&root, false)?);
        let _parent = crate::read_directory::open_root_readonly(&root, &mut || Ok(()))?;
        let destination = root.join("auth.json");
        let stage = root.join("stage");
        {
            let mut file = new_file(&destination)?;
            file.write_all(b"old")?;
            file.sync_all()?;
        }
        let mut retained = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&destination)?;
        {
            let mut file = new_file(&stage)?;
            file.write_all(b"new")?;
            file.sync_all()?;
        }
        let replacement = replace_existing_file(&stage, &destination);
        let native_failure = replacement
            .as_ref()
            .err()
            .map(|error| (error.kind(), error.raw_os_error()));
        assert!(replacement.is_ok(), "replace {native_failure:?}");
        let current = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&destination)?;
        let descriptor = private_object_descriptor(&current, false)?;
        let control = descriptor_control(&descriptor)?;
        let owner_matches = descriptor.owner() == private_descriptor()?.owner();
        let ace_count = descriptor.dacl().map(Acl::len);
        let verification = verify(&current, false);
        let policy_failure = verification
            .as_ref()
            .err()
            .map(|error| (error.kind(), error.raw_os_error()));
        assert!(
            verification.is_ok(),
            "private verification {policy_failure:?}; control={control:#x}, owner_matches={owner_matches}, ace_count={ace_count:?}"
        );
        assert_eq!(std::fs::read(&destination)?, b"new");
        let mut old = Vec::new();
        retained.read_to_end(&mut old)?;
        assert_eq!(old, b"old");
        assert!(!stage.exists());
        assert!(std::fs::rename(&root, temp.path().join("moved")).is_err());
        Ok(())
    }

    #[test]
    fn database_sidecars_inherit_only_the_current_users_access() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let name = std::ffi::OsStr::new("private α");
        let path = temp.path().join(name);
        drop(new_directory(&path, true)?);
        let root = crate::read_directory::open_root_readonly(temp.path(), &mut || Ok(()))?;
        let directory = crate::write_relative::open_private_directory_relative(
            &root,
            name,
            name.encode_wide().count() * std::mem::size_of::<u16>(),
            &mut || Ok(()),
        )?;
        verify_database_directory(&directory)?;
        let sidecar = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path.join("broker.db-wal"))?;
        verify_database_file(&sidecar)?;
        assert!(verify(&sidecar, false).is_err());

        let explicit = owner_dacl(private_descriptor()?.as_ref())?;
        let broad = format!("{}(A;;FR;;;WD)", explicit.to_str().unwrap()).parse()?;
        let exposed = create_file(
            &WidePath::new(&path.join("exposed"))?,
            &broad,
            FILE_SHARE_READ,
        )?;
        assert!(verify_database_file(&exposed).is_err());
        Ok(())
    }

    #[test]
    fn lock_contenders_can_open_readwrite_but_stage_and_delete_sharing_stay_closed() {
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("lock");
        let held = new_lock_file(&path).unwrap();
        verify(&held, false).unwrap();
        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
            .unwrap();
        verify(&contender, false).unwrap();
        assert!(std::fs::remove_file(&path).is_err());
        assert!(held.try_lock().is_ok());
        assert!(matches!(
            contender.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        held.unlock().unwrap();
        contender.try_lock().unwrap();
        contender.unlock().unwrap();
        let stage_path = temp.path().join("stage");
        let _stage = new_file(&stage_path).unwrap();
        assert!(
            OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .open(&stage_path)
                .is_err()
        );
    }
    #[test]
    fn private_birth_readback_and_readonly_changes() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("control");
        let directory = new_directory(&root, false).unwrap();
        verify(&directory, true).unwrap();
        let path = root.join("stage");
        let mut file = new_file(&path).unwrap();
        file.write_all(b"fixture").unwrap();
        verify(&file, false).unwrap();
        assert!(new_file(&path).is_err());
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_readonly(true);
        file.set_permissions(permissions).unwrap();
        assert!(verify(&file, false).is_err());
        let mut permissions = file.metadata().unwrap().permissions();
        // Windows-only fixture: restore the attribute, without changing Unix mode bits.
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        file.set_permissions(permissions).unwrap();
        verify(&file, false).unwrap();
    }
    #[test]
    fn native_paths_preserve_surrogates_and_unc_without_device_or_streams() {
        let mut units: Vec<u16> = r"C:\fixture\".encode_utf16().collect();
        units.push(0xd800);
        let path = OsString::from_wide(&units);
        let encoded = WidePath::new(Path::new(&path)).unwrap();
        assert!(encoded.units.contains(&0xd800));
        assert_eq!(encoded.units.last(), Some(&0));
        for value in [
            r"\\server\share\stage",
            r"\\?\UNC\server\share\stage",
            r"\\?\C:\fixture\stage",
        ] {
            assert!(WidePath::new(Path::new(value)).is_ok());
        }
        for value in [
            r"\\.\NUL",
            r"\\?\GLOBALROOT\Device\x",
            r"C:\fixture\stage:stream",
            r"C:\fixture\..\stage",
            "C:\\fixture\\nul\0stage",
        ] {
            assert!(WidePath::new(Path::new(value)).is_err());
        }
    }
}
