//! Current native uploads, physically staged and admitted through their actual SDK snapshots.
use crate::{native_publication::ast_components, source_server::SourcePolicy};
use graphit_core::artifact_store::CapturedSourceReader;
use graphit_data::{
    CapturedLocalGraph,
    knowledge_embeddings::{
        KnowledgeEmbeddingInput, KnowledgeEmbeddingSearchReadiness,
        KnowledgeEmbeddingSnapshotCapture, capture_knowledge_embedding_snapshots_guarded,
    },
    knowledge_index::{KnowledgeIndexContentLimits, KnowledgeIndexReadLimits},
    lance_branch_store::BranchSnapshot,
};
use graphit_hub::{
    ArtifactType,
    native_package::{self, PackageKind, StagedImport},
    native_root::{NativeComponent, NativeRole, NativeSnapshot},
    policy::RequestContext,
};
use graphit_knowledge::embedding_projection as projection;
use object_store::path::Path as ObjectPath;
use std::{
    future::Future,
    io,
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::Arc,
};

fn package_error(error: native_package::PackageError) -> io::Error {
    let kind = match &error {
        native_package::PackageError::Io(error) => error.kind(),
        _ => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, error)
}
fn graph_error(error: graphit_data::DataReadError) -> io::Error {
    let kind = match &error {
        graphit_data::DataReadError::Source(error) => error.kind(),
        graphit_data::DataReadError::LimitExceeded(_) => io::ErrorKind::FileTooLarge,
        _ => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, error)
}
fn embedding_error(
    error: graphit_data::knowledge_embeddings::KnowledgeEmbeddingError,
) -> io::Error {
    let kind = match &error {
        graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Io(error) => error.kind(),
        graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::InvalidInput(_)
        | graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::InvalidStored(_)
        | graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Conflict
        | graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::Arrow(_) => {
            io::ErrorKind::InvalidData
        }
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, error)
}

struct KnowledgeSnapshot {
    reader: CapturedSourceReader,
    capture: KnowledgeEmbeddingSnapshotCapture,
}
enum Snapshot {
    Ast(Arc<CapturedLocalGraph>),
    Knowledge(Box<KnowledgeSnapshot>),
}

/// SDK handles and source descriptors close before the owned extraction is removed.
pub(crate) struct Prepared<'a> {
    snapshot: Snapshot,
    components: Vec<NativeComponent>,
    source: &'a CapturedSourceReader,
    _stage: StagedImport<'a>,
}
impl Prepared<'_> {
    pub(crate) fn reader(&self, request: &RequestContext<'_>) -> io::Result<&CapturedSourceReader> {
        match &self.snapshot {
            Snapshot::Ast(graph) => graph.source_reader(request.snapshot()).map_err(graph_error),
            Snapshot::Knowledge(snapshot) => {
                snapshot.reader.check_request(request.snapshot())?;
                Ok(&snapshot.reader)
            }
        }
    }
    pub(crate) fn components(&self) -> &[NativeComponent] {
        &self.components
    }
    pub(crate) fn recheck(
        &self,
        request: &RequestContext<'_>,
        check: &impl Fn() -> io::Result<()>,
    ) -> io::Result<()> {
        request.check()?;
        check()?;
        self.source.check_request(request.snapshot())?;
        self.source.revalidate_bindings()?;
        match &self.snapshot {
            Snapshot::Ast(graph) => graph.revalidate(request.snapshot()).map_err(graph_error)?,
            Snapshot::Knowledge(snapshot) => {
                snapshot.reader.revalidate_bindings()?;
                snapshot
                    .capture
                    .recheck_guarded(request.snapshot(), || check().map_err(Into::into))
                    .map_err(embedding_error)?;
            }
        }
        check()
    }
}

async fn checked<T>(
    request: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
    future: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
    request.check()?;
    check()?;
    let result = tokio::select! {
        biased;
        () = request.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(request.snapshot().required_deadline()?.into()) => return Err(io::ErrorKind::TimedOut.into()),
        result = future => result?,
    };
    request.check()?;
    check()?;
    Ok(result)
}

pub(crate) async fn prepare<'a>(
    source: &'a CapturedSourceReader,
    parent: &Path,
    artifact_type: ArtifactType,
    policy: &SourcePolicy,
    request: &'a RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<Prepared<'a>> {
    request.check()?;
    check()?;
    let kind = match artifact_type {
        ArtifactType::Ast => PackageKind::Ast,
        ArtifactType::Knowledge => PackageKind::Knowledge,
        _ => return Err(io::ErrorKind::InvalidInput.into()),
    };
    let stage = native_package::stage_import(
        source,
        Some(kind),
        &parent.join("received-native"),
        policy.native_install.hub.copy(),
        request.snapshot(),
    )
    .map_err(package_error)?;
    check()?;
    let (snapshot, components) = match kind {
        PackageKind::Ast => {
            let graph = Arc::new(
                CapturedLocalGraph::capture(
                    &stage.path().join("graph.icebug"),
                    &policy.file,
                    policy.read,
                    Arc::clone(request.snapshot()),
                    policy.native_install.graph,
                )
                .map_err(graph_error)?,
            );
            let components = checked(request, check, ast_components(&graph, request)).await?;
            (Snapshot::Ast(graph), components)
        }
        PackageKind::Knowledge => {
            let reader = CapturedSourceReader::capture(
                stage.path(),
                graphit_fs::NodeKind::Directory,
                &policy.file,
                policy.read,
                Arc::clone(request.snapshot()),
            )?;
            let source_path = stage.path().join("index.lance");
            let source_uri = source_path.to_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "received Knowledge Source directory is not a local absolute path",
                )
            })?;
            let source = checked(request, check, async {
                BranchSnapshot::capture_observed(source_uri, None, None)
                    .await
                    .map_err(io::Error::other)
            })
            .await?;
            let source_version = source.version();
            let semantic =
                graphit_data::knowledge_embeddings::native_knowledge_embedding_dimensions(
                    source.schema(),
                )
                .map_err(embedding_error)?
                .map(|_| source.clone());
            let source_pin = source.module_snapshot();
            let limits = KnowledgeIndexReadLimits {
                native: policy.index,
                batch_rows: NonZeroU32::new(8192).unwrap(),
                content: KnowledgeIndexContentLimits {
                    max_identifier_bytes: NonZeroUsize::new(policy.content.max_scalar_bytes)
                        .ok_or(io::ErrorKind::InvalidInput)?,
                },
            };
            let capture = capture_knowledge_embedding_snapshots_guarded(
                request.snapshot(),
                source_pin,
                semantic,
                limits,
                || check().map_err(Into::into),
            )
            .await
            .map_err(embedding_error)?;
            let identity = capture
                .stored_inputs()
                .next()
                .map(|(_, identity)| identity.clone());
            if identity.as_ref().is_some_and(|identity| {
                identity.projection_recipe() != projection::KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE
            }) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unsupported Knowledge document projection",
                ));
            }
            let mut observed = Vec::new();
            while let Some(documents) = capture
                .next_documents_guarded(request.snapshot(), || check().map_err(Into::into))
                .await
                .map_err(embedding_error)?
            {
                if let Some(identity) = &identity {
                    for document in documents {
                        check()?;
                        let text = projection::document_text(&document)
                            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                        let fingerprint = projection::fingerprint(&text, identity)
                            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                        observed.push((document.slug, fingerprint));
                    }
                }
            }
            if let Some(identity) = &identity {
                let inputs: Vec<_> = observed
                    .iter()
                    .map(|(slug, fingerprint)| KnowledgeEmbeddingInput {
                        slug,
                        input_fingerprint: *fingerprint,
                    })
                    .collect();
                if matches!(
                    capture
                        .search_readiness_guarded(request.snapshot(), identity, &inputs, || check()
                            .map_err(Into::into))
                        .map_err(embedding_error)?,
                    KnowledgeEmbeddingSearchReadiness::Unavailable(_)
                ) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Knowledge semantic contents differ from Source",
                    ));
                }
            }
            let components = vec![
                lance_component(
                    &reader,
                    "index.lance",
                    NativeRole::Wiki,
                    source_version,
                    request,
                    check,
                )
                .await?,
            ];
            (
                Snapshot::Knowledge(Box::new(KnowledgeSnapshot { reader, capture })),
                components,
            )
        }
    };
    let prepared = Prepared {
        snapshot,
        components,
        source,
        _stage: stage,
    };
    prepared.recheck(request, check)?;
    Ok(prepared)
}

async fn lance_component(
    reader: &CapturedSourceReader,
    directory: &str,
    role: NativeRole,
    version: u64,
    request: &RequestContext<'_>,
    check: &impl Fn() -> io::Result<()>,
) -> io::Result<NativeComponent> {
    let root: PathBuf = reader.source_path(request.snapshot())?.join(directory);
    let uri = root.to_str().ok_or(io::ErrorKind::InvalidData)?;
    let dataset = checked(request, check, async {
        lance::dataset::builder::DatasetBuilder::from_uri(uri)
            .with_version(version)
            .load()
            .await
            .map_err(io::Error::other)
    })
    .await?;
    let location = dataset.manifest_location();
    if location.version != version {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let base = ObjectPath::from_filesystem_path(&root)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let relative = location
        .path
        .prefix_match(&base)
        .ok_or(io::ErrorKind::InvalidData)?
        .map(|part| part.as_ref().to_owned())
        .collect::<Vec<_>>()
        .join("/");
    let file = reader.open_file(&Path::new(directory).join(&relative))?;
    let digest =
        graphit_hub::remote_storage::hash_reader(&mut file.stream(0)?, request.snapshot())?;
    Ok(NativeComponent {
        role,
        path: directory.to_owned(),
        snapshot: NativeSnapshot::Lance {
            dataset_version: version,
            manifest_path: relative,
            manifest_sha256: digest,
        },
    })
}
