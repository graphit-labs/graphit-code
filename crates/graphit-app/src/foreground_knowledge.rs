//! PROJECT Knowledge catalogue and Markdown reader.
use crate::task_server::{
    TaskPolicy,
    browser::codec::{self, AdmittedSelection, Body},
    request::Arguments,
};
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    knowledge_document_lifecycle::{
        KnowledgeSourcePathComponents, decode_managed_knowledge_source_path,
    },
    knowledge_index::{
        self, KnowledgeDocument, KnowledgeHead, KnowledgeHeadRead, KnowledgeHeadReadLimits,
        KnowledgeIndexContentLimits, KnowledgeIndexError, KnowledgeIndexPageRead,
        KnowledgeIndexReadLimits,
    },
    knowledge_reference_query::KnowledgeReferenceQueryError,
    knowledge_snapshot::KnowledgeSnapshotPage,
};
use graphit_hub::native::ContentLimits;
use serde::{Deserialize, Serialize, Serializer, ser::SerializeSeq};
use std::{
    num::NonZeroUsize,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
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
    CatalogueInput(&'static str),
    Lint(graphit_knowledge::published_lint::PublishedLintError),
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
        batch_rows: policy.read.batch_rows,
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: positive(policy.content.max_scalar_bytes)?,
        },
    })
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
struct Head<'a> {
    slug: &'a str,
    title: &'a str,
    updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<std::borrow::Cow<'a, str>>,
}

fn portable_source_path(path: &str) -> Option<std::borrow::Cow<'_, str>> {
    let bytes = NonZeroUsize::new(path.len())?;
    match decode_managed_knowledge_source_path(path, bytes).ok()? {
        Some(KnowledgeSourcePathComponents::Portable(parts)) => Some(parts.join("/").into()),
        None if !path.contains([':', '\\'])
            && !path.chars().any(char::is_control)
            && path.split('/').all(|part| !matches!(part, "" | "." | "..")) =>
        {
            Some(path.into())
        }
        None => None,
    }
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
                source_path: h.source_path.as_deref().and_then(portable_source_path),
            })?;
        }
        seq.end()
    }
}
#[derive(Serialize)]
struct Document<'a> {
    slug: &'a str,
    title: &'a str,
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<std::borrow::Cow<'a, str>>,
    resolved_wiki_targets: &'a [String],
    previous: Option<&'a graphit_knowledge::navigation::NavigationLink>,
    next: Option<&'a graphit_knowledge::navigation::NavigationLink>,
}

impl<'a> Document<'a> {
    fn new(
        document: &'a KnowledgeDocument,
        navigation: &'a graphit_knowledge::navigation::DocumentNavigation,
    ) -> Self {
        Self {
            slug: &document.slug,
            title: &document.title,
            body: &document.body,
            source_path: portable_source_path(&document.source_path),
            resolved_wiki_targets: &document.resolved_wiki_targets,
            previous: navigation.previous.as_ref(),
            next: navigation.next.as_ref(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PageBinding<'a, T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    selection: Option<&'a AdmittedSelection>,
    source: &'a crate::reference_explorer::SourceKey,
    focused_source: Option<PageFocus<'a>>,
    native: T,
}

#[derive(Serialize)]
pub(crate) struct PageFocus<'a> {
    pub(crate) slug: &'a str,
    pub(crate) namespace:
        &'a graphit_data::knowledge_reference_query::KnowledgeReferenceOwnedNamespace,
}

impl<'a, T> PageBinding<'a, T> {
    pub(crate) fn new(
        selection: Option<&'a AdmittedSelection>,
        source: &'a crate::reference_explorer::SourceKey,
        focused_source: Option<PageFocus<'a>>,
        native: T,
    ) -> Self {
        Self {
            selection,
            source,
            focused_source,
            native,
        }
    }
}

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
                document: Document::new(v, self.1),
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
    Published { page: PageView<'a> },
}
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum HeadRead<'a> {
    NoCorpus,
    Published { heads: Heads<'a> },
}
#[allow(
    clippy::too_many_arguments,
    reason = "caller authority, admitted selection, policy and model context remain explicit"
)]
pub(crate) async fn read<'a>(
    request: &'a graphit_hub::policy::RequestContext<'a>,
    caller: search::ReadIdentity<'a>,
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
                    batch_rows: limits.batch_rows,
                },
            )
            .await?;
            let view = match &native {
                KnowledgeHeadRead::NoCorpus => HeadRead::NoCorpus,
                KnowledgeHeadRead::Published { heads, .. } => HeadRead::Published {
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
            let native =
                knowledge_index::read_native_knowledge_page(admission, &input.slug, limits).await?;
            let focused_source = match &native {
                KnowledgeIndexPageRead::Published {
                    page:
                        KnowledgeSnapshotPage::Found {
                            document,
                            namespace: Some(namespace),
                        },
                    ..
                } => Some(PageFocus {
                    slug: &document.slug,
                    namespace,
                }),
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
                KnowledgeIndexPageRead::Published { page, .. } => PageRead::Published {
                    page: PageView(page, &navigation),
                },
            };
            codec::encode_response(
                &PageBinding::new(
                    Some(selection),
                    &crate::reference_explorer::SourceKey::ProjectKnowledge,
                    focused_source,
                    &view,
                ),
                maximum,
            )?
        }
    };
    original.check().map_err(KnowledgeIndexError::from)?;
    Ok(result)
}
