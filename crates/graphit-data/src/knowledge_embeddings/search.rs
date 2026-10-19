//! Search borrows the existing physical-manifest capture without opening NN storage.
use super::*;
use crate::knowledge_search::{KnowledgeSearchError as E, validate_membership_counted};
use tracing::Instrument;

pub(crate) struct LexicalSourceCapture {
    original: Arc<SnapshotContext>,
    binding: ScopeBindingRecord,
    branch: Option<String>,
    source: ManifestCapture,
    directory: CapturedProjectDocumentDirectory,
    pub(crate) slugs: BTreeSet<String>,
    pub(crate) generation: KnowledgeGeneration,
    pub(crate) limits: KnowledgeIndexReadLimits,
    source_rows: usize,
    documents: tokio::sync::Mutex<DocumentCursor>,
}
impl LexicalSourceCapture {
    pub(crate) async fn capture(
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        limits: KnowledgeIndexReadLimits,
    ) -> Result<Option<Self>, E> {
        let project = index::check_read_project(original, read)?;
        let scope = project.scope.live;
        let corpus =
            index::local_corpus(scope, project.version.version(), original, limits.native)?;
        let selected = branch(project.version.lance_branch());
        let Some(directory) =
            index::capture_index(scope, &corpus, original, limits.native, || {
                index::check_read(original, read)
            })?
        else {
            return Ok(None);
        };
        let before = || -> std::result::Result<(), index::KnowledgeIndexError> {
            index::check_read(original, read)?;
            directory.revalidate(original)?;
            Ok(())
        };
        before()?;
        let uri = corpus.uri.clone();
        let pin = BranchSnapshot::capture_observed(&uri, selected.as_deref(), None)
            .instrument(tracing::debug_span!("knowledge_pin"))
            .await
            .map_err(index::KnowledgeIndexError::from)?;
        before()?;
        index::semantic::dimensions(pin.schema())?;
        let source = capture_manifest(scope, original, pin, &corpus.path, limits.native, || {
            before().map_err(KnowledgeEmbeddingError::from)
        })
        .instrument(tracing::debug_span!("knowledge_manifest"))
        .await
        .map_err(E::Capture)?;
        let graphit_core::artifact_store::StoreOwner::Project { project_id } = scope.owner() else {
            return Err(index::KnowledgeIndexError::WrongAdmission.into());
        };
        let (slugs, source_rows) = validate_membership_counted(
            original,
            source.pin.scan(),
            limits,
            Some(*project_id),
            || {
                before()?;
                source.recheck(original).map_err(E::Capture)?;
                Ok(())
            },
        )
        .instrument(tracing::debug_span!("knowledge_membership"))
        .await?;
        before()?;
        source.recheck(original).map_err(E::Capture)?;
        let generation = index::generation(&uri, &source.pin);
        Ok(Some(Self {
            original: Arc::clone(original),
            binding: project.scope.record.clone(),
            branch: selected,
            source,
            directory,
            slugs,
            generation,
            limits,
            source_rows,
            documents: tokio::sync::Mutex::new(DocumentCursor {
                counts: index::NamespaceValidation::bound(Some(*project_id)),
                seen: BTreeSet::new(),
                stream: None,
                pending: None,
                eof: false,
                failed: false,
            }),
        }))
    }
    pub(crate) fn check(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<(), E> {
        let project = index::check_read_project(original, read)?;
        if !Arc::ptr_eq(original, &self.original) {
            return Err(index::KnowledgeIndexError::DifferentRequest.into());
        }
        if project.scope.record != &self.binding
            || branch(project.version.lance_branch()) != self.branch
        {
            return Err(index::KnowledgeIndexError::WrongAdmission.into());
        }
        self.directory
            .revalidate(original)
            .map_err(index::KnowledgeIndexError::from)?;
        self.source.recheck(original).map_err(E::Capture)
    }
    pub(crate) fn validate_export_destination(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        destination: &Path,
    ) -> Result<(), E> {
        self.check(original, read)?;
        let root =
            super::readonly::local_corpus(&self.generation.corpus_uri).map_err(E::Capture)?;
        crate::knowledge_search::validate_knowledge_export_destination_path(&root, destination)
            .map_err(index::KnowledgeIndexError::Request)?;
        self.check(original, read)
    }
    pub(crate) async fn next_documents(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<Option<Vec<KnowledgeDocument>>, E> {
        let mut check = || self.check(original, read);
        check()?;
        let view = EmbeddingReadView {
            semantic: None,
            source_slugs: &self.slugs,
            source_rows: self.source_rows,
            read_limits: self.limits,
            documents: &self.documents,
            stored: &[],
            source_generation: &self.generation,
        };
        super::next_documents_guarded(original, view, Some(self.source.pin.scan()), || {
            embedding_check(&mut check)
        })
        .await
        .map_err(E::Capture)
    }
    pub(crate) fn scan(&self) -> lance::dataset::scanner::Scanner {
        self.source.pin.scan()
    }
    pub(crate) fn project(
        &self,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<graphit_core::project_identity::ProjectId, E> {
        let scope = read
            .project()
            .map_err(index::KnowledgeIndexError::from)?
            .scope
            .live;
        let graphit_core::artifact_store::StoreOwner::Project { project_id } = scope.owner() else {
            return Err(index::KnowledgeIndexError::WrongAdmission.into());
        };
        Ok(*project_id)
    }
}

/// Lexical results from the Source pin already owned by a semantic capture.
/// Models availability belongs to the caller; no identity or query vector is needed.
pub async fn query_native_knowledge_embedding_source_search(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    capture: &KnowledgeEmbeddingCapture,
    query: crate::knowledge_search::KnowledgeLexicalQuery<'_>,
    limits: crate::knowledge_search::KnowledgeSearchLimits,
    preview: impl FnMut(&str, &str, usize) -> Result<crate::knowledge_search::KnowledgeSearchPreview, E>,
) -> Result<crate::knowledge_search::KnowledgeSearchPage, E> {
    let check = || capture.read(original, read).map_err(E::Capture);
    check()?;
    let project = read.project().map_err(index::KnowledgeIndexError::from)?;
    let graphit_core::artifact_store::StoreOwner::Project { project_id } =
        project.scope.live.owner()
    else {
        return Err(index::KnowledgeIndexError::WrongAdmission.into());
    };
    source_search_guarded(
        original,
        capture.view(),
        || capture.source.pin.scan(),
        Some(*project_id),
        query,
        limits,
        check,
        preview,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn source_search_guarded(
    original: &SnapshotContext,
    view: EmbeddingReadView<'_>,
    scan: impl Fn() -> lance::dataset::scanner::Scanner,
    expected: Option<graphit_core::project_identity::ProjectId>,
    query: crate::knowledge_search::KnowledgeLexicalQuery<'_>,
    limits: crate::knowledge_search::KnowledgeSearchLimits,
    mut check: impl FnMut() -> Result<(), E>,
    preview: impl FnMut(&str, &str, usize) -> Result<crate::knowledge_search::KnowledgeSearchPreview, E>,
) -> Result<crate::knowledge_search::KnowledgeSearchPage, E> {
    use crate::knowledge_search as search;
    use sha2::{Digest, Sha256};
    check()?;
    search::input(query, limits)?;
    if limits.read.content.max_identifier_bytes != view.read_limits.content.max_identifier_bytes
        || limits.read.batch_rows != view.read_limits.batch_rows
    {
        return Err(E::InvalidQuery("capture_limits"));
    }
    let mut hash = Sha256::new();
    hash.update(b"knowledge-embedding-source-fts/v1");
    hash.update(search::query_binding(query, limits));
    let semantic = view
        .semantic
        .map(generation)
        .map(|v| (v.version, v.physical_branch, v.inherited_target));
    search::search_validated(
        original,
        scan,
        view.source_generation,
        view.source_slugs,
        expected,
        query,
        limits,
        Some(hash.finalize().into()),
        semantic,
        check,
        preview,
    )
    .await
}

/// Query vectors are ordinary model output; the caller retains Models authority.
#[allow(clippy::too_many_arguments)]
pub async fn query_native_knowledge_vector_search(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    capture: &KnowledgeEmbeddingCapture,
    identity: &KnowledgeEmbeddingIdentity,
    current_inputs: &[KnowledgeEmbeddingInput<'_>],
    query: crate::knowledge_search::KnowledgeVectorSearchQuery<'_>,
    limits: crate::knowledge_search::KnowledgeSearchLimits,
    preview: impl FnMut(&str, &str, usize) -> Result<crate::knowledge_search::KnowledgeSearchPreview, E>,
) -> Result<crate::knowledge_search::KnowledgeVectorSearchPage, E> {
    let check = || capture.read(original, read).map_err(E::Capture);
    check()?;
    let project = read.project().map_err(index::KnowledgeIndexError::from)?;
    let graphit_core::artifact_store::StoreOwner::Project { project_id } =
        project.scope.live.owner()
    else {
        return Err(index::KnowledgeIndexError::WrongAdmission.into());
    };
    vector_search_guarded(
        original,
        capture.view(),
        || capture.source.pin.scan(),
        Some(*project_id),
        identity,
        current_inputs,
        query,
        limits,
        check,
        preview,
    )
    .await
}

fn embedding_check(
    check: &mut impl FnMut() -> Result<(), E>,
) -> Result<(), KnowledgeEmbeddingError> {
    check().map_err(|error| match error {
        E::Capture(error) => error,
        error => index::KnowledgeIndexError::Request(io::Error::other(error)).into(),
    })
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn vector_search_guarded(
    original: &SnapshotContext,
    view: EmbeddingReadView<'_>,
    scan: impl Fn() -> lance::dataset::scanner::Scanner,
    expected: Option<graphit_core::project_identity::ProjectId>,
    identity: &KnowledgeEmbeddingIdentity,
    current_inputs: &[KnowledgeEmbeddingInput<'_>],
    query: crate::knowledge_search::KnowledgeVectorSearchQuery<'_>,
    limits: crate::knowledge_search::KnowledgeSearchLimits,
    mut check: impl FnMut() -> Result<(), E>,
    preview: impl FnMut(&str, &str, usize) -> Result<crate::knowledge_search::KnowledgeSearchPreview, E>,
) -> Result<crate::knowledge_search::KnowledgeVectorSearchPage, E> {
    use crate::knowledge_search::{
        self as search, KnowledgeSearchCursor, KnowledgeVectorSearchHit,
        KnowledgeVectorSearchMode as Mode, KnowledgeVectorSearchPage,
        KnowledgeVectorSearchState as State,
    };
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    check()?;
    search::input(query.page, limits)?;
    if limits.read.content.max_identifier_bytes != view.read_limits.content.max_identifier_bytes
        || limits.read.batch_rows != view.read_limits.batch_rows
    {
        return Err(E::InvalidQuery("capture_limits"));
    }
    let readiness = read_search_readiness(view, identity, current_inputs, || {
        embedding_check(&mut check)
    })
    .map_err(E::Capture)?;
    if let Some(vector) = query.vector {
        if vector.len() != identity.dimensions.get() as usize
            || vector.iter().any(|v| !v.is_finite())
        {
            return Err(E::Capture(KnowledgeEmbeddingError::InvalidInput(
                "query vector",
            )));
        }
    } else if readiness == KnowledgeEmbeddingSearchReadiness::Ready {
        return Err(E::InvalidQuery("missing_query_vector"));
    }
    let semantic_generation = view.semantic.map(generation);
    let cursor_semantic = semantic_generation.as_ref().map(|v| {
        (
            v.version,
            v.physical_branch.clone(),
            v.inherited_target.clone(),
        )
    });
    let mut hash = Sha256::new();
    hash.update(b"knowledge-vector-search/v1");
    hash.update(search::query_binding(query.page, limits));
    hash.update([match query.mode {
        Mode::Semantic => 0,
        Mode::Hybrid => 1,
    }]);
    hash.update([match query.metric {
        KnowledgeEmbeddingMetric::L2 => 0,
        KnowledgeEmbeddingMetric::Cosine => 1,
        KnowledgeEmbeddingMetric::Dot => 2,
    }]);
    hash.update(identity.vector_id);
    hash.update(identity.dimensions.get().to_le_bytes());
    hash.update((identity.projection_recipe.len() as u64).to_le_bytes());
    hash.update(identity.projection_recipe.as_bytes());
    match query.vector {
        Some(vector) => {
            hash.update([1]);
            for v in vector {
                hash.update(v.to_bits().to_le_bytes());
            }
        }
        None => hash.update([0]),
    }
    // Continuations also bind the compact recipe observations supplied by
    // the caller after the real document cursor reached EOF.
    let mut inputs: Vec<_> = current_inputs.iter().collect();
    inputs.sort_unstable_by(|a, b| a.slug.cmp(b.slug));
    for input in inputs {
        hash.update((input.slug.len() as u64).to_le_bytes());
        hash.update(input.slug.as_bytes());
        hash.update(input.input_fingerprint);
    }
    let binding: [u8; 32] = hash.finalize().into();
    let generation = view.source_generation;
    let offset = if let Some(cursor) = query.page.cursor {
        if cursor.uri != generation.corpus_uri
            || cursor.version != generation.version
            || cursor.physical_branch != generation.physical_branch
            || cursor.inherited_target != generation.inherited_target
            || cursor.semantic_generation != cursor_semantic
        {
            return Err(E::GenerationChanged);
        }
        if cursor.binding != binding || cursor.offset == 0 {
            return Err(E::InvalidCursor);
        }
        cursor.offset
    } else {
        0
    };
    #[derive(Default)]
    struct Scores {
        lexical: Option<f32>,
        distance: Option<f32>,
        rrf: f64,
    }
    let mut ranked: BTreeMap<String, Scores> = BTreeMap::new();
    if query.mode == Mode::Hybrid {
        let lexical = search::rank_lexical(
            original,
            &scan,
            view.source_slugs,
            query.page.text,
            limits,
            &mut check,
        )
        .await?;
        for (rank, (slug, score)) in lexical.into_iter().enumerate() {
            check()?;
            let entry = ranked.entry(slug).or_default();
            entry.lexical = Some(score);
            entry.rrf += 1.0 / (60.0 + (rank + 1) as f64);
        }
    }
    if readiness == KnowledgeEmbeddingSearchReadiness::Ready {
        // Rank every compatible embedding before RRF/top-k, including while Attach is still
        // filling the source. Missing or stale vectors are excluded by the same NN prefilter.
        let k = NonZeroUsize::new(
            current_inputs
                .iter()
                .filter(|input| compatible_input(view.stored, identity, input))
                .count(),
        )
        .ok_or(E::Capture(KnowledgeEmbeddingError::InvalidStored(
            "ready empty eligible set",
        )))?;
        let vector = query
            .vector
            .ok_or(E::InvalidQuery("missing_query_vector"))?;
        let result = query_embedding_pin(
            view,
            identity,
            current_inputs,
            KnowledgeEmbeddingQuery {
                vector,
                top_k: k,
                metric: query.metric,
            },
            || embedding_check(&mut check),
        )
        .await
        .map_err(E::Capture)?;
        let KnowledgeSemanticQuery::Hits {
            source_generation,
            semantic_generation: observed,
            mut hits,
        } = result
        else {
            return Err(E::Capture(KnowledgeEmbeddingError::InvalidStored(
                "ready query state",
            )));
        };
        if source_generation != *generation
            || Some(&observed) != semantic_generation.as_ref()
            || hits.len() != k.get()
        {
            return Err(E::Capture(KnowledgeEmbeddingError::InvalidStored(
                "complete NN ranking",
            )));
        }
        hits.sort_unstable_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| a.slug.cmp(&b.slug))
        });
        for (rank, hit) in hits.into_iter().enumerate() {
            check()?;
            let entry = ranked.entry(hit.slug).or_default();
            entry.distance = Some(hit.distance);
            if query.mode == Mode::Hybrid {
                entry.rrf += 1.0 / (60.0 + (rank + 1) as f64);
            }
        }
    }
    if query.mode == Mode::Semantic && ranked.values().any(|score| score.distance.is_none()) {
        return Err(E::Capture(KnowledgeEmbeddingError::InvalidStored(
            "semantic rank",
        )));
    }
    let mut ranked: Vec<_> = ranked.into_iter().collect();
    ranked.sort_unstable_by(|a, b| match query.mode {
        Mode::Hybrid => b.1.rrf.total_cmp(&a.1.rrf).then_with(|| a.0.cmp(&b.0)),
        Mode::Semantic => match (a.1.distance, b.1.distance) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            _ => std::cmp::Ordering::Equal,
        }
        .then_with(|| a.0.cmp(&b.0)),
    });
    if query.page.top_k > 0 {
        ranked.truncate(query.page.top_k);
    }
    let total = ranked.len();
    if offset > total {
        return Err(E::InvalidCursor);
    }
    let end = offset.saturating_add(query.page.page_size.get()).min(total);
    let selected: Vec<_> = ranked[offset..end]
        .iter()
        .map(|(slug, _)| slug.clone())
        .collect();
    let materialized = search::materialize_hits(
        original,
        scan,
        expected,
        &selected,
        query.page.preview,
        limits,
        &mut check,
        preview,
    )
    .await?;
    let hits = materialized
        .into_iter()
        .zip(&ranked[offset..end])
        .map(|(hit, (_, score))| KnowledgeVectorSearchHit {
            source: hit.source,
            lexical_score: score.lexical,
            semantic_distance: score.distance,
            rrf_score: (query.mode == Mode::Hybrid
                && readiness == KnowledgeEmbeddingSearchReadiness::Ready)
                .then_some(score.rrf),
            preview: hit.preview,
        })
        .collect();
    let next_cursor = (end < total).then(|| KnowledgeSearchCursor {
        binding,
        semantic_generation: cursor_semantic,
        uri: generation.corpus_uri.clone(),
        version: generation.version,
        physical_branch: generation.physical_branch.clone(),
        inherited_target: generation.inherited_target.clone(),
        offset: end,
    });
    let state = match (&readiness, query.mode) {
        (KnowledgeEmbeddingSearchReadiness::PublishedEmpty, _) => State::PublishedEmpty,
        (KnowledgeEmbeddingSearchReadiness::Unavailable(_), Mode::Semantic) => State::Unavailable,
        (KnowledgeEmbeddingSearchReadiness::Unavailable(_), Mode::Hybrid) => State::Partial,
        _ if total == 0 => State::NoHits,
        _ => State::Hits,
    };
    check()?;
    Ok(KnowledgeVectorSearchPage {
        generation: generation.clone(),
        semantic_generation,
        readiness,
        state,
        hits,
        total,
        next_cursor,
    })
}
