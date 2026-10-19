//! Current Session state and explicitly selected, generation-bound detail pages.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::num::NonZeroUsize;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionGetInput {
    #[schemars(with = "super::query_model::QueryIdentifierSchema")]
    pub id: SessionId,
    pub history: Option<SessionDetailPageInput>,
}
impl SessionGetInput {
    pub fn current(id: SessionId) -> Self {
        Self { id, history: None }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionDetailPageInput {
    pub page_size: NonZeroUsize,
    pub cursor: Option<SessionDetailCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionDetailBinding {
    pub corpus_key: String,
    pub root: graphit_core::native_path::NativePath,
    pub project_id: graphit_core::project_identity::ProjectId,
    pub brand: String,
    pub project_version: String,
    pub generation: TaskGeneration,
    pub branch_provenance: Option<graphit_data::lance_branch_store::BranchProvenance>,
    pub session_id: SessionId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SessionDetailSection {
    History,
}
/// Continuation selects an admitted historical pin; it never supplies authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionDetailCursor {
    pub(super) binding: Box<SessionDetailBinding>,
    pub(super) section: SessionDetailSection,
    pub(super) after_revision: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorFields {
    binding: Box<SessionDetailBinding>,
    section: SessionDetailSection,
    after_revision: u64,
}
impl Serialize for SessionDetailCursor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let fields = CursorFields {
            binding: self.binding.clone(),
            section: self.section,
            after_revision: self.after_revision,
        };
        serializer.serialize_str(
            &URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&fields).map_err(serde::ser::Error::custom)?),
        )
    }
}
impl<'de> Deserialize<'de> for SessionDetailCursor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = URL_SAFE_NO_PAD.decode(&text).map_err(D::Error::custom)?;
        if URL_SAFE_NO_PAD.encode(&bytes) != text {
            return Err(D::Error::custom("noncanonical Session detail cursor"));
        }
        let fields: CursorFields = serde_json::from_slice(&bytes).map_err(D::Error::custom)?;
        Ok(Self {
            binding: fields.binding,
            section: fields.section,
            after_revision: fields.after_revision,
        })
    }
}
impl JsonSchema for SessionDetailCursor {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SessionDetailCursor".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({"type":"string","pattern":"^[A-Za-z0-9_-]+$","description":"Opaque Session/section/publication-bound continuation; grants no authority."})
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionHistoryPage {
    pub before: Session,
    pub events: Vec<LifecycleEvent<Session>>,
    pub next_cursor: Option<SessionDetailCursor>,
}
/// Null sections were not requested; a present empty page is a completed read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SelectedSessionRead {
    pub session: Session,
    pub creation: Creation<Session>,
    pub last_event: Option<LifecycleEvent<Session>>,
    pub generation: TaskGeneration,
    pub history_start_cursor: SessionDetailCursor,
    pub history: Option<SessionHistoryPage>,
}
