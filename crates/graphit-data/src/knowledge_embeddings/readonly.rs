//! Readonly semantic queries on retained SDK pins and caller-owned authority.
use super::*;
use crate::{
    knowledge_search::{self as search, KnowledgeSearchError as E},
    lance_module_store::ModuleSnapshot,
};
use graphit_core::native_document_file::CapturedDocumentDirectory;

struct ManifestObservation {
    directory: CapturedDocumentDirectory,
    observation: NativeDocumentObservation,
}
impl ManifestObservation {
    fn capture(
        local: &Path,
        corpus: &Path,
        limits: NativeDocumentFileLimits,
        original: &SnapshotContext,
    ) -> Result<Self, KnowledgeEmbeddingError> {
        let local = native_manifest_path(local, corpus)?;
        let parent = local
            .parent()
            .ok_or(KnowledgeEmbeddingError::InvalidStored("manifest path"))?;
        let name = local
            .file_name()
            .ok_or(KnowledgeEmbeddingError::InvalidStored("manifest name"))?;
        let directory = CapturedDocumentDirectory::capture(parent, limits, original)?;
        if directory.canonical_path() != parent {
            return Err(KnowledgeEmbeddingError::Conflict);
        }
        let observation = directory.read(name, original)?;
        if observation.bytes().is_none() {
            return Err(KnowledgeEmbeddingError::Conflict);
        }
        Ok(Self {
            directory,
            observation,
        })
    }
    fn check(&self, original: &SnapshotContext) -> Result<(), KnowledgeEmbeddingError> {
        self.directory
            .revalidate_observation(&self.observation, original)?;
        Ok(())
    }
}

/// Owns retained Source/NN pins, physical observations and the actual document cursor.
/// This carries no PROJECT grant or writable publication scope.
pub struct KnowledgeEmbeddingSnapshotCapture {
    original: Arc<SnapshotContext>,
    source: ModuleSnapshot,
    semantic: Option<BranchSnapshot>,
    source_manifest: Option<ManifestObservation>,
    semantic_manifest: Option<ManifestObservation>,
    source_slugs: BTreeSet<String>,
    source_rows: usize,
    read_limits: KnowledgeIndexReadLimits,
    documents: tokio::sync::Mutex<DocumentCursor>,
    stored: Vec<StoredRow>,
    source_generation: KnowledgeGeneration,
}
impl KnowledgeEmbeddingSnapshotCapture {
    fn view(&self) -> EmbeddingReadView<'_> {
        EmbeddingReadView {
            semantic: self.semantic.as_ref(),
            source_slugs: &self.source_slugs,
            source_rows: self.source_rows,
            read_limits: self.read_limits,
            documents: &self.documents,
            stored: &self.stored,
            source_generation: &self.source_generation,
        }
    }
    fn check(
        &self,
        original: &Arc<SnapshotContext>,
        mut external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    ) -> Result<(), KnowledgeEmbeddingError> {
        if !Arc::ptr_eq(original, &self.original) {
            return Err(index::KnowledgeIndexError::DifferentRequest.into());
        }
        original.check()?;
        external()?;
        if let Some(source) = &self.source_manifest {
            source.check(original)?;
        }
        if let Some(semantic) = &self.semantic_manifest {
            semantic.check(original)?;
        }
        external()?;
        original.check()?;
        Ok(())
    }
    pub fn source_generation(&self) -> &KnowledgeGeneration {
        &self.source_generation
    }
    /// Revalidate retained manifests and caller authority without opening a new head.
    pub fn recheck_guarded(
        &self,
        original: &Arc<SnapshotContext>,
        external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    ) -> Result<(), KnowledgeEmbeddingError> {
        self.check(original, external)
    }
    pub fn semantic_generation(&self) -> Option<KnowledgeEmbeddingGeneration> {
        self.semantic.as_ref().map(generation)
    }
    /// The actual validated NN schema width, including a published empty dataset.
    pub fn semantic_dimensions(&self) -> Result<Option<NonZeroU32>, KnowledgeEmbeddingError> {
        self.semantic
            .as_ref()
            .map(|pin| {
                let dimensions = u32::try_from(width(pin.schema())?)
                    .map_err(|_| KnowledgeEmbeddingError::InvalidStored("dimensions"))?;
                NonZeroU32::new(dimensions)
                    .ok_or(KnowledgeEmbeddingError::InvalidStored("dimensions"))
            })
            .transpose()
    }
    /// Compact observations decoded through the real NN EOF; vectors are not retained.
    pub fn stored_inputs(
        &self,
    ) -> impl Iterator<Item = (KnowledgeEmbeddingInput<'_>, &KnowledgeEmbeddingIdentity)> {
        self.stored.iter().map(|row| {
            (
                KnowledgeEmbeddingInput {
                    slug: &row.slug,
                    input_fingerprint: row.fingerprint,
                },
                &row.identity,
            )
        })
    }
    /// Independently validate this retained Source to EOF and own selected
    /// bodies, without consuming or certifying the indexing document cursor.
    pub async fn read_selected_documents_guarded(
        &self,
        original: &Arc<SnapshotContext>,
        slugs: &[&str],
        mut external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    ) -> Result<search::KnowledgeSelectedDocuments, E> {
        let mut check = || self.check(original, &mut external).map_err(E::Capture);
        check()?;
        search::read_selected_validated(
            original,
            self.source.scan(),
            &self.source_generation,
            &self.source_slugs,
            None,
            slugs,
            self.read_limits,
            check,
        )
        .await
    }

    /// Only None certifies EOF; an abandoned or failed read invalidates continuation.
    pub async fn next_documents_guarded(
        &self,
        original: &Arc<SnapshotContext>,
        mut external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    ) -> Result<Option<Vec<KnowledgeDocument>>, KnowledgeEmbeddingError> {
        next_documents_guarded(original, self.view(), None, || {
            self.check(original, &mut external)
        })
        .await
    }
    pub fn search_readiness_guarded(
        &self,
        original: &Arc<SnapshotContext>,
        identity: &KnowledgeEmbeddingIdentity,
        current_inputs: &[KnowledgeEmbeddingInput<'_>],
        mut external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    ) -> Result<KnowledgeEmbeddingSearchReadiness, KnowledgeEmbeddingError> {
        read_search_readiness(self.view(), identity, current_inputs, || {
            self.check(original, &mut external)
        })
    }
}

/// Capture exactly the supplied pins, never a current/latest Source generation.
/// The caller must revalidate imported authority, registry, root and semantic absence.
pub async fn capture_knowledge_embedding_snapshots_guarded(
    original: &Arc<SnapshotContext>,
    source: ModuleSnapshot,
    semantic: Option<BranchSnapshot>,
    limits: KnowledgeIndexReadLimits,
    mut external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<KnowledgeEmbeddingSnapshotCapture, KnowledgeEmbeddingError> {
    let mut before = || {
        original.check()?;
        external()?;
        Ok(())
    };
    before()?;
    let dimensions = index::semantic::dimensions(source.schema())?;
    if dimensions.is_none() && semantic.is_some() {
        return Err(KnowledgeEmbeddingError::InvalidStored(
            "missing vector column",
        ));
    }
    if let Some(pin) = &semantic
        && (pin.corpus_uri() != source.corpus_uri()
            || pin.version() != source.version()
            || pin.schema() != source.schema())
    {
        return Err(KnowledgeEmbeddingError::InvalidStored(
            "different vector corpus",
        ));
    }
    let metadata = embedding_guarded(source.native_metadata(original), &mut before).await??;
    let source_corpus = local_corpus(source.corpus_uri())?;
    let source_path = PathBuf::from(lance_io::local::to_local_path(
        &metadata.manifest_location().path,
    ));
    let source_manifest =
        ManifestObservation::capture(&source_path, &source_corpus, limits.native, original)?;
    before()?;
    // Reopen this retained version solely to bind the physical observation.
    let sdk_uri = crate::lance_module_store::sdk_uri(metadata.uri())?;
    let builder = lance::dataset::builder::DatasetBuilder::from_uri(sdk_uri.as_ref())
        .with_session(Arc::new(lance::session::Session::default()));
    let builder = match metadata.manifest().branch.as_deref() {
        Some(branch) => builder.with_branch(branch, Some(source.version())),
        None => builder.with_version(source.version()),
    };
    let reopened = embedding_guarded(builder.load(), &mut before).await??;
    if reopened.manifest_location().path != metadata.manifest_location().path
        || reopened.version_id() != source.version()
    {
        return Err(KnowledgeEmbeddingError::Conflict);
    }
    source_manifest.check(original)?;
    drop(metadata);
    drop(reopened);
    let semantic_manifest = if let Some(pin) = &semantic {
        let store = embedding_guarded(pin.dataset().object_store(None), &mut before).await??;
        if !store.has_direct_local_paths() {
            return Err(KnowledgeEmbeddingError::InvalidInput("nonlocal manifest"));
        }
        let corpus = local_corpus(pin.corpus_uri())?;
        let path = PathBuf::from(lance_io::local::to_local_path(
            &pin.dataset().manifest_location().path,
        ));
        let observation = ManifestObservation::capture(&path, &corpus, limits.native, original)?;
        let reopened = embedding_guarded(pin.reopen(), &mut before).await??;
        if reopened.dataset().manifest_location().path != pin.dataset().manifest_location().path {
            return Err(KnowledgeEmbeddingError::Conflict);
        }
        observation.check(original)?;
        Some(observation)
    } else {
        None
    };
    capture_pins(
        original,
        source,
        semantic,
        limits,
        Some(source_manifest),
        semantic_manifest,
        external,
    )
    .await
}

/// Capture exact native SDK pins after the composing consumer validates their
/// immutable manifests, digests and explicit object-store session. The guard is
/// required at every read boundary; this operation grants no storage authority.
pub async fn capture_native_knowledge_embedding_snapshots_guarded(
    original: &Arc<SnapshotContext>,
    source: ModuleSnapshot,
    semantic: Option<BranchSnapshot>,
    limits: KnowledgeIndexReadLimits,
    external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<KnowledgeEmbeddingSnapshotCapture, KnowledgeEmbeddingError> {
    capture_pins(original, source, semantic, limits, None, None, external).await
}

async fn capture_pins(
    original: &Arc<SnapshotContext>,
    source: ModuleSnapshot,
    semantic: Option<BranchSnapshot>,
    limits: KnowledgeIndexReadLimits,
    source_manifest: Option<ManifestObservation>,
    semantic_manifest: Option<ManifestObservation>,
    mut external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<KnowledgeEmbeddingSnapshotCapture, KnowledgeEmbeddingError> {
    let mut check = || {
        original.check()?;
        external()?;
        if let Some(source) = &source_manifest {
            source.check(original)?;
        }
        if let Some(semantic) = &semantic_manifest {
            semantic.check(original)?;
        }
        original.check()?;
        Ok(())
    };
    check()?;
    let dimensions = index::semantic::dimensions(source.schema())?;
    if dimensions.is_none() && semantic.is_some() {
        return Err(KnowledgeEmbeddingError::InvalidStored(
            "missing vector column",
        ));
    }
    if let Some(pin) = &semantic
        && (pin.corpus_uri() != source.corpus_uri()
            || pin.version() != source.version()
            || pin.schema() != source.schema())
    {
        return Err(KnowledgeEmbeddingError::InvalidStored(
            "different vector corpus",
        ));
    }
    let (source_slugs, source_rows) =
        audit_source_pin(original, source.scan(), limits, None, &mut check).await?;
    let mut stored = Vec::new();
    if let Some(pin) = &semantic {
        scan_semantic_pin(original, pin, limits, &mut check, |row, _| {
            stored.push(row);
            Ok(())
        })
        .await?;
        let validated = validate_stored_rows(&mut stored);
        check()?;
        validated?;
    }
    let mut scanner = source.scan();
    scanner.project(&index::semantic::DOCUMENT_COLUMNS)?;
    scanner.batch_size(limits.batch_rows.get() as usize);
    let documents_stream = embedding_guarded(scanner.try_into_stream(), &mut check).await??;
    check()?;
    let source_generation = KnowledgeGeneration {
        corpus_uri: source.corpus_uri().to_owned(),
        version: source.version(),
        physical_branch: None,
        inherited_target: None,
    };
    Ok(KnowledgeEmbeddingSnapshotCapture {
        original: Arc::clone(original),
        source,
        semantic,
        source_manifest,
        semantic_manifest,
        source_slugs,
        source_rows,
        read_limits: limits,
        documents: tokio::sync::Mutex::new(DocumentCursor {
            counts: index::NamespaceValidation::bound(None),
            seen: BTreeSet::new(),
            stream: Some(documents_stream),
            pending: None,
            eof: false,
            failed: false,
        }),
        stored,
        source_generation,
    })
}

pub(super) fn local_corpus(uri: &str) -> Result<PathBuf, KnowledgeEmbeddingError> {
    let path = std::path::Path::new(uri);
    if !path.is_absolute() {
        return Err(KnowledgeEmbeddingError::InvalidInput("nonlocal corpus"));
    }
    Ok(path.to_owned())
}

#[allow(clippy::too_many_arguments)]
pub async fn query_knowledge_vector_snapshots_guarded(
    original: &Arc<SnapshotContext>,
    capture: &KnowledgeEmbeddingSnapshotCapture,
    identity: &KnowledgeEmbeddingIdentity,
    current_inputs: &[KnowledgeEmbeddingInput<'_>],
    query: search::KnowledgeVectorSearchQuery<'_>,
    limits: search::KnowledgeSearchLimits,
    mut external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    preview: impl FnMut(&str, &str, usize) -> Result<search::KnowledgeSearchPreview, E>,
) -> Result<search::KnowledgeVectorSearchPage, E> {
    super::search::vector_search_guarded(
        original,
        capture.view(),
        || capture.source.scan(),
        None,
        identity,
        current_inputs,
        query,
        limits,
        || capture.check(original, &mut external).map_err(E::Capture),
        preview,
    )
    .await
}

/// Lexical results from the same Source pin when Models cannot be used.
pub async fn query_knowledge_embedding_snapshot_source_search_guarded(
    original: &Arc<SnapshotContext>,
    capture: &KnowledgeEmbeddingSnapshotCapture,
    query: search::KnowledgeLexicalQuery<'_>,
    limits: search::KnowledgeSearchLimits,
    mut external: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
    preview: impl FnMut(&str, &str, usize) -> Result<search::KnowledgeSearchPreview, E>,
) -> Result<search::KnowledgeSearchPage, E> {
    super::search::source_search_guarded(
        original,
        capture.view(),
        || capture.source.scan(),
        None,
        query,
        limits,
        || capture.check(original, &mut external).map_err(E::Capture),
        preview,
    )
    .await
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    #[tokio::test]
    async fn readonly_source_and_semantic_keep_original_locator_and_named_manifest()
    -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let corpus = root.path().canonicalize()?.join("source %20 λ.lance");
        let uri = corpus.to_str().unwrap();
        let schema = index::semantic::schema(NonZeroU32::new(2).unwrap())?;
        crate::lance_module_store::LanceModuleStore::create(
            uri,
            Arc::clone(&schema),
            vec![RecordBatch::new_empty(Arc::clone(&schema))],
        )
        .await?;
        let store = crate::lance_branch_store::LanceBranchStore::open(uri, schema, None).await?;
        let original = Arc::new(SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(60),
            graphit_core::file_lock::Cancellation::default(),
        ));
        let limits = KnowledgeIndexReadLimits {
            content: index::KnowledgeIndexContentLimits {
                max_identifier_bytes: NonZeroUsize::new(4096).unwrap(),
            },
            native: NativeDocumentFileLimits::new(65536, 65536, 256, 65536)?,
            batch_rows: NonZeroU32::new(32).unwrap(),
        };
        for branch in [None, Some("dev")] {
            let pin = store.capture_target(branch).await?.materialize().await?;
            let version = pin.version();
            let capture = capture_knowledge_embedding_snapshots_guarded(
                &original,
                pin.module_snapshot(),
                Some(pin),
                limits,
                || Ok(()),
            )
            .await?;
            assert_eq!(capture.source.corpus_uri(), uri);
            assert_eq!(capture.semantic.as_ref().unwrap().corpus_uri(), uri);
            assert_eq!(capture.source.version(), version);
            assert_eq!(capture.semantic.as_ref().unwrap().physical_branch(), branch);
            capture.check(&original, &mut || Ok(()))?;
        }
        Ok(())
    }

    #[test]
    fn sdk_manifest_capture_preserves_literal_names_and_rejects_foreign_corpus()
    -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let corpus = root.path().canonicalize()?.join("source %20 λ.lance");
        let parent = corpus.join("_versions");
        std::fs::create_dir_all(&parent)?;
        let manifest = parent.join("1.manifest");
        std::fs::write(&manifest, b"retained manifest")?;
        let sdk = crate::lance_module_store::sdk_uri(corpus.to_str().unwrap())?;
        let sdk_manifest = Path::new(sdk.as_ref()).join("_versions/1.manifest");
        let original = SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            graphit_core::file_lock::Cancellation::default(),
        );
        let limits = NativeDocumentFileLimits::new(65536, 65536, 256, 65536)?;
        let captured = ManifestObservation::capture(&sdk_manifest, &corpus, limits, &original)?;
        captured.check(&original)?;
        assert_eq!(captured.directory.canonical_path(), parent);
        assert!(matches!(
            ManifestObservation::capture(
                &sdk_manifest,
                &corpus.with_file_name("foreign.lance"),
                limits,
                &original
            ),
            Err(KnowledgeEmbeddingError::InvalidStored("manifest path"))
        ));
        assert!(native_manifest_path(Path::new(sdk.as_ref()), &corpus).is_err());
        assert!(
            native_manifest_path(
                &Path::new(sdk.as_ref()).join("../foreign/1.manifest"),
                &corpus
            )
            .is_err()
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn sdk_manifest_capture_rejects_symlink_leaf() -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let corpus = root.path().canonicalize()?.join("source.lance");
        let parent = corpus.join("_versions");
        std::fs::create_dir_all(&parent)?;
        let target = parent.join("actual.manifest");
        std::fs::write(&target, b"must not follow")?;
        let manifest = parent.join("1.manifest");
        std::os::unix::fs::symlink(&target, &manifest)?;
        let original = SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            graphit_core::file_lock::Cancellation::default(),
        );
        assert!(
            ManifestObservation::capture(
                &manifest,
                &corpus,
                NativeDocumentFileLimits::new(65536, 65536, 256, 65536)?,
                &original
            )
            .is_err()
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn sdk_manifest_paths_rejoin_verbatim_drive_and_unc_without_changing_names() {
        for (corpus, local) in [
            (
                r"\\?\C:\source %20 λ.lance",
                "C:/source %20 λ.lance/_versions/1.manifest",
            ),
            (
                r"\\?\UNC\server\share\source %20 λ.lance",
                "//server/share/source %20 λ.lance/tree/dev/_versions/1.manifest",
            ),
        ] {
            let path = native_manifest_path(Path::new(local), Path::new(corpus)).unwrap();
            assert!(path.starts_with(corpus));
            assert_eq!(path.file_name().unwrap(), "1.manifest");
            assert_eq!(
                path.components()
                    .filter(|c| matches!(c, std::path::Component::ParentDir))
                    .count(),
                0
            );
        }
    }
}
