//! Local Hub metadata. Intent descriptors are data, never filesystem authority.
mod codec;
mod display;
pub use display::ArtifactDisplayMetadata;
use display::RawDisplayMetadata;
#[cfg(test)]
mod tests;

use crate::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, NativeBinding, ProjectionLimits,
    ProjectionPlan, SourceBinding, SourceBindingRecord, SourceRecord, SourceRef,
    projection::{NodeKindDef, validate_name},
    source::normal_absolute,
};
use graphit_core::{
    artifact_store::{
        ArtifactKey, ArtifactLimits, ArtifactMode, CopyPolicy, PayloadSelection, PublicationSlot,
    },
    file_lock::{Cancellation, FileLock},
    installation_identity::InstallationId,
    local_files,
    native_path::NativePath,
    project_identity::{ProjectId, ProjectIdentity, ProjectIdentityStore},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_fs::NodeKind;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    time::Duration,
};

fn invalid() -> io::Error {
    io::ErrorKind::InvalidData.into()
}
fn projection_error(_: crate::ProjectionError) -> io::Error {
    invalid()
}
fn cancelled(cancellation: &Cancellation) -> io::Result<()> {
    if cancellation.is_cancelled() {
        Err(io::ErrorKind::Interrupted.into())
    } else {
        Ok(())
    }
}

/// All capacities are caller inputs; there are no product defaults.
#[derive(Clone, Copy, Debug)]
pub struct HubLimits {
    document_bytes: usize,
    entries: usize,
    key_bytes: usize,
    json_nodes: usize,
    json_string_bytes: usize,
    projection: ProjectionLimits,
    artifacts: ArtifactLimits,
    materialization: crate::materialization::MaterializationLimits,
}
#[derive(Clone, Copy, Debug)]
pub struct DocumentLimits {
    pub bytes: usize,
    pub entries: usize,
    pub key_bytes: usize,
    pub json_nodes: usize,
    pub json_string_bytes: usize,
}
impl HubLimits {
    pub fn materialization(self) -> crate::materialization::MaterializationLimits {
        self.materialization
    }
    pub fn projection(self) -> ProjectionLimits {
        self.projection
    }
    pub fn artifacts(self) -> ArtifactLimits {
        self.artifacts
    }
    pub fn new(
        document: DocumentLimits,
        projection: ProjectionLimits,
        artifacts: ArtifactLimits,
        materialization: crate::materialization::MaterializationLimits,
    ) -> io::Result<Self> {
        if !(1..=16 * 1024 * 1024).contains(&document.bytes)
            || !(1..=100_000).contains(&document.entries)
            || !(1..=64 * 1024).contains(&document.key_bytes)
            || !(1..=1_000_000).contains(&document.json_nodes)
            || !(1..=16 * 1024 * 1024).contains(&document.json_string_bytes)
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            document_bytes: document.bytes,
            entries: document.entries,
            key_bytes: document.key_bytes,
            json_nodes: document.json_nodes,
            json_string_bytes: document.json_string_bytes,
            projection,
            artifacts,
            materialization,
        })
    }
    fn path(self, path: &NativePath) -> io::Result<()> {
        if !normal_absolute(path.as_path()) {
            return Err(invalid());
        }
        self.projection
            .capture_path(path.as_path())
            .map(|_| ())
            .map_err(projection_error)
    }
    pub(crate) fn key(self, key: &ArtifactKey) -> io::Result<()> {
        self.artifacts.validate_key(key)?;
        if key
            .adapter()
            .len()
            .checked_add(key.artifact_type().len())
            .and_then(|n| n.checked_add(key.id().len()))
            .is_none_or(|n| n > self.key_bytes)
        {
            return Err(invalid());
        }
        InstallationTarget::from_key(key)?;
        ArtifactId::new(key.id(), self.projection).map_err(projection_error)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum HubScope {
    Project { project_id: ProjectId },
    Global { installation_id: String },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum InstalledBackend {
    Physical {
        mode: ArtifactMode,
        #[serde(with = "NodeKindDef")]
        node_kind: NodeKind,
        destination: NativePath,
        name: String,
    },
    DynamicContext,
    GlobalCache,
    Native,
    RemoteNative(crate::remote_native::RemoteNativeRecord),
}
#[derive(Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
enum RawInstalledBackend {
    Physical {
        mode: ArtifactMode,
        #[serde(with = "NodeKindDef")]
        node_kind: NodeKind,
        destination: NativePath,
        name: String,
    },
    DynamicContext,
    GlobalCache,
    Native,
    RemoteNative(crate::remote_native::RawRemoteNativeRecord),
}
impl RawInstalledBackend {
    fn restore(self, limits: HubLimits) -> io::Result<InstalledBackend> {
        Ok(match self {
            Self::Physical {
                mode,
                node_kind,
                destination,
                name,
            } => InstalledBackend::Physical {
                mode,
                node_kind,
                destination,
                name,
            },
            Self::DynamicContext => InstalledBackend::DynamicContext,
            Self::GlobalCache => InstalledBackend::GlobalCache,
            Self::Native => InstalledBackend::Native,
            Self::RemoteNative(record) => {
                InstalledBackend::RemoteNative(record.restore(limits.projection)?)
            }
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Installation {
    pub target: InstallationTarget,
    pub original_source: SourceRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<ArtifactDisplayMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_name: Option<String>,
    pub backend: InstalledBackend,
    pub payload: InstalledPayload,
}

impl Installation {
    /// Compare effective source/publication bindings; display metadata is descriptive data.
    /// Persistent equality still includes display so metadata-only changes are saved.
    pub fn same_source(&self, other: &Self) -> bool {
        self.target == other.target
            && self.original_source == other.original_source
            && self.alias == other.alias
            && self.entry_name == other.entry_name
            && self.backend == other.backend
            && self.payload == other.payload
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum InstalledPayload {
    Original {
        #[serde(skip_serializing_if = "Option::is_none")]
        selection_binding: Option<SourceBindingRecord>,
    },
    Derived(Box<DerivedPayload>),
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedPayload {
    pub source_policy: CopyPolicy,
    pub source_fingerprint: String,
    /// Actual original input, independent of the rendered payload selection.
    pub original_selection: PayloadSelection,
}
impl std::fmt::Debug for InstalledPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Original { .. } => f.write_str("Original { .. }"),
            Self::Derived(_) => f.write_str("Derived { .. }"),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInstallation {
    target: InstallationTarget,
    original_source: SourceRecord,
    display: Option<RawDisplayMetadata>,
    alias: Option<String>,
    entry_name: Option<String>,
    backend: RawInstalledBackend,
    payload: InstalledPayload,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InstalledArtifact {
    pub consumer: PublicationSlot,
    pub artifact_type: ArtifactType,
    pub id: ArtifactId,
    pub installations: Vec<Installation>,
}

/// Only these capabilities publish a physical projection in the consumer checkout.
/// Native and dynamic metadata keep the scope-wide selection independently of paths.
pub fn consumer_for_artifact(slot: PublicationSlot, kind: ArtifactType) -> PublicationSlot {
    match kind {
        ArtifactType::Agent
        | ArtifactType::Skill
        | ArtifactType::Command
        | ArtifactType::Workflow => slot,
        _ => PublicationSlot::Scope,
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInstalledArtifact {
    consumer: PublicationSlot,
    artifact_type: ArtifactType,
    id: String,
    installations: Vec<RawInstallation>,
}
impl RawInstalledArtifact {
    fn restore(self, limits: HubLimits) -> io::Result<InstalledArtifact> {
        if self.installations.is_empty() || self.installations.len() > NativeBinding::ALL.len() {
            return Err(invalid());
        }
        let mut installations = Vec::with_capacity(self.installations.len());
        for installation in self.installations {
            installations.push(Installation {
                target: installation.target,
                original_source: SourceRef::restore(
                    installation.original_source,
                    limits.projection,
                )
                .map_err(projection_error)?,
                display: installation
                    .display
                    .map(|display| display.restore(limits))
                    .transpose()?,
                alias: installation.alias,
                entry_name: installation.entry_name,
                backend: installation.backend.restore(limits)?,
                payload: installation.payload,
            });
        }
        let entry = InstalledArtifact {
            consumer: self.consumer,
            artifact_type: self.artifact_type,
            id: ArtifactId::new(&self.id, limits.projection).map_err(projection_error)?,
            installations,
        };
        entry.validate(limits)?;
        Ok(entry)
    }
}
impl InstalledArtifact {
    fn validate(&self, limits: HubLimits) -> io::Result<()> {
        ArtifactId::new(self.id.as_str(), limits.projection).map_err(projection_error)?;
        if self.installations.is_empty() || self.installations.len() > NativeBinding::ALL.len() {
            return Err(invalid());
        }
        for (index, installation) in self.installations.iter().enumerate() {
            installation.target.validate_type(self.artifact_type)?;
            if let Some(display) = &installation.display {
                display.validate(limits)?;
            }
            let global_cache = installation.target == InstallationTarget::GlobalCache;
            if global_cache
                && (self.installations.len() != 1
                    || !matches!(installation.backend, InstalledBackend::GlobalCache)
                    || !matches!(
                        installation.payload,
                        InstalledPayload::Original {
                            selection_binding: None
                        }
                    )
                    || installation.alias.is_some()
                    || installation.entry_name.is_some()
                    || installation.original_source.origin() != crate::SourceOrigin::HubCache
                    || installation.original_source.remote_file_origin().is_none()
                    || installation.original_source.physical_id() != &self.id)
            {
                return Err(invalid());
            }
            if installation.target == InstallationTarget::GlobalNative
                && (self.installations.len() != 1
                    || !matches!(
                        installation.backend,
                        InstalledBackend::Native | InstalledBackend::RemoteNative(_)
                    )
                    || !matches!(
                        installation.payload,
                        InstalledPayload::Original {
                            selection_binding: None
                        }
                    )
                    || installation.alias.is_some()
                    || installation.entry_name.is_some())
            {
                return Err(invalid());
            }
            match &installation.payload {
                InstalledPayload::Original { selection_binding } => {
                    if self.artifact_type == ArtifactType::Agent && !global_cache {
                        return Err(invalid());
                    }
                    if let Some(binding) = selection_binding {
                        SourceBinding::restore(binding.clone(), limits.projection)
                            .map_err(projection_error)?;
                    }
                    if matches!(installation.backend, InstalledBackend::Physical { .. })
                        && selection_binding.is_none()
                    {
                        return Err(invalid());
                    }
                }
                InstalledPayload::Derived(payload) => {
                    let DerivedPayload {
                        source_fingerprint,
                        original_selection,
                        ..
                    } = payload.as_ref();
                    if source_fingerprint.len() != 64
                        || !source_fingerprint
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        return Err(invalid());
                    }
                    if (self.artifact_type == ArtifactType::Agent
                        && original_selection.kind() != NodeKind::File)
                        || (self.artifact_type == ArtifactType::Skill
                            && (original_selection.kind() != NodeKind::Directory
                                || !original_selection.relative().as_os_str().is_empty()))
                        || !matches!(&installation.backend, InstalledBackend::Physical { .. })
                    {
                        return Err(invalid());
                    }
                }
            }
            if self.installations[..index]
                .iter()
                .any(|v| v.target == installation.target)
                || installation.original_source.artifact_type() != self.artifact_type
                || installation.original_source.id() != &self.id
            {
                return Err(invalid());
            }
            limits
                .projection
                .identity(installation.original_source.id().as_str())
                .map_err(projection_error)?;
            limits
                .projection
                .identity(installation.original_source.physical_id().as_str())
                .map_err(projection_error)?;
            limits
                .projection
                .identity(installation.original_source.version().as_str())
                .map_err(projection_error)?;
            limits.path(installation.original_source.root())?;
            limits.path(installation.original_source.directory())?;
            for name in [&installation.alias, &installation.entry_name]
                .into_iter()
                .flatten()
            {
                if !name.is_empty() || installation.alias.as_ref() == Some(name) {
                    validate_name(name).map_err(projection_error)?;
                }
            }
            let expected_name = installation
                .alias
                .as_deref()
                .or_else(|| installation.entry_name.as_deref().filter(|s| !s.is_empty()))
                .unwrap_or_else(|| installation.original_source.id().basename());
            let key = self.key_for(installation.target)?;
            limits.key(&key)?;
            match &installation.backend {
                InstalledBackend::Physical {
                    node_kind,
                    destination,
                    name,
                    ..
                } => {
                    let kind = self.artifact_type;
                    if !matches!(
                        kind,
                        ArtifactType::Skill
                            | ArtifactType::Command
                            | ArtifactType::Workflow
                            | ArtifactType::Agent
                    ) || (matches!(kind, ArtifactType::Command | ArtifactType::Workflow)
                        && matches!(
                            installation.target.adapter(),
                            Some(crate::AdapterId::Kimi | crate::AdapterId::Kiro)
                        ))
                        || (*node_kind == NodeKind::Directory) != (kind == ArtifactType::Skill)
                        || name != expected_name
                    {
                        return Err(invalid());
                    }
                    validate_name(name).map_err(projection_error)?;
                    limits.path(destination)?;
                    let extension = match kind {
                        ArtifactType::Skill => "",
                        ArtifactType::Agent => match installation.target {
                            InstallationTarget::NativeBinding { binding } => {
                                binding.agent_extension()
                            }
                            InstallationTarget::SharedAdapter { .. }
                            | InstallationTarget::GlobalNative
                            | InstallationTarget::GlobalCache => return Err(invalid()),
                        },
                        _ => "md",
                    };
                    let filename = if extension.is_empty() {
                        name.clone()
                    } else {
                        format!("{name}.{extension}")
                    };
                    validate_name(&filename).map_err(projection_error)?;
                    if destination.as_path().file_name() != Some(std::ffi::OsStr::new(&filename)) {
                        return Err(invalid());
                    }
                }
                InstalledBackend::DynamicContext if self.artifact_type == ArtifactType::Rule => {}
                InstalledBackend::GlobalCache if global_cache => {}
                InstalledBackend::RemoteNative(record) => {
                    if installation.target != InstallationTarget::GlobalNative
                        || !matches!(
                            self.artifact_type,
                            ArtifactType::Ast | ArtifactType::Knowledge
                        )
                        || installation.original_source.origin() != crate::SourceOrigin::HubCache
                        || installation.original_source.physical_id() != &self.id
                    {
                        return Err(invalid());
                    }
                    record.validate_source(&installation.original_source, limits.projection)?;
                }
                InstalledBackend::Native
                    if matches!(
                        self.artifact_type,
                        ArtifactType::Mcp
                            | ArtifactType::Ast
                            | ArtifactType::Knowledge
                            | ArtifactType::Language
                            | ArtifactType::Power
                    ) => {}
                _ => return Err(invalid()),
            }
        }
        Ok(())
    }
    fn validate_scope(&self, scope: &HubScope) -> io::Result<()> {
        let physical = self
            .installations
            .iter()
            .any(|installation| matches!(installation.backend, InstalledBackend::Physical { .. }));
        if match scope {
            HubScope::Global { .. } => self.consumer != PublicationSlot::Scope,
            HubScope::Project { .. } => {
                physical != matches!(self.consumer, PublicationSlot::ProjectRoot(_))
            }
        } {
            return Err(invalid());
        }
        let global_native = matches!(
            self.artifact_type,
            ArtifactType::Ast | ArtifactType::Knowledge | ArtifactType::Language
        ) && matches!(scope, HubScope::Global { .. });
        if self
            .installations
            .iter()
            .any(|installation| match installation.target {
                InstallationTarget::GlobalCache => !matches!(scope, HubScope::Global { .. }),
                _ => (installation.target == InstallationTarget::GlobalNative) != global_native,
            })
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn key_for(&self, target: InstallationTarget) -> io::Result<ArtifactKey> {
        target.key(self.artifact_type, self.id.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationBindings {
    pub binding: NativeBinding,
    pub directory: SourceBindingRecord,
    pub selected: SourceBindingRecord,
    pub destination_root: NativePath,
    pub runtime: NativePath,
    pub input_parent: NativePath,
    pub parent: NativePath,
    pub destination: NativePath,
    pub name: String,
    pub mode: ArtifactMode,
    pub physical_input: PublicationInput,
    /// Missing at preparation; after a crash these are possible residues, not ownership proofs.
    pub possible_parents: Vec<NativePath>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum PublicationInput {
    External,
    Generated { fingerprint: [u8; 32] },
}
impl PublicationBindings {
    pub fn from_plan(plan: &ProjectionPlan, destination_root: &NativePath) -> io::Result<Self> {
        Ok(Self {
            binding: plan.binding(),
            directory: plan.directory_binding().record(),
            selected: plan.selected().record(),
            destination_root: destination_root.clone(),
            runtime: plan.runtime().clone(),
            input_parent: plan.input_parent().clone(),
            parent: plan.canonical_parent().clone(),
            destination: plan.destination().clone(),
            name: plan.name().to_owned(),
            mode: plan.mode(),
            physical_input: PublicationInput::External,
            possible_parents: Vec::new(),
        })
    }
    fn validate(
        &self,
        key: &ArtifactKey,
        desired: &Installation,
        limits: HubLimits,
    ) -> io::Result<()> {
        for path in [
            &self.destination_root,
            &self.runtime,
            &self.input_parent,
            &self.parent,
            &self.destination,
        ] {
            limits.path(path)?;
        }
        let directory = SourceBinding::restore(self.directory.clone(), limits.projection)
            .map_err(projection_error)?;
        let selected = SourceBinding::restore(self.selected.clone(), limits.projection)
            .map_err(projection_error)?;
        match (&self.physical_input, &desired.payload) {
            (PublicationInput::External, InstalledPayload::Original { selection_binding })
                if selection_binding.as_ref() == Some(&self.selected) => {}
            (PublicationInput::Generated { .. }, InstalledPayload::Derived(_)) => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "publication input does not match the desired payload",
                ));
            }
        }
        if directory.kind() != NodeKind::Directory
            || self.directory.input.as_path().file_name()
                != desired.original_source.directory().as_path().file_name()
            || (selected.kind() == NodeKind::Directory && self.selected != self.directory)
            || (selected.kind() == NodeKind::File
                && self.selected.input.as_path().parent()
                    != Some(self.directory.canonical.as_path()))
            || !self
                .parent
                .as_path()
                .starts_with(self.destination_root.as_path())
            || !self
                .input_parent
                .as_path()
                .starts_with(self.destination_root.as_path())
            || self.destination.as_path().parent() != Some(self.parent.as_path())
            || self
                .destination
                .as_path()
                .starts_with(self.runtime.as_path())
            || self
                .runtime
                .as_path()
                .starts_with(self.destination.as_path())
            || self.possible_parents.len() > 2
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "publication source or destination bindings are inconsistent",
            ));
        }
        if let ArtifactMode::Copy(policy) = self.mode
            && self.selected.file_link
            && (self.selected.kind == NodeKind::Directory
                || policy == graphit_core::artifact_store::CopyPolicy::RejectFileLinks)
        {
            return Err(invalid());
        }
        for (index, parent) in self.possible_parents.iter().enumerate() {
            limits.path(parent)?;
            if !self.input_parent.as_path().starts_with(parent.as_path())
                || parent.as_path() == self.destination_root.as_path()
                || !parent
                    .as_path()
                    .starts_with(self.destination_root.as_path())
                || parent.as_path().starts_with(self.runtime.as_path())
                || self.possible_parents[..index].contains(parent)
                || (index > 0
                    && parent.as_path().parent()
                        != Some(self.possible_parents[index - 1].as_path()))
            {
                return Err(invalid());
            }
        }
        let target = InstallationTarget::from_key(key)?;
        if desired.target != target
            || InstallationTarget::for_artifact(
                self.binding,
                desired.original_source.artifact_type(),
            ) != target
        {
            return Err(invalid());
        }
        match &desired.backend {
            InstalledBackend::Physical {
                mode,
                node_kind,
                destination,
                name,
            } if *mode == self.mode
                && *node_kind == self.selected.kind
                && destination == &self.destination
                && name == &self.name =>
            {
                Ok(())
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "publication backend does not match the captured physical plan",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum IntentPhase {
    PreparingParents,
    IntentReady,
    PhysicalStarted,
    PhysicalComplete,
    LogicalCommitted,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalDescriptor {
    pub source: NativePath,
    pub destination: NativePath,
    #[serde(with = "NodeKindDef")]
    pub node_kind: NodeKind,
    pub mode: ArtifactMode,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingDescriptor {
    pub phase: graphit_core::artifact_store::ArtifactPending,
    pub candidate: PhysicalDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staging_directory: Option<NativePath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_staging_directory: Option<NativePath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_destination: Option<NativePath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<NativePath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<NativePath>,
    pub permissions_may_be_modified: bool,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgetPreview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned: Option<PhysicalDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingDescriptor>,
    pub possible_parents: Vec<NativePath>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum PreviousGoal {
    Publish { bindings: Box<PublicationBindings> },
    Remove,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviousIntent {
    pub phase: IntentPhase,
    pub goal: PreviousGoal,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum LogicalGoal {
    Publish {
        bindings: Box<PublicationBindings>,
    },
    Remove,
    Forget {
        preview: Box<ForgetPreview>,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous: Option<Box<PreviousIntent>>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LogicalIntent {
    pub consumer: PublicationSlot,
    pub key: ArtifactKey,
    pub phase: IntentPhase,
    pub goal: LogicalGoal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<InstalledArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desired: Option<InstalledArtifact>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLogicalIntent {
    consumer: PublicationSlot,
    key: ArtifactKey,
    phase: IntentPhase,
    goal: LogicalGoal,
    old: Option<RawInstalledArtifact>,
    desired: Option<RawInstalledArtifact>,
}
impl RawLogicalIntent {
    fn restore(self, limits: HubLimits) -> io::Result<LogicalIntent> {
        Ok(LogicalIntent {
            consumer: self.consumer,
            key: self.key,
            phase: self.phase,
            goal: self.goal,
            old: self.old.map(|v| v.restore(limits)).transpose()?,
            desired: self.desired.map(|v| v.restore(limits)).transpose()?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HubDocument {
    format: u32,
    brand: String,
    scope: HubScope,
    agents: Vec<NativeBinding>,
    artifacts: BTreeMap<ArtifactType, BTreeMap<String, Vec<InstalledArtifact>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    intent: Option<LogicalIntent>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHubDocument {
    format: u32,
    brand: String,
    scope: HubScope,
    agents: Vec<NativeBinding>,
    artifacts: BTreeMap<ArtifactType, BTreeMap<String, Vec<RawInstalledArtifact>>>,
    intent: Option<RawLogicalIntent>,
}
impl RawHubDocument {
    fn restore(self, limits: HubLimits) -> io::Result<HubDocument> {
        let mut artifacts = BTreeMap::new();
        for (kind, entries) in self.artifacts {
            let mut restored = BTreeMap::new();
            for (id, entry) in entries {
                restored.insert(
                    id,
                    entry
                        .into_iter()
                        .map(|v| v.restore(limits))
                        .collect::<io::Result<Vec<_>>>()?,
                );
            }
            artifacts.insert(kind, restored);
        }
        Ok(HubDocument {
            format: self.format,
            brand: self.brand,
            scope: self.scope,
            agents: self.agents,
            artifacts,
            intent: self.intent.map(|v| v.restore(limits)).transpose()?,
        })
    }
}
impl HubDocument {
    pub fn scope(&self) -> &HubScope {
        &self.scope
    }
    pub fn agents(&self) -> &[NativeBinding] {
        &self.agents
    }
    pub fn intent(&self) -> Option<&LogicalIntent> {
        self.intent.as_ref()
    }
    /// Select shared installation metadata; PROJECT physical entries require an explicit consumer.
    pub fn get(&self, kind: ArtifactType, id: &str) -> Option<&InstalledArtifact> {
        self.get_for_consumer(&PublicationSlot::Scope, kind, id)
    }
    pub fn get_for_consumer(
        &self,
        consumer: &PublicationSlot,
        kind: ArtifactType,
        id: &str,
    ) -> Option<&InstalledArtifact> {
        let mut matching = self
            .artifacts
            .get(&kind)?
            .get(id)?
            .iter()
            .filter(|entry| &entry.consumer == consumer);
        let entry = matching.next()?;
        matching.next().is_none().then_some(entry)
    }
    pub fn get_version(
        &self,
        kind: ArtifactType,
        id: &str,
        version: &ConcreteVersion,
    ) -> Option<&InstalledArtifact> {
        self.get_version_for_consumer(&PublicationSlot::Scope, kind, id, version)
    }
    pub fn get_version_for_consumer(
        &self,
        consumer: &PublicationSlot,
        kind: ArtifactType,
        id: &str,
        version: &ConcreteVersion,
    ) -> Option<&InstalledArtifact> {
        self.artifacts.get(&kind)?.get(id)?.iter().find(|entry| {
            &entry.consumer == consumer
                && entry
                    .installations
                    .iter()
                    .any(|installation| installation.original_source.version() == version)
        })
    }
    /// Complete ledger; callers selecting a consumer use `entries_for_consumer`.
    pub fn entries(&self) -> impl Iterator<Item = (ArtifactType, &str, &InstalledArtifact)> {
        self.artifacts.iter().flat_map(|(kind, entries)| {
            entries.iter().flat_map(move |(id, versions)| {
                versions
                    .iter()
                    .map(move |entry| (*kind, id.as_str(), entry))
            })
        })
    }
    pub fn entries_for_consumer<'a>(
        &'a self,
        consumer: &'a PublicationSlot,
    ) -> impl Iterator<Item = (ArtifactType, &'a str, &'a InstalledArtifact)> + 'a {
        self.entries()
            .filter(move |(_, _, entry)| &entry.consumer == consumer)
    }
    fn ambiguous(&self, consumer: &PublicationSlot, kind: ArtifactType, id: &str) -> bool {
        self.artifacts
            .get(&kind)
            .and_then(|entries| entries.get(id))
            .is_some_and(|versions| {
                versions
                    .iter()
                    .filter(|entry| &entry.consumer == consumer)
                    .count()
                    > 1
            })
    }
    fn for_key(&self, consumer: &PublicationSlot, key: &ArtifactKey) -> Option<&InstalledArtifact> {
        let kind = ArtifactType::ALL
            .into_iter()
            .find(|v| v.as_str() == key.artifact_type())?;
        self.get_for_consumer(consumer, kind, key.id())
    }
    fn replace_consumer(&mut self, entry: InstalledArtifact) {
        let groups = self
            .artifacts
            .entry(entry.artifact_type)
            .or_default()
            .entry(entry.id.as_str().into())
            .or_default();
        if let Some(existing) = groups
            .iter_mut()
            .find(|group| group.consumer == entry.consumer)
        {
            *existing = entry;
        } else {
            groups.push(entry);
        }
    }
    fn remove_target(&mut self, consumer: &PublicationSlot, key: &ArtifactKey) -> io::Result<()> {
        let kind = ArtifactType::ALL
            .into_iter()
            .find(|v| v.as_str() == key.artifact_type())
            .ok_or_else(invalid)?;
        if let Some(entries) = self.artifacts.get_mut(&kind) {
            if let Some(groups) = entries.get_mut(key.id()) {
                if let Some(entry) = groups.iter_mut().find(|entry| &entry.consumer == consumer) {
                    entry
                        .installations
                        .retain(|v| v.target.namespace() != key.adapter());
                }
                groups.retain(|entry| !entry.installations.is_empty());
                if groups.is_empty() {
                    entries.remove(key.id());
                }
            }
            if entries.is_empty() {
                self.artifacts.remove(&kind);
            }
        }
        Ok(())
    }
    fn validate(&self, registry: &HubRegistry) -> io::Result<usize> {
        let limits = registry.limits;
        if self.format != 4
            || self.brand != registry.brand
            || self.scope != registry.scope
            || self.agents.len() > NativeBinding::ALL.len()
        {
            return Err(invalid());
        }
        for (index, agent) in self.agents.iter().enumerate() {
            if self.agents[..index].contains(agent) {
                return Err(invalid());
            }
        }
        for entries in self.artifacts.values() {
            if entries.is_empty() {
                return Err(invalid());
            }
            for versions in entries.values() {
                if versions.is_empty() {
                    return Err(invalid());
                }
                if matches!(self.scope, HubScope::Project { .. }) {
                    let mut consumers = HashSet::new();
                    if versions
                        .iter()
                        .any(|entry| !consumers.insert(entry.consumer))
                    {
                        return Err(invalid());
                    }
                }
                if matches!(self.scope, HubScope::Global { .. }) {
                    let mut seen = HashSet::new();
                    for entry in versions {
                        let version = entry
                            .installations
                            .first()
                            .ok_or_else(invalid)?
                            .original_source
                            .version();
                        if entry
                            .installations
                            .iter()
                            .any(|v| v.original_source.version() != version)
                            || !seen.insert(version.as_str())
                        {
                            return Err(invalid());
                        }
                    }
                }
            }
        }
        let mut count = 0usize;
        for (kind, id, entry) in self.entries() {
            count = count
                .checked_add(1)
                .filter(|n| *n <= limits.entries)
                .ok_or_else(invalid)?;
            if kind != entry.artifact_type || id != entry.id.as_str() {
                return Err(invalid());
            }
            entry.validate(limits)?;
            entry.validate_scope(&registry.scope)?;
            if entry.installations.iter().any(|installation| {
                installation.target == InstallationTarget::GlobalCache
                    && installation.original_source.root() != &registry.parent
            }) {
                return Err(invalid());
            }
        }
        if let Some(intent) = &self.intent {
            registry.validate_intent(intent)?;
            let kind = ArtifactType::ALL
                .into_iter()
                .find(|v| v.as_str() == intent.key.artifact_type())
                .ok_or_else(invalid)?;
            if self.ambiguous(&intent.consumer, kind, intent.key.id()) {
                return Err(invalid());
            }
            let mut expected = intent.old.clone();
            if let LogicalGoal::Forget {
                previous: Some(previous),
                ..
            } = &intent.goal
                && previous.phase == IntentPhase::LogicalCommitted
            {
                match previous.goal {
                    PreviousGoal::Publish { .. } => expected = intent.desired.clone(),
                    PreviousGoal::Remove => discard_target(&mut expected, intent.key.adapter()),
                }
            }
            if intent.phase == IntentPhase::LogicalCommitted {
                match intent.goal {
                    LogicalGoal::Publish { .. } => expected = intent.desired.clone(),
                    _ => discard_target(&mut expected, intent.key.adapter()),
                }
            }
            if self.for_key(&intent.consumer, &intent.key) != expected.as_ref() {
                return Err(invalid());
            }
        }
        codec::count(self, limits)
    }
}

fn discard_target(entry: &mut Option<InstalledArtifact>, adapter: &str) {
    if let Some(value) = entry {
        value
            .installations
            .retain(|v| v.target.namespace() != adapter);
        if value.installations.is_empty() {
            *entry = None;
        }
    }
}

#[derive(Clone, Debug)]
pub struct HubRegistry {
    parent: NativePath,
    brand: String,
    scope: HubScope,
    project: Option<ProjectPaths>,
    limits: HubLimits,
}
impl HubRegistry {
    pub(crate) fn matches_store(
        &self,
        store: &graphit_core::artifact_store::ArtifactStore,
    ) -> bool {
        use graphit_core::artifact_store::StoreOwner;
        let owner_matches = match (self.scope(), store.owner()) {
            (HubScope::Project { project_id: a }, StoreOwner::Project { project_id: b }) => a == b,
            (
                HubScope::Global { installation_id: a },
                StoreOwner::Installation { installation_id: b },
            ) => a == b.as_str(),
            _ => false,
        };
        owner_matches
            && self.brand == store.scope().brand().key()
            && self
                .project
                .as_ref()
                .map_or(self.parent.as_path(), ProjectPaths::root)
                == store.scope().owner_root()
            && self.limits.artifacts == store.limits()
    }
    pub fn project_paths(&self) -> Option<&ProjectPaths> {
        self.project.as_ref()
    }
    pub fn limits(&self) -> HubLimits {
        self.limits
    }
    pub fn project(
        project: &ProjectPaths,
        identity: &ProjectIdentity,
        limits: HubLimits,
    ) -> io::Result<Self> {
        if !identity.allows_persistent_stores() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let parent = limits
            .projection
            .capture_path(&project.project_runtime(identity.id(), &[])?)
            .map_err(projection_error)?;
        let registry = Self {
            parent,
            brand: project.brand().key().into(),
            scope: HubScope::Project {
                project_id: identity.id(),
            },
            project: Some(project.clone()),
            limits,
        };
        registry.validate_context()?;
        registry.validate_paths()?;
        Ok(registry)
    }
    pub fn global(
        installation: &InstallationPaths,
        id: &InstallationId,
        limits: HubLimits,
    ) -> io::Result<Self> {
        let parent = limits
            .projection
            .capture_path(installation.root())
            .map_err(projection_error)?;
        limits.path(&parent)?;
        let registry = Self {
            parent,
            brand: installation.brand().key().into(),
            scope: HubScope::Global {
                installation_id: id.as_str().into(),
            },
            project: None,
            limits,
        };
        registry.validate_paths()?;
        Ok(registry)
    }
    fn validate_paths(&self) -> io::Result<()> {
        self.limits
            .projection
            .append(self.parent.as_path(), &["hub.json"])
            .map_err(projection_error)?;
        self.limits
            .projection
            .append(self.parent.as_path(), &[".hub.json.lock"])
            .map_err(projection_error)?;
        Ok(())
    }
    pub fn path(&self) -> std::path::PathBuf {
        self.parent.as_path().join("hub.json")
    }
    pub fn lock_path(&self) -> std::path::PathBuf {
        self.parent.as_path().join(".hub.json.lock")
    }
    pub fn scope(&self) -> &HubScope {
        &self.scope
    }
    fn validate_context(&self) -> io::Result<()> {
        if let Some(project) = &self.project {
            let identity = ProjectIdentityStore::at(project)
                .load()?
                .ok_or_else(invalid)?;
            if self.scope
                != (HubScope::Project {
                    project_id: identity.id(),
                })
                || !identity.allows_persistent_stores()
                || self.parent.as_path() != project.project_runtime(identity.id(), &[])?
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn empty(&self) -> HubDocument {
        HubDocument {
            format: 4,
            brand: self.brand.clone(),
            scope: self.scope.clone(),
            agents: Vec::new(),
            artifacts: BTreeMap::new(),
            intent: None,
        }
    }
    fn check_parent(&self) -> io::Result<bool> {
        match fs::symlink_metadata(self.parent.as_path()) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                if fs::canonicalize(self.parent.as_path())? != self.parent.as_path() {
                    return Err(invalid());
                }
                Ok(true)
            }
            Ok(_) => Err(invalid()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut ancestor = self.parent.as_path().parent().ok_or_else(invalid)?;
                loop {
                    match fs::symlink_metadata(ancestor) {
                        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                            if fs::canonicalize(ancestor)? != ancestor {
                                return Err(invalid());
                            }
                            return Ok(false);
                        }
                        Ok(_) => return Err(invalid()),
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {
                            ancestor = ancestor.parent().ok_or_else(invalid)?
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            Err(error) => Err(error),
        }
    }
    pub fn load(&self) -> io::Result<HubDocument> {
        self.validate_context()?;
        if !self.check_parent()? {
            return Ok(self.empty());
        }
        let Some((bytes, _)) = local_files::read_regular(&self.path(), self.limits.document_bytes)?
        else {
            return Ok(self.empty());
        };
        let document = codec::decode(&bytes, self.limits)?.restore(self.limits)?;
        document.validate(self)?;
        Ok(document)
    }
    pub fn writer(&self, timeout: Duration, cancellation: &Cancellation) -> io::Result<HubWriter> {
        cancelled(cancellation)?;
        self.validate_context()?;
        if !self.check_parent()? {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.recursive(true).create(self.parent.as_path())?;
        }
        if !self.check_parent()? {
            return Err(invalid());
        }
        local_files::regular_metadata(&self.lock_path())?;
        let lock = FileLock::acquire(self.lock_path(), timeout, cancellation)?;
        if !self.check_parent()? {
            return Err(invalid());
        }
        local_files::regular_metadata(&self.lock_path())?;
        let document = self.load()?;
        Ok(HubWriter {
            registry: self.clone(),
            document,
            cancellation: cancellation.clone(),
            _lock: lock,
            #[cfg(test)]
            fault: None,
        })
    }
    fn validate_intent(&self, intent: &LogicalIntent) -> io::Result<()> {
        let limits = self.limits;
        limits.key(&intent.key)?;
        let kind = ArtifactType::ALL
            .into_iter()
            .find(|kind| kind.as_str() == intent.key.artifact_type())
            .ok_or_else(invalid)?;
        let physical = matches!(
            kind,
            ArtifactType::Agent
                | ArtifactType::Skill
                | ArtifactType::Command
                | ArtifactType::Workflow
        );
        if match &self.scope {
            HubScope::Global { .. } => intent.consumer != PublicationSlot::Scope,
            HubScope::Project { .. } => {
                physical != matches!(intent.consumer, PublicationSlot::ProjectRoot(_))
            }
        } {
            return Err(invalid());
        }
        if matches!(
            InstallationTarget::from_key(&intent.key)?,
            InstallationTarget::GlobalNative | InstallationTarget::GlobalCache
        ) {
            return Err(invalid());
        }
        for entry in [&intent.old, &intent.desired].into_iter().flatten() {
            entry.validate(limits)?;
            entry.validate_scope(&self.scope)?;
            if entry.installations.iter().any(|v| {
                matches!(
                    v.target,
                    InstallationTarget::GlobalNative | InstallationTarget::GlobalCache
                )
            }) {
                return Err(invalid());
            }
            if entry.consumer != intent.consumer
                || entry.id.as_str() != intent.key.id()
                || entry.artifact_type.as_str() != intent.key.artifact_type()
            {
                return Err(invalid());
            }
        }
        if let Some(desired) = &intent.desired {
            let old_others = intent
                .old
                .iter()
                .flat_map(|v| &v.installations)
                .filter(|v| v.target.namespace() != intent.key.adapter());
            let new_others = desired
                .installations
                .iter()
                .filter(|v| v.target.namespace() != intent.key.adapter());
            if !old_others.eq(new_others) {
                return Err(invalid());
            }
        }
        let validate_publish = |bindings: &PublicationBindings| -> io::Result<()> {
            let desired = intent.desired.as_ref().ok_or_else(invalid)?;
            let desired = desired
                .installations
                .iter()
                .find(|v| v.target.namespace() == intent.key.adapter())
                .ok_or_else(invalid)?;
            let expected_runtime = match &self.project {
                Some(project) => {
                    let HubScope::Project { project_id } = &self.scope else {
                        return Err(invalid());
                    };
                    project.project_runtime(*project_id, &[])?
                }
                None => self.parent.as_path().join("runtime"),
            };
            if bindings.runtime.as_path() != expected_runtime {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "publication runtime differs from the registry runtime",
                ));
            }
            let binding = bindings.binding;
            let planner = crate::ProjectionPlanner::new(
                bindings.destination_root.clone(),
                bindings.runtime.clone(),
                limits.projection,
                limits.artifacts,
                bindings.mode,
            );
            let targets = planner
                .preflight(&[crate::ProjectionRequest {
                    binding,
                    source: &desired.original_source,
                    alias: desired.alias.as_deref(),
                    entry_name: desired.entry_name.as_deref(),
                }])
                .map_err(projection_error)?;
            let target = targets.first().ok_or_else(invalid)?;
            let lexical_destination = target.destination().ok_or_else(invalid)?;
            if target.key() != &intent.key
                || target.name() != bindings.name
                || lexical_destination.as_path().parent() != Some(bindings.input_parent.as_path())
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "publication target differs from the captured binding or destination",
                ));
            }
            bindings.validate(&intent.key, desired, limits)
        };
        match &intent.goal {
            LogicalGoal::Publish { bindings } => validate_publish(bindings)?,
            LogicalGoal::Remove
                if intent.desired.is_none() && intent.phase != IntentPhase::PreparingParents => {}
            LogicalGoal::Forget { preview, previous }
                if intent.phase != IntentPhase::PreparingParents =>
            {
                validate_preview(preview, limits)?;
                if let Some(previous) = previous {
                    match &previous.goal {
                        PreviousGoal::Publish { bindings } => validate_publish(bindings)?,
                        PreviousGoal::Remove
                            if intent.desired.is_none()
                                && previous.phase != IntentPhase::PreparingParents => {}
                        _ => return Err(invalid()),
                    }
                } else if intent.desired.is_some() {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
}
fn validate_preview(preview: &ForgetPreview, limits: HubLimits) -> io::Result<()> {
    let physical = |value: &PhysicalDescriptor| -> io::Result<()> {
        limits.path(&value.source)?;
        limits.path(&value.destination)
    };
    if let Some(owned) = &preview.owned {
        physical(owned)?;
    }
    if let Some(pending) = &preview.pending {
        physical(&pending.candidate)?;
        for path in [
            &pending.staging_directory,
            &pending.old_staging_directory,
            &pending.old_destination,
            &pending.payload,
            &pending.backup,
        ]
        .into_iter()
        .flatten()
        {
            limits.path(path)?;
        }
    }
    if preview.possible_parents.len() > 2 {
        return Err(invalid());
    }
    for (index, path) in preview.possible_parents.iter().enumerate() {
        limits.path(path)?;
        if preview.possible_parents[..index].contains(path) {
            return Err(invalid());
        }
    }
    Ok(())
}

pub struct HubWriter {
    registry: HubRegistry,
    document: HubDocument,
    cancellation: Cancellation,
    _lock: FileLock,
    #[cfg(test)]
    fault: Option<TestFault>,
}
#[cfg(test)]
#[derive(Clone, Copy)]
enum TestFault {
    BeforeReplace,
    AfterReplace,
}
impl HubWriter {
    pub fn snapshot(&self) -> &HubDocument {
        &self.document
    }
    fn save(&mut self, document: HubDocument) -> io::Result<bool> {
        cancelled(&self.cancellation)?;
        self.registry.validate_context()?;
        if !self.registry.check_parent()? {
            return Err(invalid());
        }
        let serialized_bytes = document.validate(&self.registry)?;
        if self.document == document {
            return Ok(false);
        }
        let bytes = codec::encode(&document, self.registry.limits, serialized_bytes)?;
        let metadata = local_files::regular_metadata(&self.registry.path())?;
        if metadata
            .as_ref()
            .is_some_and(|v| v.permissions().readonly())
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let permissions = metadata.map(|v| v.permissions());
        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            permissions.or_else(|| Some(fs::Permissions::from_mode(0o600)))
        };
        #[cfg(test)]
        let fault = self.fault.take();
        #[cfg(test)]
        if matches!(fault, Some(TestFault::BeforeReplace)) {
            return Err(io::ErrorKind::Other.into());
        }
        local_files::atomic_replace(
            &self.registry.path(),
            &bytes,
            permissions,
            &self.cancellation,
        )?;
        self.document = document;
        #[cfg(test)]
        if matches!(fault, Some(TestFault::AfterReplace)) {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(true)
    }
    fn ready(&self) -> io::Result<()> {
        if self.document.intent.is_some() {
            Err(io::ErrorKind::WouldBlock.into())
        } else {
            Ok(())
        }
    }
    pub fn set_agents(&mut self, agents: &[NativeBinding]) -> io::Result<bool> {
        self.ready()?;
        if agents.len() > NativeBinding::ALL.len() {
            return Err(invalid());
        }
        for (index, agent) in agents.iter().enumerate() {
            if agents[..index].contains(agent) {
                return Err(invalid());
            }
        }
        let mut document = self.document.clone();
        document.agents = agents.to_vec();
        self.save(document)
    }
    /// Remove selection membership only. Published agents and shared assets keep ownership.
    pub fn unregister_binding(&mut self, binding: NativeBinding) -> io::Result<bool> {
        self.ready()?;
        let mut document = self.document.clone();
        document.agents.retain(|member| *member != binding);
        self.save(document)
    }
    /// Only for logical/dynamic/native metadata; a physical entry requires a journal goal.
    pub fn put_metadata(&mut self, entry: InstalledArtifact) -> io::Result<bool> {
        self.ready()?;
        entry.validate(self.registry.limits)?;
        entry.validate_scope(&self.registry.scope)?;
        if entry
            .installations
            .iter()
            .any(|v| matches!(v.backend, InstalledBackend::Physical { .. }))
        {
            return Err(invalid());
        }
        let mut document = self.document.clone();
        let global = matches!(document.scope, HubScope::Global { .. });
        let versions = document
            .artifacts
            .entry(entry.artifact_type)
            .or_default()
            .entry(entry.id.as_str().into())
            .or_default();
        if global {
            let version = entry.installations[0].original_source.version();
            if entry
                .installations
                .iter()
                .any(|v| v.original_source.version() != version)
            {
                return Err(invalid());
            }
            if let Some(existing) = versions
                .iter_mut()
                .find(|v| v.installations[0].original_source.version() == version)
            {
                *existing = entry;
            } else {
                versions.push(entry);
            }
        } else if let Some(existing) = versions.iter_mut().find(|v| v.consumer == entry.consumer) {
            *existing = entry;
        } else {
            versions.push(entry);
        }
        self.save(document)
    }
    /// Replace one exact global installation in a single registry save.
    /// Other installations and versions remain unchanged; selection is not authority.
    pub fn replace_metadata_version(
        &mut self,
        expected: &Installation,
        entry: InstalledArtifact,
    ) -> io::Result<bool> {
        self.ready()?;
        entry.validate(self.registry.limits)?;
        entry.validate_scope(&self.registry.scope)?;
        if !matches!(self.document.scope, HubScope::Global { .. }) || entry.installations.len() != 1
        {
            return Err(invalid());
        }
        let desired = &entry.installations[0];
        if matches!(expected.backend, InstalledBackend::Physical { .. })
            || matches!(desired.backend, InstalledBackend::Physical { .. })
            || desired.target != expected.target
            || desired.original_source.producer() != expected.original_source.producer()
            || desired.original_source.artifact_type() != expected.original_source.artifact_type()
            || desired.original_source.id() != expected.original_source.id()
            || desired.original_source.remote_file_origin()
                != expected.original_source.remote_file_origin()
            || matches!((&expected.backend, &desired.backend),
                (InstalledBackend::RemoteNative(old), InstalledBackend::RemoteNative(new))
                    if old.provider_name() != new.provider_name())
        {
            return Err(invalid());
        }
        let mut document = self.document.clone();
        let versions = document
            .artifacts
            .get_mut(&entry.artifact_type)
            .and_then(|entries| entries.get_mut(entry.id.as_str()))
            .ok_or(io::ErrorKind::NotFound)?;
        let old_index = versions
            .iter()
            .position(|artifact| {
                artifact
                    .installations
                    .iter()
                    .any(|installed| installed.same_source(expected))
            })
            .ok_or(io::ErrorKind::AlreadyExists)?;
        let old = &mut versions[old_index];
        old.installations
            .retain(|installed| !installed.same_source(expected));
        if old.installations.is_empty() {
            versions.remove(old_index);
        }
        if let Some(current) = versions.iter_mut().find(|artifact| {
            artifact.installations[0].original_source.version() == desired.original_source.version()
        }) {
            if let Some(installed) = current
                .installations
                .iter_mut()
                .find(|installed| installed.target == desired.target)
            {
                if !installed.same_source(desired) {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
                *installed = desired.clone();
            } else {
                current.installations.push(desired.clone());
            }
        } else {
            versions.push(entry);
        }
        self.save(document)
    }
    /// Remove one global metadata version without touching its payload.
    pub fn remove_metadata_version(
        &mut self,
        kind: ArtifactType,
        id: &str,
        version: &ConcreteVersion,
    ) -> io::Result<bool> {
        self.ready()?;
        if !matches!(self.document.scope, HubScope::Global { .. }) {
            return Err(invalid());
        }
        let Some(entry) = self.document.get_version(kind, id, version) else {
            return Ok(false);
        };
        if entry
            .installations
            .iter()
            .any(|v| matches!(v.backend, InstalledBackend::Physical { .. }))
        {
            return Err(invalid());
        }
        let mut document = self.document.clone();
        let entries = document.artifacts.get_mut(&kind).ok_or_else(invalid)?;
        let versions = entries.get_mut(id).ok_or_else(invalid)?;
        versions.retain(|entry| entry.installations[0].original_source.version() != version);
        if versions.is_empty() {
            entries.remove(id);
        }
        if entries.is_empty() {
            document.artifacts.remove(&kind);
        }
        self.save(document)
    }
    pub fn record(&mut self, intent: LogicalIntent) -> io::Result<bool> {
        self.ready()?;
        self.registry.validate_intent(&intent)?;
        let kind = ArtifactType::ALL
            .into_iter()
            .find(|v| v.as_str() == intent.key.artifact_type())
            .ok_or_else(invalid)?;
        if self
            .document
            .ambiguous(&intent.consumer, kind, intent.key.id())
        {
            return Err(invalid());
        }
        if !matches!(
            intent.phase,
            IntentPhase::IntentReady | IntentPhase::PreparingParents
        ) || self.document.for_key(&intent.consumer, &intent.key) != intent.old.as_ref()
            || matches!(
                intent.goal,
                LogicalGoal::Forget {
                    previous: Some(_),
                    ..
                }
            )
        {
            return Err(invalid());
        }
        let mut document = self.document.clone();
        document.intent = Some(intent);
        self.preflight_committed(&document)?;
        self.save(document)
    }
    fn preflight_committed(&self, document: &HubDocument) -> io::Result<()> {
        let Some(intent) = &document.intent else {
            return Ok(());
        };
        let mut future = document.clone();
        if let LogicalGoal::Publish { .. } = intent.goal {
            let desired = intent.desired.as_ref().ok_or_else(invalid)?;
            if future.for_key(&intent.consumer, &intent.key).is_none()
                && future.entries().count() >= self.registry.limits.entries
            {
                return Err(invalid());
            }
            future.replace_consumer(desired.clone());
        } else {
            future.remove_target(&intent.consumer, &intent.key)?;
        }
        future.intent.as_mut().ok_or_else(invalid)?.phase = IntentPhase::LogicalCommitted;
        let serialized_bytes = future.validate(&self.registry)?;
        codec::encode(&future, self.registry.limits, serialized_bytes)?;
        Ok(())
    }
    pub fn advance(&mut self, phase: IntentPhase) -> io::Result<bool> {
        let intent = self.document.intent.as_ref().ok_or_else(invalid)?;
        if phase == intent.phase {
            return Ok(false);
        }
        if !matches!(
            (intent.phase, phase),
            (IntentPhase::PreparingParents, IntentPhase::IntentReady)
                | (IntentPhase::IntentReady, IntentPhase::PhysicalStarted)
                | (IntentPhase::PhysicalStarted, IntentPhase::PhysicalComplete)
        ) {
            return Err(invalid());
        }
        let mut document = self.document.clone();
        document.intent.as_mut().ok_or_else(invalid)?.phase = phase;
        self.save(document)
    }
    pub fn commit_logical(&mut self) -> io::Result<bool> {
        let intent = self.document.intent.as_ref().ok_or_else(invalid)?;
        if intent.phase == IntentPhase::LogicalCommitted {
            return Ok(false);
        }
        if intent.phase != IntentPhase::PhysicalComplete {
            return Err(invalid());
        }
        let mut document = self.document.clone();
        match &intent.goal {
            LogicalGoal::Publish { .. } => {
                let desired = intent.desired.as_ref().ok_or_else(invalid)?;
                document.replace_consumer(desired.clone());
            }
            LogicalGoal::Remove | LogicalGoal::Forget { .. } => {
                document.remove_target(&intent.consumer, &intent.key)?;
            }
        }
        document.intent.as_mut().ok_or_else(invalid)?.phase = IntentPhase::LogicalCommitted;
        self.save(document)
    }
    pub fn clear(&mut self) -> io::Result<bool> {
        let Some(intent) = self.document.intent.as_ref() else {
            return Ok(false);
        };
        if intent.phase != IntentPhase::LogicalCommitted {
            return Err(invalid());
        }
        let mut document = self.document.clone();
        document.intent = None;
        self.save(document)
    }
    /// Discard a generated intent only after Core proved that its desired publication
    /// did not commit. Existing catalog membership is preserved for an explicit retry.
    pub(crate) fn discard_unpublished_generated(&mut self) -> io::Result<bool> {
        let Some(intent) = self.document.intent.as_ref() else {
            return Ok(false);
        };
        if !matches!(&intent.goal, LogicalGoal::Publish { bindings }
            if matches!(bindings.physical_input, PublicationInput::Generated { .. }))
            || matches!(
                intent.phase,
                IntentPhase::PhysicalComplete | IntentPhase::LogicalCommitted
            )
        {
            return Err(invalid());
        }
        let mut document = self.document.clone();
        document.intent = None;
        self.save(document)
    }
    /// Explicit same-key Forget may supersede an unfinished goal without physical recovery.
    pub fn forget(
        &mut self,
        consumer: PublicationSlot,
        key: &ArtifactKey,
        preview: ForgetPreview,
    ) -> io::Result<bool> {
        self.registry.limits.key(key)?;
        validate_preview(&preview, self.registry.limits)?;
        let Some(existing) = self.document.intent.as_ref() else {
            return self.record(LogicalIntent {
                consumer,
                key: key.clone(),
                phase: IntentPhase::IntentReady,
                goal: LogicalGoal::Forget {
                    preview: Box::new(preview),
                    previous: None,
                },
                old: self.document.for_key(&consumer, key).cloned(),
                desired: None,
            });
        };
        if &existing.key != key || existing.consumer != consumer {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        if matches!(existing.goal, LogicalGoal::Forget { .. }) {
            return Ok(false);
        }
        let previous = PreviousIntent {
            phase: existing.phase,
            goal: match &existing.goal {
                LogicalGoal::Publish { bindings } => PreviousGoal::Publish {
                    bindings: bindings.clone(),
                },
                LogicalGoal::Remove => PreviousGoal::Remove,
                LogicalGoal::Forget { .. } => return Err(invalid()),
            },
        };
        let mut document = self.document.clone();
        let intent = document.intent.as_mut().ok_or_else(invalid)?;
        intent.phase = IntentPhase::IntentReady;
        intent.goal = LogicalGoal::Forget {
            preview: Box::new(preview),
            previous: Some(Box::new(previous)),
        };
        self.preflight_committed(&document)?;
        self.save(document)
    }
}
