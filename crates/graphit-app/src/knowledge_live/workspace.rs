//! A mutable working directory born by this App operation, not an immutable Core tree.
use super::Access;
use graphit_core::{
    artifact_store::{CapturedSourceReader, StoreOwner},
    file_io::SnapshotContext,
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentDirectoryIdentity, NativeDocumentFileLimits,
        NativeDocumentNamespace,
    },
    native_file_identity::{handle_id, physical_id},
};
use graphit_fs::NodeKind;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub(super) struct Workspace {
    parent: CapturedDocumentDirectory,
    root: PathBuf,
    original: Arc<SnapshotContext>,
    directories: BTreeMap<PathBuf, File>,
    removed: bool,
    owner: StoreOwner,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkspaceRecord {
    pub name: String,
    pub parent: NativeDocumentDirectoryIdentity,
    pub directory: NativeDocumentDirectoryIdentity,
}

/// Compare the live path with the retained birth handle without adopting a replacement.
fn verify_directory(file: &File, path: &Path, context: &SnapshotContext) -> io::Result<()> {
    context.check()?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || handle_id(file)? != physical_id(path, &metadata)?
    {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    graphit_fs::verify_private_directory(file)?;
    context.check()
}

impl Workspace {
    /// The caller validates the selected executable and all artifact admissions before birth.
    pub fn create(access: &Access<'_>, native: NativeDocumentFileLimits) -> io::Result<Self> {
        access.write()?;
        let context = access.original().snapshot();
        let owner_root = match access.scope().owner() {
            StoreOwner::Installation { .. } => access.scope().owner_root(),
            StoreOwner::User { .. } => access.scope().user_root()?,
            StoreOwner::Project { .. } => return Err(io::ErrorKind::PermissionDenied.into()),
        };
        let namespace = NativeDocumentNamespace::capture(
            access.scope(),
            &owner_root.join("knowledge-live"),
            native,
            context,
        )?;
        let parent = CapturedDocumentDirectory::capture(
            namespace.parent().canonical_path(),
            native,
            context,
        )?;
        let parent_file = parent.private_handle(context)?;
        let name = format!("work-{}", super::service::generation()?);
        let root = parent.canonical_path().join(&name);
        let birth = graphit_fs::write_relative::create_private_directory_relative(
            &parent_file,
            name.as_ref(),
            name.len(),
            &mut || access.write(),
        )?;
        // Return the real birth handle before any further cancelable capture.
        // The service installs this value in its custody slot immediately.
        Ok(Self {
            parent,
            root,
            original: Arc::clone(context),
            directories: BTreeMap::from([(PathBuf::new(), birth)]),
            removed: false,
            owner: access.scope().owner().clone(),
        })
    }
    pub fn path(&self) -> &Path {
        &self.root
    }
    pub fn id(&self) -> io::Result<&str> {
        self.root
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }
    pub fn record(
        &self,
        native: NativeDocumentFileLimits,
        current: &Arc<SnapshotContext>,
    ) -> io::Result<WorkspaceRecord> {
        self.revalidate(current)?;
        let captured = CapturedDocumentDirectory::capture(&self.root, native, current)?;
        let held = self
            .directories
            .get(Path::new(""))
            .ok_or(io::ErrorKind::InvalidData)?;
        captured.verify_private_handle(held, current)?;
        self.revalidate(current)?;
        Ok(WorkspaceRecord {
            name: self.id()?.to_owned(),
            parent: self.parent.directory_identity(),
            directory: captured.directory_identity(),
        })
    }
    /// Only a fresh admitted owner and a private journal record may select this resource.
    /// The serialized identity is compared with new Core captures; it creates no grant.
    pub fn restore(
        access: &Access<'_>,
        record: WorkspaceRecord,
        native: NativeDocumentFileLimits,
    ) -> io::Result<Self> {
        access.write()?;
        if record.name.len() != 37
            || !record.name.starts_with("work-")
            || !record.name.as_bytes()[5..]
                .iter()
                .all(u8::is_ascii_hexdigit)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let context = access.original().snapshot();
        let owner_root = match access.scope().owner() {
            StoreOwner::Installation { .. } => access.scope().owner_root(),
            StoreOwner::User { .. } => access.scope().user_root()?,
            StoreOwner::Project { .. } => return Err(io::ErrorKind::PermissionDenied.into()),
        };
        let parent = CapturedDocumentDirectory::capture(
            &owner_root.join("knowledge-live"),
            native,
            context,
        )?;
        if parent.directory_identity() != record.parent {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let root = parent.canonical_path().join(&record.name);
        let captured = CapturedDocumentDirectory::capture(&root, native, context)?;
        if captured.canonical_path() != root || captured.directory_identity() != record.directory {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let file = captured.private_handle(context)?;
        let value = Self {
            parent,
            root,
            original: Arc::clone(context),
            directories: BTreeMap::from([(PathBuf::new(), file)]),
            removed: false,
            owner: access.scope().owner().clone(),
        };
        value.revalidate(context)?;
        access.write()?;
        Ok(value)
    }
    /// A new admitted turn may use the retained resource, after fresh physical and owner checks.
    pub fn rebind(&mut self, access: &Access<'_>) -> io::Result<()> {
        access.write()?;
        if access.scope().owner() != &self.owner {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.identity(access.original().snapshot())?;
        self.original = Arc::clone(access.original().snapshot());
        access.write()
    }
    fn identity(&self, context: &SnapshotContext) -> io::Result<()> {
        context.check()?;
        if self.removed {
            return Err(io::ErrorKind::NotFound.into());
        }
        self.parent.revalidate(context)?;
        let held = self
            .directories
            .get(Path::new(""))
            .ok_or(io::ErrorKind::InvalidData)?;
        verify_directory(held, &self.root, context)
    }
    pub fn revalidate(&self, context: &Arc<SnapshotContext>) -> io::Result<()> {
        if !Arc::ptr_eq(context, &self.original) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.identity(context)
    }
    pub(super) fn relative(path: &Path) -> io::Result<()> {
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
    /// A captured root File has an empty key; this never relaxes destination paths.
    pub(super) fn source_entry(reader: &CapturedSourceReader, entry: &Path) -> io::Result<()> {
        if !entry.as_os_str().is_empty() {
            Self::relative(entry)?;
        }
        let _entry = reader.open_file(entry)?;
        Ok(())
    }
    fn directory(&mut self, relative: &Path) -> io::Result<()> {
        if relative.as_os_str().is_empty() {
            return Ok(());
        }
        Self::relative(relative)?;
        let mut parent = PathBuf::new();
        for component in relative.components() {
            self.revalidate(&self.original)?;
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            let child = parent.join(name);
            if !self.directories.contains_key(&child) {
                let parent_file = self
                    .directories
                    .get(&parent)
                    .ok_or(io::ErrorKind::InvalidData)?;
                let file = graphit_fs::write_relative::create_private_directory_relative(
                    parent_file,
                    name,
                    name.as_encoded_bytes().len(),
                    &mut || self.original.check(),
                )?;
                self.directories.insert(child.clone(), file);
            }
            parent = child;
        }
        Ok(())
    }
    fn file(&mut self, relative: &Path) -> io::Result<File> {
        Self::relative(relative)?;
        let parent = relative.parent().ok_or(io::ErrorKind::InvalidInput)?;
        self.directory(parent)?;
        self.revalidate(&self.original)?;
        let name = relative.file_name().ok_or(io::ErrorKind::InvalidInput)?;
        let parent_file = self
            .directories
            .get(parent)
            .ok_or(io::ErrorKind::InvalidData)?;
        verify_directory(parent_file, &self.root.join(parent), &self.original)?;
        let _ = name;
        graphit_fs::create_private_file(&self.root.join(relative))
    }
    pub fn write(
        &mut self,
        relative: &Path,
        bytes: &[u8],
        current: &Arc<SnapshotContext>,
    ) -> io::Result<()> {
        self.revalidate(current)?;
        let mut output = self.file(relative)?;
        for block in bytes.chunks(64 * 1024) {
            self.revalidate(current)?;
            output.write_all(block)?;
        }
        output.sync_all()?;
        self.revalidate(current)
    }
    pub fn copy_assets(
        &mut self,
        reader: &CapturedSourceReader,
        source_entry: &Path,
        native_entry: &Path,
        current: &Arc<SnapshotContext>,
    ) -> io::Result<()> {
        Self::relative(native_entry)?;
        let mount = native_entry.parent().ok_or(io::ErrorKind::InvalidInput)?;
        self.revalidate(current)?;
        reader.check_request(current)?;
        reader.revalidate_content()?;
        Self::source_entry(reader, source_entry)?;
        for node in reader.nodes() {
            self.revalidate(current)?;
            if node.relative() == source_entry {
                continue;
            }
            let destination = mount.join(node.relative());
            match node.kind() {
                NodeKind::Directory => self.directory(&destination)?,
                NodeKind::File => {
                    let input = reader.open_file(node.relative())?;
                    let mut stream = input.stream(0)?;
                    let mut output = self.file(&destination)?;
                    let mut block = [0_u8; 64 * 1024];
                    loop {
                        self.revalidate(current)?;
                        let read = stream.read(&mut block)?;
                        self.revalidate(current)?;
                        if read == 0 {
                            break;
                        }
                        output.write_all(&block[..read])?;
                    }
                    output.sync_all()?;
                }
            }
        }
        reader.revalidate_content()?;
        self.revalidate(current)
    }
    fn open_directory(&self, relative: &Path, current: &Arc<SnapshotContext>) -> io::Result<File> {
        if !relative.as_os_str().is_empty() {
            Self::relative(relative)?;
        }
        self.revalidate(current)?;
        let mut directory = self
            .directories
            .get(Path::new(""))
            .ok_or(io::ErrorKind::InvalidData)?
            .try_clone()?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            directory = graphit_fs::write_relative::open_private_directory_relative(
                &directory,
                name,
                name.as_encoded_bytes().len(),
                &mut || self.revalidate(current),
            )?;
        }
        self.revalidate(current)?;
        Ok(directory)
    }
    fn open(&self, relative: &Path, current: &Arc<SnapshotContext>) -> io::Result<File> {
        Self::relative(relative)?;
        let parent = self.open_directory(
            relative.parent().ok_or(io::ErrorKind::InvalidInput)?,
            current,
        )?;
        let name = relative.file_name().ok_or(io::ErrorKind::InvalidInput)?;
        graphit_fs::write_relative::open_private_file_relative(
            &parent,
            name,
            name.as_encoded_bytes().len(),
            &mut || self.revalidate(current),
        )
    }
    pub fn matches(
        &self,
        relative: &Path,
        bytes: &[u8],
        current: &Arc<SnapshotContext>,
    ) -> io::Result<bool> {
        Self::relative(relative)?;
        self.revalidate(current)?;
        let file = self.open(relative, current)?;
        let ceiling = bytes
            .len()
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        let mut observed = Vec::new();
        observed
            .try_reserve_exact(ceiling)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        file.take(u64::try_from(ceiling).map_err(|_| io::ErrorKind::FileTooLarge)?)
            .read_to_end(&mut observed)?;
        self.revalidate(current)?;
        Ok(observed == bytes)
    }
    pub fn matches_assets(
        &self,
        reader: &CapturedSourceReader,
        source_entry: &Path,
        native_entry: &Path,
        current: &Arc<SnapshotContext>,
    ) -> io::Result<bool> {
        Self::relative(native_entry)?;
        self.revalidate(current)?;
        reader.check_request(current)?;
        reader.revalidate_content()?;
        Self::source_entry(reader, source_entry)?;
        let mount = native_entry.parent().ok_or(io::ErrorKind::InvalidInput)?;
        for node in reader.nodes() {
            if node.relative() == source_entry {
                continue;
            }
            if node.kind() == NodeKind::Directory {
                let _directory = self.open_directory(&mount.join(node.relative()), current)?;
                continue;
            }
            let source = reader.open_file(node.relative())?;
            let mut source = source.stream(0)?;
            let mut output = self.open(&mount.join(node.relative()), current)?;
            let mut expected = [0_u8; 64 * 1024];
            let mut observed = [0_u8; 64 * 1024];
            loop {
                self.revalidate(current)?;
                let read = source.read(&mut expected)?;
                if read == 0 {
                    if output.read(&mut observed[..1])? != 0 {
                        return Ok(false);
                    }
                    break;
                }
                if let Err(error) = output.read_exact(&mut observed[..read]) {
                    return if error.kind() == io::ErrorKind::UnexpectedEof {
                        Ok(false)
                    } else {
                        Err(error)
                    };
                }
                self.revalidate(current)?;
                if expected[..read] != observed[..read] {
                    return Ok(false);
                }
            }
        }
        reader.revalidate_content()?;
        self.revalidate(current)?;
        Ok(true)
    }
    /// Call only after actual owned-process quiescence. A bounded cleanup context
    /// checks physical custody; it does not authorize another native execution.
    pub fn cleanup(&mut self, cleanup: &SnapshotContext) -> io::Result<()> {
        if self.removed {
            return Ok(());
        }
        self.identity(cleanup)?;
        fs::remove_dir_all(&self.root)?;
        self.removed = true;
        self.parent.revalidate(cleanup)
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    fn context(cancel: Cancellation) -> SnapshotContext {
        SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel)
    }

    #[test]
    fn retained_birth_handle_rejects_another_private_directory() -> io::Result<()> {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().join("birth");
        let other = temporary.path().join("other");
        let file = graphit_fs::create_private_directory(&path)?;
        let _other = graphit_fs::create_private_directory(&other)?;
        let context = context(Cancellation::default());
        verify_directory(&file, &path, &context)?;
        assert_eq!(
            verify_directory(&file, &other, &context)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        verify_directory(&file, &path, &context)
    }

    #[test]
    fn cancelled_original_does_not_validate_a_live_directory() -> io::Result<()> {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().join("birth");
        let file = graphit_fs::create_private_directory(&path)?;
        let cancel = Cancellation::default();
        let context = context(cancel.clone());
        verify_directory(&file, &path, &context)?;
        cancel.cancel();
        assert!(verify_directory(&file, &path, &context).is_err());
        assert!(path.is_dir());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlink_and_relaxed_access_do_not_validate_birth() -> io::Result<()> {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().join("birth");
        let file = graphit_fs::create_private_directory(&path)?;
        let link = temporary.path().join("link");
        symlink(&path, &link)?;
        let context = context(Cancellation::default());
        assert_eq!(
            verify_directory(&file, &link, &context).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        assert!(verify_directory(&file, &path, &context).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        verify_directory(&file, &path, &context)
    }
}
