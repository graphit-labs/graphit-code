mod search;

use super::{State, ToolResult, catalog::Operation, error, parse};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, VerifiedRoleGrant};
use graphit_data::knowledge_index::{self, KnowledgeHeadRead, KnowledgeIndexPageRead};
use graphit_hub::policy::RequestContext;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    slug: String,
    #[serde(default)]
    selectors: graphit_core::text_slice::Request,
}
pub(super) async fn execute<'a>(
    state: &State,
    authority: &'a ProjectRecordAuthority<'_>,
    grant: &'a VerifiedRoleGrant<'a>,
    operation: Operation,
    input: Option<Value>,
    original: &'a RequestContext<'a>,
) -> ToolResult {
    let admission = authority
        .admit_read(grant, original, ProjectReadModule::Knowledge)
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let result = match operation {
        Operation::KnowledgeId => crate::knowledge_id::calculate(input, state.task.content)?,
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        Operation::KnowledgeLog => {
            let input: crate::knowledge_log::Input = parse(input)?;
            input.validate(state.task.content)?;
            let read = graphit_data::knowledge_sync_history::read_native_knowledge_sync_history(
                original.snapshot(),
                &admission,
                input.native(),
                state.policy.knowledge,
            )
            .await
            .map_err(crate::knowledge_log::failure)?;
            crate::knowledge_log::view(read)
        }
        Operation::KnowledgeSearch => {
            search::execute(state, grant, &admission, input, original).await?
        }
        Operation::KnowledgeList => {
            let _: Empty = parse(input)?;
            match knowledge_index::read_native_knowledge_heads(
                original.snapshot(),
                &admission,
                state.policy.knowledge_heads,
            )
            .await
            .map_err(failure)?
            {
                KnowledgeHeadRead::NoCorpus => json!({"kind":"no_corpus"}),
                KnowledgeHeadRead::Published { heads, .. } => {
                    json!({"kind":"published","heads":heads.into_iter().map(|h|json!({"slug":h.slug,"title":h.title,"updated_at":h.updated_at})).collect::<Vec<_>>()})
                }
            }
        }
        Operation::KnowledgeSchema => {
            let _: Empty = parse(input)?;
            // This is the producer's canonical physical format, not a scan of corpus rows.
            let schema = knowledge_index::physical_schema();
            json!({"kind":"physical_format","columns":schema.fields().iter().map(|f|json!({"name":f.name(),"data_type":f.data_type().to_string(),"nullable":f.is_nullable()})).collect::<Vec<_>>()})
        }
        Operation::KnowledgeSource => {
            let v: Source = parse(input)?;
            use crate::knowledge_source::KnowledgeSourceRead;
            let read = knowledge_index::read_native_knowledge_page(
                &admission,
                &v.slug,
                state.policy.knowledge,
            )
            .await
            .map_err(failure)?;
            let result = match read {
                KnowledgeIndexPageRead::NoCorpus => KnowledgeSourceRead::NoCorpus,
                KnowledgeIndexPageRead::Published { page, .. } => {
                    KnowledgeSourceRead::from_page(page, &v.selectors)
                        .map_err(|_| error("invalid_source_selection"))?
                }
            };
            serde_json::to_value(result).map_err(|_| error("response_encoding_failed"))?
        }

        _ => return Err(error("unknown_tool")),
    };
    admission
        .recheck()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    Ok(result)
}

pub(super) fn failure(error_value: knowledge_index::KnowledgeIndexError) -> Value {
    use knowledge_index::KnowledgeIndexError as E;
    match error_value {
        E::Auth(_) => error("authentication_or_permission_denied"),
        E::Request(e) => json!({"code":"request_failed", "io_kind":format!("{:?}",e.kind())}),
        E::DifferentRequest => error("different_original_request"),
        E::WrongAdmission => error("wrong_admission"),
        E::Store(_) => error("store_failure"),
        E::Branch(_) => error("branch_failure"),
        E::Arrow(_) => error("arrow_failure"),
        E::DuplicateSlug => error("duplicate_slug"),
        E::InvalidField(_) => error("invalid_native_field"),
        E::LimitExceeded(_) | E::Overflow(_) => error("input_or_result_limit"),
        _ => error("knowledge_corpus_failure"),
    }
}
