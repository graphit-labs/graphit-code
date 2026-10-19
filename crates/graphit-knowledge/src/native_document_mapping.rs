//! Pure mapping of retained authored documents into Knowledge projection inputs.
use crate::{
    compilation_fingerprint::{
        CompilationFingerprintError, CompilationFingerprintLimits, fingerprint_compiled,
    },
    document::{DocumentParseError, DocumentParseLimits, parse_document},
    publication_metadata::CompiledDocument,
    references::{PageReferences, ReferenceError, ReferenceGraph, ReferenceLimits},
};
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_document_lifecycle::{
    KnowledgeSourceIdentity, KnowledgeSourceIdentityError, KnowledgeSourcePathComponents,
    decode_managed_knowledge_source_path,
};
use graphit_data::knowledge_document_projection::{
    AUTHORED_RECIPE_ID, AuthoredPageFields, AuthoredParameters, CompilationDigest32,
    PhysicalSourceDigest32, RecipeFields,
};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, LinkType, Tag};
use std::{
    borrow::Cow,
    collections::{BTreeMap, TryReserveError},
    error::Error,
    fmt, io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    str::Utf8Error,
    sync::Arc,
    time::{Duration, SystemTime},
};

/// Identity and temporal state supplied by the caller, independent of a physical filename.
#[derive(Clone, Copy)]
pub struct AuthoredDocumentInput<'document, 'source> {
    pub compiled: &'document CompiledDocument<'source>,
    pub slug: &'document str,
    pub source_path: &'document str,
    pub updated_at: Option<SystemTime>,
    pub stale_since: Option<SystemTime>,
    pub stale_after: Option<Duration>,
}

/// Individual textual bounds; parser, fingerprint and reference bounds remain independent.
#[derive(Clone, Copy, Debug)]
pub struct NativeAuthoredMappingLimits {
    pub max_field_bytes: NonZeroUsize,
    pub max_uri_bytes: NonZeroUsize,
    pub max_target_bytes: NonZeroUsize,
}

#[derive(Debug)]
pub enum NativeAuthoredMappingError {
    Request(io::Error),
    DifferentRequest,
    InvalidOrigin { document: usize },
    DuplicateOrigin { first: usize, second: usize },
    InvalidTarget { document: usize },
    InvalidTargetViews,
    InvalidSummaryLimit,
    LimitExceeded(&'static str),
    Parser(DocumentParseError),
    Fingerprint(CompilationFingerprintError),
    References(ReferenceError),
    PercentUtf8(Utf8Error),
    Identity(KnowledgeSourceIdentityError),
    Allocation(TryReserveError),
}
impl fmt::Display for NativeAuthoredMappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(e) => write!(f, "native mapping request: {e}"),
            Self::DifferentRequest => f.write_str("mapping belongs to another request"),
            Self::InvalidOrigin { document } => {
                write!(f, "invalid logical origin at document {document}")
            }
            Self::DuplicateOrigin { first, second } => write!(
                f,
                "duplicate logical origins at documents {first} and {second}"
            ),
            Self::InvalidTarget { document } => {
                write!(f, "invalid derived target at document {document}")
            }
            Self::InvalidTargetViews => f.write_str("target views do not match mapped groups"),
            Self::InvalidSummaryLimit => {
                f.write_str("summary limit cannot be represented by authored parameters")
            }
            Self::LimitExceeded(k) => write!(f, "native mapping limit exceeded: {k}"),
            Self::Parser(_) => f.write_str("native mapping document parser failed"),
            Self::Fingerprint(_) => f.write_str("native mapping fingerprint failed"),
            Self::References(_) => f.write_str("native mapping reference validation failed"),
            Self::PercentUtf8(_) => f.write_str("native mapping target is not UTF-8"),
            Self::Identity(_) => f.write_str("native mapping source identity is invalid"),
            Self::Allocation(_) => f.write_str("native mapping allocation failed"),
        }
    }
}
impl Error for NativeAuthoredMappingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Parser(e) => Some(e),
            Self::Fingerprint(e) => Some(e),
            Self::References(e) => Some(e),
            Self::PercentUtf8(e) => Some(e),
            Self::Identity(e) => Some(e),
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for NativeAuthoredMappingError {
    fn from(value: io::Error) -> Self {
        Self::Request(value)
    }
}

/// The target owner contains no references to its own allocation.
pub struct MappedAuthoredDocument<'document, 'source> {
    input: AuthoredDocumentInput<'document, 'source>,
    compilation_digest: CompilationDigest32,
    max_summary_bytes: NonZeroUsize,
    targets: Vec<String>,
}
impl MappedAuthoredDocument<'_, '_> {
    pub fn resolved_wiki_targets(&self) -> &[String] {
        &self.targets
    }
}

/// Borrows compiled documents and the original request; owns only ordinary mapping data.
pub struct PreparedAuthoredMapping<'document, 'source, 'request> {
    request: &'request Arc<SnapshotContext>,
    documents: Vec<MappedAuthoredDocument<'document, 'source>>,
}
impl fmt::Debug for PreparedAuthoredMapping<'_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedAuthoredMapping")
            .field("documents", &self.documents.len())
            .finish_non_exhaustive()
    }
}
impl<'document, 'source> PreparedAuthoredMapping<'document, 'source, '_> {
    fn check(&self, request: &Arc<SnapshotContext>) -> Result<(), NativeAuthoredMappingError> {
        self.request.check()?;
        if !self.request.same_request(request) {
            return Err(NativeAuthoredMappingError::DifferentRequest);
        }
        request.check()?;
        for document in &self.documents {
            document.input.compiled.check_request(request)?;
        }
        self.request.check()?;
        Ok(())
    }

    pub fn documents(
        &self,
        request: &Arc<SnapshotContext>,
    ) -> Result<&[MappedAuthoredDocument<'document, 'source>], NativeAuthoredMappingError> {
        self.check(request)?;
        Ok(&self.documents)
    }

    /// The caller owns the slice views, independently of this target owner.
    /// Their contents must match the mapping exactly, including raw duplicates and empty targets.
    /// ```
    /// use graphit_core::file_io::SnapshotContext;
    /// use graphit_data::knowledge_document_projection::{
    ///     self as projection, CompilationObservationFields,
    ///     KnowledgeDocumentProjectionLimits, MemberRow,
    /// };
    /// use graphit_knowledge::native_document_mapping::PreparedAuthoredMapping;
    /// use std::{error::Error, sync::Arc};
    /// fn count_headers(
    ///     request: &Arc<SnapshotContext>,
    ///     mapped: &PreparedAuthoredMapping<'_, '_, '_>,
    ///     current: &[CompilationObservationFields<'_>],
    ///     limits: KnowledgeDocumentProjectionLimits,
    /// ) -> Result<usize, Box<dyn Error>> {
    ///     let views: Vec<Vec<&str>> = mapped.documents(request)?.iter()
    ///         .map(|document| document.resolved_wiki_targets().iter()
    ///             .map(String::as_str).collect()).collect();
    ///     let fields = mapped.authored_fields(request, &views)?;
    ///     let prepared = projection::prepare(request, &fields, current, limits)?;
    ///     prepared.supplied_context(request)?;
    ///     Ok(prepared.rows(request)?.iter()
    ///         .filter(|row| matches!(row, MemberRow::DocumentHeader(_))).count())
    /// }
    /// ```
    ///
    /// Projection fields cannot outlive the caller's target views:
    /// ```compile_fail
    /// use graphit_core::file_io::SnapshotContext;
    /// use graphit_data::knowledge_document_projection::AuthoredPageFields;
    /// use graphit_knowledge::native_document_mapping::{
    ///     PreparedAuthoredMapping, NativeAuthoredMappingError,
    /// };
    /// use std::sync::Arc;
    /// fn escaping_fields<'a>(
    ///     mapping: &'a PreparedAuthoredMapping<'_, '_, '_>,
    ///     request: &Arc<SnapshotContext>,
    /// ) -> Result<Vec<AuthoredPageFields<'a>>, NativeAuthoredMappingError> {
    ///     let views: Vec<Vec<&str>> = mapping.documents(request)?.iter()
    ///         .map(|document| document.resolved_wiki_targets().iter()
    ///             .map(String::as_str).collect()).collect();
    ///     mapping.authored_fields(request, &views)
    /// }
    /// ```
    pub fn authored_fields<'view>(
        &'view self,
        request: &Arc<SnapshotContext>,
        target_views: &'view [Vec<&'view str>],
    ) -> Result<Vec<AuthoredPageFields<'view>>, NativeAuthoredMappingError> {
        self.check(request)?;
        if target_views.len() != self.documents.len() {
            return Err(NativeAuthoredMappingError::InvalidTargetViews);
        }
        let mut fields = Vec::new();
        fields
            .try_reserve_exact(self.documents.len())
            .map_err(NativeAuthoredMappingError::Allocation)?;
        for (document, targets) in self.documents.iter().zip(target_views) {
            document.input.compiled.check_request(request)?;
            if !document
                .targets
                .iter()
                .map(String::as_str)
                .eq(targets.iter().copied())
            {
                return Err(NativeAuthoredMappingError::InvalidTargetViews);
            }
            let input = document.input;
            let compiled = input.compiled;
            let policy = compiled.recipe_policy();
            fields.push(AuthoredPageFields {
                slug: Some(input.slug),
                title: Some(compiled.title()),
                body: Some(compiled.whole_source()),
                kind: Some(compiled.kind()),
                summary: Some(compiled.summary()),
                description: compiled.description(),
                summary_truncated: compiled.summary_truncated(),
                source_path: input.source_path,
                physical_digest: Some(PhysicalSourceDigest32::from_bytes(
                    *compiled.physical_source_digest().as_bytes(),
                )),
                compilation_digest: document.compilation_digest,
                recipe: RecipeFields {
                    id: AUTHORED_RECIPE_ID,
                    parameters: Some(AuthoredParameters {
                        fallback_title: &policy.fallback_title,
                        kind_on_absent: &policy.kind_on_absent,
                        max_summary_bytes: document.max_summary_bytes,
                    }),
                },
                updated_at: input.updated_at,
                stale_since: input.stale_since,
                stale_after: input.stale_after,
                resolved_wiki_targets: targets,
            });
        }
        self.check(request)?;
        Ok(fields)
    }
}

/// Reuses the existing parser, fingerprint and reference kernel. Performs no filesystem I/O.
pub fn prepare_native_authored_mapping<'document, 'source, 'request>(
    request: &'request Arc<SnapshotContext>,
    inputs: &[AuthoredDocumentInput<'document, 'source>],
    parser_limits: DocumentParseLimits,
    mapping_limits: NativeAuthoredMappingLimits,
    fingerprint_limits: CompilationFingerprintLimits,
    reference_limits: ReferenceLimits,
) -> Result<PreparedAuthoredMapping<'document, 'source, 'request>, NativeAuthoredMappingError> {
    prepare_native_authored_mapping_with_targets(
        request,
        inputs,
        parser_limits,
        mapping_limits,
        fingerprint_limits,
        reference_limits,
        &BTreeMap::new(),
    )
}

// The stream supplies identities from independently captured selected FILEs.
// These are ordinary data; no source path or slug grants filesystem authority.
pub(crate) fn prepare_native_authored_mapping_with_targets<'document, 'source, 'request>(
    request: &'request Arc<SnapshotContext>,
    inputs: &[AuthoredDocumentInput<'document, 'source>],
    parser_limits: DocumentParseLimits,
    mapping_limits: NativeAuthoredMappingLimits,
    fingerprint_limits: CompilationFingerprintLimits,
    reference_limits: ReferenceLimits,
    targets_by_path: &BTreeMap<PathBuf, String>,
) -> Result<PreparedAuthoredMapping<'document, 'source, 'request>, NativeAuthoredMappingError> {
    request.check()?;
    let mut origins = BTreeMap::new();
    for (index, input) in inputs.iter().enumerate() {
        input.compiled.check_request(request)?;
        let compiled = input.compiled;
        let policy = compiled.recipe_policy();
        for field in [
            input.slug,
            input.source_path,
            compiled.whole_source(),
            compiled.title(),
            compiled.kind(),
            compiled.summary(),
            compiled.description().unwrap_or(""),
            &policy.fallback_title,
            &policy.kind_on_absent,
        ] {
            bounded(field.len(), mapping_limits.max_field_bytes, "field bytes")?;
        }
        if !valid_origin(input.source_path) {
            return Err(NativeAuthoredMappingError::InvalidOrigin { document: index });
        }
        let relative = relative_origin(input.source_path, mapping_limits.max_field_bytes)?;
        if let Some(first) = origins.insert(relative, index) {
            return Err(NativeAuthoredMappingError::DuplicateOrigin {
                first,
                second: index,
            });
        }
    }
    let mut documents = Vec::new();
    documents
        .try_reserve_exact(inputs.len())
        .map_err(NativeAuthoredMappingError::Allocation)?;
    for (index, input) in inputs.iter().enumerate() {
        input.compiled.check_request(request)?;
        let compiled = input.compiled;
        let parsed = parse_document(
            request,
            compiled.whole_source().as_bytes(),
            &compiled.recipe_policy().fallback_title,
            parser_limits,
        )
        .map_err(NativeAuthoredMappingError::Parser)?;
        let mut targets = Vec::new();
        for markdown in &parsed.markdown {
            compiled.check_request(request)?;
            let Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) = &markdown.event
            else {
                continue;
            };
            bounded(dest_url.len(), mapping_limits.max_uri_bytes, "URI bytes")?;
            let target = if let Some(relative) = relative_markdown_target(
                dest_url,
                *link_type,
                input.source_path,
                index,
                mapping_limits,
            )? {
                if let Some(target) = origins.get(&relative) {
                    Some(Cow::Borrowed(inputs[*target].slug))
                } else if let Some(target) = targets_by_path.get(&relative) {
                    Some(Cow::Borrowed(target.as_str()))
                } else {
                    // A missing selected FILE remains a dangling canonical path,
                    // never a filename stem or an alias guessed by the reader.
                    let identity = KnowledgeSourceIdentity::from_relative_path(
                        &relative,
                        None,
                        mapping_limits.max_field_bytes,
                    )
                    .map_err(NativeAuthoredMappingError::Identity)?;
                    Some(Cow::Owned(identity.slug().to_owned()))
                }
            } else if !matches!(link_type, LinkType::WikiLink { .. })
                && dest_url
                    .split(['?', '#'])
                    .next()
                    .unwrap_or("")
                    .ends_with(".md")
            {
                // An absolute, external or non-portable Markdown path is not a
                // local page identity. In particular, do not retry it as a stem.
                None
            } else {
                local_target(dest_url, *link_type, input.slug, index)?
            };
            let Some(target) = target else {
                continue;
            };
            bounded(
                target.len(),
                mapping_limits.max_target_bytes,
                "target bytes",
            )?;
            let target = match target {
                Cow::Owned(s) => s,
                Cow::Borrowed(s) => {
                    let mut owned = String::new();
                    owned
                        .try_reserve_exact(s.len())
                        .map_err(NativeAuthoredMappingError::Allocation)?;
                    owned.push_str(s);
                    owned
                }
            };
            targets
                .try_reserve(1)
                .map_err(NativeAuthoredMappingError::Allocation)?;
            targets.push(target);
        }
        let max_summary_bytes = usize::try_from(compiled.summary_limit())
            .ok()
            .and_then(NonZeroUsize::new)
            .ok_or(NativeAuthoredMappingError::InvalidSummaryLimit)?;
        let fingerprint = fingerprint_compiled(request, compiled, fingerprint_limits)
            .map_err(NativeAuthoredMappingError::Fingerprint)?;
        documents.push(MappedAuthoredDocument {
            input: *input,
            compilation_digest: CompilationDigest32::from_bytes(*fingerprint.as_bytes()),
            max_summary_bytes,
            targets,
        });
    }
    // These views are temporary and never become references inside the returned owner.
    let mut target_views = Vec::new();
    target_views
        .try_reserve_exact(documents.len())
        .map_err(NativeAuthoredMappingError::Allocation)?;
    for document in &documents {
        document.input.compiled.check_request(request)?;
        let mut group = Vec::new();
        group
            .try_reserve_exact(document.targets.len())
            .map_err(NativeAuthoredMappingError::Allocation)?;
        group.extend(document.targets.iter().map(String::as_str));
        target_views.push(group);
    }
    let mut pages = Vec::new();
    pages
        .try_reserve_exact(documents.len())
        .map_err(NativeAuthoredMappingError::Allocation)?;
    for (document, targets) in documents.iter().zip(&target_views) {
        document.input.compiled.check_request(request)?;
        pages.push(PageReferences {
            slug: document.input.slug,
            title: document.input.compiled.title(),
            targets,
        });
    }
    ReferenceGraph::build(request, &pages, reference_limits)
        .map_err(NativeAuthoredMappingError::References)?;
    for document in &documents {
        document.input.compiled.check_request(request)?;
    }
    request.check()?;
    Ok(PreparedAuthoredMapping { request, documents })
}

fn relative_origin(
    origin: &str,
    max_bytes: NonZeroUsize,
) -> Result<PathBuf, NativeAuthoredMappingError> {
    match decode_managed_knowledge_source_path(origin, max_bytes)
        .map_err(NativeAuthoredMappingError::Identity)?
    {
        Some(KnowledgeSourcePathComponents::Portable(parts)) => Ok(parts.iter().collect()),
        None => Ok(PathBuf::from(origin)),
    }
}

/// Decode a Markdown FILE destination relative to the authored source, without
/// opening anything. Wiki IDs, URLs and anchor-only destinations keep their
/// separate literal-ID contract. Parent traversal may not escape PROJECT.
pub(crate) fn relative_markdown_target(
    raw: &str,
    link_type: LinkType,
    origin: &str,
    document: usize,
    limits: NativeAuthoredMappingLimits,
) -> Result<Option<PathBuf>, NativeAuthoredMappingError> {
    bounded(raw.len(), limits.max_uri_bytes, "URI bytes")?;
    if matches!(link_type, LinkType::WikiLink { .. }) {
        return Ok(None);
    }
    let local = raw.split(['?', '#']).next().unwrap_or(raw);
    if local.is_empty() || has_scheme(local) || local.starts_with('/') || local.contains('\\') {
        return Ok(None);
    }
    let decoded = percent_decode_str(local)
        .decode_utf8()
        .map_err(NativeAuthoredMappingError::PercentUtf8)?;
    if !decoded.ends_with(".md")
        || has_scheme(&decoded)
        || decoded.starts_with('/')
        || decoded.contains('\\')
    {
        return Ok(None);
    }
    if decoded.chars().any(char::is_control) || decoded.rsplit('/').next() == Some(".md") {
        return Err(NativeAuthoredMappingError::InvalidTarget { document });
    }
    let mut relative = relative_origin(origin, limits.max_field_bytes)?;
    relative.pop();
    for component in decoded.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if !relative.pop() {
                    return Err(NativeAuthoredMappingError::InvalidTarget { document });
                }
            }
            name => relative.push(name),
        }
    }
    bounded(
        relative.as_os_str().len(),
        limits.max_field_bytes,
        "field bytes",
    )?;
    Ok(Some(relative))
}

pub(crate) fn local_target<'a>(
    raw: &'a str,
    link_type: LinkType,
    slug: &'a str,
    document: usize,
) -> Result<Option<Cow<'a, str>>, NativeAuthoredMappingError> {
    let local = raw.split(['?', '#']).next().unwrap_or(raw);
    if local.contains(['/', '\\']) || has_scheme(local) {
        return Ok(None);
    }
    if local.is_empty() {
        return Ok(Some(Cow::Borrowed(if raw.is_empty() { "" } else { slug })));
    }
    let target = if matches!(link_type, LinkType::WikiLink { .. }) {
        local
    } else if let Some(id) = local.strip_suffix(".md") {
        if id.is_empty() {
            return Err(NativeAuthoredMappingError::InvalidTarget { document });
        }
        id
    } else if Path::new(local).extension().is_none() {
        local
    } else {
        return Ok(None);
    };
    percent_decode_str(target)
        .decode_utf8()
        .map(Some)
        .map_err(NativeAuthoredMappingError::PercentUtf8)
}

fn has_scheme(text: &str) -> bool {
    let Some((scheme, _)) = text.split_once(':') else {
        return false;
    };
    let mut bytes = scheme.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}
fn valid_origin(origin: &str) -> bool {
    !origin.is_empty()
        && !origin.contains([':', '\\'])
        && !origin.chars().any(char::is_control)
        && origin
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
fn bounded(
    value: usize,
    limit: NonZeroUsize,
    kind: &'static str,
) -> Result<(), NativeAuthoredMappingError> {
    if value > limit.get() {
        Err(NativeAuthoredMappingError::LimitExceeded(kind))
    } else {
        Ok(())
    }
}
