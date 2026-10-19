//! Strict remote catalog data. Decoding never authorizes reads or creates a source lease.
mod codec;
#[cfg(test)]
mod tests;

use crate::{ArtifactId, ArtifactType, ConcreteVersion, ProjectionLimits, policy::RequestContext};
use graphit_core::project_identity::ProjectId;
use serde::Serialize;
use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataError {
    InvalidPolicy,
    InvalidDocument,
    Limit,
    Cancelled,
    Deadline,
}
impl fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPolicy => "invalid remote metadata policy",
            Self::InvalidDocument => "invalid remote metadata document",
            Self::Limit => "remote metadata limit exceeded",
            Self::Cancelled => "remote metadata request cancelled",
            Self::Deadline => "remote metadata deadline exceeded",
        })
    }
}
impl Error for MetadataError {}

/// Explicit capacities for a single document, including JSON keys as scalar nodes.
#[derive(Clone, Copy, Debug)]
pub struct MetadataLimits {
    pub bytes: usize,
    pub nodes: usize,
    pub scalar_bytes: usize,
    pub collection_entries: usize,
    pub versions: usize,
    pub dependencies: usize,
    pub projection: ProjectionLimits,
}
impl MetadataLimits {
    fn validate(self) -> Result<(), MetadataError> {
        if [
            self.bytes,
            self.nodes,
            self.scalar_bytes,
            self.collection_entries,
            self.versions,
            self.dependencies,
        ]
        .contains(&0)
        {
            Err(MetadataError::InvalidPolicy)
        } else {
            Ok(())
        }
    }
}

/// Borrowed publication input. Encoding validates it before retaining output bytes.
#[derive(Serialize)]
pub struct ManifestInput<'a> {
    pub producer: ProjectId,
    pub artifact_type: ArtifactType,
    pub id: &'a str,
    pub name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
    #[serde(skip_serializing_if = "<[&str]>::is_empty")]
    pub tags: &'a [&'a str],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<AuthorInput<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<&'a str>,
    pub versions: &'a [VersionInput<'a>],
    pub dependencies: &'a [DependencyInput<'a>],
}
#[derive(Serialize)]
pub struct AuthorInput<'a> {
    pub username: &'a str,
}
#[derive(Serialize)]
pub struct VersionInput<'a> {
    pub version: &'a str,
    pub hash: &'a str,
}
#[derive(Serialize)]
pub struct DependencyInput<'a> {
    pub artifact_type: ArtifactType,
    pub id: &'a str,
    /// A literal selector; version selection belongs to the authorized resolver.
    pub selector: &'a str,
}

pub struct RemoteArtifactManifest {
    producer: ProjectId,
    artifact_type: ArtifactType,
    id: ArtifactId,
    name: String,
    description: Option<String>,
    tags: Vec<String>,
    author: Option<String>,
    latest: Option<ConcreteVersion>,
    versions: Vec<RemoteVersion>,
    dependencies: Vec<RemoteDependency>,
}
pub struct RemoteVersion {
    version: ConcreteVersion,
    hash: [u8; 32],
}
impl RemoteVersion {
    pub fn version(&self) -> &ConcreteVersion {
        &self.version
    }
    pub fn hash(&self) -> &[u8; 32] {
        &self.hash
    }
}
pub struct RemoteDependency {
    artifact_type: ArtifactType,
    id: ArtifactId,
    selector: String,
}
impl RemoteDependency {
    pub fn artifact_type(&self) -> ArtifactType {
        self.artifact_type
    }
    pub fn id(&self) -> &ArtifactId {
        &self.id
    }
    pub fn selector(&self) -> &str {
        &self.selector
    }
}
impl RemoteArtifactManifest {
    pub fn decode(
        bytes: &[u8],
        limits: MetadataLimits,
        context: &RequestContext<'_>,
    ) -> Result<Self, MetadataError> {
        codec::decode(bytes, limits, context)
    }
    pub fn encode(
        input: &ManifestInput<'_>,
        limits: MetadataLimits,
        context: &RequestContext<'_>,
    ) -> Result<Vec<u8>, MetadataError> {
        codec::encode(input, limits, context)
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
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }
    pub fn latest(&self) -> Option<&ConcreteVersion> {
        self.latest.as_ref()
    }
    pub fn versions(&self) -> &[RemoteVersion] {
        &self.versions
    }
    pub fn dependencies(&self) -> &[RemoteDependency] {
        &self.dependencies
    }
}
