//! Structured, generation-bound catalogue and published diagnostics over the existing EOF reader.
use crate::{foreground_knowledge::Failure, task_server::TaskPolicy};
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_index::{self, KnowledgeDocument, KnowledgeIndexRead};
use graphit_knowledge::{published_lint::*, published_metadata_lint::*};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, SystemTime},
};

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Filter {
    kind: Option<String>,
    source_prefix: Option<String>,
    stale: Option<bool>,
    /// Missing required effective metadata only; recommendations remain lint findings.
    metadata_errors: Option<bool>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cursor {
    binding: [u8; 32],
    after_slug: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct List {
    #[serde(default)]
    filter: Filter,
    #[serde(default = "page_size")]
    page_size: NonZeroUsize,
    cursor: Option<Cursor>,
}
fn page_size() -> NonZeroUsize {
    NonZeroUsize::new(25).unwrap()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lint {
    max_age_days: Option<std::num::NonZeroU64>,
}
#[derive(Serialize, Deserialize)]
pub(crate) enum Query {
    List(Box<List>),
    Lint(Lint),
}
fn positive(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).expect("validated content policy")
}
fn document_value(doc: &KnowledgeDocument) -> Value {
    let timestamp = |value: Option<SystemTime>| {
        value.map(|time| match time.duration_since(std::time::UNIX_EPOCH) {
            Ok(age) => age.as_nanos().to_string(),
            Err(age) => format!("-{}", age.duration().as_nanos()),
        })
    };
    json!({"slug":doc.slug,"title":doc.title,"kind":doc.kind,"description":doc.description,"source_path":doc.source_path,"updated_at":timestamp(doc.updated_at),"stale_since":timestamp(doc.stale_since),"stale_after":doc.stale_after})
}
pub(crate) async fn read(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    policy: &TaskPolicy,
    query: Query,
    scope: Value,
) -> Result<Value, Failure> {
    let read = knowledge_index::read_native_knowledge_index(
        original,
        admission,
        crate::foreground_knowledge::limits(policy)?,
    )
    .await?;
    let KnowledgeIndexRead::Published {
        generation,
        mut documents,
    } = read
    else {
        if matches!(&query, Query::List(input) if input.cursor.is_some()) {
            return Err(Failure::CatalogueInput("cursor_expired"));
        }
        return Ok(json!({"state":"no_corpus"}));
    };
    let native_generation = json!(generation);
    documents.sort_by(|a, b| a.slug.cmp(&b.slug));
    let value = match query {
        Query::List(input) => {
            if input.page_size.get() > policy.content.max_collection_entries {
                return Err(Failure::CatalogueInput("invalid_page_size"));
            }
            let binding: [u8; 32] = Sha256::digest(serde_json::to_vec(&(
                &scope,
                &input.filter,
                generation.version,
                &generation.corpus_uri,
                &generation.physical_branch,
                &generation.inherited_target,
            ))?)
            .into();
            if input
                .cursor
                .as_ref()
                .is_some_and(|cursor| cursor.binding != binding)
            {
                return Err(Failure::CatalogueInput("cursor_expired"));
            }
            let mut rows = documents
                .iter()
                .filter(|doc| {
                    input.filter.kind.as_ref().is_none_or(|v| v == &doc.kind)
                        && input
                            .filter
                            .source_prefix
                            .as_ref()
                            .is_none_or(|v| doc.source_path.starts_with(v))
                        && input
                            .filter
                            .stale
                            .is_none_or(|v| v == doc.stale_since.is_some())
                        && input
                            .filter
                            .metadata_errors
                            .is_none_or(|v| v == doc.kind.trim().is_empty())
                        && input
                            .cursor
                            .as_ref()
                            .is_none_or(|v| doc.slug > v.after_slug)
                })
                .take(input.page_size.get().saturating_add(1))
                .collect::<Vec<_>>();
            let more = rows.len() > input.page_size.get();
            if more {
                rows.pop();
            }
            let cursor = if more {
                rows.last().map(|doc| Cursor {
                    binding,
                    after_slug: doc.slug.clone(),
                })
            } else {
                None
            };
            json!({"state":"published","generation":native_generation,"documents":rows.into_iter().map(document_value).collect::<Vec<_>>(),"next_cursor":cursor})
        }
        Query::Lint(input) => {
            if documents.is_empty() {
                return Ok(
                    json!({"state":"published_empty","generation":native_generation,"issue_count":0,"findings":[]}),
                );
            }
            let c = policy.content;
            // Host policy supplies capacities; callers cannot select larger limits.
            let limits = PublishedLintLimits {
                max_pages: positive(c.max_nodes),
                max_input_bytes: positive(c.max_output_bytes),
                max_findings: positive(c.max_nodes),
                max_reference_result_bytes: positive(c.max_output_bytes),
                document: graphit_knowledge::document::DocumentParseLimits {
                    max_document_bytes: positive(c.max_output_bytes),
                    max_input_bytes: positive(c.max_output_bytes),
                    max_frontmatter_bytes: positive(c.max_input_bytes),
                    max_yaml_events: positive(c.max_nodes),
                    max_markdown_events: positive(c.max_nodes),
                    max_yaml_depth: positive(c.max_depth),
                    max_scalar_bytes: positive(c.max_scalar_bytes),
                    max_links: positive(c.max_nodes),
                    max_projection_bytes: positive(c.max_output_bytes),
                    max_summary_bytes: positive(c.max_scalar_bytes),
                },
                references: graphit_knowledge::references::ReferenceLimits {
                    max_identifier_bytes: positive(c.max_scalar_bytes),
                },
                metadata: PublishedMetadataLintLimits {
                    max_pages: positive(c.max_nodes),
                    max_input_bytes: positive(c.max_output_bytes),
                    max_findings: positive(c.max_nodes),
                },
            };
            let max_age = input
                .max_age_days
                .map(|value| {
                    value
                        .get()
                        .checked_mul(86400)
                        .map(Duration::from_secs)
                        .ok_or_else(|| {
                            knowledge_index::KnowledgeIndexError::InvalidField("max age overflow")
                        })
                })
                .transpose()?;
            let report = lint_published_snapshot(
                original,
                &generation,
                &documents,
                PublishedLintPolicy {
                    evaluated_at: SystemTime::now(),
                    max_age,
                },
                limits,
            )
            .map_err(Failure::Lint)?;
            let location = |slug: &str, field: &str| {
                let document = documents.iter().find(|doc| doc.slug == slug);
                json!({"slug":slug,"source_path":document.map(|doc|&doc.source_path),"field":field,"line":null,"basis":"published_metadata"})
            };
            let mut findings = Vec::new();
            for finding in &report.metadata.findings {
                let field = match finding.field {
                    PublishedMetadataField::Kind => "kind",
                    PublishedMetadataField::Title => "title",
                    PublishedMetadataField::Description => "description",
                };
                findings.push(json!({"code":"missing_metadata","severity":match finding.severity {PublishedMetadataSeverity::Required=>"error",PublishedMetadataSeverity::Recommended=>"recommendation"},"location":location(finding.slug,field),"message":format!("Set a nonempty {field} in the authored document metadata")}));
            }
            for link in &report.broken_links {
                findings.push(json!({"code":"broken_link","severity":"error","location":location(&link.source,"links"),"target":link.target,"message":"Repair the target or remove the broken link"}));
            }
            for slug in &report.orphans {
                findings.push(json!({"code":"orphan","severity":"error","location":location(slug,"links"),"message":"Link this page from the maintained domain navigation"}));
            }
            for page in &report.short_pages {
                findings.push(json!({"code":"short_page","severity":"error","location":location(page.slug,"body"),"word_count":page.word_count,"message":"Review whether this page sufficiently explains its reader goal"}));
            }
            for page in &report.stale_pages {
                findings.push(json!({"code":"stale","severity":"error","location":location(page.slug,"stale_since"),"reason":match page.reason {PublishedStaleReason::Persisted {..}=>"persisted_marker",PublishedStaleReason::Age {..}=>"explicit_age_policy"},"message":"Verify the documented contract against implementation and refresh the maintained document"}));
            }
            json!({"state":"published","generation":native_generation,"total_pages":report.total_pages,"issue_count":report.issue_count,"findings":findings})
        }
    };
    admission
        .recheck()
        .map_err(knowledge_index::KnowledgeIndexError::from)?;
    original
        .check()
        .map_err(knowledge_index::KnowledgeIndexError::from)?;
    Ok(value)
}
