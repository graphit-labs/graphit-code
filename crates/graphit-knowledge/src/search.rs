//! Search formatting while each SDK hit still borrows its own batch.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
use crate::preview::{MarkdownPreviewLimits, PreviewSource, preview_markdown};
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_search::{
    self as data, KnowledgeLexicalQuery, KnowledgeSearchCapture, KnowledgeSearchError,
    KnowledgeSearchLimits, KnowledgeSearchPage, KnowledgeSearchPreview,
    KnowledgeSearchPreviewSource,
};
use sha2::{Digest, Sha256};
use std::{num::NonZeroUsize, sync::Arc};

/// Bind preview limits and the caller's actual selection to continuation data.
pub fn preview_binding(caller: &str, limits: MarkdownPreviewLimits) -> String {
    let mut hash = Sha256::new();
    hash.update(b"knowledge-preview/v1");
    hash.update((caller.len() as u64).to_le_bytes());
    hash.update(caller.as_bytes());
    for limit in [
        limits.max_input_bytes,
        limits.max_output_bytes,
        limits.max_body_scalars,
    ] {
        hash.update((limit.get() as u64).to_le_bytes());
    }
    hex::encode(hash.finalize())
}

/// Format a borrowed SDK hit without retaining the corpus or granting access.
pub fn render_preview(
    original: &SnapshotContext,
    body: &str,
    summary: &str,
    limits: MarkdownPreviewLimits,
    remaining: usize,
) -> Result<KnowledgeSearchPreview, KnowledgeSearchError> {
    // The formatter has a NonZero limit, while a valid empty excerpt may
    // consume zero remaining bytes. Data enforces the actual remaining cap.
    let output = NonZeroUsize::new(remaining.min(limits.max_output_bytes.get()))
        .unwrap_or(NonZeroUsize::MIN);
    let preview = preview_markdown(
        original,
        body,
        summary,
        MarkdownPreviewLimits {
            max_output_bytes: output,
            ..limits
        },
    )
    .map_err(|error| KnowledgeSearchError::Preview(Box::new(error)))?;
    Ok(KnowledgeSearchPreview {
        text: preview.text().to_owned(),
        source: match preview.source() {
            PreviewSource::Summary => KnowledgeSearchPreviewSource::Summary,
            PreviewSource::Body => KnowledgeSearchPreviewSource::Body,
        },
        range: preview.source_range(),
        truncated: preview.truncated(),
    })
}

/// One pin and one borrowed formatting pass per selected hit; no retained bodies.
pub async fn query_native_knowledge_search(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    capture: &KnowledgeSearchCapture,
    query: KnowledgeLexicalQuery<'_>,
    limits: KnowledgeSearchLimits,
    preview_limits: MarkdownPreviewLimits,
) -> Result<KnowledgeSearchPage, KnowledgeSearchError> {
    if query.caller_binding.len() > limits.max_query_bytes.get() {
        return Err(KnowledgeSearchError::InvalidQuery("query_bytes"));
    }
    let binding = preview_binding(query.caller_binding, preview_limits);
    data::query_native_knowledge_search(
        original,
        read,
        capture,
        KnowledgeLexicalQuery {
            caller_binding: &binding,
            ..query
        },
        limits,
        |body, summary, remaining| {
            render_preview(original, body, summary, preview_limits, remaining)
        },
    )
    .await
}

/// Format lexical hits from an already captured Source/NN pair, without Models input.
pub async fn query_native_knowledge_embedding_source_search(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    capture: &graphit_data::knowledge_embeddings::KnowledgeEmbeddingCapture,
    query: KnowledgeLexicalQuery<'_>,
    limits: KnowledgeSearchLimits,
    preview_limits: MarkdownPreviewLimits,
) -> Result<KnowledgeSearchPage, KnowledgeSearchError> {
    if query.caller_binding.len() > limits.max_query_bytes.get() {
        return Err(KnowledgeSearchError::InvalidQuery("query_bytes"));
    }
    let binding = preview_binding(query.caller_binding, preview_limits);
    data::query_native_knowledge_embedding_source_search(
        original,
        read,
        capture,
        KnowledgeLexicalQuery {
            caller_binding: &binding,
            ..query
        },
        limits,
        |body, summary, remaining| {
            render_preview(original, body, summary, preview_limits, remaining)
        },
    )
    .await
}

/// The caller retains its real Models client/lease/runtime and Query RPC.
/// This bridge formats only borrowed hits from that same source/NN capture.
#[allow(clippy::too_many_arguments)]
pub async fn query_native_knowledge_vector_search(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    capture: &graphit_data::knowledge_embeddings::KnowledgeEmbeddingCapture,
    identity: &graphit_data::knowledge_embeddings::KnowledgeEmbeddingIdentity,
    current_inputs: &[graphit_data::knowledge_embeddings::KnowledgeEmbeddingInput<'_>],
    query: data::KnowledgeVectorSearchQuery<'_>,
    limits: KnowledgeSearchLimits,
    preview_limits: MarkdownPreviewLimits,
) -> Result<data::KnowledgeVectorSearchPage, KnowledgeSearchError> {
    if query.page.caller_binding.len() > limits.max_query_bytes.get() {
        return Err(KnowledgeSearchError::InvalidQuery("query_bytes"));
    }
    let binding = preview_binding(query.page.caller_binding, preview_limits);
    let query = data::KnowledgeVectorSearchQuery {
        page: KnowledgeLexicalQuery {
            caller_binding: &binding,
            ..query.page
        },
        ..query
    };
    data::query_native_knowledge_vector_search(
        original,
        read,
        capture,
        identity,
        current_inputs,
        query,
        limits,
        |body, summary, remaining| {
            render_preview(original, body, summary, preview_limits, remaining)
        },
    )
    .await
}

#[cfg(test)]
use crate::desired_reconciliation_fixture as fixture;

#[cfg(test)]
mod tests {
    use super::fixture::*;
    use super::*;
    use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
    use graphit_core::file_lock::Cancellation;
    use graphit_data::{knowledge_document_projection as h, knowledge_index};
    use std::error::Error;

    #[tokio::test]
    async fn native_preview_empty_exact_metadata_boundary_and_one_byte_excess()
    -> Result<(), Box<dyn Error>> {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let read =
            auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
        let write = auth.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Knowledge,
        )?;
        let fields = [h::AuthoredPageFields {
            slug: Some("a"),
            title: Some("match"),
            body: Some(""),
            kind: Some("page"),
            summary: Some(""),
            description: None,
            summary_truncated: false,
            source_path: "docs/a.md",
            physical_digest: None,
            compilation_digest: h::CompilationDigest32::from_bytes([1; 32]),
            recipe: h::RecipeFields {
                id: h::AUTHORED_RECIPE_ID,
                parameters: Some(h::AuthoredParameters {
                    fallback_title: "match",
                    kind_on_absent: "page",
                    max_summary_bytes: n(16),
                }),
            },
            updated_at: None,
            stale_since: None,
            stale_after: None,
            resolved_wiki_targets: &[],
        }];
        let h_limits = limits().projection;
        let rows = h::prepare(ctx.snapshot(), &fields, &[], h_limits)?;
        knowledge_index::publish_native_authored(
            ctx.snapshot(),
            &write,
            &rows,
            limits().index.write,
        )
        .await?;
        let data::KnowledgeSearchRead::Published(capture) =
            data::capture_native_knowledge_search(ctx.snapshot(), &read, limits().index.read)
                .await?
        else {
            panic!()
        };
        let copied_metadata = "a".len() + "match".len() + "docs/a.md".len();
        let search_limits = data::KnowledgeSearchLimits {
            read: limits().index.read,
            max_query_bytes: n(4096),
            max_output_bytes: n(copied_metadata),
        };
        let preview_limits = MarkdownPreviewLimits {
            max_input_bytes: n(65536),
            max_output_bytes: n(4096),
            max_body_scalars: n(8),
        };
        let query = KnowledgeLexicalQuery {
            text: "match",
            top_k: 0,
            page_size: n(1),
            cursor: None,
            caller_binding: "actual Native PROJECT",
            preview: true,
        };
        let page = query_native_knowledge_search(
            ctx.snapshot(),
            &read,
            &capture,
            query,
            search_limits,
            preview_limits,
        )
        .await?;
        assert_eq!(page.total, 1);
        let preview = page.hits[0].preview.as_ref().unwrap();
        assert_eq!(preview.text, "");
        assert_eq!(preview.range, 0..0);
        assert!(!preview.truncated);
        let error = query_native_knowledge_search(
            ctx.snapshot(),
            &read,
            &capture,
            query,
            KnowledgeSearchLimits {
                max_output_bytes: n(copied_metadata - 1),
                ..search_limits
            },
            preview_limits,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            KnowledgeSearchError::Data(knowledge_index::KnowledgeIndexError::LimitExceeded(
                "search_output_bytes"
            ))
        ));
        Ok(())
    }
    #[tokio::test]
    async fn preview_configuration_is_cursor_bound_and_formatter_failure_is_causal()
    -> Result<(), Box<dyn Error>> {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
        let auth = ProjectRecordAuthority::new(&issuer);
        let read =
            auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
        let write = auth.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Knowledge,
        )?;
        let field = |slug, path| h::AuthoredPageFields {
            slug: Some(slug),
            title: Some("match"),
            body: Some("body é 日本語"),
            kind: Some("page"),
            summary: Some("summary é"),
            description: None,
            summary_truncated: false,
            source_path: path,
            physical_digest: None,
            compilation_digest: h::CompilationDigest32::from_bytes([1; 32]),
            recipe: h::RecipeFields {
                id: h::AUTHORED_RECIPE_ID,
                parameters: Some(h::AuthoredParameters {
                    fallback_title: "match",
                    kind_on_absent: "page",
                    max_summary_bytes: n(16),
                }),
            },
            updated_at: None,
            stale_since: None,
            stale_after: None,
            resolved_wiki_targets: &[],
        };
        let fields = [field("a", "docs/a.md"), field("b", "docs/b.md")];
        let rows = h::prepare(ctx.snapshot(), &fields, &[], limits().projection)?;
        knowledge_index::publish_native_authored(
            ctx.snapshot(),
            &write,
            &rows,
            limits().index.write,
        )
        .await?;
        let data::KnowledgeSearchRead::Published(capture) =
            data::capture_native_knowledge_search(ctx.snapshot(), &read, limits().index.read)
                .await?
        else {
            panic!()
        };
        let search_limits = data::KnowledgeSearchLimits {
            read: limits().index.read,
            max_query_bytes: n(4096),
            max_output_bytes: n(65536),
        };
        let preview_limits = MarkdownPreviewLimits {
            max_input_bytes: n(65536),
            max_output_bytes: n(4096),
            max_body_scalars: n(8),
        };
        let query = KnowledgeLexicalQuery {
            text: "match",
            top_k: 0,
            page_size: n(1),
            cursor: None,
            caller_binding: "actual Native PROJECT",
            preview: true,
        };
        let first = query_native_knowledge_search(
            ctx.snapshot(),
            &read,
            &capture,
            query,
            search_limits,
            preview_limits,
        )
        .await?;
        let preview = first.hits[0].preview.as_ref().unwrap();
        assert_eq!(preview.source, KnowledgeSearchPreviewSource::Summary);
        assert_eq!(preview.text, "summary é");
        assert_eq!(preview.range, 0.."summary é".len());
        let continued = KnowledgeLexicalQuery {
            cursor: first.next_cursor.as_ref(),
            ..query
        };
        assert!(continued.cursor.is_some());
        let valid = query_native_knowledge_search(
            ctx.snapshot(),
            &read,
            &capture,
            continued,
            search_limits,
            preview_limits,
        )
        .await?;
        assert_ne!(valid.hits[0].source.slug, first.hits[0].source.slug);
        let changed = MarkdownPreviewLimits {
            max_body_scalars: n(9),
            ..preview_limits
        };
        assert!(matches!(
            query_native_knowledge_search(
                ctx.snapshot(),
                &read,
                &capture,
                continued,
                search_limits,
                changed
            )
            .await,
            Err(KnowledgeSearchError::InvalidCursor)
        ));
        let failure = query_native_knowledge_search(
            ctx.snapshot(),
            &read,
            &capture,
            query,
            search_limits,
            MarkdownPreviewLimits {
                max_input_bytes: n(1),
                ..preview_limits
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&failure,KnowledgeSearchError::Preview(error) if matches!(error.downcast_ref::<crate::preview::MarkdownPreviewError>(),Some(crate::preview::MarkdownPreviewError::LimitExceeded("input bytes"))))
        );
        Ok(())
    }
}
