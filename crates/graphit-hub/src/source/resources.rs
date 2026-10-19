//! Observe authored identities without capturing their payloads or granting access.
use super::{ArtifactId, ArtifactType, ConcreteVersion, identity_directory};
use crate::{ProjectionError, ProjectionLimits, ProjectionStage};
use graphit_core::project_paths::ProjectPaths;
use graphit_fs::{
    NodeKind,
    read_directory::{self, DirectoryEntries, DirectoryEntryKind},
};
use serde::Serialize;
use std::{
    cmp::Ordering, collections::BTreeSet, ffi::OsStr, fs::File, io, num::NonZeroUsize, path::Path,
};

const NATIVE_BYTE_WIDTH: usize = if cfg!(windows) { 2 } else { 1 };
const IDENTITY_COMPONENT_BYTES: usize = 130 * NATIVE_BYTE_WIDTH;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResourceIdentity {
    pub artifact_type: ArtifactType,
    pub id: ArtifactId,
    pub version: ConcreteVersion,
}
impl Ord for ResourceIdentity {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.artifact_type.as_str(),
            self.id.as_str(),
            self.version.as_str(),
        )
            .cmp(&(
                other.artifact_type.as_str(),
                other.id.as_str(),
                other.version.as_str(),
            ))
    }
}
impl PartialOrd for ResourceIdentity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ResourcesQuery<'a> {
    pub artifact_type: Option<ArtifactType>,
    pub id: Option<&'a ArtifactId>,
    pub version: Option<&'a ConcreteVersion>,
    pub after: Option<&'a ResourceIdentity>,
    pub limit: NonZeroUsize,
}
#[derive(Debug)]
pub struct ResourcesPage {
    pub entries: Vec<ResourceIdentity>,
    pub next_after: Option<ResourceIdentity>,
}

/// Lists the current Resources namespace in lexical kind/ID/version order.
///
/// The caller retains and checks PROJECT authority through `before`, including
/// before consuming this observation. A boundary is data, never authorization.
/// This is not a cross-request snapshot: refresh to see additions before it.
/// Only a page and one lookahead are retained; payload directories are not read.
pub fn list_resources(
    project: &ProjectPaths,
    query: ResourcesQuery<'_>,
    limits: ProjectionLimits,
    before: &mut impl FnMut() -> io::Result<()>,
) -> Result<ResourcesPage, ProjectionError> {
    if query.limit.get() > limits.page_entries() {
        return Err(ProjectionError::new(
            ProjectionStage::Validate,
            io::ErrorKind::InvalidInput,
        ));
    }
    if let Some(id) = query.id {
        limits.identity(id.as_str())?;
    }
    if let Some(version) = query.version {
        limits.identity(version.as_str())?;
    }
    if let Some(after) = query.after {
        limits.identity(after.id.as_str())?;
        limits.identity(after.version.as_str())?;
        if !matches_filter(query, after) {
            return Err(ProjectionError::new(
                ProjectionStage::Validate,
                io::ErrorKind::InvalidInput,
            ));
        }
    }
    let mut scan = Scan {
        query,
        limits,
        selected: BTreeSet::new(),
    };
    scan.read(project, before)
        .map_err(|error| ProjectionError::new(ProjectionStage::Enumerate, error.kind()))?;
    let has_more = scan.selected.len() > query.limit.get();
    if has_more {
        scan.selected.pop_last();
    }
    let next_after = has_more.then(|| scan.selected.last().expect("positive page limit").clone());
    Ok(ResourcesPage {
        entries: scan.selected.into_iter().collect(),
        next_after,
    })
}

fn matches_filter(query: ResourcesQuery<'_>, item: &ResourceIdentity) -> bool {
    query
        .artifact_type
        .is_none_or(|kind| kind == item.artifact_type)
        && query.id.is_none_or(|id| id == &item.id)
        && query.version.is_none_or(|version| version == &item.version)
}

struct Scan<'a> {
    query: ResourcesQuery<'a>,
    limits: ProjectionLimits,
    selected: BTreeSet<ResourceIdentity>,
}
impl Scan<'_> {
    fn read(
        &mut self,
        project: &ProjectPaths,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        let root = read_directory::open_root_readonly(project.root(), before)?;
        let Some(brand) = optional_directory(&root, project.brand().directory(), before)? else {
            return before();
        };
        let Some(artifacts) = optional_directory(&brand, "artifacts", before)? else {
            return before();
        };
        for kind in ArtifactType::ALL {
            if self
                .query
                .artifact_type
                .is_some_and(|selected| selected != kind)
            {
                continue;
            }
            let Some(directory) = optional_directory(&artifacts, kind.as_str(), before)? else {
                continue;
            };
            let Some(ids) = optional_directory(&directory, "ids", before)? else {
                continue;
            };
            let base = self
                .limits
                .append(
                    project.root(),
                    &[project.brand().directory(), "artifacts", kind.as_str()],
                )
                .map_err(|error| io::Error::new(error.kind(), error))?;
            self.identities(ids, kind, base.as_path(), before)?;
            read_directory::inspect(&directory, Some(NodeKind::Directory), before)?;
        }
        read_directory::inspect(&artifacts, Some(NodeKind::Directory), before)?;
        read_directory::inspect(&brand, Some(NodeKind::Directory), before)?;
        read_directory::inspect(&root, Some(NodeKind::Directory), before)?;
        before()
    }

    fn identities(
        &mut self,
        ids: File,
        kind: ArtifactType,
        base: &Path,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        let mut frames = vec![Frame::new(ids, Phase::Id(true), 0, 0, before)?];
        let mut id = Vec::new();
        let mut version = Vec::new();
        while let Some(frame) = frames.last_mut() {
            id.truncate(frame.id_len);
            version.truncate(frame.version_len);
            let Some(entry) = frame.entries.next_entry(IDENTITY_COMPONENT_BYTES, before)? else {
                read_directory::inspect(&frame.directory, Some(NodeKind::Directory), before)?;
                frames.pop();
                continue;
            };
            let name = entry.name.to_str().ok_or(io::ErrorKind::InvalidData)?;
            match frame.phase {
                Phase::Id(continuation) if name != "versions" => {
                    decode_chunk(
                        name,
                        "i-",
                        continuation,
                        &mut id,
                        self.limits.identity_bytes(),
                    )?;
                    require_directory(entry.kind)?;
                    let child = read_directory::open_relative_readonly(
                        &frame.directory,
                        &entry.name,
                        Some(NodeKind::Directory),
                        IDENTITY_COMPONENT_BYTES,
                        before,
                    )?;
                    frames.push(Frame::new(
                        child,
                        Phase::Id(name.len() == 130),
                        id.len(),
                        0,
                        before,
                    )?);
                }
                Phase::Id(_) => {
                    let parsed = ArtifactId::new(text(&id)?, self.limits)
                        .map_err(|_| io::ErrorKind::InvalidData)?;
                    require_directory(entry.kind)?;
                    if self.query.id.is_some_and(|selected| selected != &parsed) {
                        continue;
                    }
                    let child = read_directory::open_relative_readonly(
                        &frame.directory,
                        &entry.name,
                        Some(NodeKind::Directory),
                        IDENTITY_COMPONENT_BYTES,
                        before,
                    )?;
                    frames.push(Frame::new(
                        child,
                        Phase::Version(true),
                        id.len(),
                        0,
                        before,
                    )?);
                }
                Phase::Version(continuation) if name != "payload" => {
                    decode_chunk(
                        name,
                        "v-",
                        continuation,
                        &mut version,
                        self.limits.identity_bytes(),
                    )?;
                    require_directory(entry.kind)?;
                    let child = read_directory::open_relative_readonly(
                        &frame.directory,
                        &entry.name,
                        Some(NodeKind::Directory),
                        IDENTITY_COMPONENT_BYTES,
                        before,
                    )?;
                    frames.push(Frame::new(
                        child,
                        Phase::Version(name.len() == 130),
                        id.len(),
                        version.len(),
                        before,
                    )?);
                }
                Phase::Version(_) => {
                    let item = ResourceIdentity {
                        artifact_type: kind,
                        id: ArtifactId::new(text(&id)?, self.limits)
                            .map_err(|_| io::ErrorKind::InvalidData)?,
                        version: ConcreteVersion::new(text(&version)?, self.limits)
                            .map_err(|_| io::ErrorKind::InvalidData)?,
                    };
                    if !matches_filter(self.query, &item) {
                        continue;
                    }
                    require_directory(entry.kind)?;
                    let _payload = read_directory::open_relative_readonly(
                        &frame.directory,
                        &entry.name,
                        Some(NodeKind::Directory),
                        IDENTITY_COMPONENT_BYTES,
                        before,
                    )?;
                    identity_directory(base, &item.id, &item.version, self.limits)
                        .map_err(|error| io::Error::new(error.kind(), error))?;
                    if self.query.after.is_some_and(|after| &item <= after) {
                        continue;
                    }
                    self.selected.insert(item);
                    if self.selected.len() > self.query.limit.get() + 1 {
                        self.selected.pop_last();
                    }
                }
            }
        }
        Ok(())
    }
}

fn optional_directory(
    parent: &File,
    name: &str,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<Option<File>> {
    match read_directory::open_relative_readonly(
        parent,
        OsStr::new(name),
        Some(NodeKind::Directory),
        255 * NATIVE_BYTE_WIDTH,
        before,
    ) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn require_directory(kind: DirectoryEntryKind) -> io::Result<()> {
    if kind == DirectoryEntryKind::Directory {
        Ok(())
    } else {
        Err(io::ErrorKind::InvalidData.into())
    }
}
fn text(bytes: &[u8]) -> io::Result<&str> {
    if bytes.is_empty() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    std::str::from_utf8(bytes).map_err(|_| io::ErrorKind::InvalidData.into())
}
fn decode_chunk(
    name: &str,
    prefix: &str,
    continuation: bool,
    output: &mut Vec<u8>,
    limit: usize,
) -> io::Result<()> {
    let hex = name
        .strip_prefix(prefix)
        .ok_or(io::ErrorKind::InvalidData)?;
    if !continuation || hex.is_empty() || hex.len() > 128 || !hex.len().is_multiple_of(2) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let bytes = hex.len() / 2;
    if output
        .len()
        .checked_add(bytes)
        .is_none_or(|total| total > limit)
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    output
        .try_reserve(bytes)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    for pair in hex.as_bytes().as_chunks::<2>().0 {
        let digit = |byte| match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err(io::ErrorKind::InvalidData),
        };
        output.push((digit(pair[0])? << 4) | digit(pair[1])?);
    }
    Ok(())
}
#[derive(Clone, Copy)]
enum Phase {
    Id(bool),
    Version(bool),
}
struct Frame {
    directory: File,
    entries: DirectoryEntries,
    phase: Phase,
    id_len: usize,
    version_len: usize,
}
impl Frame {
    fn new(
        directory: File,
        phase: Phase,
        id_len: usize,
        version_len: usize,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<Self> {
        let entries = DirectoryEntries::new(&directory, 8192, before)?;
        Ok(Self {
            directory,
            entries,
            phase,
            id_len,
            version_len,
        })
    }
}

#[cfg(test)]
mod tests;
