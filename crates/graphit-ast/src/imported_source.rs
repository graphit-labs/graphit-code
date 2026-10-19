#![doc = include_str!("../../../docs/specs/rust_imported_source.md")]

use crate::{
    indexed_source::{IndexedSourceError, read_indexed_source, source_schema},
    source_inspection::{
        IndexedSource, PhysicalSource, SourceInspection, SourceInspectionError, inspect_source,
    },
};
use graphit_auth::{
    AuthError, AuthenticatedCaller, ImportedAstReadAdmission, Permission, ProjectCapture,
    RoleIssuer, ScopeCapture,
};
use graphit_core::{
    artifact_store::{
        CapturedSourceReader, RetainedBytesLimits, RetainedFileSet, ScopeContext, SnapshotPolicy,
        SourceReadLimits, StoreOwner,
    },
    file_io::SnapshotContext,
    module_settings::Module,
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    text_slice::Request,
};
use graphit_data::lance_module_store::{LanceModuleStore, ModuleSnapshot, StoreError};
use graphit_fs::NodeKind;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
    policy::RequestContext,
    registry::{HubRegistry, HubScope, InstalledBackend, InstalledPayload},
};
use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Captures and registry must belong to the issuer's actual installation.
/// The fallback is independently resolved by Runtime, including its version.
pub struct ImportedSourceInput<'a, 'r> {
    pub registry: &'r HubRegistry,
    pub id: &'r ArtifactId,
    pub version: &'r ConcreteVersion,
    pub installation: ScopeCapture<'a>,
    pub fallback: Option<ProjectCapture<'a>>,
    pub requested_relative: &'r Path,
    pub selection: &'r Request,
    pub file_policy: &'r SnapshotPolicy,
    pub read_limits: SourceReadLimits,
    pub path_limits: RetainedBytesLimits,
    pub index_limits: NativeDocumentFileLimits,
    pub request: &'a RequestContext<'a>,
}

#[derive(Debug)]
pub enum ImportedSourceError {
    Authentication(AuthError),
    InvalidInstallation,
    InstallationNotFound,
    InvalidPath,
    InvalidOrigin,
    InvalidFallback,
    SourceUnavailable,
    Operation(io::Error),
    Storage(StoreError),
    Indexed(IndexedSourceError),
    Inspection(SourceInspectionError),
}
impl fmt::Display for ImportedSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authentication(e) => write!(f, "imported Source authentication: {e}"),
            Self::InvalidInstallation => {
                f.write_str("invalid imported Source installation capture")
            }
            Self::InstallationNotFound => {
                f.write_str("requested native AST installation not found")
            }
            Self::InvalidPath => f.write_str("invalid imported Source file path"),
            Self::InvalidOrigin => {
                f.write_str("imported Source origin is not the captured native directory")
            }
            Self::InvalidFallback => {
                f.write_str("fallback does not match the independently resolved producer project")
            }
            Self::SourceUnavailable => {
                f.write_str("imported Source text unavailable without a project fallback")
            }
            Self::Operation(e) => write!(f, "imported Source operation: {e}"),
            Self::Storage(e) => write!(f, "imported Source storage: {e}"),
            Self::Indexed(e) => write!(f, "imported Source indexed text: {e}"),
            Self::Inspection(e) => write!(f, "imported Source inspection: {e}"),
        }
    }
}
impl Error for ImportedSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Authentication(e) => Some(e),
            Self::Operation(e) => Some(e),
            Self::Storage(e) => Some(e),
            Self::Indexed(e) => Some(e),
            Self::Inspection(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for ImportedSourceError {
    fn from(e: io::Error) -> Self {
        Self::Operation(e)
    }
}

fn validate_installation(
    input: &ImportedSourceInput<'_, '_>,
    original: &SnapshotContext,
) -> Result<(), ImportedSourceError> {
    original.check()?;
    let StoreOwner::Installation { installation_id } = input.installation.live.owner() else {
        return Err(ImportedSourceError::InvalidInstallation);
    };
    if !matches!(input.registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
        || input.registry.path().parent() != Some(input.installation.live.owner_root())
    {
        return Err(ImportedSourceError::InvalidInstallation);
    }
    let context = ScopeContext::from_snapshot(original);
    if !input
        .installation
        .record
        .matches_scope_bounded(input.installation.live, &context)?
    {
        return Err(ImportedSourceError::InvalidInstallation);
    }
    input.installation.live.revalidate_installation(&context)?;
    original.check()?;
    Ok(())
}

fn observe_directory(path: &Path) -> Result<(), ImportedSourceError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ImportedSourceError::InvalidOrigin);
    }
    Ok(())
}
fn capture_directory(
    path: &Path,
    limits: NativeDocumentFileLimits,
    original: &SnapshotContext,
) -> Result<CapturedDocumentDirectory, ImportedSourceError> {
    original.check()?;
    observe_directory(path)?;
    let directory = CapturedDocumentDirectory::capture(path, limits, original)?;
    if directory.canonical_path() != path {
        return Err(ImportedSourceError::InvalidOrigin);
    }
    original.check()?;
    Ok(directory)
}

struct NativeOrigin {
    root: CapturedDocumentDirectory,
    graph: CapturedDocumentDirectory,
    path: PathBuf,
    corpus: Option<CapturedDocumentDirectory>,
}
impl NativeOrigin {
    fn capture(
        source: &SourceRef,
        input: &ImportedSourceInput<'_, '_>,
        original: &SnapshotContext,
    ) -> Result<Self, ImportedSourceError> {
        let root = capture_directory(source.directory().as_path(), input.index_limits, original)?;
        let graph = capture_directory(
            &root.canonical_path().join("graph.icebug"),
            input.index_limits,
            original,
        )?;
        root.revalidate(original)?;
        let path = graph.canonical_path().join("source.lance");
        let corpus = match fs::symlink_metadata(&path) {
            Ok(_) => Some(capture_directory(&path, input.index_limits, original)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let result = Self {
            root,
            graph,
            path,
            corpus,
        };
        result.revalidate(original)?;
        Ok(result)
    }
    fn revalidate(&self, original: &SnapshotContext) -> Result<(), ImportedSourceError> {
        original.check()?;
        observe_directory(self.root.canonical_path())?;
        self.root.revalidate(original)?;
        observe_directory(self.graph.canonical_path())?;
        self.graph.revalidate(original)?;
        match &self.corpus {
            Some(directory) => {
                observe_directory(&self.path)?;
                directory.revalidate(original)?;
            }
            None => match fs::symlink_metadata(&self.path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
                Ok(_) => return Err(ImportedSourceError::InvalidOrigin),
            },
        }
        original.check()?;
        Ok(())
    }
    async fn open(
        &self,
        original: &SnapshotContext,
    ) -> Result<Option<ModuleSnapshot>, ImportedSourceError> {
        self.revalidate(original)?;
        if self.corpus.is_none() {
            return Ok(None);
        }
        let uri = self
            .path
            .to_str()
            .ok_or(ImportedSourceError::InvalidOrigin)?;
        let opened = LanceModuleStore::open(uri, source_schema()).await;
        self.revalidate(original)?;
        Ok(Some(
            opened.map_err(ImportedSourceError::Storage)?.snapshot(),
        ))
    }
}

fn validate_fallback(
    project: ProjectCapture<'_>,
    source: &SourceRef,
    input: &ImportedSourceInput<'_, '_>,
    original: &SnapshotContext,
) -> Result<(), ImportedSourceError> {
    original.check()?;
    if !matches!(project.scope.live.owner(), StoreOwner::Project { project_id } if *project_id == source.producer())
        || project.scope.live.brand() != input.installation.live.brand()
        || project.version.scope() != project.scope.live.owner_root()
    {
        return Err(ImportedSourceError::InvalidFallback);
    }
    let context = ScopeContext::from_snapshot(original);
    if !project
        .scope
        .record
        .matches_scope_bounded(project.scope.live, &context)?
    {
        return Err(ImportedSourceError::InvalidFallback);
    }
    project.scope.live.revalidate_bounded(&context)?;
    original.check()?;
    Ok(())
}

enum Authority<'a> {
    Native(Box<AuthenticatedCaller<'a>>),
    Web(&'a ImportedAstReadAdmission<'a>),
}

async fn read_authorized(
    authority: Authority<'_>,
    input: ImportedSourceInput<'_, '_>,
    entity_window: Option<(usize, usize)>,
) -> Result<SourceInspection, ImportedSourceError> {
    let authorize = || {
        match &authority {
            Authority::Native(caller) => {
                caller.authorize(Permission::Read(Module::Ast)).map(|_| ())
            }
            Authority::Web(read) => read.recheck(),
        }
        .map_err(ImportedSourceError::Authentication)
    };
    authorize()?;
    let original: &Arc<SnapshotContext> = input.request.snapshot();
    validate_installation(&input, original)?;
    if input.requested_relative.as_os_str().is_empty() {
        return Err(ImportedSourceError::InvalidPath);
    }
    let _paths = RetainedFileSet::new(
        std::iter::once(input.requested_relative.to_path_buf()),
        input.path_limits,
    )?;
    let key = input
        .requested_relative
        .to_str()
        .ok_or(ImportedSourceError::InvalidPath)?;
    let document = input.registry.load()?;
    validate_installation(&input, original)?;
    let source = document
        .get_version(ArtifactType::Ast, input.id.as_str(), input.version)
        .and_then(|artifact| {
            artifact.installations.iter().find(|installation| {
                installation.target == InstallationTarget::GlobalNative
                    && installation.backend == InstalledBackend::Native
                    && matches!(installation.payload, InstalledPayload::Original { .. })
                    && installation.original_source.version() == input.version
            })
        })
        .ok_or(ImportedSourceError::InstallationNotFound)?
        .original_source
        .clone();
    let origin = NativeOrigin::capture(&source, &input, original)?;
    validate_installation(&input, original)?;
    let snapshot = origin.open(original).await?;
    validate_installation(&input, original)?;
    let indexed = if let Some(snapshot) = &snapshot {
        let result = read_indexed_source(snapshot, key, original).await;
        origin.revalidate(original)?;
        validate_installation(&input, original)?;
        Some(result.map_err(ImportedSourceError::Indexed)?)
    } else {
        None
    };
    let raw = match &indexed {
        Some(indexed) => indexed
            .source(original)
            .map_err(ImportedSourceError::Indexed)?,
        None => IndexedSource::Missing,
    };
    let physical = if matches!(raw, IndexedSource::Missing) {
        let project = input
            .fallback
            .ok_or(ImportedSourceError::SourceUnavailable)?;
        authorize()?;
        validate_fallback(project, &source, &input, original)?;
        Some(CapturedSourceReader::capture_project(
            project.scope.live,
            input.requested_relative,
            NodeKind::File,
            input.file_policy,
            input.read_limits,
            original,
        )?)
    } else {
        None
    };
    let result = if let Some((start, end)) = entity_window {
        let IndexedSource::Available(text) = raw else {
            return Err(ImportedSourceError::SourceUnavailable);
        };
        crate::entity_source::select_entity_source(text, start, end, input.selection, original).map(
            |selection| SourceInspection {
                origin: crate::source_inspection::SourceOrigin::Indexed,
                selection,
            },
        )
    } else {
        inspect_source(
            raw,
            physical.as_ref().map(|reader| PhysicalSource {
                reader,
                relative: Path::new(""),
            }),
            input.selection,
            original,
        )
    };
    if let Some(reader) = &physical {
        validate_fallback(
            input.fallback.expect("captured fallback"),
            &source,
            &input,
            original,
        )?;
        reader.revalidate_content()?;
        reader.check_request(original)?;
    }
    origin.revalidate(original)?;
    validate_installation(&input, original)?;
    authorize()?;
    original.check()?;
    result.map_err(ImportedSourceError::Inspection)
}

pub async fn inspect_imported_source_runtime<'a>(
    issuer: &'a RoleIssuer,
    raw: &str,
    input: ImportedSourceInput<'a, '_>,
) -> Result<SourceInspection, ImportedSourceError> {
    let caller = issuer
        .authenticate_runtime(raw, input.request)
        .map_err(ImportedSourceError::Authentication)?;
    read_authorized(Authority::Native(Box::new(caller)), input, None).await
}
pub async fn inspect_imported_source_local<'a>(
    issuer: &'a RoleIssuer,
    raw: &str,
    input: ImportedSourceInput<'a, '_>,
) -> Result<SourceInspection, ImportedSourceError> {
    let caller = issuer
        .authenticate_local(raw, input.request)
        .map_err(ImportedSourceError::Authentication)?;
    read_authorized(Authority::Native(Box::new(caller)), input, None).await
}

/// Inspect only the exact import's original Source using its verified Web grant.
/// A selected PROJECT is never inferred as a fallback for an imported artifact.
pub async fn inspect_imported_source_web<'a>(
    read: &'a ImportedAstReadAdmission<'a>,
    input: ImportedSourceInput<'a, '_>,
    entity_window: Option<(usize, usize)>,
) -> Result<SourceInspection, ImportedSourceError> {
    if input.fallback.is_some() {
        return Err(ImportedSourceError::InvalidFallback);
    }
    if !Arc::ptr_eq(read.request().snapshot(), input.request.snapshot())
        || !std::ptr::eq(read.installation().live, input.installation.live)
        || !std::ptr::eq(read.installation().record, input.installation.record)
    {
        return Err(ImportedSourceError::Authentication(AuthError::Denied));
    }
    read_authorized(Authority::Web(read), input, entity_window).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::{ArrayRef, RecordBatch, StringArray};
    use graphit_core::file_lock::Cancellation;
    use graphit_hub::native::ContentLimits;
    use std::time::{Duration, Instant};

    fn batch(text: &str) -> RecordBatch {
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(vec!["file.any"])),
            Arc::new(StringArray::from(vec![Some(text)])),
        ];
        RecordBatch::try_new(source_schema(), columns).unwrap()
    }

    #[tokio::test]
    async fn captured_generation_and_origin_remain_observable() -> Result<(), Box<dyn Error>> {
        let temp = tempfile::tempdir()?;
        let cancel = Cancellation::default();
        let request = RequestContext::new(
            ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 32,
                max_nodes: 4096,
                max_scalar_bytes: 4096,
                max_collection_entries: 64,
            },
            &cancel,
            Instant::now() + Duration::from_secs(120),
        )?;
        let original = request.snapshot();
        let root = fs::canonicalize(temp.path())?;
        let graph = root.join("graph.icebug");
        fs::create_dir(&graph)?;
        let path = graph.join("source.lance");
        let store = LanceModuleStore::create(
            path.to_str().unwrap(),
            source_schema(),
            vec![batch("before")],
        )
        .await?;
        let limits = NativeDocumentFileLimits::new(65536, 4096, 128, 65536)?;
        let origin = NativeOrigin {
            root: capture_directory(&root, limits, original)?,
            graph: capture_directory(&graph, limits, original)?,
            corpus: Some(capture_directory(&path, limits, original)?),
            path: path.clone(),
        };
        let snapshot = origin.open(original).await?.unwrap();
        let next = store
            .snapshot()
            .prepare_replace(vec![batch("after")])
            .await?
            .commit()
            .await?;
        assert!(next.snapshot().version() > snapshot.version());
        let indexed = read_indexed_source(&snapshot, "file.any", original).await?;
        assert!(matches!(
            indexed.source(original)?,
            IndexedSource::Available("before")
        ));
        origin.revalidate(original)?;
        let foreign = Arc::new(SnapshotContext::new(
            original.required_deadline()?,
            original.cancellation().clone(),
        ));
        // Request identity is enforced by the existing indexed owner; no rebinding.
        assert!(matches!(
            indexed.source(&foreign),
            Err(IndexedSourceError::RequestMismatch)
        ));
        fs::rename(&path, root.join("moved.lance"))?;
        assert!(origin.revalidate(original).is_err());
        Ok(())
    }
}
