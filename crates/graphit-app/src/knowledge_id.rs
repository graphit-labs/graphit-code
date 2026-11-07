//! Pure Knowledge identity data shared by the authenticated MCP boundaries.
use graphit_data::knowledge_document_lifecycle::{
    KnowledgeSourceIdentity, KnowledgeSourceIdentityError,
};
use graphit_hub::native::ContentLimits;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{num::NonZeroUsize, path::Path};

pub(crate) const DESCRIPTION: &str = "Calculate the deterministic default Knowledge document id for a relative Unicode source path. Does not open the path, read a corpus or write a document; the returned id grants no access. An explicit frontmatter id takes precedence when indexing.";

pub(crate) fn schema() -> Value {
    json!({"type":"object","properties":{"path":{"type":"string","minLength":1,"description":"Relative Unicode source path, with native path separators, as supplied to the Knowledge publisher."}},"required":["path"],"additionalProperties":false})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    path: String,
}

pub(crate) fn calculate(input: Option<Value>, content: ContentLimits) -> Result<Value, Value> {
    let input = input.ok_or_else(|| json!({"code":"invalid_arguments"}))?;
    if serde_json::to_vec(&input).map_or(true, |bytes| bytes.len() > content.max_input_bytes) {
        return Err(json!({"code":"input_limit"}));
    }
    let input: Input =
        serde_json::from_value(input).map_err(|_| json!({"code":"invalid_arguments"}))?;
    let limit = NonZeroUsize::new(content.max_scalar_bytes)
        .ok_or_else(|| json!({"code":"invalid_identity_policy"}))?;
    let identity = KnowledgeSourceIdentity::from_relative_path(Path::new(&input.path), None, limit)
        .map_err(|cause| {
            let code = match cause {
                KnowledgeSourceIdentityError::InvalidRelativePath
                | KnowledgeSourceIdentityError::NonUnicodePath => "invalid_relative_path",
                KnowledgeSourceIdentityError::LimitExceeded(_)
                | KnowledgeSourceIdentityError::Overflow => "input_limit",
                _ => "identity_calculation_failed",
            };
            json!({"code":code})
        })?;
    Ok(json!({"id":identity.slug()}))
}
