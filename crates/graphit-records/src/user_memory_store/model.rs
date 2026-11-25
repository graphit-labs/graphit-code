use crate::memory_store::{
    CreateMemory, Memory, MemoryActivityHead, MemoryId, MemoryLexicalHit, MemoryMutation,
    MemoryReference, MemoryReferenceProvenance, MemoryReferenceResolution, MemoryReferenceTarget,
    MemoryReferencesWarning, MemoryStoreError,
};
use graphit_core::{artifact_store::UserIdentity, installation_identity::InstallationId};
use graphit_data::lance_branch_store::DeltaRecoveryKey;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Literal installation and issuer/subject identity data, never an access grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserMemoryOwner {
    #[serde(with = "installation_codec")]
    pub installation_id: InstallationId,
    pub identity: UserIdentity,
}

/// Logical USER reference identity. The private representation has no parser or
/// admission constructor; equality of scope identifiers never grants access.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct UserMemoryScopeId(String);
impl UserMemoryScopeId {
    pub fn from_owner(owner: &UserMemoryOwner) -> Result<Self, MemoryStoreError> {
        Ok(Self(serde_json::to_string(&[
            owner.installation_id.as_str(),
            owner.identity.issuer(),
            owner.identity.subject(),
        ])?))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Actual SDK publication metadata. USER callers select the default SDK target;
/// neither Git version nor PROJECT identity belongs to this observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserMemoryGeneration {
    pub owner: UserMemoryOwner,
    pub sdk_version: u64,
    pub physical_branch: Option<String>,
    pub inherited_target: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UserCreationMemory {
    pub owner: UserMemoryOwner,
    pub actor: String,
    pub input: CreateMemory,
    pub memory: Memory,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UserMemoryRevision {
    pub owner: UserMemoryOwner,
    pub actor: String,
    pub operation: MemoryMutation,
    pub before: Memory,
    pub after: Memory,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UserMemoryDetail {
    pub memory: Memory,
    pub creation: UserCreationMemory,
    pub generation: UserMemoryGeneration,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UserMemoryHistory {
    pub creation: UserCreationMemory,
    pub current: Memory,
    pub deleted: bool,
    pub revisions: Vec<UserMemoryRevision>,
    pub generation: UserMemoryGeneration,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserMemoryHeadRead {
    NoCorpus,
    Published {
        generation: UserMemoryGeneration,
        heads: Vec<MemoryActivityHead>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserMemoryListRead {
    NoCorpus,
    Published {
        generation: UserMemoryGeneration,
        memories: Vec<Memory>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UserMemoryLexicalSearchRead {
    NoCorpus,
    PublishedEmpty {
        generation: UserMemoryGeneration,
    },
    Published {
        generation: UserMemoryGeneration,
        total_hits: usize,
        hits: Vec<MemoryLexicalHit>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserMemoryLexicalIndexOutcome {
    NoCorpus,
    Ready(UserMemoryGeneration),
    Published(UserMemoryLexicalIndexPublication),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserMemoryLexicalIndexPublication {
    pub generation: UserMemoryGeneration,
    pub recovery_key: DeltaRecoveryKey,
}
/// Known ordinary SDK publication retained after a causal post-publication failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserMemoryPublication {
    pub generation: UserMemoryGeneration,
    pub recovery_key: DeltaRecoveryKey,
}

mod installation_codec {
    use super::*;
    pub fn serialize<S: Serializer>(id: &InstallationId, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(id.as_str())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<InstallationId, D::Error> {
        let raw = String::deserialize(d)?;
        let id = InstallationId::new(&raw).map_err(serde::de::Error::custom)?;
        if raw != id.as_str() {
            return Err(serde::de::Error::custom(
                "noncanonical installation identifier",
            ));
        }
        Ok(id)
    }
}

/// Closed observation data. Every continuation requires a fresh USER admission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserMemoryReferencesCursor {
    pub(super) binding: Box<UserReferenceBinding>,
    pub(super) direction: crate::memory_store::references::Direction,
    pub(super) offset: usize,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UserReferenceBinding {
    pub corpus_key: String,
    pub root: graphit_core::native_path::NativePath,
    pub generation: UserMemoryGeneration,
    pub outgoing: Option<MemoryId>,
    pub incoming: Option<MemoryReferenceTarget>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserMemoryReferencesQuery {
    pub outgoing: Option<MemoryId>,
    pub incoming: Option<MemoryReferenceTarget>,
    pub page_size: std::num::NonZeroUsize,
    pub outgoing_cursor: Option<UserMemoryReferencesCursor>,
    pub incoming_cursor: Option<UserMemoryReferencesCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UserMemoryReferenceEdge {
    pub source: MemoryId,
    pub source_scope: UserMemoryScopeId,
    pub source_revision: u64,
    pub reference_generation: u64,
    pub source_snapshot: UserMemoryGeneration,
    pub reference: MemoryReference,
    pub provenance: MemoryReferenceProvenance,
    pub ordinal: usize,
    pub source_observation: MemoryReferenceResolution,
    pub target_resolution: MemoryReferenceResolution,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UserMemoryReferencePage {
    pub results: Vec<UserMemoryReferenceEdge>,
    pub total: usize,
    pub complete: bool,
    pub next_cursor: Option<UserMemoryReferencesCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UserMemoryReferencesCoverage {
    pub owner: UserMemoryOwner,
    pub memory_user_source_complete: bool,
    pub inbound_other_sources_not_qualified: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryReferencesRead {
    NoCorpus,
    Published {
        generation: UserMemoryGeneration,
        outgoing: Option<Box<UserMemoryReferencePage>>,
        incoming: Option<Box<UserMemoryReferencePage>>,
        coverage: Box<UserMemoryReferencesCoverage>,
        warnings: Vec<MemoryReferencesWarning>,
    },
}
