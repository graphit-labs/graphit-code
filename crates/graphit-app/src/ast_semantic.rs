//! NativeNN metadata borrowed from a validated ModelService expectation.

use graphit_ast::native_nn::{NativeNnError, NativeNnModel};
use graphit_daemon::model_service::ExpectedModel;

/// Observe one validated embedding identity, including an empty corpus.
///
/// Schema/codec validation enforces NativeNN representation bounds. The caller
/// still admits the actual operation and validates its lease/config/Original.
pub fn expected_native_nn_model(
    expected: &ExpectedModel,
) -> Result<NativeNnModel<'_>, NativeNnError> {
    Ok(NativeNnModel {
        vector_id: expected.vector_id(),
        dimensions: expected.dimensions().ok_or(NativeNnError::InvalidModel)?,
        model_id: expected.model(),
        revision: expected.revision(),
    })
}

/// Observe a resolved portable vector space without fabricating a local model.
pub fn native_nn_model(
    metadata: &crate::embedding_service::EmbeddingMetadata,
) -> Result<NativeNnModel<'_>, NativeNnError> {
    if metadata.dimensions == 0 || metadata.model.is_empty() || metadata.revision.is_empty() {
        return Err(NativeNnError::InvalidModel);
    }
    Ok(NativeNnModel {
        vector_id: metadata.vector_id,
        dimensions: metadata.dimensions,
        model_id: &metadata.model,
        revision: &metadata.revision,
    })
}
