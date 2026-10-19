use super::{State, ToolResult, error, parse};
use graphit_auth::ScopeInput;
use graphit_hub::{native::RoleCredentialReference, policy::RequestContext};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    path: String,
    #[serde(default)]
    selectors: graphit_core::text_slice::Request,
}
pub(super) async fn read<'a>(
    state: &State,
    scope: ScopeInput<'a>,
    bearer: &str,
    reference: &RoleCredentialReference,
    input: Option<Value>,
    original: &'a RequestContext<'a>,
) -> ToolResult {
    let v: Input = parse(input)?;
    let policy = &state.policy.source;
    let read = graphit_ast::source_handler::inspect_source_role(
        state.auth.issuer(),
        bearer,
        reference,
        scope,
        Path::new(&v.path),
        &v.selectors,
        &policy.file,
        policy.read,
        policy.path,
        policy.index,
        original,
    )
    .await
    .map_err(|e| match e {
        graphit_ast::source_handler::SourceHandlerError::Authentication(_) => {
            error("authentication_or_permission_denied")
        }
        graphit_ast::source_handler::SourceHandlerError::MissingProject => {
            error("project_required")
        }
        graphit_ast::source_handler::SourceHandlerError::InvalidPath => error("invalid_path"),
        graphit_ast::source_handler::SourceHandlerError::Operation(e) => {
            json!({"code":"source_operation_failed","io_kind":format!("{:?}",e.kind())})
        }
        graphit_ast::source_handler::SourceHandlerError::Index(index) => {
            use graphit_ast::source_index_locator::SourceIndexError;
            let category = match &index {
                SourceIndexError::InvalidScope => "invalid_scope",
                SourceIndexError::RequestMismatch => "request_mismatch",
                SourceIndexError::InvalidLocator => "invalid_locator",
                SourceIndexError::UnexpectedEntry => "unexpected_entry",
                SourceIndexError::Operation(_) => "operation",
                SourceIndexError::Storage(_) => "storage",
                SourceIndexError::Source(_) => "source",
            };
            let mut value = json!({"code":"source_index_failed","category":category});
            if let SourceIndexError::Storage(storage) = &index {
                use graphit_data::lance_branch_store::{BranchError, LanceError};
                let kind = match storage.as_ref() {
                    BranchError::SchemaMismatch => "schema_mismatch",
                    BranchError::Sdk(error) => match error.as_ref() {
                        LanceError::DatasetNotFound { .. } => "sdk_dataset_not_found",
                        LanceError::VersionNotFound { .. } => "sdk_version_not_found",
                        LanceError::SchemaMismatch { .. } => "sdk_schema_mismatch",
                        LanceError::Schema { .. } => "sdk_schema",
                        LanceError::IO { .. } if error.is_not_found() => "sdk_io_not_found",
                        LanceError::IO { .. } => "sdk_io",
                        LanceError::CorruptFile { .. } => "sdk_corrupt_file",
                        LanceError::InvalidInput { .. } => "sdk_invalid_input",
                        LanceError::NotFound { .. } => "sdk_not_found",
                        LanceError::RefNotFound { .. } => "sdk_ref_not_found",
                        _ => "sdk_other",
                    },
                    BranchError::UnsupportedCommitScheme(_) => "unsupported_commit_scheme",
                    BranchError::InvalidVersion => "invalid_version",
                    _ => "branch_state",
                };
                value["storage_kind"] = json!(kind);
            }

            let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&index);
            while let Some(error) = cause {
                if let Some(io) = error.downcast_ref::<std::io::Error>() {
                    value["io_kind"] = json!(format!("{:?}", io.kind()));
                    break;
                }
                cause = error.source();
            }
            value
        }
        graphit_ast::source_handler::SourceHandlerError::Indexed(_) => {
            error("indexed_source_failed")
        }
        graphit_ast::source_handler::SourceHandlerError::Inspection(_) => {
            error("source_inspection_failed")
        }
    })?;
    // The existing composer owns same-request checks and typed source selection.
    let value =
        serde_json::to_value(read.selection).map_err(|_| error("response_encoding_failed"))?;
    Ok(json!({"kind":"source","result":value}))
}
