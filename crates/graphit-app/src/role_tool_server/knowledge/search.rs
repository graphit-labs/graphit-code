use super::{State, ToolResult, error, parse};
use graphit_auth::{ProjectReadAdmission, VerifiedRoleGrant};
use graphit_core::artifact_store::StoreOwner;
use graphit_data::knowledge_search::{
    KnowledgeLexicalQuery, KnowledgeSearchCursor, KnowledgeSearchError, KnowledgeSearchLimits,
    KnowledgeSearchRead, KnowledgeSearchState, capture_native_knowledge_search,
};
use graphit_hub::policy::RequestContext;
use graphit_knowledge::preview::MarkdownPreviewLimits;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{error::Error, io, num::NonZeroUsize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    query: String,
    #[serde(default)]
    top_k: usize,
    #[serde(default = "page_size")]
    page_size: usize,
    #[serde(default)]
    preview: bool,
    #[serde(default, deserialize_with = "crate::reference_explorer::present")]
    rerank: Option<bool>,
    #[serde(default, deserialize_with = "crate::reference_explorer::present")]
    cursor: Option<KnowledgeSearchCursor>,
}
fn page_size() -> usize {
    20
}
fn positive(value: usize) -> Result<NonZeroUsize, Value> {
    NonZeroUsize::new(value).ok_or_else(|| error("invalid_search_policy"))
}

pub(super) async fn execute(
    state: &State,
    grant: &VerifiedRoleGrant<'_>,
    read: &ProjectReadAdmission<'_>,
    input: Option<Value>,
    original: &RequestContext<'_>,
) -> ToolResult {
    let input: Input = parse(input)?;
    let content = state.task.content;
    if input.query.trim().is_empty()
        || input.query.contains('\0')
        || input.query.len() > content.max_scalar_bytes
        || !(1..=100).contains(&input.page_size)
    {
        return Err(error("invalid_arguments"));
    }
    let limits = KnowledgeSearchLimits {
        read: state.policy.knowledge,
        max_query_bytes: positive(content.max_scalar_bytes)?,
        max_output_bytes: positive(content.max_output_bytes)?,
    };
    let preview = MarkdownPreviewLimits {
        max_input_bytes: positive(content.max_input_bytes)?,
        max_output_bytes: positive(content.max_output_bytes)?,
        max_body_scalars: NonZeroUsize::new(1000).expect("existing preview length is positive"),
    };
    let mut actor = [0; 64];
    grant
        .actor()
        .map_err(|_| error("authentication_or_permission_denied"))?
        .write_identifier(&mut actor);
    let actor = std::str::from_utf8(&actor).map_err(|_| error("invalid_role_actor"))?;
    let project = read
        .project()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let StoreOwner::Project { project_id } = project.scope.live.owner() else {
        return Err(error("project_required"));
    };
    let root =
        graphit_core::native_path::NativePath::new(project.scope.live.owner_root().to_path_buf())
            .map_err(|_| error("invalid_project_root"))?;
    let binding = serde_json::to_string(&(actor, project_id, root, project.version.version()))
        .map_err(|_| error("request_encoding_failed"))?;
    let capture = capture_native_knowledge_search(original.snapshot(), read, limits.read)
        .await
        .map_err(failure)?;
    let capture = match capture {
        KnowledgeSearchRead::NoCorpus if input.cursor.is_some() => {
            return Err(error("generation_changed"));
        }
        KnowledgeSearchRead::NoCorpus => return Ok(json!({"kind":"no_corpus"})),
        KnowledgeSearchRead::Published(capture) => capture,
    };
    let page = graphit_knowledge::search::query_native_knowledge_search(
        original.snapshot(),
        read,
        &capture,
        KnowledgeLexicalQuery {
            text: &input.query,
            top_k: input.top_k,
            page_size: NonZeroUsize::new(input.page_size).expect("validated page size"),
            cursor: input.cursor.as_ref(),
            caller_binding: &binding,
            preview: input.preview,
        },
        limits,
        preview,
    )
    .await
    .map_err(failure)?;
    let config = if input.rerank == Some(false) || page.hits.is_empty() {
        None
    } else {
        let paths = graphit_core::project_paths::ProjectPaths::at(
            project.scope.live.owner_root(),
            &state.policy.source.installation,
        )
        .map_err(|_| error("configuration_failure"))?;
        let config = crate::ast_profile_loader::project_config(
            &state.policy.source.installation,
            &paths,
            &state.policy.config,
            original.snapshot(),
        )
        .map_err(|_| error("configuration_failure"))?;
        let enabled = match input.rerank {
            Some(value) => value,
            None => crate::search_rerank::configured(&config)
                .map_err(|_| error("configuration_failure"))?,
        };
        enabled.then_some(config)
    };
    let ranks = if let Some(config) = config {
        // Authorization and every AI resource remain behind the effective guard.
        let mut service = state
            .auth
            .issuer()
            .capture_role_service(grant, graphit_auth::ServicePurpose::Rerank)
            .map_err(|_| error("authentication_or_permission_denied"))?;
        let slugs: Vec<_> = page
            .hits
            .iter()
            .map(|hit| hit.source.slug.as_str())
            .collect();
        let selected = capture
            .read_selected_documents(original.snapshot(), read, &slugs)
            .await
            .map_err(failure)?;
        let texts =
            crate::foreground_knowledge::search::rerank_texts(&selected, &page.generation, &slugs)
                .map_err(failure)?;
        let resources = state
            .semantic
            .read()
            .map_err(|_| error("model_resource_failure"))?
            .clone();
        let verifiers = state.hub_verifiers.as_deref().or_else(|| {
            resources
                .as_deref()
                .map(|resources| resources.brokers.as_ref())
        });
        let local_control = state
            .local_models
            .read()
            .map_err(|_| error("model_resource_failure"))?
            .clone();
        let result = crate::search_rerank::run(
            original.snapshot(),
            &mut service,
            local_control
                .as_ref()
                .map(|control| (control, state.auth.runtime().as_ref())),
            &config,
            &state.policy.source.native_install.hub,
            verifiers,
            &input.query,
            &texts,
        )
        .await;
        capture
            .recheck(original.snapshot(), read)
            .map_err(failure)?;
        Some(result.map_err(|cause| error(cause.code()))?)
    } else {
        None
    };
    read.recheck()
        .map_err(|_| error("authentication_or_permission_denied"))?;
    let mut hits: Vec<_> = page
        .hits
        .iter()
        .map(|hit| {
            let preview = hit.preview.as_ref().map(|preview| {
                use graphit_data::knowledge_search::KnowledgeSearchPreviewSource;
                json!({
                    "text":preview.text,
                    "source":match preview.source {
                        KnowledgeSearchPreviewSource::Summary => "summary",
                        KnowledgeSearchPreviewSource::Body => "body",
                    },
                    "range":preview.range,
                    "truncated":preview.truncated,
                })
            });
            json!({"focused_source":hit.source,"lexical_score":hit.score,"preview":preview})
        })
        .collect();
    if let Some(ranks) = ranks {
        hits = ranks
            .order
            .iter()
            .map(|&position| {
                let mut hit = hits[position].clone();
                hit["rerank_score"] = json!(ranks.scores[position]);
                hit
            })
            .collect();
    }
    Ok(json!({
        "kind":match page.state {
            KnowledgeSearchState::PublishedEmpty => "published_empty",
            KnowledgeSearchState::NoHits => "no_hits",
            KnowledgeSearchState::Hits => "hits",
        },
        "generation":super::generation(&page.generation),
        "total":page.total,
        "hits":hits,
        "next_cursor":page.next_cursor,
    }))
}

fn failure(cause: KnowledgeSearchError) -> Value {
    use graphit_data::knowledge_embeddings::KnowledgeEmbeddingError;
    match cause {
        KnowledgeSearchError::Data(cause) => super::failure(cause),
        KnowledgeSearchError::Capture(KnowledgeEmbeddingError::Index(cause)) => {
            super::failure(cause)
        }
        KnowledgeSearchError::Capture(KnowledgeEmbeddingError::Conflict)
        | KnowledgeSearchError::GenerationChanged => error("generation_changed"),
        KnowledgeSearchError::Capture(cause) => causal("knowledge_search_capture_failed", &cause),
        KnowledgeSearchError::Preview(cause) => causal("knowledge_preview_failed", cause.as_ref()),
        KnowledgeSearchError::InvalidQuery(field) => {
            json!({"code":"invalid_arguments","field":field})
        }
        KnowledgeSearchError::InvalidCursor => error("invalid_cursor"),
    }
}
fn causal(code: &'static str, cause: &(dyn Error + 'static)) -> Value {
    let mut value = error(code);
    let mut current = Some(cause);
    while let Some(cause) = current {
        if let Some(cause) =
            cause.downcast_ref::<graphit_knowledge::preview::MarkdownPreviewError>()
        {
            value["io_kind"] = json!(format!("{:?}", cause.kind()));
            break;
        }
        if let Some(cause) = cause.downcast_ref::<io::Error>() {
            value["io_kind"] = json!(format!("{:?}", cause.kind()));
            break;
        }
        current = cause.source();
    }
    value
}
