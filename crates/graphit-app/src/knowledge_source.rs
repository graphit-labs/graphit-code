//! Authenticated PROJECT lookup of a native Knowledge virtual source.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
use graphit_auth::{
    AuthError, AuthenticatedCaller, ProjectReadAdmission, ProjectReadModule,
    ProjectRecordAuthority, ScopeInput,
};
use graphit_core::text_slice;
use graphit_data::knowledge_index::{
    KnowledgeGeneration, KnowledgeIndexError, KnowledgeIndexPageRead, KnowledgeIndexReadLimits,
    read_native_knowledge_page,
};
use graphit_data::knowledge_snapshot::KnowledgeSnapshotPage;
use graphit_hub::policy::RequestContext;
use graphit_knowledge::document_source::{
    DocumentSourceError, DocumentSourceLimits, render_document_source,
};
use std::{error::Error, fmt, io};

/// `slug` is the exact native identity, rather than an ingress alias.
#[derive(Clone, Copy)]
pub struct KnowledgeSourceRequest<'a> {
    pub slug: &'a str,
    pub selection: &'a text_slice::Request,
}

#[derive(Clone, Copy)]
pub struct KnowledgeSourcePolicy {
    pub index: KnowledgeIndexReadLimits,
    pub source: DocumentSourceLimits,
}

/// No handles, admissions or borrowed corpus data escape the read.
#[derive(Debug, PartialEq, Eq)]
pub enum KnowledgeSourceRead {
    NoCorpus,
    PublishedEmpty {
        generation: KnowledgeGeneration,
    },
    PageAbsent {
        generation: KnowledgeGeneration,
    },
    Found {
        generation: KnowledgeGeneration,
        slug: String,
        selection: text_slice::Result,
        focused_source:
            Option<Box<graphit_data::knowledge_reference_query::KnowledgeReferenceSource>>,
    },
}

pub enum KnowledgeSourceError {
    Request(io::Error),
    Auth(AuthError),
    Data(Box<KnowledgeIndexError>),
    Source(Box<DocumentSourceError>),
    InvalidSlug,
    SlugLimit,
}
impl fmt::Debug for KnowledgeSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Request(_) => "request",
            Self::Auth(_) => "authorization",
            Self::Data(_) => "corpus",
            Self::Source(_) => "source selection",
            Self::InvalidSlug => "empty slug",
            Self::SlugLimit => "slug byte limit",
        })
    }
}
impl fmt::Display for KnowledgeSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Knowledge source: {self:?}")
    }
}
impl Error for KnowledgeSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Auth(e) => Some(e),
            Self::Data(e) => Some(e.as_ref()),
            Self::Source(e) => Some(e.as_ref()),
            Self::InvalidSlug | Self::SlugLimit => None,
        }
    }
}

fn recheck(
    original: &RequestContext<'_>,
    admission: &ProjectReadAdmission<'_>,
) -> Result<(), KnowledgeSourceError> {
    original
        .snapshot()
        .check()
        .map_err(KnowledgeSourceError::Request)?;
    admission.recheck().map_err(KnowledgeSourceError::Auth)?;
    Ok(())
}

/// One authorized selected EOF scan and one Core selection inside the native renderer.
/// A successful empty pattern selection remains `Found`, with its real line count.
pub async fn read_knowledge_source_authenticated<'a>(
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
    scope: ScopeInput<'a>,
    request: KnowledgeSourceRequest<'_>,
    policy: KnowledgeSourcePolicy,
    original: &'a RequestContext<'a>,
) -> Result<KnowledgeSourceRead, KnowledgeSourceError> {
    original
        .snapshot()
        .check()
        .map_err(KnowledgeSourceError::Request)?;
    let admission = authority
        .admit_authenticated_read(caller, original, scope, ProjectReadModule::Knowledge)
        .map_err(KnowledgeSourceError::Auth)?;
    if request.slug.is_empty() {
        return Err(KnowledgeSourceError::InvalidSlug);
    }
    if request.slug.len() > policy.index.content.max_identifier_bytes.get() {
        return Err(KnowledgeSourceError::SlugLimit);
    }
    let read =
        read_native_knowledge_page(original.snapshot(), &admission, request.slug, policy.index)
            .await
            .map_err(|e| KnowledgeSourceError::Data(Box::new(e)))?;
    recheck(original, &admission)?;
    let result = match read {
        KnowledgeIndexPageRead::NoCorpus => KnowledgeSourceRead::NoCorpus,
        KnowledgeIndexPageRead::Published { generation, page } => match page {
            KnowledgeSnapshotPage::PublishedEmpty => {
                KnowledgeSourceRead::PublishedEmpty { generation }
            }
            KnowledgeSnapshotPage::PageAbsent => KnowledgeSourceRead::PageAbsent { generation },
            KnowledgeSnapshotPage::Found {
                document,
                focused_source,
            } => {
                let selection = render_document_source(
                    original.snapshot(),
                    &document,
                    request.selection,
                    policy.source,
                )
                .map_err(|e| KnowledgeSourceError::Source(Box::new(e)))?;
                KnowledgeSourceRead::Found {
                    generation,
                    slug: document.slug,
                    selection,
                    focused_source,
                }
            }
        },
    };
    recheck(original, &admission)?;
    Ok(result)
}
