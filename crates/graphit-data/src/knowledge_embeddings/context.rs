//! A retained Source SDK pin for read-context preparation; no row scan.
use super::*;

pub struct KnowledgeContextCapture {
    original: Arc<SnapshotContext>,
    scope: PublicationScope,
    branch: Option<String>,
    source: BranchSnapshot,
    generation: KnowledgeGeneration,
}
impl KnowledgeContextCapture {
    pub fn generation(&self) -> &KnowledgeGeneration {
        &self.generation
    }

    /// Read one page from this same retained Source pin, with PROJECT namespace checks.
    pub async fn page(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        slug: &str,
        limits: KnowledgeIndexReadLimits,
    ) -> Result<crate::knowledge_snapshot::KnowledgeSnapshotPage, KnowledgeEmbeddingError> {
        index::validate_slug(slug)?;
        if slug.len() > limits.content.max_identifier_bytes.get() {
            return Err(index::KnowledgeIndexError::LimitExceeded("identifier_bytes").into());
        }
        self.recheck(original, read)?;
        let project = read.project()?;
        let graphit_core::artifact_store::StoreOwner::Project { project_id } =
            project.scope.owner()
        else {
            return Err(index::KnowledgeIndexError::WrongAdmission.into());
        };
        let before = || {
            self.recheck(original, read)
                .map_err(|cause| index::KnowledgeIndexError::Request(io::Error::other(cause)))
        };
        let page = crate::knowledge_snapshot::read_page_guarded(
            original,
            self.source.schema(),
            || self.source.scan(),
            slug,
            limits,
            Some(*project_id),
            before,
        )
        .await?;
        self.recheck(original, read)?;
        Ok(page)
    }

    /// Read references from the identical pin used for this context's pages.
    pub async fn references(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
        query: crate::knowledge_reference_query::KnowledgeReferenceQuery<'_>,
        limits: KnowledgeIndexReadLimits,
    ) -> Result<
        crate::knowledge_reference_query::KnowledgeReferenceSnapshotRead,
        crate::knowledge_reference_query::KnowledgeReferenceQueryError,
    > {
        let before = || {
            self.recheck(original, read)
                .map_err(|cause| index::KnowledgeIndexError::Request(io::Error::other(cause)))
        };
        before()?;
        let project = read.project().map_err(index::KnowledgeIndexError::from)?;
        let graphit_core::artifact_store::StoreOwner::Project { project_id } =
            project.scope.owner()
        else {
            return Err(index::KnowledgeIndexError::WrongAdmission.into());
        };
        let result = crate::knowledge_reference_query::read_knowledge_reference_scan_guarded(
            original,
            self.source.scan(),
            &self.generation,
            query,
            limits,
            Some(*project_id),
            before,
        )
        .await?;
        before()?;
        Ok(result)
    }

    /// Recheck the same request and PROJECT selection.
    /// Row contents remain validated by the individual Knowledge query.
    pub fn recheck(
        &self,
        original: &Arc<SnapshotContext>,
        read: &ProjectReadAdmission<'_>,
    ) -> Result<(), KnowledgeEmbeddingError> {
        let project = index::check_read_project(original, read)?;
        if !Arc::ptr_eq(original, &self.original) {
            return Err(index::KnowledgeIndexError::DifferentRequest.into());
        }
        if project.scope != &self.scope || branch(project.version.lance_branch()) != self.branch {
            return Err(index::KnowledgeIndexError::WrongAdmission.into());
        }
        original.check()?;
        Ok(())
    }
}

/// Retain the actual Source generation for an admitted Live context without
/// projecting document rows, collecting slugs, opening NN or running inference.
pub async fn capture_native_knowledge_context(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
) -> Result<Option<KnowledgeContextCapture>, KnowledgeEmbeddingError> {
    let project = index::check_read_project(original, read)?;
    let scope = project.scope;
    let corpus = index::local_corpus(scope)?;
    let selected = branch(project.version.lance_branch());
    if !graphit_core::local_files::directory_exists(&corpus.path)? {
        return Ok(None);
    };
    let before = || -> Result<(), KnowledgeEmbeddingError> {
        index::check_read(original, read)?;

        Ok(())
    };
    before()?;
    let uri = corpus.uri.clone();
    let pin = BranchSnapshot::capture_observed(&uri, selected.as_deref(), None).await?;
    before()?;
    crate::knowledge_index::semantic::dimensions(pin.schema())?;
    let source = pin;
    let generation = index::generation(&uri, &source);
    let captured = KnowledgeContextCapture {
        original: Arc::clone(original),
        scope: project.scope.clone(),
        branch: selected,
        source,
        generation,
    };
    Ok(Some(captured))
}
