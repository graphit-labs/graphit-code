//! Browser Knowledge reads retain their own Web admission and explicit HubRead lease.
use super::*;
use crate::remote_native_mount::Selection as RemoteSelection;
use crate::source_server::remote_native;
use graphit_data::knowledge_index::KnowledgeHeadRead;

pub(super) async fn read(
    resources: &Resources<'_>,
    admission: &graphit_auth::ImportedKnowledgeReadAdmission<'_>,
    selection: RemoteSelection<'_>,
    input: &Query,
    original: &RequestContext<'_>,
    model: impl FnOnce() -> io::Result<Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>>,
    rerank: &foreground_knowledge::imported_search::ImportedRerankContext<'_>,
) -> Result<Vec<u8>, Failure> {
    resources.check(original)?;
    if let Query::Search(input) = input {
        input
            .search
            .validate(resources.policy.content)
            .map_err(Failure::Invalid)?;
    }
    let policy = resources
        .policy
        .imported_knowledge
        .as_ref()
        .ok_or(Failure::Unavailable)?;
    let mut service = resources
        .auth
        .issuer()
        .capture_imported_knowledge_hub_service(admission, selection.record().provider_name())
        .map_err(|error| Failure::Remote(Box::new(error.into())))?;
    let verifier = if service
        .mode()
        .map_err(|error| Failure::Remote(Box::new(error.into())))?
        == graphit_auth::ServiceMode::BrokerNative
    {
        Some(
            resources
                .native_verifiers
                .ok_or(Failure::Unavailable)?
                .get(&mut service)
                .map_err(|error| Failure::Physical(io::Error::other(error)))?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => service.native_credential(verifier).await,
        None => service.credential(),
    }
    .map_err(|error| Failure::Remote(Box::new(error.into())))?;
    let metadata = crate::native_publication::metadata_limits(
        resources.policy.content,
        policy.registry.limits().projection(),
    );
    let opened = remote_native::open(
        selection,
        &lease,
        resources.native_http,
        resources.native_allow_loopback_http,
        metadata,
    )
    .await?;
    let bytes = match input {
        Query::Heads(input) => {
            let limits = foreground_knowledge::limits(resources.policy)?;
            let read = opened
                .heads(
                    &lease,
                    graphit_data::knowledge_index::KnowledgeHeadReadLimits {
                        max_slug_bytes: limits.content.max_identifier_bytes,
                        native: limits.native,
                        batch_rows: limits.batch_rows,
                    },
                )
                .await?;
            let KnowledgeHeadRead::Published {
                generation,
                mut heads,
            } = read
            else {
                return Err(Failure::Invalid("invalid_remote_native_heads"));
            };
            heads.sort_unstable_by(|left, right| left.slug.cmp(&right.slug));
            encode(
                &super::imported_knowledge_heads::Response {
                    source: &input.source,
                    native: super::imported_knowledge_heads::Read::Published {
                        generation: (&generation).into(),
                        heads: foreground_knowledge::Heads(&heads),
                    },
                },
                original.snapshot(),
                resources.stop,
                resources.policy.content.max_output_bytes,
            )?
        }
        Query::Page(input) => {
            let page = opened
                .page(
                    &lease,
                    &input.slug,
                    foreground_knowledge::limits(resources.policy)?,
                )
                .await?;
            encode(
                &ReferenceBinding {
                    selection: None,
                    source: &input.source,
                    focused_source: page.focused_source.as_ref(),
                    native: PageRead::Published {
                        generation: &page.generation,
                        page: foreground_knowledge::PageView(&page.page),
                    },
                },
                original.snapshot(),
                resources.stop,
                resources.policy.content.max_output_bytes,
            )?
        }
        Query::Search(input) => {
            let limits = foreground_knowledge::search::query_limits(
                foreground_knowledge::limits(resources.policy)?,
                resources.policy.content,
            )?;
            let preview = foreground_knowledge::search::preview_limits(resources.policy.content)?;
            let outcome = if input.search.mode == foreground_knowledge::search::Mode::Fts {
                opened
                    .search(
                        &lease,
                        input.search.native("web"),
                        limits,
                        preview,
                        admission,
                        &input.search,
                        rerank,
                    )
                    .await?
            } else {
                let resources = model().map_err(Failure::Physical)?;
                let binding =
                    serde_json::to_string(&("web", input.search.mode, input.search.metric))?;
                remote_native::semantic::query(
                    &opened,
                    &lease,
                    admission,
                    resources.as_deref(),
                    rerank,
                    &input.search,
                    crate::knowledge_semantic::KnowledgeSemanticSearchOptions {
                        page: input.search.native(&binding),
                        mode: input.search.vector_mode(),
                        metric: input.search.metric.native(),
                        limits,
                        preview,
                    },
                )
                .await?
            };
            encode(
                &ReferenceBinding {
                    selection: None,
                    source: &input.source,
                    focused_source: None,
                    native: outcome.view(),
                },
                original.snapshot(),
                resources.stop,
                resources.policy.content.max_output_bytes,
            )?
        }
        Query::Contexts => return Err(Failure::Invalid("invalid_remote_native_operation")),
    };
    opened.check(&lease)?;
    admission.recheck()?;
    resources.check(original)?;
    Ok(bytes)
}
