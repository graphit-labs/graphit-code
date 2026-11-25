//! Exact SDK interop: owned descriptors, lossless paths and private handle adoption.
use std::{
    fs::{File, OpenOptions},
    io,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
        io::{FromRawHandle, OwnedHandle},
    },
    path::{Component, Path},
    ptr,
};
use windows_permissions::{
    Acl, LocalBox, SecurityDescriptor,
    constants::{AceType, SeObjectType, SecurityInformation},
    wrappers::{GetNamedSecurityInfo, GetSecurityInfo, IsValidAcl},
};
use windows_sys::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
    Security::{
        ACE_HEADER, CONTAINER_INHERIT_ACE, GetAce, GetSecurityDescriptorControl, INHERIT_ONLY_ACE,
        INHERITED_ACE, NO_PROPAGATE_INHERIT_ACE, OBJECT_INHERIT_ACE, SE_DACL_PROTECTED,
        SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::{
        CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
    },
};

struct WidePath {
    units: Vec<u16>,
}
impl WidePath {
    fn new(path: &Path) -> io::Result<Self> {
        use std::path::Prefix;
        if path.as_os_str().encode_wide().any(|unit| unit == 0) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let path = std::path::absolute(path)?;
        let prefix = path.components().next();
        let mut units = Vec::new();
        match prefix {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(_) => {
                    units.extend([92, 92, 63, 92]);
                    units.extend(path.as_os_str().encode_wide());
                }
                Prefix::UNC(_, _) => {
                    units.extend([92, 92, 63, 92, 85, 78, 67, 92]);
                    units.extend(path.as_os_str().encode_wide().skip(2));
                }
                _ => units.extend(path.as_os_str().encode_wide()),
            },
            _ => units.extend(path.as_os_str().encode_wide()),
        }
        for unit in &mut units {
            if *unit == u16::from(b'/') {
                *unit = u16::from(b'\\');
            }
        }
        // The native extended-length path limit is UTF-16 units, not a generic
        // ancestry/path-byte budget. Keep the terminator outside that count.
        if units.len() > 32767 {
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
    let file = create_file(
        &path,
        &descriptor,
        FILE_SHARE_READ,
        GENERIC_READ | GENERIC_WRITE,
    )?;
    verify(&file, false)?;
    Ok(file)
}

pub(super) fn new_lock_file(path: &Path) -> io::Result<File> {
    let path = WidePath::new(path)?;
    let descriptor = private_descriptor()?;
    let file = create_file(
        &path,
        &descriptor,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        GENERIC_READ | GENERIC_WRITE,
    )?;
    verify(&file, false)?;
    Ok(file)
}

pub(super) fn new_database_file(path: &Path) -> io::Result<File> {
    let path = WidePath::new(path)?;
    let descriptor = private_descriptor()?;
    let file = create_file(
        &path,
        &descriptor,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        GENERIC_READ | GENERIC_WRITE,
    )?;
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

fn private_object_descriptor(
    file: &File,
    directory: bool,
) -> io::Result<LocalBox<SecurityDescriptor>> {
    let m = file.metadata()?;
    if m.is_dir() != directory || (!directory && !m.is_file()) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    GetSecurityInfo(
        file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
}

pub(super) fn verify(file: &File, directory: bool) -> io::Result<()> {
    let descriptor = private_object_descriptor(file, directory)?;
    verify_private_descriptor(&descriptor, directory, false)
}

fn verify_private_descriptor(
    actual: &SecurityDescriptor,
    directory: bool,
    inherit_files: bool,
) -> io::Result<()> {
    let expected = private_descriptor()?;
    let owner = expected.owner().ok_or(io::ErrorKind::InvalidData)?;
    if actual.owner() != Some(owner) || descriptor_control(actual)? & SE_DACL_PROTECTED == 0 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let dacl = actual
        .dacl()
        .filter(|dacl| IsValidAcl(dacl) && dacl.len() != 0)
        .ok_or(io::ErrorKind::PermissionDenied)?;
    // Inspect who receives access and, for database directories, whether
    // sidecars inherit owner-only ACEs. Actual operations decide access rights. Equivalent ACEs and control bookkeeping need not
    // have the creation descriptor's exact serialization. INHERITED_ACE records
    // origin, not different access; the protected DACL cannot inherit new ACEs.
    let allowed_flags = INHERITED_ACE
        | if directory {
            OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | NO_PROPAGATE_INHERIT_ACE | INHERIT_ONLY_ACE
        } else {
            0
        };
    let mut file_inheritance = false;
    for index in 0..dacl.len() {
        let header = descriptor_ace_header(dacl, index)?;
        if header.AceType != AceType::ACCESS_ALLOWED_ACE_TYPE as u8
            || u32::from(header.AceFlags) & !allowed_flags != 0
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let ace = dacl.get_ace(index).ok_or(io::ErrorKind::PermissionDenied)?;
        if ace.sid() != Some(owner) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if u32::from(header.AceFlags) & OBJECT_INHERIT_ACE != 0 {
            file_inheritance = true;
        }
    }
    if inherit_files && !file_inheritance {
        return Err(io::ErrorKind::PermissionDenied.into());
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
    verify_private_descriptor(private_object_descriptor(file, true)?.as_ref(), true, true)
}

/// Inspect an existing SQLite file's credential privacy without opening a File.
pub(super) fn verify_private_file_path(path: &Path) -> io::Result<()> {
    let descriptor = private_path_descriptor(path, false)?;
    verify_database_descriptor(&descriptor)
}

/// Require the existing database directory's owner-only sidecar inheritance.
pub(super) fn verify_private_database_directory_path(path: &Path) -> io::Result<()> {
    let descriptor = private_path_descriptor(path, true)?;
    verify_private_descriptor(&descriptor, true, true)
}

pub(super) fn verify_private_directory_path(path: &Path) -> io::Result<()> {
    let descriptor = private_path_descriptor(path, true)?;
    verify_private_descriptor(&descriptor, true, false)
}

fn private_path_descriptor(
    path: &Path,
    directory: bool,
) -> io::Result<LocalBox<SecurityDescriptor>> {
    let path = WidePath::new(path)?;
    let path = std::ffi::OsString::from_wide(&path.units[..path.units.len() - 1]);
    // Resolve the effective object for the named-security API. This temporary
    // ordinary path is not an identity/ancestor capture or a retained DB handle.
    let path = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&path)?;
    if metadata.is_dir() != directory || (!directory && !metadata.is_file()) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    GetNamedSecurityInfo(
        &path,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
}

#[cfg(test)]
fn verify_database_file(file: &File) -> io::Result<()> {
    verify_database_descriptor(private_object_descriptor(file, false)?.as_ref())
}

fn verify_database_descriptor(actual: &SecurityDescriptor) -> io::Result<()> {
    use winsafe::{HPROCESS, TokenInfo, co};
    let private = private_descriptor()?;
    match verify_private_descriptor(actual, false, false) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {}
        Err(error) => return Err(error),
    }
    // SQLite creates sidecars with the directory's inherited ACE and the
    // creator token's default owner, which need not be its user SID.
    let user = private.owner().ok_or(io::ErrorKind::InvalidData)?;
    let dacl = actual
        .dacl()
        .filter(|dacl| IsValidAcl(dacl) && dacl.len() != 0)
        .ok_or(io::ErrorKind::PermissionDenied)?;
    for index in 0..dacl.len() {
        let header = descriptor_ace_header(dacl, index)?;
        if header.AceType != AceType::ACCESS_ALLOWED_ACE_TYPE as u8
            || u32::from(header.AceFlags) != INHERITED_ACE
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let ace = dacl.get_ace(index).ok_or(io::ErrorKind::PermissionDenied)?;
        if ace.sid() != Some(user) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
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
    access: u32,
) -> io::Result<File> {
    let attributes = attributes(descriptor);
    // SAFETY: initialized generated SDK types and owned path/descriptor live
    // throughout the call; CREATE_NEW never truncates/adopts an existing entry.
    let handle = unsafe {
        CreateFileW(
            path.units.as_ptr(),
            access,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};

    fn owner_dacl(descriptor: &SecurityDescriptor) -> io::Result<OsString> {
        windows_permissions::wrappers::ConvertSecurityDescriptorToStringSecurityDescriptor(
            descriptor,
            SecurityInformation::Owner | SecurityInformation::Dacl,
        )
    }

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
            format!("{owner}D:PAI(A;ID;FA;;;{user})"),
            format!("{owner}D:P(A;;FR;;;{user})"),
            format!("{owner}D:P(A;ID;FR;;;{user})"),
        ] {
            let descriptor: LocalBox<SecurityDescriptor> = acl.parse()?;
            assert!(verify_private_descriptor(&descriptor, false, false).is_ok());
        }
        for acl in [
            format!("{owner}D:(A;;FA;;;{user})"),
            format!("{owner}D:P"),
            format!("{owner}D:P(A;;FA;;;{user})(A;;FR;;;WD)"),
            format!("{owner}D:P(A;ID;FA;;;{user})(A;ID;FR;;;WD)"),
            format!("{owner}D:(A;ID;FA;;;{user})"),
            format!("{owner}D:P(A;OI;FA;;;{user})"),
            format!("{owner}D:P(A;CR;FA;;;{user})"),
            format!("{owner}D:P(D;;FR;;;WD)(A;;FA;;;{user})"),
            format!("O:WDD:P(A;;FA;;;{user})"),
        ] {
            let descriptor: LocalBox<SecurityDescriptor> = acl.parse()?;
            assert!(matches!(
                verify_private_descriptor(&descriptor, false, false),
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied
            ));
        }
        Ok(())
    }

    #[test]
    fn private_directory_acl_checks_effective_access_and_sidecar_inheritance() -> io::Result<()> {
        use windows_sys::Win32::Storage::FileSystem::{FILE_READ_ATTRIBUTES, READ_CONTROL};
        let private = owner_dacl(private_descriptor()?.as_ref())?;
        let (owner, _) = private
            .to_str()
            .and_then(|s| s.split_once("D:"))
            .ok_or(io::ErrorKind::InvalidData)?;
        let user = owner.strip_prefix("O:").ok_or(io::ErrorKind::InvalidData)?;
        let temp = tempfile::tempdir()?;
        for (index, (acl, sidecars)) in [
            (format!("{owner}D:PAI(A;;FA;;;{user})"), false),
            (format!("{owner}D:PAI(A;ID;FA;;;{user})"), false),
            (format!("{owner}D:PAI(A;OI;FA;;;{user})"), true),
            (format!("{owner}D:PAI(A;OIID;FA;;;{user})"), true),
            (
                format!("{owner}D:PAI(A;OI;FA;;;{user})(A;OI;FA;;;{user})"),
                true,
            ),
            (
                format!("{owner}D:PAI(A;;FA;;;{user})(A;OIIO;FA;;;{user})"),
                true,
            ),
            (format!("{owner}D:PAI(A;OICI;FA;;;{user})"), true),
            (format!("{owner}D:PAI(A;OINP;FA;;;{user})"), true),
            (
                format!("{owner}D:PAI(A;OI;0x120089;;;{user})(A;OI;0xd0176;;;{user})"),
                true,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let descriptor: LocalBox<SecurityDescriptor> = acl.parse()?;
            verify_private_descriptor(&descriptor, true, sidecars)?;
            let path = temp.path().join(index.to_string());
            create_directory(&WidePath::new(&path)?, &descriptor)?;
            let directory = OpenOptions::new()
                .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path)?;
            verify(&directory, true)?;
            assert_eq!(verify_database_directory(&directory).is_ok(), sidecars);
            assert_eq!(
                verify_private_database_directory_path(&path).is_ok(),
                sidecars
            );
            if sidecars {
                let database = new_database_file(&path.join("database"))?;
                verify_database_file(&database)?;
                verify_private_file_path(&path.join("database"))?;
                let sidecar = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(path.join("database-wal"))?;
                verify_database_file(&sidecar)?;
                verify_private_file_path(&path.join("database-wal"))?;
                assert!(verify(&sidecar, false).is_err());
            }
        }
        for acl in [
            format!("{owner}D:P(A;;FR;;;{user})"),
            format!("{owner}D:P(A;OI;FR;;;{user})"),
            format!("{owner}D:P(A;OIIO;FA;;;{user})"),
        ] {
            let descriptor: LocalBox<SecurityDescriptor> = acl.parse()?;
            verify_private_descriptor(&descriptor, true, false)?;
            assert_eq!(
                verify_private_descriptor(&descriptor, true, true).is_ok(),
                acl.contains(";OI;") || acl.contains(";OIIO;")
            );
        }
        for acl in [
            format!("{owner}D:(A;;FA;;;{user})"),
            format!("{owner}D:NO_ACCESS_CONTROL"),
            format!("{owner}D:P"),
            format!("{owner}D:P(A;;FA;;;{user})(A;;FR;;;WD)"),
            format!("{owner}D:P(A;;FA;;;{user})(A;OIIO;FR;;;WD)"),
            format!("{owner}D:P(A;ID;FA;;;{user})(A;ID;FR;;;WD)"),
            format!("{owner}D:(A;ID;FA;;;{user})"),
            format!("{owner}D:P(A;CR;FA;;;{user})"),
            format!("{owner}D:P(D;;FR;;;WD)(A;;FA;;;{user})"),
            format!("O:WDD:P(A;;FA;;;{user})"),
        ] {
            let descriptor: LocalBox<SecurityDescriptor> = acl.parse()?;
            for sidecars in [false, true] {
                assert!(matches!(
                    verify_private_descriptor(&descriptor, true, sidecars),
                    Err(error) if error.kind() == io::ErrorKind::PermissionDenied
                ));
            }
        }
        for (name, acl) in [
            ("unprotected", format!("{owner}D:(A;;FA;;;{user})")),
            ("null", format!("{owner}D:NO_ACCESS_CONTROL")),
            ("foreign", format!("{owner}D:P(A;;FA;;;{user})(A;;FR;;;WD)")),
        ] {
            let path = temp.path().join(name);
            create_directory(&WidePath::new(&path)?, &acl.parse()?)?;
            let directory = OpenOptions::new()
                .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path)?;
            let descriptor = private_object_descriptor(&directory, true)?;
            // Creation can protect a legacy descriptor. Test the actual object;
            // null DACLs and foreign grants must still be rejected in every case.
            let protected = descriptor_control(&descriptor)? & SE_DACL_PROTECTED != 0;
            let private = name == "unprotected"
                && protected
                && descriptor.owner() == private_descriptor()?.owner();
            assert_eq!(verify(&directory, true).is_ok(), private, "{name}");
            assert!(verify_database_directory(&directory).is_err(), "{name}");
            assert!(
                verify_private_database_directory_path(&path).is_err(),
                "{name}"
            );
        }
        let parent = temp.path().join("inheritable");
        let parent_descriptor = format!("{owner}D:P(A;CI;FA;;;{user})").parse()?;
        create_directory(&WidePath::new(&parent)?, &parent_descriptor)?;
        let child = parent.join("inherited");
        std::fs::create_dir(&child)?;
        let directory = OpenOptions::new()
            .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&child)?;
        let descriptor = private_object_descriptor(&directory, true)?;
        assert!(descriptor.dacl().is_some_and(|dacl| {
            (0..dacl.len()).any(|index| {
                descriptor_ace_header(dacl, index)
                    .is_ok_and(|header| u32::from(header.AceFlags) & INHERITED_ACE != 0)
            })
        }));
        let private = descriptor_control(&descriptor)? & SE_DACL_PROTECTED != 0
            && descriptor.owner() == private_descriptor()?.owner();
        assert_eq!(verify(&directory, true).is_ok(), private, "inherited child");
        assert!(verify_database_directory(&directory).is_err());
        Ok(())
    }

    #[test]
    fn owner_readonly_acl_is_private_and_os_controls_writes() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let owner = owner_dacl(private_descriptor()?.as_ref())?;
        let owner = owner
            .to_str()
            .and_then(|s| s.split_once("D:"))
            .ok_or(io::ErrorKind::InvalidData)?
            .0;
        let user = owner.strip_prefix("O:").ok_or(io::ErrorKind::InvalidData)?;
        let descriptor = format!("{owner}D:P(A;;FR;;;{user})").parse()?;
        let path = temp.path().join("readonly-secret");
        let file = create_file(
            &WidePath::new(&path)?,
            &descriptor,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            GENERIC_READ,
        )?;
        verify(&file, false)?;
        verify_private_file_path(&path)?;
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        let exposed = format!("{owner}D:P(A;;FR;;;{user})(A;;FR;;;WD)").parse()?;
        let exposed_path = temp.path().join("exposed-readonly");
        let exposed_file = create_file(
            &WidePath::new(&exposed_path)?,
            &exposed,
            FILE_SHARE_READ,
            GENERIC_READ,
        )?;
        assert!(verify(&exposed_file, false).is_err());
        assert!(verify_private_file_path(&exposed_path).is_err());
        Ok(())
    }

    #[test]
    fn private_paths_accept_hardlinks_and_configured_directory_links() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("database");
        drop(new_directory(&root, true)?);
        let database = root.join("broker.db");
        let file = new_database_file(&database)?;
        let alias = root.join("alias.db");
        std::fs::hard_link(&database, &alias)?;
        verify_database_file(&file)?;
        verify_private_file_path(&alias)?;
        let permissions = file.metadata()?.permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        std::fs::set_permissions(&database, readonly)?;
        verify_database_file(&file)?;
        verify_private_file_path(&alias)?;
        assert!(std::fs::metadata(&database)?.permissions().readonly());
        // Restore only this fixture's attributes for TempDir cleanup.
        std::fs::set_permissions(&database, permissions)?;
        let configured = temp.path().join("configured");
        crate::create_link(&root, &configured).map_err(io::Error::other)?;
        verify_private_database_directory_path(&configured)?;
        verify_private_file_path(&configured.join("broker.db"))?;
        assert!(verify_private_file_path(&root).is_err());
        assert!(verify_private_database_directory_path(&database).is_err());
        assert_eq!(
            verify_private_file_path(&root.join("absent"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound,
        );
        Ok(())
    }

    #[test]
    fn database_sidecars_inherit_only_the_current_users_access() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let name = std::ffi::OsStr::new("private α");
        let path = temp.path().join(name);
        drop(new_directory(&path, true)?);
        let directory = OpenOptions::new()
            .access_mode(windows_sys::Win32::Storage::FileSystem::READ_CONTROL)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&path)?;
        verify_database_directory(&directory)?;
        verify_private_database_directory_path(&path)?;
        let sidecar = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path.join("broker.db-wal"))?;
        verify_database_file(&sidecar)?;
        verify_private_file_path(&path.join("broker.db-wal"))?;
        assert!(verify(&sidecar, false).is_err());

        let explicit = owner_dacl(private_descriptor()?.as_ref())?;
        let broad = format!("{}(A;;FR;;;WD)", explicit.to_str().unwrap()).parse()?;
        let exposed = create_file(
            &WidePath::new(&path.join("exposed"))?,
            &broad,
            FILE_SHARE_READ,
            GENERIC_READ | GENERIC_WRITE,
        )?;
        assert!(verify_database_file(&exposed).is_err());
        assert!(verify_private_file_path(&path.join("exposed")).is_err());
        let inherited_parent = temp.path().join("foreign-sidecars");
        let inherited = owner_dacl(private_descriptor_with_flags("OI")?.as_ref())?;
        let inherited = format!("{}(A;OI;FR;;;WD)", inherited.to_str().unwrap()).parse()?;
        create_directory(&WidePath::new(&inherited_parent)?, &inherited)?;
        let exposed_sidecar = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(inherited_parent.join("database-wal"))?;
        assert!(verify_database_file(&exposed_sidecar).is_err());
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
        verify_private_directory_path(&root).unwrap();
        assert!(verify_private_database_directory_path(&root).is_err());
        let path = root.join("stage");
        let mut file = new_file(&path).unwrap();
        file.write_all(b"fixture").unwrap();
        verify(&file, false).unwrap();
        assert!(new_file(&path).is_err());
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_readonly(true);
        file.set_permissions(permissions).unwrap();
        assert!(file.metadata().unwrap().permissions().readonly());
        verify(&file, false).unwrap();
        assert_eq!(
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"fixture");
        let mut permissions = file.metadata().unwrap().permissions();
        // Windows-only fixture: restore the attribute, without changing Unix mode bits.
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        file.set_permissions(permissions).unwrap();
        verify(&file, false).unwrap();
    }
    #[test]
    fn native_paths_preserve_surrogates_and_support_platform_path_syntax() {
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
            r"C:\fixture\stage:stream",
            r"C:\fixture\..\stage",
            r"relative\stage",
        ] {
            assert!(WidePath::new(Path::new(value)).is_ok());
        }
        assert!(WidePath::new(Path::new("C:\\fixture\\nul\0stage")).is_err());
    }
}
