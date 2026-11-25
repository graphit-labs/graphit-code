//! Browser and native questions share selection, retrieval and process custody.
use graphit_hub::native::ContentLimits;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use tokio::sync::{mpsc, oneshot};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Retrieval {
    pub(crate) mode: super::search::Mode,
    pub(crate) top_k: NonZeroUsize,
    #[serde(
        default,
        deserialize_with = "crate::reference_explorer::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) metric: Option<super::search::Metric>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Input {
    pub(crate) query: String,
    #[serde(
        default,
        deserialize_with = "crate::reference_explorer::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) selected_slugs: Option<Vec<String>>,
    pub(crate) model: String,
    #[serde(
        default,
        deserialize_with = "crate::reference_explorer::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) retrieval: Option<Retrieval>,
}
impl Input {
    pub(crate) fn validate_selection(&self) -> Result<(), &'static str> {
        if self.selected_slugs.is_some() == self.retrieval.is_some() {
            return Err("invalid_arguments");
        }
        if self.retrieval.as_ref().is_some_and(|retrieval| {
            retrieval.mode == super::search::Mode::Fts && retrieval.metric.is_some()
        }) {
            return Err("invalid_arguments");
        }
        Ok(())
    }
    pub(crate) fn validate(&self, limits: ContentLimits) -> Result<(), &'static str> {
        self.validate_selection()?;
        if [&self.query, &self.model].into_iter().any(|value| {
            value.is_empty() || value.contains('\0') || value.len() > limits.max_scalar_bytes
        }) {
            return Err("invalid_arguments");
        }
        if let Some(slugs) = &self.selected_slugs {
            if slugs.len() > limits.max_collection_entries {
                return Err("invalid_arguments");
            }
            for slug in slugs {
                if slug.len() > limits.max_scalar_bytes {
                    return Err("invalid_arguments");
                }
                graphit_data::knowledge_index::validate_slug(slug)
                    .map_err(|_| "invalid_arguments")?;
            }
        }
        if self
            .retrieval
            .as_ref()
            .is_some_and(|value| value.top_k.get() > limits.max_collection_entries)
        {
            return Err("invalid_arguments");
        }
        Ok(())
    }
}
pub(crate) struct Transport {
    pub(crate) ready: oneshot::Sender<()>,
    pub(crate) progress: mpsc::Sender<axum::response::sse::Event>,
    pub(crate) session_live: Box<dyn Fn() -> bool + Send + Sync>,
}
