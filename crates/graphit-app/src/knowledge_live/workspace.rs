//! Private prepared files owned until the real native process has quiesced.
use super::Access;
use graphit_core::{
    artifact_store::{CapturedSourceReader, StoreOwner},
    file_io::SnapshotContext,
};
use graphit_fs::NodeKind;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub(super) struct Workspace {
    root: PathBuf,
    removed: bool,
    owner: StoreOwner,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkspaceRecord {
    pub name: String,
}

fn owner_root<'a>(access: &'a Access<'_>) -> io::Result<&'a Path> {
    match access.scope().owner() {
        StoreOwner::Installation { .. } => Ok(access.scope().owner_root()),
        StoreOwner::User { .. } => access.scope().user_root(),
        StoreOwner::Project { .. } => Err(io::ErrorKind::PermissionDenied.into()),
    }
}
impl Workspace {
    pub fn create(access: &Access<'_>) -> io::Result<Self> {
        access.write()?;
        let parent = owner_root(access)?.join("knowledge-live");
        fs::create_dir_all(&parent)?;
        let root = parent.join(format!("work-{}", super::service::generation()?));
        // The leaf holds prepared prompts. Privacy applies to the actual resource,
        // without retaining directory handles or imposing rules on configured parents.
        drop(graphit_fs::create_private_directory(&root)?);
        Ok(Self {
            root,
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
    pub fn record(&self, current: &Arc<SnapshotContext>) -> io::Result<WorkspaceRecord> {
        self.ready(current)?;
        Ok(WorkspaceRecord {
            name: self.id()?.to_owned(),
        })
    }
    /// Only a freshly admitted owner may select a workspace named by private history.
    pub fn restore(access: &Access<'_>, record: WorkspaceRecord) -> io::Result<Self> {
        access.write()?;
        if record.name.len() != 37
            || !record.name.starts_with("work-")
            || !record.name.as_bytes()[5..]
                .iter()
                .all(u8::is_ascii_hexdigit)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let root = owner_root(access)?.join("knowledge-live").join(record.name);
        if !fs::metadata(&root)?.is_dir() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        access.write()?;
        Ok(Self {
            root,
            removed: false,
            owner: access.scope().owner().clone(),
        })
    }
    pub fn admit(&self, access: &Access<'_>) -> io::Result<()> {
        access.write()?;
        if access.scope().owner() != &self.owner {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.ready(access.original().snapshot())
    }
    fn ready(&self, current: &SnapshotContext) -> io::Result<()> {
        current.check()?;
        if self.removed {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(())
    }
    /// Destination locators from external artifacts cannot escape the workspace.
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
    pub(super) fn source_entry(reader: &CapturedSourceReader, entry: &Path) -> io::Result<()> {
        if !entry.as_os_str().is_empty() {
            Self::relative(entry)?;
        }
        let _entry = reader.open_file(entry)?;
        Ok(())
    }
    fn directory(&self, relative: &Path, current: &SnapshotContext) -> io::Result<()> {
        self.ready(current)?;
        let mut path = self.root.clone();
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            path.push(name);
            match graphit_fs::create_private_directory(&path) {
                Ok(_) => (),
                Err(error)
                    if error.kind() == io::ErrorKind::AlreadyExists
                        && fs::metadata(&path)?.is_dir() => {}
                Err(error) => return Err(error),
            }
            current.check()?;
        }
        Ok(())
    }
    fn file(&self, relative: &Path, current: &SnapshotContext) -> io::Result<File> {
        Self::relative(relative)?;
        self.directory(
            relative.parent().ok_or(io::ErrorKind::InvalidInput)?,
            current,
        )?;
        graphit_fs::create_private_file(&self.root.join(relative))
    }
    pub fn write(
        &mut self,
        relative: &Path,
        bytes: &[u8],
        current: &Arc<SnapshotContext>,
    ) -> io::Result<()> {
        self.ready(current)?;
        let mut output = self.file(relative, current)?;
        for block in bytes.chunks(64 * 1024) {
            current.check()?;
            output.write_all(block)?;
        }
        output.sync_all()?;
        current.check()
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
        self.ready(current)?;
        Self::source_entry(reader, source_entry)?;
        for node in reader.nodes() {
            self.ready(current)?;
            if node.relative() == source_entry {
                continue;
            }
            let destination = mount.join(node.relative());
            match node.kind() {
                NodeKind::Directory => self.directory(&destination, current)?,
                NodeKind::File => {
                    let input = reader.open_file(node.relative())?;
                    let mut stream = input.stream(0)?;
                    let mut output = self.file(&destination, current)?;
                    let mut block = [0; 64 * 1024];
                    loop {
                        current.check()?;
                        let count = stream.read(&mut block)?;
                        if count == 0 {
                            break;
                        }
                        output.write_all(&block[..count])?;
                    }
                    output.sync_all()?;
                }
            }
        }
        self.ready(current)
    }
    fn open(&self, relative: &Path, current: &SnapshotContext) -> io::Result<File> {
        Self::relative(relative)?;
        self.ready(current)?;
        let file = File::open(self.root.join(relative))?;
        graphit_fs::verify_private_file(&file)?;
        Ok(file)
    }
    pub fn matches(
        &self,
        relative: &Path,
        bytes: &[u8],
        current: &Arc<SnapshotContext>,
    ) -> io::Result<bool> {
        let mut file = self.open(relative, current)?;
        let mut offset = 0;
        let mut block = [0; 64 * 1024];
        loop {
            current.check()?;
            let count = file.read(&mut block)?;
            if count == 0 {
                return Ok(offset == bytes.len());
            }
            if bytes.get(offset..offset + count) != Some(&block[..count]) {
                return Ok(false);
            }
            offset += count;
        }
    }
    pub fn matches_assets(
        &self,
        reader: &CapturedSourceReader,
        source_entry: &Path,
        native_entry: &Path,
        current: &Arc<SnapshotContext>,
    ) -> io::Result<bool> {
        Self::relative(native_entry)?;
        self.ready(current)?;
        Self::source_entry(reader, source_entry)?;
        let mount = native_entry.parent().ok_or(io::ErrorKind::InvalidInput)?;
        for node in reader.nodes() {
            current.check()?;
            if node.relative() == source_entry {
                continue;
            }
            let path = mount.join(node.relative());
            if node.kind() == NodeKind::Directory {
                if !fs::metadata(self.root.join(&path))?.is_dir() {
                    return Ok(false);
                }
                continue;
            }
            let source = reader.open_file(node.relative())?;
            let mut source = source.stream(0)?;
            let mut output = self.open(&path, current)?;
            let mut expected = [0; 64 * 1024];
            let mut observed = [0; 64 * 1024];
            loop {
                current.check()?;
                let count = source.read(&mut expected)?;
                if count == 0 {
                    if output.read(&mut observed[..1])? != 0 {
                        return Ok(false);
                    }
                    break;
                }
                if let Err(error) = output.read_exact(&mut observed[..count]) {
                    return if error.kind() == io::ErrorKind::UnexpectedEof {
                        Ok(false)
                    } else {
                        Err(error)
                    };
                }
                if expected[..count] != observed[..count] {
                    return Ok(false);
                }
            }
        }
        self.ready(current)?;
        Ok(true)
    }
    /// Call only after actual owned-process quiescence.
    pub fn cleanup(&mut self, cleanup: &SnapshotContext) -> io::Result<()> {
        if self.removed {
            return Ok(());
        }
        cleanup.check()?;
        fs::remove_dir_all(&self.root)?;
        self.removed = true;
        Ok(())
    }
}
