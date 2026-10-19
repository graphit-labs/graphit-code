//! Borrowed reader observations for navigation; every subsequent read needs admission.
use crate::{reference_explorer::SourceKey, task_server::browser::codec::AdmittedSelection};
use graphit_data::knowledge_reference_query::KnowledgeReferenceSource;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct ReferenceBinding<'a, T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selection: Option<&'a AdmittedSelection>,
    pub(crate) source: &'a SourceKey,
    pub(crate) focused_source: Option<&'a KnowledgeReferenceSource>,
    pub(crate) native: T,
}
