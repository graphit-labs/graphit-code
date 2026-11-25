//! Logical projection planning using ordinary configured paths.
use crate::source::invalid;
use crate::{ArtifactType, InstallationTarget, NativeBinding, SourceRef};
use graphit_core::{
    artifact_store::{ArtifactKey, ArtifactMode, CopyPolicy, PayloadSelection},
    native_path::NativePath,
};
use graphit_fs::NodeKind;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionStage {
    Validate,
    Source,
    Select,
    Enumerate,
    Destination,
    Collision,
    Unsupported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionError {
    stage: ProjectionStage,
    kind: io::ErrorKind,
}
impl ProjectionError {
    pub(crate) fn new(stage: ProjectionStage, kind: io::ErrorKind) -> Self {
        Self { stage, kind }
    }
    pub fn stage(self) -> ProjectionStage {
        self.stage
    }
    pub fn kind(self) -> io::ErrorKind {
        self.kind
    }
}
impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "artifact projection {:?}: {:?}", self.stage, self.kind)
    }
}
impl std::error::Error for ProjectionError {}
fn at(stage: ProjectionStage, e: io::Error) -> ProjectionError {
    ProjectionError::new(stage, e.kind())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionLimits {
    identity_bytes: usize,
    entries: usize,
    name_bytes: usize,
    batch: usize,
}
impl ProjectionLimits {
    pub(crate) fn identity_bytes(self) -> usize {
        self.identity_bytes
    }
    pub(crate) fn page_entries(self) -> usize {
        self.entries
    }
    pub(crate) fn validate_batch(self, items: usize) -> Result<(), ProjectionError> {
        if items > self.batch {
            Err(invalid(ProjectionStage::Validate))
        } else {
            Ok(())
        }
    }
    pub fn new(
        identity_bytes: usize,
        entries: usize,
        name_bytes: usize,
        batch: usize,
    ) -> Result<Self, ProjectionError> {
        if !(1..=64 * 1024).contains(&identity_bytes)
            || !(1..=100_000).contains(&entries)
            || !(1..=16 * 1024 * 1024).contains(&name_bytes)
            || !(1..=100_000).contains(&batch)
        {
            return Err(invalid(ProjectionStage::Validate));
        }
        Ok(Self {
            identity_bytes,
            entries,
            name_bytes,
            batch,
        })
    }
    pub(crate) fn identity(self, value: &str) -> Result<(), ProjectionError> {
        if value.is_empty()
            || value.len() > self.identity_bytes
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(invalid(ProjectionStage::Validate));
        }
        Ok(())
    }
    pub(crate) fn path(self, path: PathBuf) -> Result<NativePath, ProjectionError> {
        NativePath::new(path).map_err(|e| at(ProjectionStage::Validate, e))
    }
    pub(crate) fn join(
        self,
        parent: &Path,
        name: &std::ffi::OsStr,
    ) -> Result<NativePath, ProjectionError> {
        self.path(parent.join(name))
    }
    pub(crate) fn append(self, root: &Path, parts: &[&str]) -> Result<NativePath, ProjectionError> {
        let mut path = self.path(root.to_owned())?;
        for part in parts {
            path = self.join(path.as_path(), std::ffi::OsStr::new(part))?;
        }
        Ok(path)
    }
}
#[cfg(windows)]
fn native_len(path: &Path) -> usize {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().count().saturating_mul(2)
}
#[cfg(not(windows))]
fn native_len(path: &Path) -> usize {
    path.as_os_str().as_encoded_bytes().len()
}

pub(crate) fn validate_name(value: &str) -> Result<(), ProjectionError> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 255
        || value.encode_utf16().count() > 255
        || value.ends_with('.')
        || matches!(value, "." | "..")
        || value
            .chars()
            .any(|c| c.is_control() || "/\\<>:\"|?*".contains(c))
    {
        return Err(invalid(ProjectionStage::Validate));
    }
    let base = value.split('.').next().unwrap_or_default();
    if ["CON", "PRN", "AUX", "NUL"]
        .iter()
        .any(|reserved| base.eq_ignore_ascii_case(reserved))
        || (["COM", "LPT"].iter().any(|prefix| {
            base.get(..3)
                .is_some_and(|v| v.eq_ignore_ascii_case(prefix))
        }) && matches!(
            base.get(3..),
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
        ))
    {
        return Err(invalid(ProjectionStage::Validate));
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "NodeKind")]
pub(crate) enum NodeKindDef {
    File,
    Directory,
}

#[derive(Clone, Copy)]
pub struct ProjectionRequest<'a> {
    pub binding: NativeBinding,
    pub source: &'a SourceRef,
    pub alias: Option<&'a str>,
    pub entry_name: Option<&'a str>,
}
impl ProjectionRequest<'_> {
    pub fn target(&self) -> InstallationTarget {
        InstallationTarget::for_artifact(self.binding, self.source.artifact_type())
    }
}
#[derive(Clone, Debug)]
pub struct ProjectionPlan {
    binding: NativeBinding,
    key: ArtifactKey,
    source: SourceRef,
    selected: PathBuf,
    kind: NodeKind,
    destination: NativePath,
    name: String,
    mode: ArtifactMode,
}
impl ProjectionPlan {
    pub fn binding(&self) -> NativeBinding {
        self.binding
    }
    pub fn key(&self) -> &ArtifactKey {
        &self.key
    }
    pub fn source(&self) -> &SourceRef {
        &self.source
    }
    pub fn selected(&self) -> &Path {
        &self.selected
    }
    pub fn kind(&self) -> NodeKind {
        self.kind
    }
    pub fn destination(&self) -> &NativePath {
        &self.destination
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn mode(&self) -> ArtifactMode {
        self.mode
    }
}
#[derive(Clone, Debug)]
pub struct RulePlan {
    key: ArtifactKey,
    source: SourceRef,
    selected: PathBuf,
}
impl RulePlan {
    pub fn key(&self) -> &ArtifactKey {
        &self.key
    }
    pub fn source(&self) -> &SourceRef {
        &self.source
    }
    pub fn selected(&self) -> &Path {
        &self.selected
    }
}
#[derive(Clone, Debug)]
pub struct NativeCapability {
    key: ArtifactKey,
    source: SourceRef,
}
impl NativeCapability {
    pub fn key(&self) -> &ArtifactKey {
        &self.key
    }
    pub fn source(&self) -> &SourceRef {
        &self.source
    }
}
#[derive(Clone, Debug)]
pub enum Projection {
    Physical(Box<ProjectionPlan>),
    DynamicRule(Box<RulePlan>),
    Native(Box<NativeCapability>),
}

pub struct ProjectionPlanner {
    destination_root: NativePath,
    limits: ProjectionLimits,
    mode: ArtifactMode,
}
/// Pure derived data for provisioning/removal. No filesystem observation or ownership proof.
#[derive(Clone, Debug)]
pub struct ProjectionTarget {
    key: ArtifactKey,
    name: String,
    destination: Option<NativePath>,
    canonical: Option<&'static str>,
}
impl ProjectionTarget {
    pub fn key(&self) -> &ArtifactKey {
        &self.key
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn destination(&self) -> Option<&NativePath> {
        self.destination.as_ref()
    }
    pub fn kind(&self) -> Option<NodeKind> {
        self.destination.as_ref().map(|_| {
            if self.canonical.is_some() {
                NodeKind::File
            } else {
                NodeKind::Directory
            }
        })
    }
}
impl ProjectionPlanner {
    pub fn new(destination_root: NativePath, limits: ProjectionLimits, mode: ArtifactMode) -> Self {
        Self {
            destination_root,
            limits,
            mode,
        }
    }
    pub fn plan(&self, request: ProjectionRequest<'_>) -> Result<Projection, ProjectionError> {
        let prepared = self.prepare(&request)?;
        self.resolve(request, prepared)
    }
    pub(crate) fn plan_derived(
        &self,
        request: ProjectionRequest<'_>,
        original: &PayloadSelection,
    ) -> Result<Projection, ProjectionError> {
        // DerivedArtifact already validated this selection against its input snapshot.
        // Retained rendered output does not require reopening that original path.
        let prepared = self.prepare(&request)?;
        let selected = request
            .source
            .directory()
            .as_path()
            .join(original.relative());
        Ok(self.project_selected(request, prepared, selected))
    }
    /// Pure name/key/lexical-collision validation, usable before caller parent provisioning.
    /// This neither observes filesystem aliases nor reserves any destination.
    pub fn preflight(
        &self,
        requests: &[ProjectionRequest<'_>],
    ) -> Result<Vec<ProjectionTarget>, ProjectionError> {
        self.prepare_batch(requests)
    }
    fn prepare_batch(
        &self,
        requests: &[ProjectionRequest<'_>],
    ) -> Result<Vec<ProjectionTarget>, ProjectionError> {
        self.limits.validate_batch(requests.len())?;
        let mut keys = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        let mut prepared = Vec::with_capacity(requests.len());
        for request in requests {
            let entry = self.prepare(request)?;
            if !keys.insert((
                request.target().namespace(),
                request.source.artifact_type().as_str(),
                request.source.id().as_str(),
            )) || entry
                .destination
                .as_ref()
                .is_some_and(|d| !destinations.insert(fold_path(d.as_path())))
            {
                return Err(ProjectionError::new(
                    ProjectionStage::Collision,
                    io::ErrorKind::AlreadyExists,
                ));
            }
            prepared.push(entry);
        }
        Ok(prepared)
    }
    pub(crate) fn prepare(
        &self,
        request: &ProjectionRequest<'_>,
    ) -> Result<ProjectionTarget, ProjectionError> {
        self.limits.identity(request.source.id().as_str())?;
        self.limits
            .identity(request.source.physical_id().as_str())?;
        self.limits.identity(request.source.version().as_str())?;
        let name = request
            .alias
            .or_else(|| request.entry_name.filter(|name| !name.is_empty()))
            .unwrap_or_else(|| request.source.id().basename());
        validate_name(name)?;
        let key = ArtifactKey::new(
            request.target().namespace(),
            request.source.artifact_type().as_str(),
            request.source.id().as_str(),
        )
        .map_err(|e| at(ProjectionStage::Validate, e))?;
        let layout = request.binding.adapter().layout();
        let (folder, canonical, extension) = match request.source.artifact_type() {
            ArtifactType::Skill => (Some(layout.skills), None, ""),
            ArtifactType::Command | ArtifactType::Workflow => {
                // Kiro workflows require the selected engine's hook producer, not Markdown.
                if request.binding.adapter() == crate::AdapterId::Kiro {
                    return Err(ProjectionError::new(
                        ProjectionStage::Unsupported,
                        io::ErrorKind::Unsupported,
                    ));
                }
                let folder = layout.commands.ok_or_else(|| {
                    ProjectionError::new(ProjectionStage::Unsupported, io::ErrorKind::Unsupported)
                })?;
                (Some(folder), Some("COMMAND.md"), "md")
            }
            ArtifactType::Agent => (
                Some(layout.agents),
                Some("AGENT.md"),
                request.binding.agent_extension(),
            ),
            ArtifactType::Rule => (None, Some("RULE.md"), ""),
            _ => (None, None, ""),
        };
        let destination = if let Some(folder) = folder {
            let file = if extension.is_empty() {
                name.to_owned()
            } else {
                format!("{name}.{extension}")
            };
            validate_name(&file)?;
            Some(self.limits.append(
                self.destination_root.as_path(),
                &[
                    if request.source.artifact_type() == ArtifactType::Skill
                        && request.binding == NativeBinding::Codex
                    {
                        ".agents"
                    } else {
                        layout.root
                    },
                    folder,
                    &file,
                ],
            )?)
        } else {
            None
        };
        Ok(ProjectionTarget {
            key,
            name: name.into(),
            destination,
            canonical,
        })
    }
    fn resolve(
        &self,
        request: ProjectionRequest<'_>,
        prepared: ProjectionTarget,
    ) -> Result<Projection, ProjectionError> {
        if prepared.destination.is_none() && prepared.canonical.is_none() {
            return Ok(self.project_selected(
                request,
                prepared,
                request.source.directory().as_path().to_owned(),
            ));
        }
        let directory = request.source.directory().as_path();
        if !fs::metadata(directory)
            .map_err(|e| at(ProjectionStage::Source, e))?
            .is_dir()
        {
            return Err(ProjectionError::new(
                ProjectionStage::Source,
                io::ErrorKind::NotADirectory,
            ));
        }
        let selected = match prepared.canonical {
            Some(name) => select_file(directory, name, self.limits, self.mode)?,
            None => directory.to_owned(),
        };
        Ok(self.project_selected(request, prepared, selected))
    }
    fn project_selected(
        &self,
        request: ProjectionRequest<'_>,
        prepared: ProjectionTarget,
        selected: PathBuf,
    ) -> Projection {
        if prepared.destination.is_none() && prepared.canonical.is_none() {
            return Projection::Native(Box::new(NativeCapability {
                key: prepared.key,
                source: request.source.clone(),
            }));
        }
        let kind = if prepared.canonical.is_some() {
            NodeKind::File
        } else {
            NodeKind::Directory
        };
        let Some(destination) = prepared.destination else {
            return Projection::DynamicRule(Box::new(RulePlan {
                key: prepared.key,
                source: request.source.clone(),
                selected,
            }));
        };
        Projection::Physical(Box::new(ProjectionPlan {
            binding: request.binding,
            key: prepared.key,
            source: request.source.clone(),
            selected,
            kind,
            destination,
            name: prepared.name,
            mode: self.mode,
        }))
    }
}

fn select_file(
    root: &Path,
    canonical: &str,
    limits: ProjectionLimits,
    mode: ArtifactMode,
) -> Result<PathBuf, ProjectionError> {
    let path = limits.join(root, std::ffi::OsStr::new(canonical))?;
    match fs::symlink_metadata(path.as_path()) {
        Ok(metadata) => {
            if metadata.is_symlink()
                && matches!(mode, ArtifactMode::Copy(CopyPolicy::RejectEntryLinks))
            {
                return Err(invalid(ProjectionStage::Select));
            }
            if !fs::metadata(path.as_path())
                .map_err(|e| at(ProjectionStage::Select, e))?
                .is_file()
            {
                return Err(invalid(ProjectionStage::Select));
            }
            return Ok(path.as_path().to_owned());
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(at(ProjectionStage::Select, e)),
    }
    let mut paths = Vec::new();
    let mut bytes = 0usize;
    for item in fs::read_dir(root).map_err(|e| at(ProjectionStage::Enumerate, e))? {
        if paths.len() == limits.entries {
            return Err(ProjectionError::new(
                ProjectionStage::Enumerate,
                io::ErrorKind::FileTooLarge,
            ));
        }
        let item = item.map_err(|e| at(ProjectionStage::Enumerate, e))?;
        let name = item.file_name();
        bytes = bytes
            .checked_add(native_len(Path::new(&name)))
            .filter(|n| *n <= limits.name_bytes)
            .ok_or_else(|| {
                ProjectionError::new(ProjectionStage::Enumerate, io::ErrorKind::FileTooLarge)
            })?;
        paths.push(name);
    }
    paths.sort_by_cached_key(|name| native_sort_key(Path::new(name)));
    for name in paths {
        let path = limits.join(root, &name)?;
        let original =
            fs::symlink_metadata(path.as_path()).map_err(|e| at(ProjectionStage::Select, e))?;
        if original.file_type().is_symlink() {
            let target =
                fs::metadata(path.as_path()).map_err(|e| at(ProjectionStage::Select, e))?;
            if !target.is_file() {
                continue;
            }
            if matches!(mode, ArtifactMode::Copy(CopyPolicy::RejectEntryLinks)) {
                continue;
            }
        } else if !original.is_file() {
            continue;
        }
        return Ok(path.as_path().to_owned());
    }
    Err(ProjectionError::new(
        ProjectionStage::Select,
        io::ErrorKind::NotFound,
    ))
}
#[cfg(windows)]
fn native_sort_key(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().collect()
}
#[cfg(not(windows))]
fn native_sort_key(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_vec()
}
#[cfg(windows)]
pub(crate) fn fold_path(path: &Path) -> Vec<u16> {
    native_sort_key(path)
        .into_iter()
        .map(|n| if (65..=90).contains(&n) { n + 32 } else { n })
        .collect()
}
#[cfg(not(windows))]
pub(crate) fn fold_path(path: &Path) -> Vec<u8> {
    native_sort_key(path)
        .into_iter()
        .map(|n| n.to_ascii_lowercase())
        .collect()
}
