//! Complete Knowledge selection and borrowed document preparation, without a Data effect.
use crate::{
    document::{
        DocumentParseError, DocumentParseLimits, MetadataKind, ParsedDocument, parse_document,
    },
    ignore_policy::{KnowledgeIgnorePolicy, KnowledgeIgnorePolicyError},
    native_document_mapping::{AuthoredDocumentInput, NativeAuthoredMappingLimits},
    publication_metadata::{
        CompilationMetadataError, CompiledDocument, PublicationMetadataLimits,
        PublicationMetadataPolicy, compile_captured_document,
    },
    settings::{KnowledgeSettings, KnowledgeSettingsError, KnowledgeSettingsLimits},
};
use graphit_auth::{AuthError, ProjectReadAdmission, ProjectReadModule};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, ProjectIgnoreCriteria,
        ProjectSelectionCriteria, ProjectSelectionLimits, ScopeBindingRecord, SourceReadLimits,
    },
    config::ConfigSnapshot,
    file_io::SnapshotContext,
    version_context::VersionContext,
};
use graphit_data::knowledge_document_lifecycle::{
    KnowledgeSourceIdentity, KnowledgeSourceIdentityError,
};
use graphit_fs::NodeKind;
use serde::{
    Deserialize, Deserializer,
    de::{self, Visitor},
};
use std::{
    collections::{HashMap, TryReserveError},
    error::Error,
    fmt, io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

#[derive(Clone, Copy)]
pub struct KnowledgeSourceCaptureLimits {
    pub settings: KnowledgeSettingsLimits,
    pub selection: ProjectSelectionLimits,
    pub read: SourceReadLimits,
}
#[derive(Clone, Copy)]
pub struct KnowledgeDesiredPreparationLimits {
    pub parser: DocumentParseLimits,
    pub metadata: PublicationMetadataLimits,
    pub mapping: NativeAuthoredMappingLimits,
}
#[derive(Debug)]
pub enum DesiredCorpusError {
    Auth(AuthError),
    Request(io::Error),
    /// The selected FILE is operation context, not proof of which member changed.
    Source {
        operation: &'static str,
        relative: Option<PathBuf>,
        cause: io::Error,
    },
    Settings(KnowledgeSettingsError),
    Ignore(KnowledgeIgnorePolicyError),
    Parser(DocumentParseError),
    Compilation(CompilationMetadataError),
    TypedId(serde_saphyr::Error),
    Identity(KnowledgeSourceIdentityError),
    WrongAdmission,
    DifferentRequest,
    ScopeChanged,
    InvalidId,
    MissingRetainedFile,
    DuplicateSlug {
        first: usize,
        second: usize,
    },
    DuplicateOrigin {
        first: usize,
        second: usize,
    },
    LimitExceeded(&'static str),
    Overflow,
    Allocation(TryReserveError),
    CaptureWorker(tokio::task::JoinError),
}
impl fmt::Display for DesiredCorpusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(_) => f.write_str("Knowledge desired corpus authorization failed"),
            Self::Request(_) => f.write_str("Knowledge desired corpus request is not live"),
            Self::Source {
                operation,
                relative,
                ..
            } => match relative {
                Some(relative) => write!(f, "Knowledge source {operation} failed at {relative:?}"),
                None => write!(f, "Knowledge source {operation} failed"),
            },
            Self::Settings(_) => f.write_str("Knowledge desired corpus configuration is invalid"),
            Self::Ignore(_) => f.write_str("Knowledge desired corpus ignore policy failed"),
            Self::Parser(_) => f.write_str("Knowledge desired corpus document parsing failed"),
            Self::Compilation(_) => {
                f.write_str("Knowledge desired corpus metadata compilation failed")
            }
            Self::TypedId(_) | Self::InvalidId => {
                f.write_str("Knowledge authored id must be a literal string")
            }
            Self::Identity(_) => f.write_str("Knowledge desired corpus source identity is invalid"),
            Self::WrongAdmission => f.write_str("Knowledge read admission is required"),
            Self::DifferentRequest => {
                f.write_str("Knowledge desired corpus belongs to another request")
            }
            Self::ScopeChanged => {
                f.write_str("Knowledge desired corpus belongs to another project selection")
            }
            Self::MissingRetainedFile => f.write_str("Knowledge selected file was not retained"),
            Self::DuplicateSlug { first, second } => write!(
                f,
                "duplicate Knowledge slug at documents {first} and {second}"
            ),
            Self::DuplicateOrigin { first, second } => write!(
                f,
                "duplicate Knowledge origin at documents {first} and {second}"
            ),
            Self::LimitExceeded(field) => write!(f, "Knowledge desired corpus exceeds {field}"),
            Self::Overflow => f.write_str("Knowledge desired corpus input arithmetic overflow"),
            Self::Allocation(_) => f.write_str("Knowledge desired corpus allocation failed"),
            Self::CaptureWorker(_) => f.write_str("Knowledge source worker failed"),
        }
    }
}
impl Error for DesiredCorpusError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Auth(e) => Some(e),
            Self::Request(e) | Self::Source { cause: e, .. } => Some(e),
            Self::Settings(e) => Some(e),
            Self::Ignore(e) => Some(e),
            Self::Parser(e) => Some(e),
            Self::Compilation(e) => Some(e),
            Self::TypedId(e) => Some(e),
            Self::Identity(e) => Some(e),
            Self::Allocation(e) => Some(e),
            Self::CaptureWorker(e) => Some(e),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, DesiredCorpusError>;

pub struct CapturedKnowledgeSources {
    reader: CapturedSourceReader,
    original: Arc<SnapshotContext>,
    binding: ScopeBindingRecord,
    version: VersionContext,
    settings: Arc<KnowledgeSettings>,
}
impl fmt::Debug for CapturedKnowledgeSources {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapturedKnowledgeSources")
            .finish_non_exhaustive()
    }
}
// Auth can observe Request between the Original check and an accessor. Only
// an actual error of that same Original supplies a concrete I/O cause.
pub(crate) fn request_after_auth(
    original: &SnapshotContext,
    cause: &AuthError,
) -> Option<io::Error> {
    if matches!(cause, AuthError::Request) {
        original.check().err()
    } else {
        None
    }
}
fn auth_error(original: &SnapshotContext, cause: AuthError) -> DesiredCorpusError {
    match request_after_auth(original, &cause) {
        Some(request) => DesiredCorpusError::Request(request),
        None => DesiredCorpusError::Auth(cause),
    }
}

fn check_read(original: &Arc<SnapshotContext>, read: &ProjectReadAdmission<'_>) -> Result<()> {
    original.check().map_err(DesiredCorpusError::Request)?;
    if read.module() != ProjectReadModule::Knowledge {
        return Err(DesiredCorpusError::WrongAdmission);
    }
    if !Arc::ptr_eq(
        original,
        read.request()
            .map_err(|cause| auth_error(original, cause))?
            .snapshot(),
    ) {
        return Err(DesiredCorpusError::DifferentRequest);
    }
    read.recheck().map_err(|cause| auth_error(original, cause))
}
impl CapturedKnowledgeSources {
    pub(crate) fn check(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<()> {
        check_read(original, read)?;
        if !Arc::ptr_eq(&self.original, original) {
            return Err(DesiredCorpusError::DifferentRequest);
        }
        let project = read
            .project()
            .map_err(|cause| auth_error(original, cause))?;
        if &self.binding != project.scope.record
            || &self.version != project.version
            || !self.binding.matches_scope(project.scope.live)
        {
            return Err(DesiredCorpusError::ScopeChanged);
        }
        self.reader
            .check_request(original)
            .map_err(DesiredCorpusError::Request)?;
        Ok(())
    }
    pub fn revalidate(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<()> {
        self.check(original, read)?;
        self.reader
            .revalidate_content()
            .map_err(|cause| DesiredCorpusError::Source {
                operation: "selection_revalidate",
                relative: None,
                cause,
            })?;
        self.check(original, read)
    }
    /// Frozen ordinary criteria; these values grant no access.
    pub fn settings(&self) -> &KnowledgeSettings {
        &self.settings
    }
    pub fn reader<'view>(
        &'view self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<&'view CapturedSourceReader> {
        self.revalidate(original, read)?;
        self.checked_reader(original, read)
    }
    pub(crate) fn checked_reader<'view>(
        &'view self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<&'view CapturedSourceReader> {
        self.check(original, read)?;
        Ok(&self.reader)
    }
}
fn borrowed_texts(values: &[String]) -> Result<Vec<&str>> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(values.len())
        .map_err(DesiredCorpusError::Allocation)?;
    output.extend(values.iter().map(String::as_str));
    Ok(output)
}
/// Capture the complete configured selection through the real Core PROJECT producer.
pub fn capture_knowledge_sources(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    config: &ConfigSnapshot,
    index_path: Option<&str>,
    limits: KnowledgeSourceCaptureLimits,
) -> Result<CapturedKnowledgeSources> {
    capture_sources(original, read, config, index_path, limits, false)
}

/// Capture selection metadata without retaining the selected corpus bodies.
/// Physical hashing and subsequent revalidation remain Core responsibilities.
pub fn capture_knowledge_source_metadata(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    config: &ConfigSnapshot,
    index_path: Option<&str>,
    limits: KnowledgeSourceCaptureLimits,
) -> Result<CapturedKnowledgeSources> {
    capture_sources(original, read, config, index_path, limits, true)
}

// Frozen by the admitted caller; physical scope and Original are genuine owners.
struct SourceCaptureWork {
    scope: CapturedPublicationScope,
    original: Arc<SnapshotContext>,
    binding: ScopeBindingRecord,
    version: VersionContext,
    settings: Arc<KnowledgeSettings>,
    ignore: KnowledgeIgnorePolicy,
    limits: KnowledgeSourceCaptureLimits,
}
fn source_capture_work(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    config: &ConfigSnapshot,
    index_path: Option<&str>,
    limits: KnowledgeSourceCaptureLimits,
) -> Result<SourceCaptureWork> {
    check_read(original, read)?;
    let project = read
        .project()
        .map_err(|cause| auth_error(original, cause))?;
    let settings = KnowledgeSettings::resolve(original, config, index_path, limits.settings)
        .map_err(DesiredCorpusError::Settings)?;
    let ignore =
        KnowledgeIgnorePolicy::resolve(original, config).map_err(DesiredCorpusError::Ignore)?;
    check_read(original, read)?;
    Ok(SourceCaptureWork {
        scope: project.scope.live.clone(),
        original: Arc::clone(original),
        binding: project.scope.record.clone(),
        version: project.version.clone(),
        settings: Arc::new(settings),
        ignore,
        limits,
    })
}
impl SourceCaptureWork {
    fn capture(
        &self,
        metadata_only: bool,
        changes: Option<&std::collections::BTreeSet<PathBuf>>,
    ) -> Result<CapturedKnowledgeSources> {
        self.original.check().map_err(DesiredCorpusError::Request)?;
        let extensions = borrowed_texts(self.settings.extensions())?;
        let defaults = borrowed_texts(self.ignore.patterns())?;
        let criteria = ProjectSelectionCriteria {
            subtree: Some(Path::new(self.settings.docs_dir())),
            extras: &[],
            include_readme: self.settings.include_readme(),
            extensions: &extensions,
            ignore: ProjectIgnoreCriteria {
                protected_defaults: &defaults,
                custom_control: Some(self.ignore.custom_filename()),
                git_control: Some(self.ignore.git_filename()),
            },
        };
        let reader = if let Some(changes) = changes {
            CapturedSourceReader::capture_selected_project_changes_metadata(
                &self.scope,
                criteria,
                changes,
                self.limits.selection,
                self.limits.read,
                &self.original,
            )
        } else {
            let capture = if metadata_only {
                CapturedSourceReader::capture_selected_project_metadata
            } else {
                CapturedSourceReader::capture_selected_project
            };
            capture(
                &self.scope,
                criteria,
                self.limits.selection,
                self.limits.read,
                &self.original,
            )
        }
        .map_err(|cause| DesiredCorpusError::Source {
            operation: "selection_capture",
            relative: None,
            cause,
        })?;
        reader
            .revalidate_content()
            .map_err(|cause| DesiredCorpusError::Source {
                operation: "selection_revalidate_capture",
                relative: None,
                cause,
            })?;
        self.original.check().map_err(DesiredCorpusError::Request)?;
        Ok(CapturedKnowledgeSources {
            reader,
            original: Arc::clone(&self.original),
            binding: self.binding.clone(),
            version: self.version.clone(),
            settings: Arc::clone(&self.settings),
        })
    }
}
fn capture_sources(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    config: &ConfigSnapshot,
    index_path: Option<&str>,
    limits: KnowledgeSourceCaptureLimits,
    metadata_only: bool,
) -> Result<CapturedKnowledgeSources> {
    let sources = source_capture_work(original, read, config, index_path, limits)?
        .capture(metadata_only, None)?;
    sources.check(original, read)?;
    Ok(sources)
}
/// Frozen selection criteria and FILE names; payloads are captured only when used.
#[derive(Clone)]
pub(crate) struct KnowledgeSourceSelection {
    work: Arc<SourceCaptureWork>,
    pub(crate) paths: Vec<PathBuf>,
    changes: Option<std::collections::BTreeSet<PathBuf>>,
}
impl SourceCaptureWork {
    fn paths(&self, changes: Option<&std::collections::BTreeSet<PathBuf>>) -> Result<Vec<PathBuf>> {
        let extensions = borrowed_texts(self.settings.extensions())?;
        let defaults = borrowed_texts(self.ignore.patterns())?;
        let criteria = ProjectSelectionCriteria {
            subtree: Some(Path::new(self.settings.docs_dir())),
            extras: &[],
            include_readme: self.settings.include_readme(),
            extensions: &extensions,
            ignore: ProjectIgnoreCriteria {
                protected_defaults: &defaults,
                custom_control: Some(self.ignore.custom_filename()),
                git_control: Some(self.ignore.git_filename()),
            },
        };
        match changes {
            Some(changes) => graphit_core::artifact_store::selected_project_changed_paths(
                &self.scope,
                criteria,
                changes,
                self.limits.selection,
                &self.original,
            ),
            None => graphit_core::artifact_store::selected_project_paths(
                &self.scope,
                criteria,
                self.limits.selection,
                &self.original,
            ),
        }
        .map_err(|cause| DesiredCorpusError::Source {
            operation: "selection_discover",
            relative: None,
            cause,
        })
    }
}
pub(crate) async fn capture_selection_worker(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    config: &ConfigSnapshot,
    changes: Option<&std::collections::BTreeSet<PathBuf>>,
    limits: KnowledgeSourceCaptureLimits,
) -> Result<KnowledgeSourceSelection> {
    let work = Arc::new(source_capture_work(original, read, config, None, limits)?);
    let captured = Arc::clone(&work);
    let changes = changes.cloned();
    let selected = changes.clone();
    let paths = tokio::task::spawn_blocking(move || captured.paths(selected.as_ref()))
        .await
        .map_err(DesiredCorpusError::CaptureWorker)??;
    check_read(original, read)?;
    Ok(KnowledgeSourceSelection {
        work,
        paths,
        changes,
    })
}
impl KnowledgeSourceSelection {
    /// Capture only requested destinations through the same configured PROJECT
    /// criteria as the source. A link never bypasses ignores or source bounds.
    pub(crate) fn capture_targets(
        &self,
        paths: &std::collections::BTreeSet<PathBuf>,
    ) -> Result<CapturedKnowledgeSources> {
        self.work.capture(true, Some(paths))
    }

    pub(crate) async fn capture_worker(
        &self,
        read: &ProjectReadAdmission<'_>,
        paths: &std::collections::BTreeSet<PathBuf>,
    ) -> Result<Arc<CapturedKnowledgeSources>> {
        let work = Arc::clone(&self.work);
        let paths = paths.clone();
        let sources = tokio::task::spawn_blocking(move || work.capture(true, Some(&paths)))
            .await
            .map_err(DesiredCorpusError::CaptureWorker)??;
        sources.check(&self.work.original, read)?;
        Ok(Arc::new(sources))
    }
    pub(crate) async fn revalidate_worker(&self) -> Result<()> {
        let work = Arc::clone(&self.work);
        let changes = self.changes.clone();
        let paths = tokio::task::spawn_blocking(move || work.paths(changes.as_ref()))
            .await
            .map_err(DesiredCorpusError::CaptureWorker)??;
        if paths != self.paths {
            return Err(DesiredCorpusError::Source {
                operation: "selection_revalidate_names",
                relative: None,
                cause: io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Knowledge selection changed before EOF",
                ),
            });
        }
        Ok(())
    }
}
impl CapturedKnowledgeSources {
    /// Physical owner only; the caller still holds and rechecks its admitted read.
    pub(crate) fn worker_reader(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<&CapturedSourceReader> {
        original.check().map_err(DesiredCorpusError::Request)?;
        if !Arc::ptr_eq(original, &self.original) {
            return Err(DesiredCorpusError::DifferentRequest);
        }
        self.reader
            .check_request(original)
            .map_err(DesiredCorpusError::Request)?;
        Ok(&self.reader)
    }
}

pub struct DesiredKnowledgeDocument<'source> {
    relative: PathBuf,
    identity: KnowledgeSourceIdentity,
    compiled: CompiledDocument<'source>,
}
impl<'source> DesiredKnowledgeDocument<'source> {
    pub fn relative(&self) -> &Path {
        &self.relative
    }
    pub fn slug(&self) -> &str {
        self.identity.slug()
    }
    pub fn source_path(&self) -> &str {
        self.identity.source_path()
    }
    pub fn compiled(&self) -> &CompiledDocument<'source> {
        &self.compiled
    }
}
pub struct DesiredKnowledgeCorpus<'source> {
    sources: &'source CapturedKnowledgeSources,
    documents: Vec<DesiredKnowledgeDocument<'source>>,
}
impl fmt::Debug for DesiredKnowledgeCorpus<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DesiredKnowledgeCorpus")
            .field("documents", &self.documents.len())
            .finish_non_exhaustive()
    }
}
impl<'source> DesiredKnowledgeCorpus<'source> {
    pub fn revalidate(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<()> {
        self.sources.revalidate(original, read)
    }
    pub fn documents<'view>(
        &'view self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<&'view [DesiredKnowledgeDocument<'source>]> {
        self.revalidate(original, read)?;
        Ok(&self.documents)
    }
    pub fn authored_inputs<'view>(
        &'view self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        updated_at: Option<SystemTime>,
        stale_after: Option<Duration>,
    ) -> Result<Vec<AuthoredDocumentInput<'view, 'source>>> {
        self.revalidate(original, read)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(self.documents.len())
            .map_err(DesiredCorpusError::Allocation)?;
        for document in &self.documents {
            self.sources.check(original, read)?;
            output.push(AuthoredDocumentInput {
                compiled: &document.compiled,
                slug: document.slug(),
                source_path: document.source_path(),
                updated_at,
                stale_since: None,
                stale_after,
            });
        }
        self.revalidate(original, read)?;
        Ok(output)
    }
}
pub(crate) fn bound(bytes: usize, max: NonZeroUsize, label: &'static str) -> Result<()> {
    if bytes > max.get() {
        return Err(DesiredCorpusError::LimitExceeded(label));
    }
    Ok(())
}
pub(crate) fn copy_text(text: &str) -> Result<String> {
    let mut result = String::new();
    result
        .try_reserve_exact(text.len())
        .map_err(DesiredCorpusError::Allocation)?;
    result.push_str(text);
    Ok(result)
}
fn copy_path(path: &Path) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    result
        .try_reserve_exact(path.as_os_str().len())
        .map_err(DesiredCorpusError::Allocation)?;
    result.push(path);
    Ok(result)
}
// String-only semantic validation. The arena supplies the literal text; no second owner is needed.
struct StringType;
impl<'de> Deserialize<'de> for StringType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct StringVisitor;
        impl<'de> Visitor<'de> for StringVisitor {
            type Value = StringType;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a YAML string")
            }
            fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<Self::Value, E> {
                Ok(StringType)
            }
            fn visit_string<E: de::Error>(self, _: String) -> std::result::Result<Self::Value, E> {
                Ok(StringType)
            }
        }
        d.deserialize_any(StringVisitor)
    }
}
#[derive(Deserialize)]
struct IdTypeProbe {
    #[serde(rename = "id")]
    _id: StringType,
}
fn id_options(limits: DocumentParseLimits) -> serde_saphyr::Options {
    let budget = serde_saphyr::budget! {
        max_reader_input_bytes: None, max_buffered_comment_events: 0,
        simple_key_max_lookahead: limits.max_frontmatter_bytes.get(),
        flow_nesting_limit: limits.max_yaml_depth.get(), max_events: limits.max_yaml_events.get(),
        max_aliases: 0, max_anchors: 0, max_recorded_anchor_events: 0, max_recorded_anchor_bytes: 0,
        max_property_expansion_depth: 0, max_total_property_interpolation_work: 0,
        max_depth: limits.max_yaml_depth.get(), max_inclusion_depth: 0, max_documents: 1,
        max_nodes: limits.max_yaml_events.get(), max_total_scalar_bytes: limits.max_projection_bytes.get(),
        max_total_comment_bytes: 0, max_merge_keys: 0,
    };
    serde_saphyr::options! {
        budget: budget, no_schema: false, strict_booleans: false, legacy_octal_numbers: false,
        merge_keys: serde_saphyr::options::MergeKeyPolicy::Error,
        duplicate_keys: serde_saphyr::options::DuplicateKeyPolicy::FirstWins,
        emit_comments: false, with_snippet: false, reject_unsupported_tags: true,
        reject_non_finite_typeless_float: true,
    }
}
pub(crate) fn authored_id<'view>(
    parsed: &'view ParsedDocument<'_>,
    limits: DocumentParseLimits,
) -> Result<Option<&'view str>> {
    let Some(root) = parsed.metadata.root else {
        return Ok(None);
    };
    let root = parsed
        .metadata
        .node(root)
        .ok_or(DesiredCorpusError::InvalidId)?;
    let MetadataKind::Mapping { entries } = &root.kind else {
        return Err(DesiredCorpusError::InvalidId);
    };
    for (key, value_id) in entries {
        let key = parsed
            .metadata
            .node(*key)
            .ok_or(DesiredCorpusError::InvalidId)?;
        if !matches!(&key.kind, MetadataKind::Scalar { value, .. } if value == "id") {
            continue;
        }
        let node = parsed
            .metadata
            .node(*value_id)
            .ok_or(DesiredCorpusError::InvalidId)?;
        let MetadataKind::Scalar { value, .. } = &node.kind else {
            return Err(DesiredCorpusError::InvalidId);
        };
        // Keep the original field separator/properties and mapping context.
        // A strict probe of the whole mapping would reject unrelated .nan values;
        // a scalar-only probe would reinterpret id: --- as a document marker.
        let yaml = parsed
            .envelope
            .yaml
            .as_ref()
            .ok_or(DesiredCorpusError::InvalidId)?;
        if key.span.end < yaml.start || node.span.end > yaml.end {
            return Err(DesiredCorpusError::InvalidId);
        }
        let field = parsed
            .source
            .get(key.span.end..node.span.end)
            .ok_or(DesiredCorpusError::InvalidId)?;
        let indent_bytes = field
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            .checked_mul(2)
            .ok_or(DesiredCorpusError::Overflow)?;
        let size = field
            .len()
            .checked_add(indent_bytes)
            .and_then(|size| size.checked_add(3))
            .ok_or(DesiredCorpusError::Overflow)?;
        let mut probe = String::new();
        probe
            .try_reserve_exact(size)
            .map_err(DesiredCorpusError::Allocation)?;
        probe.push_str("id");
        for line in field.split_inclusive('\n') {
            probe.push_str(line);
            if line.ends_with('\n') {
                probe.push_str("  ");
            }
        }
        probe.push('\n');
        serde_saphyr::from_str_with_options::<IdTypeProbe>(&probe, id_options(limits))
            .map_err(DesiredCorpusError::TypedId)?;
        return Ok(Some(value.as_ref()));
    }
    Ok(None)
}
/// Prepare every selected FILE before returning a complete borrowed corpus.
pub fn prepare_desired_knowledge_corpus<'source>(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    sources: &'source CapturedKnowledgeSources,
    kind_on_absent: &str,
    limits: KnowledgeDesiredPreparationLimits,
) -> Result<DesiredKnowledgeCorpus<'source>> {
    sources.revalidate(original, read)?;
    let count = sources
        .reader
        .nodes()
        .filter(|node| node.kind() == NodeKind::File)
        .count();
    bound(
        kind_on_absent.len(),
        limits.mapping.max_field_bytes,
        "field bytes",
    )?;
    let mut documents = Vec::new();
    documents
        .try_reserve_exact(count)
        .map_err(DesiredCorpusError::Allocation)?;
    let mut slugs = HashMap::new();
    slugs
        .try_reserve(count)
        .map_err(DesiredCorpusError::Allocation)?;
    let mut origins = HashMap::new();
    origins
        .try_reserve(count)
        .map_err(DesiredCorpusError::Allocation)?;
    for node in sources
        .reader
        .nodes()
        .filter(|node| node.kind() == NodeKind::File)
    {
        sources.check(original, read)?;
        let relative = node.relative();
        let bytes = sources
            .reader
            .retained(relative)
            .ok_or(DesiredCorpusError::MissingRetainedFile)?;
        let provisional_title = relative
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        let parsed = parse_document(original, bytes, provisional_title, limits.parser)
            .map_err(DesiredCorpusError::Parser)?;
        let identity = KnowledgeSourceIdentity::from_relative_path(
            relative,
            authored_id(&parsed, limits.parser)?,
            limits.mapping.max_field_bytes,
        )
        .map_err(DesiredCorpusError::Identity)?;
        let fallback = relative
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(identity.slug());
        bound(
            fallback.len(),
            limits.mapping.max_field_bytes,
            "field bytes",
        )?;
        let compiled = compile_captured_document(
            &sources.reader,
            relative,
            original,
            PublicationMetadataPolicy {
                fallback_title: copy_text(fallback)?,
                kind_on_absent: copy_text(kind_on_absent)?,
            },
            limits.parser,
            limits.metadata,
        )
        .map_err(DesiredCorpusError::Compilation)?;
        for field in [
            identity.slug(),
            identity.source_path(),
            compiled.whole_source(),
            compiled.title(),
            compiled.kind(),
            compiled.summary(),
            compiled.description().unwrap_or(""),
            &compiled.recipe_policy().fallback_title,
            &compiled.recipe_policy().kind_on_absent,
        ] {
            bound(field.len(), limits.mapping.max_field_bytes, "field bytes")?;
        }
        let second = documents.len();
        if let Some(first) = slugs.insert(copy_text(identity.slug())?, second) {
            return Err(DesiredCorpusError::DuplicateSlug { first, second });
        }
        if let Some(first) = origins.insert(copy_text(identity.source_path())?, second) {
            return Err(DesiredCorpusError::DuplicateOrigin { first, second });
        }
        documents.push(DesiredKnowledgeDocument {
            relative: copy_path(relative)?,
            identity,
            compiled,
        });
    }
    let corpus = DesiredKnowledgeCorpus { sources, documents };
    corpus.revalidate(original, read)?;
    Ok(corpus)
}
