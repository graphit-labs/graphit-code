//! Read original Knowledge Markdown through the admitted PROJECT or artifact.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
use graphit_auth::{
    AuthError, AuthenticatedCaller, ProjectReadAdmission, ProjectReadModule,
    ProjectRecordAuthority, ScopeInput,
};
use graphit_core::text_slice;
use graphit_data::knowledge_index::{
    KnowledgeIndexError, KnowledgeIndexPageRead, KnowledgeIndexReadLimits,
    read_native_knowledge_page,
};
use graphit_data::knowledge_snapshot::KnowledgeSnapshotPage;
use graphit_hub::policy::RequestContext;
use std::{error::Error, fmt, io};

/// `slug` is the exact native identity, rather than an ingress alias.
#[derive(Clone, Copy)]
pub struct KnowledgeSourceRequest<'a> {
    pub slug: &'a str,
    pub selection: &'a text_slice::Request,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KnowledgeSourceRead {
    NoCorpus,
    RegistryAbsent,
    PublishedEmpty,
    PageAbsent,
    Found {
        slug: String,
        title: String,
        selection: SourceSlice,
    },
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct SourceSlice {
    pub source: String,
    pub total_lines: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
}

impl KnowledgeSourceRead {
    pub fn from_page(
        page: KnowledgeSnapshotPage,
        selection: &text_slice::Request,
    ) -> Result<Self, KnowledgeSourceError> {
        Ok(match page {
            KnowledgeSnapshotPage::PublishedEmpty => Self::PublishedEmpty,
            KnowledgeSnapshotPage::PageAbsent => Self::PageAbsent,
            KnowledgeSnapshotPage::Found { document, .. } => {
                let selected = text_slice::apply(&document.body, selection)
                    .map_err(KnowledgeSourceError::Selection)?;
                Self::Found {
                    slug: document.slug,
                    title: document.title,
                    selection: SourceSlice {
                        source: selected.source,
                        total_lines: selected.total_lines,
                        start_line: selected.start_line,
                        end_line: selected.end_line,
                    },
                }
            }
        })
    }
}

pub enum KnowledgeSourceError {
    Request(io::Error),
    Auth(AuthError),
    Data(Box<KnowledgeIndexError>),
    Selection(String),
}
impl fmt::Debug for KnowledgeSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Request(_) => "request",
            Self::Auth(_) => "authorization",
            Self::Data(_) => "corpus",
            Self::Selection(_) => "source selection",
        })
    }
}
impl fmt::Display for KnowledgeSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Selection(error) => write!(f, "Invalid source selection: {error}"),
            _ => write!(f, "Knowledge source: {self:?}"),
        }
    }
}
impl Error for KnowledgeSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Auth(e) => Some(e),
            Self::Data(e) => Some(e.as_ref()),
            Self::Selection(_) => None,
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
    limits: KnowledgeIndexReadLimits,
    original: &'a RequestContext<'a>,
) -> Result<KnowledgeSourceRead, KnowledgeSourceError> {
    original
        .snapshot()
        .check()
        .map_err(KnowledgeSourceError::Request)?;
    let admission = authority
        .admit_authenticated_read(caller, original, scope, ProjectReadModule::Knowledge)
        .map_err(KnowledgeSourceError::Auth)?;
    let read = read_native_knowledge_page(&admission, request.slug, limits)
        .await
        .map_err(|e| KnowledgeSourceError::Data(Box::new(e)))?;
    let result = match read {
        KnowledgeIndexPageRead::NoCorpus => KnowledgeSourceRead::NoCorpus,
        KnowledgeIndexPageRead::Published { page, .. } => {
            KnowledgeSourceRead::from_page(page, request.selection)?
        }
    };
    recheck(original, &admission)?;
    Ok(result)
}
