use super::MemoryStoreError;
use crate::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    reference_projection::ReferenceTargetKind,
};
use graphit_core::project_identity::ProjectId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, num::NonZeroU128, str::FromStr};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MemoryId(NonZeroU128);
impl MemoryId {
    pub const fn new(value: NonZeroU128) -> Self {
        Self(value)
    }
    pub const fn get(self) -> NonZeroU128 {
        self.0
    }
}
impl fmt::Display for MemoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        ulid::Ulid::from(self.0.get()).fmt(f)
    }
}
impl FromStr for MemoryId {
    type Err = MemoryStoreError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let parsed =
            ulid::Ulid::from_string(text).map_err(|_| MemoryStoreError::Invalid("Memory ID"))?;
        if parsed.to_string() != text {
            return Err(MemoryStoreError::Invalid("noncanonical Memory ID"));
        }
        NonZeroU128::new(parsed.into())
            .map(Self)
            .ok_or(MemoryStoreError::Invalid("zero Memory ID"))
    }
}
impl Serialize for MemoryId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for MemoryId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MemoryReferenceScope {
    Project(ProjectId),
    User(String),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReference {
    #[serde(with = "kind_codec")]
    pub target_kind: ReferenceTargetKind,
    pub scope: MemoryReferenceScope,
    pub id: String,
    pub context: Option<String>,
    pub relation: String,
    pub field: Option<String>,
}
/// Owned authored library input. Host JSON omission/null/shape parsing is separate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreateMemory {
    pub idempotency_key: String,
    pub title: String,
    pub body: String,
    #[serde(with = "attributes_codec")]
    pub attributes: MemoryAttributes,
    pub tags: Vec<String>,
    pub references: Option<Vec<MemoryReference>>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub id: MemoryId,
    pub revision: u64,
    pub title: String,
    pub body: String,
    #[serde(with = "attributes_codec")]
    pub attributes: MemoryAttributes,
    pub tags: Vec<String>,
    pub references: Vec<MemoryReference>,
    pub reference_generation: u64,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: u64,
    pub updated_at: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreationMemory {
    pub project_id: ProjectId,
    pub operation_version: String,
    pub actor: String,
    pub input: CreateMemory,
    pub memory: Memory,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryDetail {
    pub memory: Memory,
    pub creation: CreationMemory,
    pub generation: MemoryGeneration,
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "The creation contract returns the owned initial Memory directly; boxing would add an allocation and alter that public result."
)]
pub enum MemoryCreateObservation {
    Found(Memory),
    Unresolved,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryActivityHead {
    pub id: MemoryId,
    pub title: String,
    pub updated_by: String,
    pub updated_at: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryGeneration {
    pub sdk_version: u64,
    pub project_id: ProjectId,
    pub project_version: String,
    pub physical_branch: Option<String>,
    pub inherited_target: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryHeadRead {
    NoCorpus,
    Published {
        generation: MemoryGeneration,
        heads: Vec<MemoryActivityHead>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UpdateMemory {
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub reason: String,
    pub title: Option<String>,
    pub body: Option<String>,
    #[serde(with = "optional_class_codec")]
    pub classification: Option<MemoryClass>,
    pub mandatory: Option<bool>,
    pub important: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub references: Option<Vec<MemoryReference>>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeleteMemory {
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub reason: String,
}
/// A direct importance transition, with its own durable operation identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChangeMemoryImportance {
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub reason: String,
}
/// An observed member and its own explicit attempt key; none of these fields is authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryFoldMember {
    pub id: MemoryId,
    pub expected_revision: u64,
    pub idempotency_key: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MemoryFoldKind {
    Merge,
    Conflict,
}
/// Authored content and selected revisions. Removal members exclude the survivor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FoldMemories {
    pub kind: MemoryFoldKind,
    pub survivor: MemoryFoldMember,
    pub members: Vec<MemoryFoldMember>,
    pub title: Option<String>,
    pub body: String,
    pub reason: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomaticMemoryEffect {
    pub memory_id: MemoryId,
    pub before_revision: u64,
    pub after_revision: u64,
    pub idempotency_key: String,
    /// Present only when this call actually published an SDK manifest.
    pub sdk_version: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomaticDeleteRefusal {
    Protected,
    LastRecord,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AutomaticDeleteOutcome {
    Deleted(AutomaticMemoryEffect),
    Refused(AutomaticDeleteRefusal),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryFoldReceipt {
    pub survivor: AutomaticMemoryEffect,
    pub removed: Vec<AutomaticMemoryEffect>,
}
/// Persisted facts are supplied only by the admitted automatic application.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryFoldSurvivor {
    pub(super) input: FoldMemories,
    pub(super) observations: Vec<FoldAttributeObservation>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct FoldAttributeObservation {
    pub id: MemoryId,
    pub revision: u64,
    #[serde(with = "attributes_codec")]
    pub attributes: MemoryAttributes,
}
/// The deletion's domain-event anchor is provenance, not a grant or dispatch right.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryFoldDelete {
    pub(super) member: MemoryFoldMember,
    pub(super) survivor: MemoryId,
    pub(super) survivor_key: String,
    pub(super) survivor_revision: u64,
    pub(super) reason: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum MemoryMutation {
    Update(UpdateMemory),
    Delete(DeleteMemory),
    Promote(ChangeMemoryImportance),
    Demote(ChangeMemoryImportance),
    AutomaticDelete(DeleteMemory),
    FoldSurvivor(MemoryFoldSurvivor),
    FoldDelete(MemoryFoldDelete),
}
impl MemoryMutation {
    pub(crate) fn key(&self) -> &str {
        match self {
            Self::Update(v) => &v.idempotency_key,
            Self::Delete(v) => &v.idempotency_key,
            Self::Promote(v) | Self::Demote(v) => &v.idempotency_key,
            Self::AutomaticDelete(v) => &v.idempotency_key,
            Self::FoldSurvivor(v) => &v.input.survivor.idempotency_key,
            Self::FoldDelete(v) => &v.member.idempotency_key,
        }
    }
    pub(crate) fn reason(&self) -> &str {
        match self {
            Self::Update(v) => &v.reason,
            Self::Delete(v) => &v.reason,
            Self::Promote(v) | Self::Demote(v) => &v.reason,
            Self::AutomaticDelete(v) => &v.reason,
            Self::FoldSurvivor(v) => &v.input.reason,
            Self::FoldDelete(v) => &v.reason,
        }
    }
    pub(crate) fn expected(&self) -> u64 {
        match self {
            Self::Update(v) => v.expected_revision,
            Self::Delete(v) => v.expected_revision,
            Self::Promote(v) | Self::Demote(v) => v.expected_revision,
            Self::AutomaticDelete(v) => v.expected_revision,
            Self::FoldSurvivor(v) => v.input.survivor.expected_revision,
            Self::FoldDelete(v) => v.member.expected_revision,
        }
    }
    pub(crate) fn deleted(&self) -> bool {
        matches!(
            self,
            Self::Delete(_) | Self::AutomaticDelete(_) | Self::FoldDelete(_)
        )
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryRevision {
    pub project_id: ProjectId,
    pub operation_version: String,
    pub actor: String,
    pub operation: MemoryMutation,
    pub before: Memory,
    pub after: Memory,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryHistory {
    pub creation: CreationMemory,
    pub current: Memory,
    pub deleted: bool,
    pub revisions: Vec<MemoryRevision>,
    pub generation: MemoryGeneration,
}

/// All supplied predicates are combined with AND; tag matching is literal.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryListFilter {
    pub classification: Option<MemoryClass>,
    pub tag: Option<String>,
    pub mandatory: Option<bool>,
    pub important: Option<bool>,
    pub exclude_mandatory: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryListRead {
    NoCorpus,
    Published {
        generation: MemoryGeneration,
        memories: Vec<Memory>,
    },
}

/// Literal lexical input; filters apply to the current head, including archive hits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryLexicalQuery {
    pub text: String,
    pub filter: MemoryListFilter,
    pub include_deleted_history: bool,
    pub top_k: std::num::NonZeroUsize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryMatchLocation {
    Current,
    Archived { revision: std::num::NonZeroU64 },
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MemoryLexicalHit {
    pub current: Memory,
    pub matched: Memory,
    pub location: MemoryMatchLocation,
    pub deleted: bool,
    pub score: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MemoryLexicalSearchRead {
    NoCorpus,
    PublishedEmpty {
        generation: MemoryGeneration,
    },
    Published {
        generation: MemoryGeneration,
        total_hits: usize,
        hits: Vec<MemoryLexicalHit>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredInput {
    idempotency_key: String,
    title: String,
    body: String,
    #[serde(with = "attributes_codec")]
    attributes: MemoryAttributes,
    tags: Vec<String>,
    references: Option<Vec<MemoryReference>>,
}
impl From<CreateMemory> for StoredInput {
    fn from(v: CreateMemory) -> Self {
        Self {
            idempotency_key: v.idempotency_key,
            title: v.title,
            body: v.body,
            attributes: v.attributes,
            tags: v.tags,
            references: v.references,
        }
    }
}
impl From<StoredInput> for CreateMemory {
    fn from(v: StoredInput) -> Self {
        Self {
            idempotency_key: v.idempotency_key,
            title: v.title,
            body: v.body,
            attributes: v.attributes,
            tags: v.tags,
            references: v.references,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredCreation {
    pub project_id: ProjectId,
    pub operation_version: String,
    pub actor: String,
    pub input: StoredInput,
    pub memory: Memory,
}
impl From<CreationMemory> for StoredCreation {
    fn from(v: CreationMemory) -> Self {
        Self {
            project_id: v.project_id,
            operation_version: v.operation_version,
            actor: v.actor,
            input: v.input.into(),
            memory: v.memory,
        }
    }
}
impl From<StoredCreation> for CreationMemory {
    fn from(v: StoredCreation) -> Self {
        Self {
            project_id: v.project_id,
            operation_version: v.operation_version,
            actor: v.actor,
            input: v.input.into(),
            memory: v.memory,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredUpdate {
    expected_revision: u64,
    idempotency_key: String,
    reason: String,
    title: Option<String>,
    body: Option<String>,
    #[serde(with = "optional_class_codec")]
    classification: Option<MemoryClass>,
    mandatory: Option<bool>,
    important: Option<bool>,
    tags: Option<Vec<String>>,
    references: Option<Vec<MemoryReference>>,
}
impl From<UpdateMemory> for StoredUpdate {
    fn from(v: UpdateMemory) -> Self {
        Self {
            expected_revision: v.expected_revision,
            idempotency_key: v.idempotency_key,
            reason: v.reason,
            title: v.title,
            body: v.body,
            classification: v.classification,
            mandatory: v.mandatory,
            important: v.important,
            tags: v.tags,
            references: v.references,
        }
    }
}
impl From<StoredUpdate> for UpdateMemory {
    fn from(v: StoredUpdate) -> Self {
        Self {
            expected_revision: v.expected_revision,
            idempotency_key: v.idempotency_key,
            reason: v.reason,
            title: v.title,
            body: v.body,
            classification: v.classification,
            mandatory: v.mandatory,
            important: v.important,
            tags: v.tags,
            references: v.references,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredDelete {
    expected_revision: u64,
    idempotency_key: String,
    reason: String,
}
impl From<DeleteMemory> for StoredDelete {
    fn from(v: DeleteMemory) -> Self {
        Self {
            expected_revision: v.expected_revision,
            idempotency_key: v.idempotency_key,
            reason: v.reason,
        }
    }
}
impl From<StoredDelete> for DeleteMemory {
    fn from(v: StoredDelete) -> Self {
        Self {
            expected_revision: v.expected_revision,
            idempotency_key: v.idempotency_key,
            reason: v.reason,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredFoldMember {
    id: MemoryId,
    expected_revision: u64,
    idempotency_key: String,
}
impl From<MemoryFoldMember> for StoredFoldMember {
    fn from(v: MemoryFoldMember) -> Self {
        Self {
            id: v.id,
            expected_revision: v.expected_revision,
            idempotency_key: v.idempotency_key,
        }
    }
}
impl From<StoredFoldMember> for MemoryFoldMember {
    fn from(v: StoredFoldMember) -> Self {
        Self {
            id: v.id,
            expected_revision: v.expected_revision,
            idempotency_key: v.idempotency_key,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredFoldInput {
    kind: MemoryFoldKind,
    survivor: StoredFoldMember,
    members: Vec<StoredFoldMember>,
    title: Option<String>,
    body: String,
    reason: String,
}
impl From<FoldMemories> for StoredFoldInput {
    fn from(v: FoldMemories) -> Self {
        Self {
            kind: v.kind,
            survivor: v.survivor.into(),
            members: v.members.into_iter().map(Into::into).collect(),
            title: v.title,
            body: v.body,
            reason: v.reason,
        }
    }
}
impl From<StoredFoldInput> for FoldMemories {
    fn from(v: StoredFoldInput) -> Self {
        Self {
            kind: v.kind,
            survivor: v.survivor.into(),
            members: v.members.into_iter().map(Into::into).collect(),
            title: v.title,
            body: v.body,
            reason: v.reason,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredFoldObservation {
    id: MemoryId,
    revision: u64,
    #[serde(with = "attributes_codec")]
    attributes: MemoryAttributes,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredFoldSurvivor {
    input: StoredFoldInput,
    observations: Vec<StoredFoldObservation>,
}
impl From<MemoryFoldSurvivor> for StoredFoldSurvivor {
    fn from(v: MemoryFoldSurvivor) -> Self {
        Self {
            input: v.input.into(),
            observations: v
                .observations
                .into_iter()
                .map(|o| StoredFoldObservation {
                    id: o.id,
                    revision: o.revision,
                    attributes: o.attributes,
                })
                .collect(),
        }
    }
}
impl From<StoredFoldSurvivor> for MemoryFoldSurvivor {
    fn from(v: StoredFoldSurvivor) -> Self {
        Self {
            input: v.input.into(),
            observations: v
                .observations
                .into_iter()
                .map(|o| FoldAttributeObservation {
                    id: o.id,
                    revision: o.revision,
                    attributes: o.attributes,
                })
                .collect(),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredFoldDelete {
    member: StoredFoldMember,
    survivor: MemoryId,
    survivor_key: String,
    survivor_revision: u64,
    reason: String,
}
impl From<MemoryFoldDelete> for StoredFoldDelete {
    fn from(v: MemoryFoldDelete) -> Self {
        Self {
            member: v.member.into(),
            survivor: v.survivor,
            survivor_key: v.survivor_key,
            survivor_revision: v.survivor_revision,
            reason: v.reason,
        }
    }
}
impl From<StoredFoldDelete> for MemoryFoldDelete {
    fn from(v: StoredFoldDelete) -> Self {
        Self {
            member: v.member.into(),
            survivor: v.survivor,
            survivor_key: v.survivor_key,
            survivor_revision: v.survivor_revision,
            reason: v.reason,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredImportance {
    expected_revision: u64,
    idempotency_key: String,
    reason: String,
}
impl From<ChangeMemoryImportance> for StoredImportance {
    fn from(v: ChangeMemoryImportance) -> Self {
        Self {
            expected_revision: v.expected_revision,
            idempotency_key: v.idempotency_key,
            reason: v.reason,
        }
    }
}
impl From<StoredImportance> for ChangeMemoryImportance {
    fn from(v: StoredImportance) -> Self {
        Self {
            expected_revision: v.expected_revision,
            idempotency_key: v.idempotency_key,
            reason: v.reason,
        }
    }
}
#[derive(Serialize, Deserialize)]
pub(crate) enum StoredMutation {
    Update(StoredUpdate),
    Delete(StoredDelete),
    Promote(StoredImportance),
    Demote(StoredImportance),
    AutomaticDelete(StoredDelete),
    FoldSurvivor(StoredFoldSurvivor),
    FoldDelete(StoredFoldDelete),
}
impl From<MemoryMutation> for StoredMutation {
    fn from(v: MemoryMutation) -> Self {
        match v {
            MemoryMutation::Update(v) => Self::Update(v.into()),
            MemoryMutation::Promote(v) => Self::Promote(v.into()),
            MemoryMutation::Demote(v) => Self::Demote(v.into()),
            MemoryMutation::AutomaticDelete(v) => Self::AutomaticDelete(v.into()),
            MemoryMutation::FoldSurvivor(v) => Self::FoldSurvivor(v.into()),
            MemoryMutation::FoldDelete(v) => Self::FoldDelete(v.into()),
            MemoryMutation::Delete(v) => Self::Delete(StoredDelete {
                expected_revision: v.expected_revision,
                idempotency_key: v.idempotency_key,
                reason: v.reason,
            }),
        }
    }
}
impl From<StoredMutation> for MemoryMutation {
    fn from(v: StoredMutation) -> Self {
        match v {
            StoredMutation::Update(v) => Self::Update(v.into()),
            StoredMutation::Promote(v) => Self::Promote(v.into()),
            StoredMutation::Demote(v) => Self::Demote(v.into()),
            StoredMutation::AutomaticDelete(v) => Self::AutomaticDelete(v.into()),
            StoredMutation::FoldSurvivor(v) => Self::FoldSurvivor(v.into()),
            StoredMutation::FoldDelete(v) => Self::FoldDelete(v.into()),
            StoredMutation::Delete(v) => Self::Delete(DeleteMemory {
                expected_revision: v.expected_revision,
                idempotency_key: v.idempotency_key,
                reason: v.reason,
            }),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredRevision {
    project_id: ProjectId,
    operation_version: String,
    actor: String,
    operation: StoredMutation,
    before: Memory,
    after: Memory,
}
impl From<MemoryRevision> for StoredRevision {
    fn from(v: MemoryRevision) -> Self {
        Self {
            project_id: v.project_id,
            operation_version: v.operation_version,
            actor: v.actor,
            operation: v.operation.into(),
            before: v.before,
            after: v.after,
        }
    }
}
impl From<StoredRevision> for MemoryRevision {
    fn from(v: StoredRevision) -> Self {
        Self {
            project_id: v.project_id,
            operation_version: v.operation_version,
            actor: v.actor,
            operation: v.operation.into(),
            before: v.before,
            after: v.after,
        }
    }
}
mod optional_class_codec {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Class {
        Fact,
        Skill,
        Tension,
        Decision,
        Convention,
        Correction,
    }
    pub fn serialize<S: Serializer>(v: &Option<MemoryClass>, s: S) -> Result<S::Ok, S::Error> {
        v.map(|v| match v {
            MemoryClass::Fact => Class::Fact,
            MemoryClass::Skill => Class::Skill,
            MemoryClass::Tension => Class::Tension,
            MemoryClass::Decision => Class::Decision,
            MemoryClass::Convention => Class::Convention,
            MemoryClass::Correction => Class::Correction,
        })
        .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<MemoryClass>, D::Error> {
        Ok(Option::<Class>::deserialize(d)?.map(|v| match v {
            Class::Fact => MemoryClass::Fact,
            Class::Skill => MemoryClass::Skill,
            Class::Tension => MemoryClass::Tension,
            Class::Decision => MemoryClass::Decision,
            Class::Convention => MemoryClass::Convention,
            Class::Correction => MemoryClass::Correction,
        }))
    }
}
mod attributes_codec {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
        classification: Class,
        mandatory: bool,
        important: bool,
    }
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Class {
        Fact,
        Skill,
        Tension,
        Decision,
        Convention,
        Correction,
    }
    pub fn serialize<S: Serializer>(v: &MemoryAttributes, s: S) -> Result<S::Ok, S::Error> {
        Wire {
            classification: match v.classification {
                MemoryClass::Fact => Class::Fact,
                MemoryClass::Skill => Class::Skill,
                MemoryClass::Tension => Class::Tension,
                MemoryClass::Decision => Class::Decision,
                MemoryClass::Convention => Class::Convention,
                MemoryClass::Correction => Class::Correction,
            },
            mandatory: v.mandatory,
            important: v.important,
        }
        .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<MemoryAttributes, D::Error> {
        let v = Wire::deserialize(d)?;
        Ok(MemoryAttributes {
            classification: match v.classification {
                Class::Fact => MemoryClass::Fact,
                Class::Skill => MemoryClass::Skill,
                Class::Tension => MemoryClass::Tension,
                Class::Decision => MemoryClass::Decision,
                Class::Convention => MemoryClass::Convention,
                Class::Correction => MemoryClass::Correction,
            },
            mandatory: v.mandatory,
            important: v.important,
        })
    }
}
pub(super) mod kind_codec {
    use super::*;
    pub fn serialize<S: Serializer>(v: &ReferenceTargetKind, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(kind(*v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ReferenceTargetKind, D::Error> {
        match String::deserialize(d)?.as_str() {
            "task" => Ok(ReferenceTargetKind::Task),
            "session" => Ok(ReferenceTargetKind::Session),
            "memory" => Ok(ReferenceTargetKind::Memory),
            "knowledge" => Ok(ReferenceTargetKind::Knowledge),
            _ => Err(serde::de::Error::custom("unknown reference kind")),
        }
    }
}
pub(crate) fn kind(v: ReferenceTargetKind) -> &'static str {
    match v {
        ReferenceTargetKind::Task => "task",
        ReferenceTargetKind::Session => "session",
        ReferenceTargetKind::Memory => "memory",
        ReferenceTargetKind::Knowledge => "knowledge",
    }
}

/// Explicit derived-index maintenance under ordinary write authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryLexicalIndexOperation {
    Ensure,
    Refresh,
}
/// Ready observes an existing pin and carries no fabricated transaction key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryLexicalIndexOutcome {
    NoCorpus,
    Ready(MemoryGeneration),
    Published(MemoryLexicalIndexPublication),
}
/// SDK generation and transaction identity retained after confirmed publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryLexicalIndexPublication {
    pub generation: MemoryGeneration,
    pub recovery_key: graphit_data::lance_branch_store::DeltaRecoveryKey,
}

#[cfg(test)]
mod browser_serialization {
    use super::*;
    fn memory() -> Memory {
        Memory {
            id: MemoryId::new(NonZeroU128::new(1).unwrap()),
            revision: 1,
            title: "Initial".into(),
            body: "Native body".into(),
            attributes: MemoryAttributes {
                classification: MemoryClass::Fact,
                mandatory: false,
                important: true,
            },
            tags: vec!["literal".into()],
            references: vec![],
            reference_generation: 1,
            created_by: "actor".into(),
            updated_by: "actor".into(),
            created_at: 1,
            updated_at: 1,
        }
    }
    #[test]
    fn creation_output_is_the_complete_canonical_persisted_shape() {
        let initial = memory();
        let creation = CreationMemory {
            project_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            operation_version: "main".into(),
            actor: "actor".into(),
            input: CreateMemory {
                idempotency_key: "create".into(),
                title: initial.title.clone(),
                body: initial.body.clone(),
                attributes: initial.attributes,
                tags: initial.tags.clone(),
                references: None,
            },
            memory: initial,
        };
        assert_eq!(
            serde_json::to_value(&creation).unwrap(),
            serde_json::to_value(StoredCreation::from(creation)).unwrap()
        );
    }
    #[test]
    fn all_seven_history_operations_preserve_private_fold_data_and_native_codecs() {
        let initial = memory();
        let delete = DeleteMemory {
            expected_revision: 1,
            idempotency_key: "delete".into(),
            reason: "Selected".into(),
        };
        let importance = ChangeMemoryImportance {
            expected_revision: 1,
            idempotency_key: "importance".into(),
            reason: "Selected".into(),
        };
        let member = MemoryFoldMember {
            id: initial.id,
            expected_revision: 1,
            idempotency_key: "survivor".into(),
        };
        let fold = MemoryFoldSurvivor {
            input: FoldMemories {
                kind: MemoryFoldKind::Merge,
                survivor: member.clone(),
                members: vec![],
                title: None,
                body: "Merged".into(),
                reason: "Observed".into(),
            },
            observations: vec![FoldAttributeObservation {
                id: initial.id,
                revision: 1,
                attributes: initial.attributes,
            }],
        };
        let operations = [
            MemoryMutation::Update(UpdateMemory {
                expected_revision: 1,
                idempotency_key: "update".into(),
                reason: "Selected".into(),
                title: None,
                body: None,
                classification: Some(MemoryClass::Correction),
                mandatory: None,
                important: None,
                tags: None,
                references: None,
            }),
            MemoryMutation::Delete(delete.clone()),
            MemoryMutation::Promote(importance.clone()),
            MemoryMutation::Demote(importance),
            MemoryMutation::AutomaticDelete(delete),
            MemoryMutation::FoldSurvivor(fold),
            MemoryMutation::FoldDelete(MemoryFoldDelete {
                member,
                survivor: initial.id,
                survivor_key: "anchor".into(),
                survivor_revision: 2,
                reason: "Observed".into(),
            }),
        ];
        for operation in operations {
            assert_eq!(
                serde_json::to_value(&operation).unwrap(),
                serde_json::to_value(StoredMutation::from(operation)).unwrap()
            );
        }
    }
}
