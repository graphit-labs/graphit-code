//! Local Hub installation metadata; reproducible publications are retried by sync.
mod codec;
mod display;
pub use display::ArtifactDisplayMetadata;
use display::RawDisplayMetadata;
#[cfg(test)]
mod tests;

use crate::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, NativeBinding, ProjectionLimits,
    SourceRecord, SourceRef,
    projection::{NodeKindDef, validate_name},
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
    time::{Duration, Instant},
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
        self.projection
            .path(path.as_path().to_owned())
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
    Original,
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
            Self::Original => f.write_str("Original"),
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
                    || !matches!(installation.payload, InstalledPayload::Original)
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
                    || !matches!(installation.payload, InstalledPayload::Original)
                    || installation.alias.is_some()
                    || installation.entry_name.is_some())
            {
                return Err(invalid());
            }
            match &installation.payload {
                InstalledPayload::Original => {
                    if self.artifact_type == ArtifactType::Agent && !global_cache {
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HubDocument {
    format: u32,
    brand: String,
    scope: HubScope,
    agents: Vec<NativeBinding>,
    artifacts: BTreeMap<ArtifactType, BTreeMap<String, Vec<InstalledArtifact>>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHubDocument {
    format: u32,
    brand: String,
    scope: HubScope,
    agents: Vec<NativeBinding>,
    artifacts: BTreeMap<ArtifactType, BTreeMap<String, Vec<RawInstalledArtifact>>>,
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
                        .any(|entry| !consumers.insert(entry.consumer.clone()))
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
        codec::count(self, limits)
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
            && self.limits.artifacts == store.limits()
    }
    pub fn project_paths(&self) -> Option<&ProjectPaths> {
        self.project.as_ref()
    }
    pub fn limits(&self) -> HubLimits {
        self.limits
    }
    /// Configured brand data; this observation grants no filesystem authority.
    pub fn brand(&self) -> &str {
        &self.brand
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
            .path(project.project_runtime(identity.id(), &[])?)
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
            .path(installation.root().to_owned())
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
        }
    }
    fn check_parent(&self) -> io::Result<bool> {
        match fs::metadata(self.parent.as_path()) {
            Ok(metadata) if metadata.is_dir() => Ok(true),
            Ok(_) => Err(io::ErrorKind::NotADirectory.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
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
    /// Open a logical metadata snapshot; commits acquire the short metadata lock.
    pub(crate) fn writer_for_request(
        &self,
        original: &graphit_core::file_io::SnapshotContext,
    ) -> io::Result<HubWriter> {
        let deadline = original.required_deadline()?;
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(io::ErrorKind::TimedOut)?;
        let mut writer = self.writer(remaining, original.cancellation())?;
        writer.deadline = deadline;
        Ok(writer)
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
        let document = self.load()?;
        Ok(HubWriter {
            registry: self.clone(),
            document,
            cancellation: cancellation.clone(),
            deadline: Instant::now()
                .checked_add(timeout)
                .ok_or(io::ErrorKind::InvalidInput)?,
        })
    }
}
pub struct HubWriter {
    registry: HubRegistry,
    document: HubDocument,
    cancellation: Cancellation,
    deadline: Instant,
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
        let bytes = if self.document == document {
            None
        } else {
            Some(codec::encode(
                &document,
                self.registry.limits,
                serialized_bytes,
            )?)
        };
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(io::ErrorKind::TimedOut)?;
        let _lock = FileLock::acquire(self.registry.lock_path(), remaining, &self.cancellation)?;
        if self.registry.load()? != self.document {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Hub metadata changed",
            ));
        }
        let Some(bytes) = bytes else {
            return Ok(false);
        };
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
        local_files::atomic_replace(
            &self.registry.path(),
            &bytes,
            permissions,
            &self.cancellation,
        )?;
        self.document = document;
        Ok(true)
    }
    pub fn remove_target(
        &mut self,
        consumer: &PublicationSlot,
        key: &ArtifactKey,
    ) -> io::Result<bool> {
        self.registry.limits.key(key)?;
        let mut document = self.document.clone();
        document.remove_target(consumer, key)?;
        self.save(document)
    }
    pub fn set_agents(&mut self, agents: &[NativeBinding]) -> io::Result<bool> {
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
        let mut document = self.document.clone();
        document.agents.retain(|member| *member != binding);
        self.save(document)
    }
    /// Save the current logical installation metadata after any physical publication.
    pub fn preflight_metadata(&self, entry: &InstalledArtifact) -> io::Result<()> {
        let document = self.metadata_document(entry.clone())?;
        let count = document.validate(&self.registry)?;
        codec::encode(&document, self.registry.limits, count)?;
        Ok(())
    }
    pub fn put_metadata(&mut self, entry: InstalledArtifact) -> io::Result<bool> {
        self.save(self.metadata_document(entry)?)
    }
    fn metadata_document(&self, entry: InstalledArtifact) -> io::Result<HubDocument> {
        entry.validate(self.registry.limits)?;
        entry.validate_scope(&self.registry.scope)?;
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
        Ok(document)
    }
    /// Replace one exact global installation in a single registry save.
    /// Other installations and versions remain unchanged; selection is not authority.
    pub fn replace_metadata_version(
        &mut self,
        expected: &Installation,
        entry: InstalledArtifact,
    ) -> io::Result<bool> {
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
        if !matches!(self.document.scope, HubScope::Global { .. }) {
            return Err(invalid());
        }
        let Some(_) = self.document.get_version(kind, id, version) else {
            return Ok(false);
        };
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
}
