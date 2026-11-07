//! PROJECT Knowledge metadata and one selected FULL16 page, over native readers.
use crate::task_server::{
    TaskPolicy,
    browser::codec::{self, AdmittedSelection, Body},
    request::Arguments,
};
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    knowledge_index::{
        self, KnowledgeDocument, KnowledgeGeneration, KnowledgeHead, KnowledgeHeadRead,
        KnowledgeHeadReadLimits, KnowledgeIndexContentLimits, KnowledgeIndexError,
        KnowledgeIndexPageRead, KnowledgeIndexReadLimits,
    },
    knowledge_reference_query::KnowledgeReferenceQueryError,
    knowledge_snapshot::KnowledgeSnapshotPage,
};
use graphit_hub::native::ContentLimits;
use serde::{Deserialize, Serialize, Serializer, ser::SerializeSeq};
use std::{
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(crate) mod ask;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(crate) mod imported_search;
pub(crate) mod navigation;
pub(crate) mod reference_binding;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub(crate) mod search;
use reference_binding::ReferenceBinding;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageInput {
    slug: String,
}
#[derive(Deserialize, Serialize)]
pub(crate) enum Query {
    Heads,
    Page(PageInput),
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    Search(search::Input),
}
pub(crate) fn parse(
    bytes: &[u8],
    limits: ContentLimits,
    operation: &str,
) -> Result<Arguments, &'static str> {
    match operation {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        "knowledge_search" => {
            let body: Body<search::Input> = codec::parse_body(bytes, limits)?;
            body.input.validate(limits)?;
            codec::arguments(body.selection, Query::Search(body.input))
        }
        "knowledge_heads" => {
            let body: Body<Empty> = codec::parse_body(bytes, limits)?;
            codec::arguments(body.selection, Query::Heads)
        }
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        "knowledge_ask" => {
            let body: Body<ask::Input> = codec::parse_body(bytes, limits)?;
            body.input.validate(limits)?;
            codec::arguments(body.selection, body.input)
        }
        "knowledge_page" => {
            let body: Body<PageInput> = codec::parse_body(bytes, limits)?;
            if body.input.slug.len() > limits.max_scalar_bytes {
                return Err("invalid_arguments");
            }
            knowledge_index::validate_slug(&body.input.slug).map_err(|_| "invalid_arguments")?;
            codec::arguments(body.selection, Query::Page(body.input))
        }
        _ => Err("invalid_arguments"),
    }
}
#[derive(Debug)]
pub(crate) enum Failure {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    Semantic(Box<crate::knowledge_semantic::KnowledgeSemanticError>),
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    Search(Box<graphit_data::knowledge_search::KnowledgeSearchError>),
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    Rerank(crate::search_rerank::SearchRerankError),
    Native(Box<KnowledgeIndexError>),
    References(Box<KnowledgeReferenceQueryError>),
    Encoding(serde_json::Error),
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
impl From<crate::knowledge_semantic::KnowledgeSemanticError> for Failure {
    fn from(error: crate::knowledge_semantic::KnowledgeSemanticError) -> Self {
        Self::Semantic(Box::new(error))
    }
}
impl From<KnowledgeIndexError> for Failure {
    fn from(e: KnowledgeIndexError) -> Self {
        Self::Native(Box::new(e))
    }
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
impl From<graphit_data::knowledge_search::KnowledgeSearchError> for Failure {
    fn from(error: graphit_data::knowledge_search::KnowledgeSearchError) -> Self {
        Self::Search(Box::new(error))
    }
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
impl From<crate::search_rerank::SearchRerankError> for Failure {
    fn from(error: crate::search_rerank::SearchRerankError) -> Self {
        Self::Rerank(error)
    }
}
impl From<serde_json::Error> for Failure {
    fn from(e: serde_json::Error) -> Self {
        Self::Encoding(e)
    }
}
impl From<KnowledgeReferenceQueryError> for Failure {
    fn from(error: KnowledgeReferenceQueryError) -> Self {
        Self::References(Box::new(error))
    }
}

pub(crate) fn limits(policy: &TaskPolicy) -> Result<KnowledgeIndexReadLimits, KnowledgeIndexError> {
    let positive =
        |v| NonZeroUsize::new(v).ok_or(KnowledgeIndexError::InvalidField("browser policy"));
    Ok(KnowledgeIndexReadLimits {
        native: policy.read.native,
        batch_rows: policy.read.batch_rows,
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: positive(policy.content.max_scalar_bytes)?,
        },
    })
}
#[derive(Serialize)]
pub(crate) struct Generation<'a> {
    corpus_uri: &'a str,
    version: u64,
    physical_branch: Option<&'a str>,
    inherited_target: Option<&'a str>,
}
impl<'a> From<&'a KnowledgeGeneration> for Generation<'a> {
    fn from(v: &'a KnowledgeGeneration) -> Self {
        Self {
            corpus_uri: &v.corpus_uri,
            version: v.version,
            physical_branch: v.physical_branch.as_deref(),
            inherited_target: v.inherited_target.as_deref(),
        }
    }
}
struct Timestamp(SystemTime);
impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let nanos = match self.0.duration_since(UNIX_EPOCH) {
            Ok(d) => i128::try_from(d.as_nanos()).map_err(serde::ser::Error::custom)?,
            Err(e) => {
                -i128::try_from(e.duration().as_nanos()).map_err(serde::ser::Error::custom)?
            }
        };
        s.collect_str(&nanos)
    }
}
#[derive(Serialize)]
struct Age {
    seconds: u64,
    nanoseconds: u32,
}
impl From<Duration> for Age {
    fn from(v: Duration) -> Self {
        Self {
            seconds: v.as_secs(),
            nanoseconds: v.subsec_nanos(),
        }
    }
}
#[derive(Serialize)]
struct Head<'a> {
    slug: &'a str,
    title: &'a str,
    updated_at: Option<Timestamp>,
}
pub(crate) struct Heads<'a>(pub(crate) &'a [KnowledgeHead]);
impl Serialize for Heads<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for h in self.0 {
            seq.serialize_element(&Head {
                slug: &h.slug,
                title: &h.title,
                updated_at: h.updated_at.map(Timestamp),
            })?;
        }
        seq.end()
    }
}
#[derive(Serialize)]
struct Recipe<'a> {
    fallback_title: &'a str,
    kind_on_absent: &'a str,
    max_summary_bytes: NonZeroUsize,
}
#[derive(Serialize)]
struct DocumentFields<'a> {
    slug: &'a str,
    title: &'a str,
    body: &'a str,
    kind: &'a str,
    summary: &'a str,
    description: Option<&'a str>,
    summary_truncated: bool,
    source_path: &'a str,
    physical_digest: Option<&'a [u8; 32]>,
    compilation_digest: &'a [u8; 32],
    recipe: &'a str,
    recipe_parameters: Recipe<'a>,
    updated_at: Option<Timestamp>,
    stale_since: Option<Timestamp>,
    stale_after: Option<Age>,
    resolved_wiki_targets: &'a [String],
    previous: Option<&'a graphit_knowledge::navigation::NavigationLink>,
    next: Option<&'a graphit_knowledge::navigation::NavigationLink>,
}
impl<'a> From<&'a KnowledgeDocument> for DocumentFields<'a> {
    fn from(v: &'a KnowledgeDocument) -> Self {
        Self {
            slug: &v.slug,
            title: &v.title,
            body: &v.body,
            kind: &v.kind,
            summary: &v.summary,
            description: v.description.as_deref(),
            summary_truncated: v.summary_truncated,
            source_path: &v.source_path,
            physical_digest: v.physical_digest.as_ref().map(|d| d.as_bytes()),
            compilation_digest: v.compilation_digest.as_bytes(),
            recipe: &v.recipe,
            recipe_parameters: Recipe {
                fallback_title: &v.recipe_parameters.fallback_title,
                kind_on_absent: &v.recipe_parameters.kind_on_absent,
                max_summary_bytes: v.recipe_parameters.max_summary_bytes,
            },
            updated_at: v.updated_at.map(Timestamp),
            stale_since: v.stale_since.map(Timestamp),
            stale_after: v.stale_after.map(Age::from),
            resolved_wiki_targets: &v.resolved_wiki_targets,
            previous: None,
            next: None,
        }
    }
}
struct Document<'a>(
    &'a KnowledgeDocument,
    &'a graphit_knowledge::navigation::DocumentNavigation,
);
impl Serialize for Document<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut fields = DocumentFields::from(self.0);
        fields.previous = self.1.previous.as_ref();
        fields.next = self.1.next.as_ref();
        fields.serialize(s)
    }
}
/// Reuse the native FULL16 page view for the retained imported reader.
pub(crate) struct PageView<'a>(
    pub(crate) &'a KnowledgeSnapshotPage,
    pub(crate) &'a graphit_knowledge::navigation::DocumentNavigation,
);
impl Serialize for PageView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            KnowledgeSnapshotPage::PublishedEmpty => SelectedPage::PublishedEmpty,
            KnowledgeSnapshotPage::PageAbsent => SelectedPage::PageAbsent,
            KnowledgeSnapshotPage::Found { document: v, .. } => SelectedPage::Found {
                document: Document(v.as_ref(), self.1),
            },
        }
        .serialize(s)
    }
}
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum SelectedPage<'a> {
    PublishedEmpty,
    PageAbsent,
    Found { document: Document<'a> },
}
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum PageRead<'a> {
    NoCorpus,
    Published {
        generation: Generation<'a>,
        page: SelectedPage<'a>,
    },
}
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum HeadRead<'a> {
    NoCorpus,
    Published {
        generation: Generation<'a>,
        heads: Heads<'a>,
    },
}
#[allow(
    clippy::too_many_arguments,
    reason = "caller authority, admitted selection, policy and model context remain explicit"
)]
pub(crate) async fn read<'a>(
    request: &'a graphit_hub::policy::RequestContext<'a>,
    caller: &'a graphit_auth::broker::oauth::web::AuthenticatedWebCaller<'a>,
    admission: &'a ProjectReadAdmission<'a>,
    policy: &TaskPolicy,
    selection: &AdmittedSelection,
    query: Query,
    resources: Option<&'a crate::foreground_memory_jobs::MemoryJobResources>,
    context: &search::BrowserSearchContext<'_>,
) -> Result<Vec<u8>, Failure> {
    let original = request.snapshot();
    let limits = limits(policy)?;
    let maximum = policy.content.max_output_bytes;
    let result = match query {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        Query::Search(input) => {
            return search::read(
                request, caller, admission, policy, selection, &input, resources, context,
            )
            .await;
        }
        Query::Heads => {
            let native = knowledge_index::read_native_knowledge_heads(
                original,
                admission,
                KnowledgeHeadReadLimits {
                    max_slug_bytes: limits.content.max_identifier_bytes,
                    native: limits.native,
                    batch_rows: limits.batch_rows,
                },
            )
            .await?;
            let view = match &native {
                KnowledgeHeadRead::NoCorpus => HeadRead::NoCorpus,
                KnowledgeHeadRead::Published { generation, heads } => HeadRead::Published {
                    generation: generation.into(),
                    heads: Heads(heads),
                },
            };
            codec::encode_response(
                &ReferenceBinding {
                    selection: Some(selection),
                    source: &crate::reference_explorer::SourceKey::ProjectKnowledge,
                    focused_source: None,
                    native: &view,
                },
                maximum,
            )?
        }
        Query::Page(input) => {
            let native = knowledge_index::read_native_knowledge_page(
                original,
                admission,
                &input.slug,
                limits,
            )
            .await?;
            let focused_source = match &native {
                KnowledgeIndexPageRead::Published {
                    page: KnowledgeSnapshotPage::Found { focused_source, .. },
                    ..
                } => focused_source.as_deref(),
                _ => None,
            };
            let navigation = match &native {
                KnowledgeIndexPageRead::Published {
                    page: KnowledgeSnapshotPage::Found { document, .. },
                    ..
                } => navigation::project(original, admission, document, limits).await?,
                _ => graphit_knowledge::navigation::DocumentNavigation::default(),
            };
            let view = match &native {
                KnowledgeIndexPageRead::NoCorpus => PageRead::NoCorpus,
                KnowledgeIndexPageRead::Published { generation, page } => PageRead::Published {
                    generation: generation.into(),
                    page: match page {
                        KnowledgeSnapshotPage::PublishedEmpty => SelectedPage::PublishedEmpty,
                        KnowledgeSnapshotPage::PageAbsent => SelectedPage::PageAbsent,
                        KnowledgeSnapshotPage::Found { document: v, .. } => SelectedPage::Found {
                            document: Document(v.as_ref(), &navigation),
                        },
                    },
                },
            };
            codec::encode_response(
                &ReferenceBinding {
                    selection: Some(selection),
                    source: &crate::reference_explorer::SourceKey::ProjectKnowledge,
                    focused_source,
                    native: &view,
                },
                maximum,
            )?
        }
    };
    original.check().map_err(KnowledgeIndexError::from)?;
    Ok(result)
}
