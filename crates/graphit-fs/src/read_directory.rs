//! Read-only native primitives. Admission/authority/ownership belong to callers.
use crate::NodeKind;
use std::{
    ffi::{OsStr, OsString},
    fs::{File, Metadata},
    io,
    path::{Component, Path},
};

pub fn open_root_readonly(
    path: &Path,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    if !path.is_absolute() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        super::unix_relative::root(path, before)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
            FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, SYNCHRONIZE,
        };
        before()?;
        let file = std::fs::OpenOptions::new()
            .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        inspect(&file, Some(NodeKind::Directory), before)?;
        Ok(file)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = before;
        Err(io::ErrorKind::Unsupported.into())
    }
}
pub fn open_relative_readonly(
    parent: &File,
    name: &OsStr,
    kind: Option<NodeKind>,
    max_component_bytes: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    open_relative_profile(
        parent,
        name,
        kind,
        max_component_bytes,
        false,
        false,
        before,
    )
}

/// Idle Source FILE observation. Windows allows external write/delete/rename;
/// callers must reopen through the held parent with the strict read profile for
/// each actual read and reject a changed captured binding. Rights stay read-only.
pub fn open_relative_source_readonly(
    parent: &File,
    name: &OsStr,
    max_component_bytes: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    open_relative_profile(
        parent,
        name,
        Some(NodeKind::File),
        max_component_bytes,
        false,
        true,
        before,
    )
}

/// Existing-file document read, including native access observation rights.
pub fn open_relative_document_read(
    parent: &File,
    name: &OsStr,
    max_component_bytes: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    open_relative_profile(
        parent,
        name,
        Some(NodeKind::File),
        max_component_bytes,
        true,
        false,
        before,
    )
}

fn open_relative_profile(
    parent: &File,
    name: &OsStr,
    kind: Option<NodeKind>,
    max_component_bytes: usize,
    document_read: bool,
    source_read: bool,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    let path = Path::new(name);
    if name.is_empty()
        || max_component_bytes == 0
        || !matches!(path.components().next(), Some(Component::Normal(value)) if value == name)
        || path.components().count() != 1
        || name.as_encoded_bytes().contains(&0)
        || (cfg!(windows)
            && name
                .as_encoded_bytes()
                .iter()
                .any(|b| matches!(b, b'/' | b'\\' | b':')))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    #[cfg(unix)]
    if name.as_encoded_bytes().len() > max_component_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    #[cfg(not(windows))]
    let _ = (document_read, source_read);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let file = super::unix_relative::relative(parent, name, kind, before)?;
    #[cfg(windows)]
    let file = super::windows_relative_sdk::open_relative_readonly(
        parent,
        name,
        kind,
        max_component_bytes,
        document_read,
        source_read,
        before,
    )?;
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    {
        inspect(&file, kind, before)?;
        #[cfg(windows)]
        {
            before()?;
            let original = winapi_util::file::information(parent)?;
            before()?;
            if original.volume_serial_number()
                != winapi_util::file::information(&file)?.volume_serial_number()
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        Ok(file)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (parent, kind, before);
        Err(io::ErrorKind::Unsupported.into())
    }
}
pub fn inspect(
    file: &File,
    kind: Option<NodeKind>,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<Metadata> {
    before()?;
    let metadata = file.metadata()?;
    if (!metadata.is_dir() && !metadata.is_file())
        || kind.is_some_and(|kind| metadata.is_dir() != (kind == NodeKind::Directory))
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() == 0 || (metadata.is_file() && metadata.nlink() != 1) {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        before()?;
        let information = winapi_util::file::information(file)?;
        before()?;
        if information.file_attributes() & u64::from(FILE_ATTRIBUTE_REPARSE_POINT) != 0
            || (metadata.is_file() && information.number_of_links() != 1)
            || !winapi_util::file::typ(file)?.is_disk()
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
    Ok(metadata)
}
/// Native enumeration data only; selected handles still need inspection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryEntryKind {
    File,
    Directory,
    Link,
    Other,
}
pub struct DirectoryEntry {
    pub name: OsString,
    pub kind: DirectoryEntryKind,
}

pub struct DirectoryEntries {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    entries: rustix::fs::Dir,
    #[cfg(windows)]
    file: File,
    #[cfg(windows)]
    buffer: Vec<u64>,
    #[cfg(windows)]
    offset: Option<usize>,
    #[cfg(windows)]
    refill: bool,
    #[cfg(windows)]
    complete: bool,
}
impl DirectoryEntries {
    /// Open an independent enumeration cursor. The byte limit bounds the Windows buffer.
    pub fn new(
        parent: &File,
        buffer_bytes: usize,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<Self> {
        if buffer_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let file = super::unix_relative::enumeration_fd(parent, before)?;
            before()?;
            let entries =
                rustix::fs::Dir::new(std::os::fd::OwnedFd::from(file)).map_err(io::Error::from)?;
            Ok(Self { entries })
        }
        #[cfg(windows)]
        {
            // Reopen the retained object, not an NT pathname containing ".".
            // DuplicateHandle would share the original enumeration cursor.
            let file = super::windows_relative_sdk::reopen_directory(parent, before)?;
            let words = buffer_bytes
                .checked_add(7)
                .ok_or(io::ErrorKind::FileTooLarge)?
                / 8;
            let actual = words.checked_mul(8).ok_or(io::ErrorKind::FileTooLarge)?;
            if actual > buffer_bytes || actual > u32::MAX as usize {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            let mut buffer = Vec::new();
            buffer
                .try_reserve_exact(words)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            buffer.resize(words, 0);
            Ok(Self {
                file,
                buffer,
                offset: None,
                refill: true,
                complete: false,
            })
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            let _ = (parent, before);
            Err(io::ErrorKind::Unsupported.into())
        }
    }
    /// Name-only callers do not perform type fallback probes.
    pub fn next_name(
        &mut self,
        max_component_bytes: usize,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<Option<OsString>> {
        self.next_record(max_component_bytes, false, before)
            .map(|entry| entry.map(|entry| entry.name))
    }
    /// Return one bounded native name and its enumeration kind.
    pub fn next_entry(
        &mut self,
        max_component_bytes: usize,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<Option<DirectoryEntry>> {
        self.next_record(max_component_bytes, true, before)
    }
    fn next_record(
        &mut self,
        max_component_bytes: usize,
        classify: bool,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<Option<DirectoryEntry>> {
        #[cfg(windows)]
        let _ = classify;
        loop {
            before()?;
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            let record = {
                use std::os::unix::ffi::OsStringExt;
                let Some(item) = self.entries.next() else {
                    return Ok(None);
                };
                let item = item.map_err(io::Error::from)?;
                let name = item.file_name().to_bytes();
                if matches!(name, b"." | b"..") {
                    continue;
                }
                if name.is_empty()
                    || name.len() > max_component_bytes
                    || name.iter().any(|b| matches!(b, 0 | b'/'))
                {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                let kind = if classify {
                    let mut kind = item.file_type();
                    if kind == rustix::fs::FileType::Unknown {
                        before()?;
                        let fd = self.entries.fd().map_err(io::Error::from)?;
                        before()?;
                        let metadata = rustix::fs::statat(
                            fd,
                            item.file_name(),
                            rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
                        )
                        .map_err(io::Error::from)?;
                        kind = rustix::fs::FileType::from_raw_mode(metadata.st_mode);
                    }
                    match kind {
                        rustix::fs::FileType::RegularFile => DirectoryEntryKind::File,
                        rustix::fs::FileType::Directory => DirectoryEntryKind::Directory,
                        rustix::fs::FileType::Symlink => DirectoryEntryKind::Link,
                        _ => DirectoryEntryKind::Other,
                    }
                } else {
                    DirectoryEntryKind::Other
                };
                let mut copy = Vec::new();
                copy.try_reserve_exact(name.len())
                    .map_err(|_| io::ErrorKind::OutOfMemory)?;
                copy.extend_from_slice(name);
                DirectoryEntry {
                    name: OsString::from_vec(copy),
                    kind,
                }
            };
            #[cfg(windows)]
            let record = {
                if self.complete {
                    return Ok(None);
                }
                if self.refill {
                    if !super::windows_relative_sdk::query_directory(
                        &self.file,
                        &mut self.buffer,
                        before,
                    )? {
                        self.complete = true;
                        return Ok(None);
                    }
                    self.offset = Some(0);
                    self.refill = false;
                }
                let offset = self.offset.ok_or(io::ErrorKind::InvalidData)?;
                let (name, attributes, next) = super::windows_relative_sdk::directory_record(
                    &self.buffer,
                    offset,
                    max_component_bytes,
                )?;
                self.offset = next;
                self.refill = next.is_none();
                if name == OsStr::new(".") || name == OsStr::new("..") {
                    continue;
                }
                use windows_sys::Win32::Storage::FileSystem::{
                    FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
                };
                let kind = if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    DirectoryEntryKind::Link
                } else if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                    DirectoryEntryKind::Directory
                } else if attributes & FILE_ATTRIBUTE_DEVICE != 0 {
                    DirectoryEntryKind::Other
                } else {
                    DirectoryEntryKind::File
                };
                DirectoryEntry { name, kind }
            };
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            return Ok(Some(record));
            #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
            {
                let _ = (max_component_bytes, classify, before);
                return Err(io::ErrorKind::Unsupported.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> (tempfile::TempDir, File) {
        let directory = tempfile::tempdir().unwrap();
        let file = open_root_readonly(directory.path(), &mut || Ok(())).unwrap();
        (directory, file)
    }
    #[test]
    fn independent_enumerators_keep_their_own_cursor() {
        let (directory, root) = root();
        std::fs::write(directory.path().join("alpha"), b"A").unwrap();
        std::fs::write(directory.path().join("beta"), b"B").unwrap();
        let mut a = DirectoryEntries::new(&root, 4096, &mut || Ok(())).unwrap();
        let mut b = DirectoryEntries::new(&root, 4096, &mut || Ok(())).unwrap();
        let collect = |entries: &mut DirectoryEntries| {
            let mut result = Vec::new();
            while let Some(name) = entries.next_name(256, &mut || Ok(())).unwrap() {
                result.push(name);
            }
            result.sort();
            result
        };
        let first = a.next_name(256, &mut || Ok(())).unwrap().unwrap();
        let mut names = collect(&mut a);
        names.push(first);
        names.sort();
        assert_eq!(names, collect(&mut b));
        assert_eq!(names, vec![OsString::from("alpha"), OsString::from("beta")]);
    }
    #[cfg(windows)]
    #[test]
    fn source_readers_allow_external_write_but_document_readers_keep_their_fence() {
        use std::io::Write;
        let (directory, root) = root();
        let path = directory.path().join("source.rs");
        std::fs::write(&path, b"old").unwrap();
        let mut source =
            open_relative_source_readonly(&root, OsStr::new("source.rs"), 256, &mut || Ok(()))
                .unwrap();
        assert!(source.write_all(b"denied").is_err());
        std::fs::write(&path, b"edited while retained").unwrap();
        let document =
            open_relative_document_read(&root, OsStr::new("source.rs"), 256, &mut || Ok(()))
                .unwrap();
        assert!(std::fs::write(&path, b"denied by document profile").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"edited while retained");
        drop(document);
        std::fs::write(&path, b"editable again").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"editable again");
    }
    #[cfg(windows)]
    #[test]
    fn source_file_idle_sharing_allows_replace_but_strict_read_and_directory_do_not() {
        let (directory, root) = root();
        let path = directory.path().join("source.rs");
        let staged = directory.path().join("staged.rs");
        std::fs::write(&path, b"old").unwrap();
        std::fs::write(&staged, b"new").unwrap();
        let source =
            open_relative_source_readonly(&root, OsStr::new("source.rs"), 256, &mut || Ok(()))
                .unwrap();
        let strict = open_relative_readonly(
            &root,
            OsStr::new("source.rs"),
            Some(NodeKind::File),
            256,
            &mut || Ok(()),
        )
        .unwrap();
        assert!(std::fs::rename(&staged, &path).is_err());
        assert!(std::fs::remove_file(&path).is_err());
        drop(strict);
        std::fs::rename(&staged, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        std::fs::remove_file(&path).unwrap();
        drop(source);
        let child_path = directory.path().join("child");
        std::fs::create_dir(&child_path).unwrap();
        assert!(
            open_relative_source_readonly(&root, OsStr::new("child"), 256, &mut || Ok(())).is_err()
        );
        let child = open_relative_readonly(
            &root,
            OsStr::new("child"),
            Some(NodeKind::Directory),
            256,
            &mut || Ok(()),
        )
        .unwrap();
        assert!(std::fs::rename(&child_path, directory.path().join("moved")).is_err());
        drop(child);
    }
    #[cfg(windows)]
    #[test]
    fn relative_directory_enumerators_retain_identity_cursor_and_rename_barrier() {
        let (directory, root) = root();
        let child_path = directory.path().join("child");
        let moved = directory.path().join("moved");
        std::fs::create_dir(&child_path).unwrap();
        for name in ["alpha", "beta"] {
            std::fs::write(child_path.join(name), name).unwrap();
        }
        let child = open_relative_readonly(
            &root,
            OsStr::new("child"),
            Some(NodeKind::Directory),
            256,
            &mut || Ok(()),
        )
        .unwrap();
        let error =
            DirectoryEntries::new(&child, 4096, &mut || Err(io::ErrorKind::Interrupted.into()))
                .err()
                .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        let mut a = DirectoryEntries::new(&child, 4096, &mut || Ok(())).unwrap();
        let mut b = DirectoryEntries::new(&child, 4096, &mut || Ok(())).unwrap();
        drop(child);
        drop(root);
        let first = a.next_name(256, &mut || Ok(())).unwrap().unwrap();
        let collect = |entries: &mut DirectoryEntries| {
            let mut names = Vec::new();
            while let Some(name) = entries.next_name(256, &mut || Ok(())).unwrap() {
                names.push(name);
            }
            names.sort();
            names
        };
        let mut names = collect(&mut a);
        names.push(first);
        names.sort();
        assert_eq!(names, collect(&mut b));
        assert_eq!(names, [OsString::from("alpha"), OsString::from("beta")]);
        assert!(std::fs::rename(&child_path, &moved).is_err());
        drop(a);
        assert!(std::fs::rename(&child_path, &moved).is_err());
        drop(b);
        std::fs::rename(child_path, moved).unwrap();
    }
    #[test]
    fn denied_native_attempt_precedes_acquisition() {
        let (directory, root) = root();
        std::fs::write(directory.path().join("file"), b"payload").unwrap();
        let error = open_relative_readonly(
            &root,
            OsStr::new("file"),
            Some(NodeKind::File),
            256,
            &mut || Err(io::ErrorKind::Interrupted.into()),
        )
        .err()
        .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    }
    #[test]
    fn raw_component_aliases_and_file_hardlinks_deny() {
        let (directory, root) = root();
        std::fs::write(directory.path().join("file"), b"payload").unwrap();
        for name in ["", ".", "..", "file/", "file/.", "../file"] {
            assert!(
                open_relative_readonly(&root, OsStr::new(name), None, 256, &mut || Ok(()),)
                    .is_err(),
                "{name}"
            );
        }
        std::fs::hard_link(
            directory.path().join("file"),
            directory.path().join("alias"),
        )
        .unwrap();
        assert_eq!(
            open_relative_readonly(
                &root,
                OsStr::new("file"),
                Some(NodeKind::File),
                256,
                &mut || Ok(()),
            )
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_are_denied_and_native_names_are_preserved() {
        use std::os::unix::fs::symlink;
        let (directory, root) = root();
        #[cfg(target_os = "linux")]
        let name = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![b'f', 0xff])
        };
        #[cfg(not(target_os = "linux"))]
        let name = OsString::from("f-é 🚀");
        std::fs::write(directory.path().join(&name), b"native").unwrap();
        open_relative_readonly(&root, &name, Some(NodeKind::File), 256, &mut || Ok(())).unwrap();
        symlink(&name, directory.path().join("link")).unwrap();
        assert!(
            open_relative_readonly(
                &root,
                OsStr::new("link"),
                Some(NodeKind::File),
                256,
                &mut || Ok(()),
            )
            .is_err()
        );
    }
    #[cfg(unix)]
    #[test]
    fn removed_directory_is_not_an_empty_success() {
        let (directory, root) = root();
        std::fs::create_dir(directory.path().join("selected")).unwrap();
        let selected = open_relative_readonly(
            &root,
            OsStr::new("selected"),
            Some(NodeKind::Directory),
            256,
            &mut || Ok(()),
        )
        .unwrap();
        std::fs::remove_dir(directory.path().join("selected")).unwrap();
        assert!(DirectoryEntries::new(&selected, 4096, &mut || Ok(())).is_err());
    }
}

#[cfg(test)]
mod entry_tests {
    use super::*;
    #[test]
    fn cancellation_before_next_entry_preserves_the_cursor() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("alpha"), b"a").unwrap();
        std::fs::write(temp.path().join("beta"), b"b").unwrap();
        let root = open_root_readonly(temp.path(), &mut || Ok(())).unwrap();
        let mut entries = DirectoryEntries::new(&root, 4096, &mut || Ok(())).unwrap();
        let first = entries.next_entry(256, &mut || Ok(())).unwrap().unwrap();
        assert_eq!(
            entries
                .next_entry(256, &mut || Err(io::ErrorKind::Interrupted.into()))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        let second = entries.next_entry(256, &mut || Ok(())).unwrap().unwrap();
        assert_ne!(first.name, second.name);
        assert!(entries.next_entry(256, &mut || Ok(())).unwrap().is_none());
    }
    #[test]
    fn component_byte_limit_still_rejects_long_names() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("long-name"), b"x").unwrap();
        let root = open_root_readonly(temp.path(), &mut || Ok(())).unwrap();
        let mut entries = DirectoryEntries::new(&root, 4096, &mut || Ok(())).unwrap();
        assert!(entries.next_name(1, &mut || Ok(())).is_err());
    }
    #[test]
    fn metadata_entry_reports_kind_without_opening_file_payload() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("file"), b"x").unwrap();
        std::fs::create_dir(temp.path().join("directory")).unwrap();
        let root = open_root_readonly(temp.path(), &mut || Ok(())).unwrap();
        let mut iter = DirectoryEntries::new(&root, 4096, &mut || Ok(())).unwrap();
        let mut observed = Vec::new();
        while let Some(entry) = iter.next_entry(256, &mut || Ok(())).unwrap() {
            observed.push((entry.name, entry.kind));
        }
        observed.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            observed,
            [
                (OsString::from("directory"), DirectoryEntryKind::Directory),
                (OsString::from("file"), DirectoryEntryKind::File)
            ]
        );
        drop(iter);
        drop(root);
        drop(temp);
    }
    #[cfg(unix)]
    #[test]
    fn native_link_and_denied_payload_are_enumeration_data() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("file"), b"x").unwrap();
        std::fs::set_permissions(
            temp.path().join("file"),
            std::fs::Permissions::from_mode(0o0),
        )
        .unwrap();
        symlink("file", temp.path().join("link")).unwrap();
        let root = open_root_readonly(temp.path(), &mut || Ok(())).unwrap();
        let mut iter = DirectoryEntries::new(&root, 4096, &mut || Ok(())).unwrap();
        let mut entries = Vec::new();
        while let Some(entry) = iter.next_entry(256, &mut || Ok(())).unwrap() {
            entries.push((entry.name, entry.kind));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            entries,
            [
                (OsString::from("file"), DirectoryEntryKind::File),
                (OsString::from("link"), DirectoryEntryKind::Link)
            ]
        );
        drop(iter);
        drop(root);
        drop(temp);
    }
}
