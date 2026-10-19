//! Freeze authorized Knowledge SDK snapshots before exporting a native package.
use graphit_auth::{
    AuthenticatedCaller, ExternalEffect, Obligation, Permission, ProjectReadAdmission,
    ProjectReadModule,
};
use graphit_core::{
    artifact_store::{CapturedSourceReader, SourceReadLimits},
    module_settings::Module,
};
use graphit_data::{
    knowledge_embeddings::{KnowledgeEmbeddingRead, capture_knowledge_embedding_corpus},
    knowledge_index::{KnowledgeGeneration, KnowledgeIndexReadLimits},
    lance_module_store::NativeModuleClone,
};
use graphit_fs::NodeKind;
use graphit_hub::{
    native_package::{self, PackageKind},
    policy::RequestContext,
};
use std::{error::Error, fmt, io, path::Path, sync::Arc};
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// Public storage observation, without a host path or any access capability.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct KnowledgePackageGeneration {
    pub version: u64,
    pub physical_branch: Option<String>,
    pub inherited_target: Option<String>,
}
impl From<&KnowledgeGeneration> for KnowledgePackageGeneration {
    fn from(generation: &KnowledgeGeneration) -> Self {
        Self {
            version: generation.version,
            physical_branch: generation.physical_branch.clone(),
            inherited_target: generation.inherited_target.clone(),
        }
    }
}

#[derive(Debug)]
pub enum KnowledgePackageExportOutcome {
    NoCorpus,
    Exported {
        generation: Box<KnowledgePackageGeneration>,
    },
}

/// The archive was published successfully before the final request check failed.
/// Consumers must retain this known effect instead of claiming no publication.
#[derive(Debug)]
pub struct KnowledgePackagePublishedFailure {
    pub generation: KnowledgePackageGeneration,
    pub cause: io::Error,
}
impl fmt::Display for KnowledgePackagePublishedFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Knowledge package published before final request check failed")
    }
}
impl Error for KnowledgePackagePublishedFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}

fn check_access(
    original: &RequestContext<'_>,
    admission: &ProjectReadAdmission<'_>,
    caller: &AuthenticatedCaller<'_>,
) -> io::Result<()> {
    original.snapshot().check()?;
    if admission.module() != ProjectReadModule::Knowledge
        || !std::ptr::eq(caller.request(), original)
        || !std::ptr::eq(admission.request().map_err(io::Error::other)?, original)
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    admission.recheck().map_err(io::Error::other)?;
    // The admission discharges the PROJECT read obligation for this request.
    caller
        .authorize(Permission::Read(Module::Knowledge))
        .map_err(io::Error::other)?;
    Ok(())
}

type CloneJob = JoinHandle<io::Result<NativeModuleClone>>;
async fn join_clone(job: CloneJob) -> io::Result<NativeModuleClone> {
    job.await.map_err(io::Error::other)?
}

// Owners precede the payload so their SDK handles close before its directories disappear.
struct FrozenPayload {
    source: NativeModuleClone,
    directory: TempDir,
}

/// Export an already admitted PROJECT under its incoming caller and Original.
/// The single documentary/vector pin is validated through EOF and cloned once.
/// No model inference or second semantic staging directory is needed.
pub(crate) struct FrozenKnowledge {
    pub(crate) reader: CapturedSourceReader,
    pub(crate) generation: KnowledgePackageGeneration,
    pub(crate) components: Vec<graphit_hub::native_root::NativeComponent>,
    original_roots: Vec<std::path::PathBuf>,
    _payload: FrozenPayload,
}

pub(crate) async fn capture_native_knowledge(
    original: &RequestContext<'_>,
    admission: &ProjectReadAdmission<'_>,
    caller: &AuthenticatedCaller<'_>,
    index_limits: KnowledgeIndexReadLimits,
) -> io::Result<Option<FrozenKnowledge>> {
    check_access(original, admission, caller)?;
    let capture =
        match capture_knowledge_embedding_corpus(original.snapshot(), admission, index_limits)
            .await
            .map_err(io::Error::other)?
        {
            KnowledgeEmbeddingRead::NoCorpus => {
                check_access(original, admission, caller)?;
                return Ok(None);
            }
            KnowledgeEmbeddingRead::Published(capture) => capture,
        };
    let source_generation = capture.source_generation().clone();
    while capture
        .next_documents(original.snapshot(), admission)
        .await
        .map_err(io::Error::other)?
        .is_some()
    {
        check_access(original, admission, caller)?;
    }
    let directory = tempfile::tempdir()?;
    let source_stage = TempDir::new_in(directory.path())?;
    check_access(original, admission, caller)?;
    let source_job = capture
        .spawn_source_clone(original.snapshot(), admission, source_stage)
        .map_err(io::Error::other)?;
    let source = join_clone(source_job).await?;
    drop(capture);
    check_access(original, admission, caller)?;
    let source_component = capture_clone(
        &source,
        "index.lance",
        graphit_hub::native_root::NativeRole::Wiki,
        original.snapshot(),
    )
    .await?;
    let components = vec![source_component];
    let original_root = Path::new(&source_generation.corpus_uri);
    if !original_root.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Knowledge export requires a local corpus",
        ));
    }
    let original_roots = vec![original_root.to_owned()];
    let frozen = FrozenPayload { source, directory };
    std::fs::rename(
        frozen.source.directory(),
        frozen.directory.path().join("index.lance"),
    )?;
    let reader = CapturedSourceReader::capture_generated(
        frozen.directory.path(),
        NodeKind::Directory,
        SourceReadLimits::generated(64 * 1024, 64 * 1024)?,
        Arc::clone(original.snapshot()),
    )?;
    check_access(original, admission, caller)?;
    Ok(Some(FrozenKnowledge {
        reader,
        generation: KnowledgePackageGeneration::from(&source_generation),
        components,
        original_roots,
        _payload: frozen,
    }))
}

/// Export an admitted capture with separate filesystem publication authority.
pub async fn export_native_knowledge_package(
    original: &RequestContext<'_>,
    admission: &ProjectReadAdmission<'_>,
    caller: &AuthenticatedCaller<'_>,
    destination: &Path,
    index_limits: KnowledgeIndexReadLimits,
) -> io::Result<KnowledgePackageExportOutcome> {
    let check = || -> io::Result<()> {
        check_access(original, admission, caller)?;
        if caller
            .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
            .map_err(io::Error::other)?
            != Obligation::Unconditional
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    };
    check()?;
    if !destination.is_absolute() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let destination =
        std::fs::canonicalize(destination.parent().ok_or(io::ErrorKind::InvalidInput)?)?
            .join(destination.file_name().ok_or(io::ErrorKind::InvalidInput)?);
    let Some(frozen) = capture_native_knowledge(original, admission, caller, index_limits).await?
    else {
        return Ok(KnowledgePackageExportOutcome::NoCorpus);
    };
    for root in &frozen.original_roots {
        if destination.starts_with(std::fs::canonicalize(root)?) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "package destination is inside Knowledge dataset",
            ));
        }
    }
    check()?;
    let snapshot = Arc::clone(original.snapshot());
    let generation = tokio::task::spawn_blocking(move || {
        snapshot.check()?;
        native_package::export(
            &frozen.reader,
            PackageKind::Knowledge,
            &destination,
            &snapshot,
        )
        .map_err(io::Error::other)?;
        Ok::<_, io::Error>(frozen.generation)
    })
    .await
    .map_err(io::Error::other)??;
    if let Err(cause) = check() {
        return Err(io::Error::new(
            cause.kind(),
            KnowledgePackagePublishedFailure { generation, cause },
        ));
    }
    Ok(KnowledgePackageExportOutcome::Exported {
        generation: Box::new(generation),
    })
}
/// Read the SDK's actual manifest path relative to the actual local clone root.
async fn capture_clone(
    clone: &graphit_data::lance_module_store::NativeModuleClone,
    path: &str,
    role: graphit_hub::native_root::NativeRole,
    original: &graphit_core::file_io::SnapshotContext,
) -> io::Result<graphit_hub::native_root::NativeComponent> {
    let metadata = clone.native_metadata(original).await?;
    let base = object_store::path::Path::from_filesystem_path(clone.directory())
        .map_err(io::Error::other)?;
    let location = metadata.manifest_location();
    let relative = location
        .path
        .prefix_match(&base)
        .ok_or(io::ErrorKind::InvalidData)?
        .map(|s| s.as_ref().to_owned())
        .collect::<Vec<_>>()
        .join("/");
    let mut manifest = std::fs::File::open(clone.directory().join(&relative))?;
    let digest = graphit_hub::remote_storage::hash_reader(&mut manifest, original)?;
    Ok(graphit_hub::native_root::NativeComponent {
        role,
        path: path.to_owned(),
        snapshot: graphit_hub::native_root::NativeSnapshot::Lance {
            dataset_version: location.version,
            manifest_path: relative,
            manifest_sha256: digest,
        },
    })
}
