//! Producer identity and source paths are data, never hydration or ownership proof.
mod resources;
use crate::projection::{ProjectionError, ProjectionLimits, ProjectionStage, validate_name};
use graphit_core::{
    native_path::NativePath,
    project_identity::ProjectId,
    project_paths::{InstallationPaths, ProjectPaths},
};
pub use resources::{ResourceIdentity, ResourcesPage, ResourcesQuery, list_resources};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{fmt::Write as _, io, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactType {
    Skill,
    Command,
    Workflow,
    Agent,
    Rule,
    Mcp,
    Ast,
    Knowledge,
    Language,
    Power,
}
impl ArtifactType {
    pub const ALL: [Self; 10] = [
        Self::Skill,
        Self::Command,
        Self::Workflow,
        Self::Agent,
        Self::Rule,
        Self::Mcp,
        Self::Ast,
        Self::Knowledge,
        Self::Language,
        Self::Power,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Command => "command",
            Self::Workflow => "workflow",
            Self::Agent => "agent",
            Self::Rule => "rule",
            Self::Mcp => "mcp",
            Self::Ast => "ast",
            Self::Knowledge => "knowledge",
            Self::Language => "language",
            Self::Power => "power",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
pub struct ArtifactId(String);
impl ArtifactId {
    pub fn new(value: &str, limits: ProjectionLimits) -> Result<Self, ProjectionError> {
        limits.identity(value)?;
        for component in value.split('/') {
            validate_name(component)?;
        }
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn basename(&self) -> &str {
        self.0.rsplit('/').next().expect("validated artifact ID")
    }
}

/// An upstream-resolved value; this type never interprets selectors or fetches versions.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize)]
pub struct ConcreteVersion(String);
impl ConcreteVersion {
    pub fn new(value: &str, limits: ProjectionLimits) -> Result<Self, ProjectionError> {
        limits.identity(value)?;
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SourceOrigin {
    HubCache,
    Resources,
    LocalLink,
}

/// Configured origin references, never credentials, identity or project authorization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemoteFileOrigin {
    Broker {
        provider: String,
    },
    Standalone {
        provider: String,
        profile: Option<String>,
    },
}
impl RemoteFileOrigin {
    pub fn provider(&self) -> &str {
        match self {
            Self::Broker { provider } | Self::Standalone { provider, .. } => provider,
        }
    }
    fn validate(&self, limits: ProjectionLimits) -> Result<(), ProjectionError> {
        limits.identity(self.provider())?;
        if let Self::Standalone {
            profile: Some(profile),
            ..
        } = self
        {
            limits.identity(profile)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceRef {
    origin: SourceOrigin,
    producer: ProjectId,
    artifact_type: ArtifactType,
    id: ArtifactId,
    #[serde(skip_serializing_if = "Option::is_none")]
    physical_id: Option<ArtifactId>,
    version: ConcreteVersion,
    root: NativePath,
    #[serde(skip)]
    directory: NativePath,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_file_origin: Option<RemoteFileOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_sha256: Option<[u8; 32]>,
}
/// Stored observations only: decoding this record never resolves a source or grants access.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecord {
    pub origin: SourceOrigin,
    pub producer: ProjectId,
    pub artifact_type: ArtifactType,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_id: Option<String>,
    pub version: String,
    pub root: NativePath,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_file_origin: Option<RemoteFileOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_sha256: Option<[u8; 32]>,
}

pub(crate) fn normal_absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path.components().any(|part| {
            matches!(
                part,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
}

impl SourceRef {
    /// Validate stored data under the current policy without filesystem I/O.
    pub fn restore(
        record: SourceRecord,
        limits: ProjectionLimits,
    ) -> Result<Self, ProjectionError> {
        if record.remote_file_origin.is_some() != record.archive_sha256.is_some() {
            return Err(invalid(ProjectionStage::Validate));
        }
        if let Some(remote) = &record.remote_file_origin {
            if record.origin != SourceOrigin::HubCache
                || matches!(
                    record.artifact_type,
                    ArtifactType::Ast | ArtifactType::Knowledge
                )
            {
                return Err(invalid(ProjectionStage::Validate));
            }
            remote.validate(limits)?;
        }
        let id = ArtifactId::new(&record.id, limits)?;
        let physical_id = record
            .physical_id
            .as_ref()
            .map(|value| {
                let physical = ArtifactId::new(value, limits)?;
                if physical == id {
                    return Err(invalid(ProjectionStage::Validate));
                }
                Ok(physical)
            })
            .transpose()?;
        let version = ConcreteVersion::new(&record.version, limits)?;
        if !normal_absolute(record.root.as_path()) {
            return Err(invalid(ProjectionStage::Validate));
        }
        let root = limits.capture_path(record.root.as_path())?;
        let directory = match record.origin {
            SourceOrigin::HubCache => cache_directory(
                root.as_path(),
                record.producer,
                record.artifact_type,
                physical_id.as_ref().unwrap_or(&id),
                &version,
                record
                    .remote_file_origin
                    .as_ref()
                    .zip(record.archive_sha256.as_ref()),
                limits,
            )?,
            SourceOrigin::Resources => {
                if physical_id.is_some() {
                    return Err(invalid(ProjectionStage::Validate));
                }
                let base = limits.append(
                    root.as_path(),
                    &["artifacts", record.artifact_type.as_str()],
                )?;
                identity_directory(base.as_path(), &id, &version, limits)?
            }
            SourceOrigin::LocalLink => {
                if physical_id.is_some() {
                    return Err(invalid(ProjectionStage::Validate));
                }
                root.clone()
            }
        };
        Ok(Self {
            remote_file_origin: record.remote_file_origin,
            archive_sha256: record.archive_sha256,
            origin: record.origin,
            producer: record.producer,
            artifact_type: record.artifact_type,
            id,
            physical_id,
            version,
            root,
            directory,
        })
    }

    pub fn hub_cache(
        installation: &InstallationPaths,
        producer: ProjectId,
        artifact_type: ArtifactType,
        id: ArtifactId,
        remote_id: Option<ArtifactId>,
        version: ConcreteVersion,
        limits: ProjectionLimits,
    ) -> Result<Self, ProjectionError> {
        limits.identity(id.as_str())?;
        limits.identity(version.as_str())?;
        if let Some(remote) = &remote_id {
            limits.identity(remote.as_str())?;
        }
        let physical_id = remote_id.filter(|physical| physical != &id);
        let root = limits.capture_path(installation.root())?;
        let directory = cache_directory(
            root.as_path(),
            producer,
            artifact_type,
            physical_id.as_ref().unwrap_or(&id),
            &version,
            None,
            limits,
        )?;
        Ok(Self {
            remote_file_origin: None,
            archive_sha256: None,
            origin: SourceOrigin::HubCache,
            producer,
            artifact_type,
            id,
            physical_id,
            version,
            root,
            directory,
        })
    }
    pub fn resources(
        project: &ProjectPaths,
        producer: ProjectId,
        artifact_type: ArtifactType,
        id: ArtifactId,
        version: ConcreteVersion,
        limits: ProjectionLimits,
    ) -> Result<Self, ProjectionError> {
        let root = limits.append(project.root(), &[project.brand().directory()])?;
        let base = limits.append(root.as_path(), &["artifacts", artifact_type.as_str()])?;
        let directory = identity_directory(base.as_path(), &id, &version, limits)?;
        Ok(Self {
            remote_file_origin: None,
            archive_sha256: None,
            origin: SourceOrigin::Resources,
            producer,
            artifact_type,
            physical_id: None,
            id,
            version,
            root,
            directory,
        })
    }
    pub fn local_link(
        directory: NativePath,
        producer: ProjectId,
        artifact_type: ArtifactType,
        id: ArtifactId,
        version: ConcreteVersion,
        limits: ProjectionLimits,
    ) -> Result<Self, ProjectionError> {
        limits.identity(id.as_str())?;
        limits.identity(version.as_str())?;
        let directory = limits.capture_path(directory.as_path())?;
        Ok(Self {
            remote_file_origin: None,
            archive_sha256: None,
            origin: SourceOrigin::LocalLink,
            producer,
            artifact_type,
            physical_id: None,
            id,
            version,
            root: directory.clone(),
            directory,
        })
    }
    pub fn with_remote_file_origin(
        mut self,
        origin: RemoteFileOrigin,
        archive_sha256: [u8; 32],
        limits: ProjectionLimits,
    ) -> Result<Self, ProjectionError> {
        if self.origin != SourceOrigin::HubCache
            || matches!(
                self.artifact_type,
                ArtifactType::Ast | ArtifactType::Knowledge
            )
        {
            return Err(invalid(ProjectionStage::Validate));
        }
        origin.validate(limits)?;
        self.directory = cache_directory(
            self.root.as_path(),
            self.producer,
            self.artifact_type,
            self.physical_id(),
            &self.version,
            Some((&origin, &archive_sha256)),
            limits,
        )?;
        self.remote_file_origin = Some(origin);
        self.archive_sha256 = Some(archive_sha256);
        Ok(self)
    }
    pub fn remote_file_origin(&self) -> Option<&RemoteFileOrigin> {
        self.remote_file_origin.as_ref()
    }
    pub fn archive_sha256(&self) -> Option<&[u8; 32]> {
        self.archive_sha256.as_ref()
    }
    pub fn origin(&self) -> SourceOrigin {
        self.origin
    }
    pub fn producer(&self) -> ProjectId {
        self.producer
    }
    pub fn artifact_type(&self) -> ArtifactType {
        self.artifact_type
    }
    pub fn id(&self) -> &ArtifactId {
        &self.id
    }
    pub fn physical_id(&self) -> &ArtifactId {
        self.physical_id.as_ref().unwrap_or(&self.id)
    }
    pub fn version(&self) -> &ConcreteVersion {
        &self.version
    }
    pub fn root(&self) -> &NativePath {
        &self.root
    }
    pub fn directory(&self) -> &NativePath {
        &self.directory
    }
}

fn cache_directory(
    root: &Path,
    producer: ProjectId,
    kind: ArtifactType,
    physical_id: &ArtifactId,
    version: &ConcreteVersion,
    file: Option<(&RemoteFileOrigin, &[u8; 32])>,
    limits: ProjectionLimits,
) -> Result<NativePath, ProjectionError> {
    let mut base = limits.append(
        root,
        &["artifacts", "modules", &producer.to_string(), kind.as_str()],
    )?;
    if let Some((origin, digest)) = file {
        let origin = serde_json::to_vec(origin).map_err(|_| invalid(ProjectionStage::Validate))?;
        let origin_digest: [u8; 32] = Sha256::digest(origin).into();
        let hex = |bytes: &[u8; 32]| {
            let mut encoded = String::with_capacity(64);
            for byte in bytes {
                write!(&mut encoded, "{byte:02x}").expect("String formatting is infallible");
            }
            encoded
        };
        base = limits.append(
            base.as_path(),
            &["origins", &hex(&origin_digest), "sha256", &hex(digest)],
        )?;
    }
    identity_directory(base.as_path(), physical_id, version, limits)
}

fn identity_directory(
    base: &Path,
    id: &ArtifactId,
    version: &ConcreteVersion,
    limits: ProjectionLimits,
) -> Result<NativePath, ProjectionError> {
    limits.identity(id.as_str())?;
    limits.identity(version.as_str())?;
    let mut path = limits.join(base, std::ffi::OsStr::new("ids"))?;
    for (prefix, value, marker) in [
        ("i-", id.as_str(), "versions"),
        ("v-", version.as_str(), "payload"),
    ] {
        for chunk in value.as_bytes().chunks(64) {
            let mut component = String::with_capacity(2 + chunk.len() * 2);
            component.push_str(prefix);
            const HEX: &[u8; 16] = b"0123456789abcdef";
            for byte in chunk {
                component.push(HEX[(byte >> 4) as usize] as char);
                component.push(HEX[(byte & 15) as usize] as char);
            }
            path = limits.join(path.as_path(), std::ffi::OsStr::new(&component))?;
        }
        path = limits.join(path.as_path(), std::ffi::OsStr::new(marker))?;
    }
    Ok(path)
}

pub(crate) fn invalid(stage: ProjectionStage) -> ProjectionError {
    ProjectionError::new(stage, io::ErrorKind::InvalidInput)
}
