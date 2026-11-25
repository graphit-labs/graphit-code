#![doc = include_str!("../../../docs/specs/rust_imported_source.md")]

use crate::{
    indexed_source::{IndexedSourceError, read_indexed_source, source_schema},
    source_inspection::{
        IndexedSource, PhysicalSource, SourceInspection, SourceInspectionError, inspect_source,
    },
};
use graphit_auth::{
    AuthError, AuthenticatedCaller, ImportedAstReadAdmission, Permission, ProjectSelection,
    RoleIssuer,
};
use graphit_core::artifact_store::PublicationScope;
use graphit_core::{
    artifact_store::{
        CapturedSourceReader, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy,
        SourceReadLimits, StoreOwner,
    },
    file_io::SnapshotContext,
    module_settings::Module,
    text_slice::Request,
};
use graphit_data::lance_module_store::{LanceModuleStore, StoreError};
use graphit_fs::NodeKind;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
    policy::RequestContext,
    registry::{HubRegistry, HubScope, InstalledBackend, InstalledPayload},
};
use std::{error::Error, fmt, fs, io, path::Path, sync::Arc};

/// Captures and registry must belong to the issuer's actual installation.
/// The fallback is independently resolved by Runtime, including its version.
pub struct ImportedSourceInput<'a, 'r> {
    pub registry: &'r HubRegistry,
    pub id: &'r ArtifactId,
    pub version: &'r ConcreteVersion,
    pub installation: &'a PublicationScope,
    pub fallback: Option<ProjectSelection<'a>>,
    pub requested_relative: &'r Path,
    pub selection: &'r Request,
    pub file_policy: &'r SnapshotPolicy,
    pub read_limits: SourceReadLimits,
    pub path_limits: RetainedBytesLimits,
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
            Self::InvalidInstallation => f.write_str("invalid imported Source installation"),
            Self::InstallationNotFound => {
                f.write_str("requested native AST installation not found")
            }
            Self::InvalidPath => f.write_str("invalid imported Source file path"),
            Self::InvalidOrigin => f.write_str("invalid imported Source path"),
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
    let StoreOwner::Installation { installation_id } = input.installation.owner() else {
        return Err(ImportedSourceError::InvalidInstallation);
    };
    if !matches!(input.registry.scope(), HubScope::Global { installation_id: id } if id == installation_id.as_str())
    {
        return Err(ImportedSourceError::InvalidInstallation);
    }
    original.check()?;
    Ok(())
}

fn validate_fallback(
    project: ProjectSelection<'_>,
    source: &SourceRef,
    input: &ImportedSourceInput<'_, '_>,
    original: &SnapshotContext,
) -> Result<(), ImportedSourceError> {
    original.check()?;
    if !matches!(project.scope.owner(), StoreOwner::Project { project_id } if *project_id == source.producer())
        || project.scope.brand() != input.installation.brand()
        || project.version.scope() != project.scope.owner_root()
    {
        return Err(ImportedSourceError::InvalidFallback);
    }
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
                    && matches!(installation.payload, InstalledPayload::Original)
                    && installation.original_source.version() == input.version
            })
        })
        .ok_or(ImportedSourceError::InstallationNotFound)?
        .original_source
        .clone();
    let path = source
        .directory()
        .as_path()
        .join("graph.icebug/source.lance");
    let snapshot = match fs::metadata(&path) {
        Ok(_) => {
            let uri = path.to_str().ok_or(ImportedSourceError::InvalidOrigin)?;
            Some(
                LanceModuleStore::open(uri, source_schema())
                    .await
                    .map_err(ImportedSourceError::Storage)?
                    .snapshot(),
            )
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    validate_installation(&input, original)?;
    let indexed = if let Some(snapshot) = &snapshot {
        let result = read_indexed_source(snapshot, key, original).await;
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
            project.scope,
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
    if physical.is_some() {
        validate_fallback(
            input.fallback.expect("captured fallback"),
            &source,
            &input,
            original,
        )?;
    }
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
        || read.installation() != input.installation
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
    async fn sdk_snapshot_and_original_request_remain_observable() -> Result<(), Box<dyn Error>> {
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
        let snapshot = LanceModuleStore::open(path.to_str().unwrap(), source_schema())
            .await?
            .snapshot();
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
        let foreign = Arc::new(SnapshotContext::new(
            original.required_deadline()?,
            original.cancellation().clone(),
        ));
        // Request identity is enforced by the existing indexed owner; no rebinding.
        assert!(matches!(
            indexed.source(&foreign),
            Err(IndexedSourceError::RequestMismatch)
        ));
        Ok(())
    }
}
