//! Read-only planning. Captured observations do not reserve paths or authorize publication.
use crate::source::{invalid, normal_absolute};
use crate::{ArtifactType, InstallationTarget, NativeBinding, SourceRef};
use graphit_core::{
    artifact_store::{ArtifactKey, ArtifactLimits, ArtifactMode, CopyPolicy},
    native_path::{MAX_PATH_BYTES, NativePath},
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
    SourceChanged,
    DestinationChanged,
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
    path_bytes: usize,
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
        path_bytes: usize,
        entries: usize,
        name_bytes: usize,
        batch: usize,
    ) -> Result<Self, ProjectionError> {
        if !(1..=64 * 1024).contains(&identity_bytes)
            || !(1..=MAX_PATH_BYTES).contains(&path_bytes)
            || !(1..=100_000).contains(&entries)
            || !(1..=16 * 1024 * 1024).contains(&name_bytes)
            || !(1..=100_000).contains(&batch)
        {
            return Err(invalid(ProjectionStage::Validate));
        }
        Ok(Self {
            identity_bytes,
            path_bytes,
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
        if native_len(&path) > self.path_bytes {
            return Err(invalid(ProjectionStage::Validate));
        }
        NativePath::new(path).map_err(|e| at(ProjectionStage::Validate, e))
    }
    pub(crate) fn capture_path(self, path: &Path) -> Result<NativePath, ProjectionError> {
        if native_len(path) > self.path_bytes {
            return Err(invalid(ProjectionStage::Validate));
        }
        self.path(path.to_owned())
    }
    pub(crate) fn join(
        self,
        parent: &Path,
        name: &std::ffi::OsStr,
    ) -> Result<NativePath, ProjectionError> {
        let bytes = native_len(parent)
            .checked_add(native_len(Path::new(name)))
            .and_then(|n| n.checked_add(if cfg!(windows) { 2 } else { 1 }));
        if bytes.is_none_or(|n| n > self.path_bytes) {
            return Err(invalid(ProjectionStage::Validate));
        }
        self.path(parent.join(name))
    }
    pub(crate) fn append(self, root: &Path, parts: &[&str]) -> Result<NativePath, ProjectionError> {
        let mut path = self.capture_path(root)?;
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

#[derive(Clone, Debug)]
pub struct SourceBinding {
    input: NativePath,
    canonical: NativePath,
    kind: NodeKind,
    file_link: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBindingRecord {
    pub input: NativePath,
    pub canonical: NativePath,
    #[serde(with = "NodeKindDef")]
    pub kind: NodeKind,
    pub file_link: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "NodeKind")]
pub(crate) enum NodeKindDef {
    File,
    Directory,
}

impl SourceBinding {
    pub fn record(&self) -> SourceBindingRecord {
        SourceBindingRecord {
            input: self.input.clone(),
            canonical: self.canonical.clone(),
            kind: self.kind,
            file_link: self.file_link,
        }
    }
    /// Stored bindings are data. Revalidation is a separate, read-only observation.
    pub fn restore(
        record: SourceBindingRecord,
        limits: ProjectionLimits,
    ) -> Result<Self, ProjectionError> {
        if !normal_absolute(record.input.as_path()) || !normal_absolute(record.canonical.as_path())
        {
            return Err(invalid(ProjectionStage::Validate));
        }
        if record.input.as_path().file_name().is_none() || record.input.as_path().parent().is_none()
        {
            return Err(invalid(ProjectionStage::Validate));
        }
        if !record.file_link {
            #[cfg(not(windows))]
            if record.input != record.canonical {
                return Err(invalid(ProjectionStage::Validate));
            }
            #[cfg(windows)]
            if record.input.as_path().parent() != record.canonical.as_path().parent() {
                return Err(invalid(ProjectionStage::Validate));
            }
        }
        let input = limits.capture_path(record.input.as_path())?;
        let canonical = limits.capture_path(record.canonical.as_path())?;
        Ok(Self {
            input,
            canonical,
            kind: record.kind,
            file_link: record.file_link,
        })
    }

    fn capture(
        path: &Path,
        kind: NodeKind,
        limits: ProjectionLimits,
    ) -> Result<Self, ProjectionError> {
        let parent = path
            .parent()
            .ok_or_else(|| invalid(ProjectionStage::Source))?;
        let name = path
            .file_name()
            .ok_or_else(|| invalid(ProjectionStage::Source))?;
        let parent = fs::canonicalize(parent).map_err(|e| at(ProjectionStage::Source, e))?;
        let input = limits.join(&parent, name)?;
        let original =
            fs::symlink_metadata(input.as_path()).map_err(|e| at(ProjectionStage::Source, e))?;
        let canonical = limits
            .path(fs::canonicalize(input.as_path()).map_err(|e| at(ProjectionStage::Source, e))?)?;
        let observed =
            fs::metadata(canonical.as_path()).map_err(|e| at(ProjectionStage::Source, e))?;
        if !kind_matches(&observed, kind) {
            return Err(ProjectionError::new(
                ProjectionStage::Source,
                if kind == NodeKind::Directory {
                    io::ErrorKind::NotADirectory
                } else {
                    io::ErrorKind::InvalidInput
                },
            ));
        }
        Ok(Self {
            input,
            canonical,
            kind,
            file_link: original.file_type().is_symlink(),
        })
    }
    pub fn input(&self) -> &NativePath {
        &self.input
    }
    pub fn canonical(&self) -> &NativePath {
        &self.canonical
    }
    pub fn kind(&self) -> NodeKind {
        self.kind
    }
    pub fn is_link(&self) -> bool {
        self.file_link
    }
    pub fn revalidate(&self) -> Result<(), ProjectionError> {
        let original = fs::symlink_metadata(self.input.as_path())
            .map_err(|e| at(ProjectionStage::SourceChanged, e))?;
        let canonical = fs::canonicalize(self.input.as_path())
            .map_err(|e| at(ProjectionStage::SourceChanged, e))?;
        let metadata =
            fs::metadata(&canonical).map_err(|e| at(ProjectionStage::SourceChanged, e))?;
        if canonical != self.canonical.as_path()
            || original.file_type().is_symlink() != self.file_link
            || !kind_matches(&metadata, self.kind)
        {
            return Err(invalid(ProjectionStage::SourceChanged));
        }
        Ok(())
    }
}
fn kind_matches(metadata: &fs::Metadata, kind: NodeKind) -> bool {
    match kind {
        NodeKind::File => metadata.is_file(),
        NodeKind::Directory => metadata.is_dir(),
    }
}
fn allowed(binding: &SourceBinding, mode: ArtifactMode) -> Result<(), ProjectionError> {
    if binding.file_link
        && let ArtifactMode::Copy(policy) = mode
        && (binding.kind == NodeKind::Directory || policy == CopyPolicy::RejectFileLinks)
    {
        return Err(invalid(ProjectionStage::Select));
    }
    Ok(())
}

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
    directory: SourceBinding,
    selected: SourceBinding,
    destination: NativePath,
    parent: NativePath,
    input_parent: NativePath,
    name: String,
    mode: ArtifactMode,
    limits: ProjectionLimits,
    artifacts: ArtifactLimits,
    runtime: NativePath,
}
impl ProjectionPlan {
    pub fn binding(&self) -> NativeBinding {
        self.binding
    }
    pub fn directory_binding(&self) -> &SourceBinding {
        &self.directory
    }
    pub fn canonical_parent(&self) -> &NativePath {
        &self.parent
    }
    pub fn input_parent(&self) -> &NativePath {
        &self.input_parent
    }
    pub fn runtime(&self) -> &NativePath {
        &self.runtime
    }

    pub fn key(&self) -> &ArtifactKey {
        &self.key
    }
    pub fn source(&self) -> &SourceRef {
        &self.source
    }
    pub fn selected(&self) -> &SourceBinding {
        &self.selected
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
    pub fn limits(&self) -> ProjectionLimits {
        self.limits
    }
    pub fn artifact_limits(&self) -> ArtifactLimits {
        self.artifacts
    }
    pub fn revalidate_bindings(&self) -> Result<(), ProjectionError> {
        self.directory.revalidate()?;
        self.selected.revalidate()?;
        let current = fs::canonicalize(self.input_parent.as_path())
            .map_err(|e| at(ProjectionStage::DestinationChanged, e))?;
        if current != self.parent.as_path()
            || !fs::metadata(&current)
                .map_err(|e| at(ProjectionStage::DestinationChanged, e))?
                .is_dir()
        {
            return Err(invalid(ProjectionStage::DestinationChanged));
        }
        validate_runtime(
            self.runtime.as_path(),
            self.destination.as_path(),
            self.limits,
        )
    }
}
#[derive(Clone, Debug)]
pub struct RulePlan {
    key: ArtifactKey,
    source: SourceRef,
    directory: SourceBinding,
    selected: SourceBinding,
}
impl RulePlan {
    pub fn key(&self) -> &ArtifactKey {
        &self.key
    }
    pub fn source(&self) -> &SourceRef {
        &self.source
    }
    pub fn selected(&self) -> &SourceBinding {
        &self.selected
    }
    pub fn revalidate_bindings(&self) -> Result<(), ProjectionError> {
        self.directory.revalidate()?;
        self.selected.revalidate()
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
    runtime: NativePath,
    limits: ProjectionLimits,
    artifacts: ArtifactLimits,
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
    pub fn new(
        destination_root: NativePath,
        runtime: NativePath,
        limits: ProjectionLimits,
        artifacts: ArtifactLimits,
        mode: ArtifactMode,
    ) -> Self {
        Self {
            destination_root,
            runtime,
            limits,
            artifacts,
            mode,
        }
    }
    pub fn plan(&self, request: ProjectionRequest<'_>) -> Result<Projection, ProjectionError> {
        let prepared = self.prepare(&request)?;
        self.capture(request, prepared)
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
        self.limits
            .capture_path(request.source.directory().as_path())?;
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
        self.artifacts
            .validate_key(&key)
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
    fn capture(
        &self,
        request: ProjectionRequest<'_>,
        prepared: ProjectionTarget,
    ) -> Result<Projection, ProjectionError> {
        self.capture_prepared(request, prepared, false)
            .map(|(plan, _)| plan)
    }
    // Consumer preparation observes sources before any parent provisioning.
    pub(crate) fn prepare_publication(
        &self,
        request: ProjectionRequest<'_>,
    ) -> Result<(Projection, Vec<NativePath>), ProjectionError> {
        let target = self.prepare(&request)?;
        self.capture_prepared(request, target, true)
    }
    fn capture_prepared(
        &self,
        request: ProjectionRequest<'_>,
        prepared: ProjectionTarget,
        allow_missing: bool,
    ) -> Result<(Projection, Vec<NativePath>), ProjectionError> {
        if prepared.destination.is_none() && prepared.canonical.is_none() {
            return Ok((
                Projection::Native(Box::new(NativeCapability {
                    key: prepared.key,
                    source: request.source.clone(),
                })),
                Vec::new(),
            ));
        }
        let directory = SourceBinding::capture(
            request.source.directory().as_path(),
            NodeKind::Directory,
            self.limits,
        )?;
        let selected = if let Some(canonical) = prepared.canonical {
            select_file(
                directory.canonical.as_path(),
                canonical,
                self.limits,
                self.mode,
            )?
        } else {
            directory.clone()
        };
        allowed(&selected, self.mode)?;
        let Some(destination) = prepared.destination else {
            return Ok((
                Projection::DynamicRule(Box::new(RulePlan {
                    key: prepared.key,
                    source: request.source.clone(),
                    directory,
                    selected,
                })),
                Vec::new(),
            ));
        };
        let input_parent = self.limits.path(
            destination
                .as_path()
                .parent()
                .ok_or_else(|| invalid(ProjectionStage::Destination))?
                .to_owned(),
        )?;
        let (parent, possible_parents) =
            expected_parent(input_parent.as_path(), self.limits, allow_missing)?;
        if !parent
            .as_path()
            .starts_with(self.destination_root.as_path())
        {
            return Err(invalid(ProjectionStage::Destination));
        }
        let destination = self.limits.join(
            parent.as_path(),
            destination
                .as_path()
                .file_name()
                .ok_or_else(|| invalid(ProjectionStage::Destination))?,
        )?;
        let runtime = self.runtime.clone();
        validate_runtime(runtime.as_path(), destination.as_path(), self.limits)?;
        Ok((
            Projection::Physical(Box::new(ProjectionPlan {
                binding: request.binding,
                key: prepared.key,
                source: request.source.clone(),
                directory,
                selected,
                destination,
                parent,
                input_parent,
                name: prepared.name,
                mode: self.mode,
                limits: self.limits,
                artifacts: self.artifacts,
                runtime,
            })),
            possible_parents,
        ))
    }
}

pub(crate) fn expected_parent(
    input: &Path,
    limits: ProjectionLimits,
    allow_missing: bool,
) -> Result<(NativePath, Vec<NativePath>), ProjectionError> {
    let mut current = input;
    let mut missing = Vec::new();
    let canonical = loop {
        match fs::symlink_metadata(current) {
            Ok(_) => {
                let canonical =
                    fs::canonicalize(current).map_err(|e| at(ProjectionStage::Destination, e))?;
                if !fs::metadata(&canonical)
                    .map_err(|e| at(ProjectionStage::Destination, e))?
                    .is_dir()
                {
                    return Err(ProjectionError::new(
                        ProjectionStage::Destination,
                        io::ErrorKind::NotADirectory,
                    ));
                }
                break limits.path(canonical)?;
            }
            Err(e) if allow_missing && e.kind() == io::ErrorKind::NotFound => {
                if missing.len() == 2 {
                    return Err(invalid(ProjectionStage::Destination));
                }
                missing.push(limits.capture_path(current)?);
                current = current
                    .parent()
                    .ok_or_else(|| invalid(ProjectionStage::Destination))?;
            }
            Err(e) => return Err(at(ProjectionStage::Destination, e)),
        }
    };
    missing.reverse();
    let mut parent = canonical;
    for path in &missing {
        parent = limits.join(
            parent.as_path(),
            path.as_path()
                .file_name()
                .ok_or_else(|| invalid(ProjectionStage::Destination))?,
        )?;
    }
    Ok((parent, missing))
}

pub(crate) fn validate_runtime(
    runtime: &Path,
    destination: &Path,
    limits: ProjectionLimits,
) -> Result<(), ProjectionError> {
    let observed = match fs::symlink_metadata(runtime) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(invalid(ProjectionStage::Destination));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(ProjectionError::new(
                ProjectionStage::Destination,
                io::ErrorKind::NotADirectory,
            ));
        }
        Ok(_) => limits
            .path(fs::canonicalize(runtime).map_err(|e| at(ProjectionStage::Destination, e))?)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => limits.capture_path(runtime)?,
        Err(e) => return Err(at(ProjectionStage::Destination, e)),
    };
    if destination.starts_with(observed.as_path()) || observed.as_path().starts_with(destination) {
        return Err(invalid(ProjectionStage::Destination));
    }
    Ok(())
}

fn select_file(
    root: &Path,
    canonical: &str,
    limits: ProjectionLimits,
    mode: ArtifactMode,
) -> Result<SourceBinding, ProjectionError> {
    let path = limits.join(root, std::ffi::OsStr::new(canonical))?;
    match fs::symlink_metadata(path.as_path()) {
        Ok(_) => {
            let binding = SourceBinding::capture(path.as_path(), NodeKind::File, limits)?;
            allowed(&binding, mode)?;
            return Ok(binding);
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
            if matches!(mode, ArtifactMode::Copy(CopyPolicy::RejectFileLinks)) {
                continue;
            }
        } else if !original.is_file() {
            continue;
        }
        return SourceBinding::capture(path.as_path(), NodeKind::File, limits);
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
