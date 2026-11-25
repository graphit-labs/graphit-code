//! Observe the physical JSON FTS policy of the original captured dataset.
use super::{BranchError, BranchSnapshot};
use arrow_schema::DataType;
use lance::index::scalar::{
    IndexDetails, load_segment_details, load_segment_params, load_segments,
};
use lance_index::scalar::{InvertedIndexParams, inverted::DocumentGranularity};
use lance_table::io::manifest::read_manifest_indexes;
use std::sync::Arc;

/// Availability of this adapter's JSON full-text policy in one captured pin.
/// This observation grants no authority and does not certify Memory coverage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JsonFtsIndexState {
    /// No full-text index is declared for the selected column.
    Missing,
    /// A declared index is ambiguous, unusable, or has a different policy.
    Incompatible,
    /// The SDK selects physical segments with the expected JSON policy.
    Ready,
}

impl BranchSnapshot {
    /// Inspect complete manifest metadata and physical tokenizer parameters.
    ///
    /// No branch, index, manifest or row is created or changed. SDK failures
    /// remain errors; a missing index must not authorize textual flat fallback
    /// for a JSON path query. The caller retains authorization and cancellation.
    pub async fn json_fts_index_state(
        &self,
        column: &str,
    ) -> Result<JsonFtsIndexState, BranchError> {
        if !self
            .schema
            .field_with_name(column)
            .is_ok_and(|field| field.data_type() == &DataType::Utf8)
        {
            return Err(BranchError::JsonIndexColumn(column.to_owned()));
        }
        let field_id = self.dataset.schema().field_id(column)?;
        let store = self.dataset.object_store(None).await?;
        let metadata = read_manifest_indexes(
            store.as_ref(),
            self.dataset.manifest_location(),
            self.dataset.manifest(),
        )
        .await?;
        let mut candidates = Vec::new();
        for index in metadata {
            if !index.fields.contains(&field_id) {
                continue;
            }
            index.validate_covering_fields()?;
            let Some(details) = &index.index_details else {
                return Ok(JsonFtsIndexState::Incompatible);
            };
            if !IndexDetails(Arc::clone(details)).supports_fts() {
                continue;
            }
            if index.keyed_fields() != [field_id] || index.fields != [field_id] {
                return Ok(JsonFtsIndexState::Incompatible);
            }
            candidates.push(index);
        }
        let Some(first) = candidates.first() else {
            return Ok(JsonFtsIndexState::Missing);
        };
        // MatchQuery chooses a column, not an index name. Multiple segments of
        // one logical index are valid; competing logical indexes are ambiguous.
        if candidates.iter().any(|index| index.name != first.name) {
            return Ok(JsonFtsIndexState::Incompatible);
        }
        let details = load_segment_details(&self.dataset, column, &candidates).await?;
        if InvertedIndexParams::try_from(&details)?.get_document_granularity()
            != DocumentGranularity::Row
        {
            return Ok(JsonFtsIndexState::Incompatible);
        }
        let Some(selected) = load_segments(&self.dataset, column, DocumentGranularity::Row).await?
        else {
            return Ok(JsonFtsIndexState::Incompatible);
        };
        if selected.is_empty()
            || selected.iter().any(|index| {
                !candidates.iter().any(|known| {
                    known.uuid == index.uuid
                        && known.fields == index.fields
                        && known.name == index.name
                })
            })
        {
            return Ok(JsonFtsIndexState::Incompatible);
        }
        load_segment_details(&self.dataset, column, &selected).await?;
        let expected = InvertedIndexParams::default()
            .lance_tokenizer("json".into())
            .base_tokenizer("simple".into())
            .max_token_length(None)
            .lower_case(true)
            .stem(false)
            .remove_stop_words(false)
            .ascii_folding(false);
        for segment in selected {
            let physical = load_segment_params(&self.dataset, &segment).await?;
            // Training resolves the posting format; it does not change the
            // JSON analyzer policy. Compare through the SDK's public setter.
            let format = physical.resolved_format_version();
            if physical.format_version(format) != expected.clone().format_version(format) {
                return Ok(JsonFtsIndexState::Incompatible);
            }
        }
        Ok(JsonFtsIndexState::Ready)
    }
}
