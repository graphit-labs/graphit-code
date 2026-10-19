//! Imported Knowledge queries use one Source/NN capture and its own authority.
use super::search::{SearchOutcome, WithoutModelPage};
use crate::{
    foreground_memory_jobs::MemoryJobResources,
    knowledge_semantic::{KnowledgeSemanticError, KnowledgeSemanticSearchOptions},
};
use graphit_auth::ImportedKnowledgeReadAdmission;
use graphit_daemon::model_service::EmbedInput;
use graphit_data::knowledge_embeddings::{
    KnowledgeEmbeddingIdentity, KnowledgeEmbeddingSearchReadiness,
};
use graphit_data::knowledge_search::{
    KnowledgeSearchError, KnowledgeVectorSearchMode, KnowledgeVectorSearchQuery,
};
use graphit_knowledge::{
    embedding_projection::KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE,
    imported_knowledge::CapturedImportedKnowledge,
};
use std::{io, sync::Arc};

pub(crate) async fn query(
    reader: &CapturedImportedKnowledge<'_>,
    admission: &ImportedKnowledgeReadAdmission<'_>,
    resources: Option<&MemoryJobResources>,
    options: KnowledgeSemanticSearchOptions<'_>,
) -> Result<SearchOutcome, KnowledgeSemanticError> {
    let original = admission.request().snapshot();
    original.check()?;
    admission.recheck()?;
    reader.recheck()?;
    let capture = reader.capture_semantic(options.limits.read).await?;
    let Some(resources) = resources else {
        if options.mode == KnowledgeVectorSearchMode::Semantic && options.page.cursor.is_some() {
            return Err(KnowledgeSemanticError::Search(Box::new(
                KnowledgeSearchError::InvalidCursor,
            )));
        }
        let mut empty = true;
        while let Some(documents) = capture.next_documents().await? {
            empty &= documents.is_empty();
        }
        let lexical = if options.mode == KnowledgeVectorSearchMode::Hybrid {
            Some(
                capture
                    .source_search(options.page, options.limits, options.preview)
                    .await?,
            )
        } else {
            None
        };
        capture.recheck()?;
        return Ok(SearchOutcome::WithoutModel(Some(WithoutModelPage {
            generation: capture.source_generation().clone(),
            semantic_generation: capture.semantic_generation(),
            lexical,
            empty,
        })));
    };
    let mut service = resources
        .auth
        .issuer()
        .capture_imported_knowledge_embedding_service(admission)?;
    if !Arc::ptr_eq(original, service.original()?.snapshot()) {
        return Err(KnowledgeSemanticError::ForeignRequest);
    }
    let metadata = crate::embedding_service::metadata(&mut service, &resources.execution()).await;
    capture.recheck()?;
    let metadata = metadata?;
    let dimensions =
        std::num::NonZeroU32::new(u32::try_from(metadata.dimensions).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "embedding dimensions exceed u32",
            )
        })?)
        .ok_or(KnowledgeSemanticError::MissingEmbeddingDimensions)?;
    let identity = KnowledgeEmbeddingIdentity::new(
        metadata.vector_id,
        dimensions,
        KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE.to_owned(),
    )?;
    let mut observed_inputs = Vec::new();
    while let Some(inputs) = capture.next_inputs(&identity).await? {
        observed_inputs.extend(inputs);
    }
    let inputs: Vec<_> = observed_inputs
        .iter()
        .map(|input| input.as_input())
        .collect();
    let readiness = capture.search_readiness(&identity, &inputs)?;
    let output = if readiness == KnowledgeEmbeddingSearchReadiness::Ready {
        capture.recheck()?;
        let result = resources
            .embed(
                &mut service,
                &metadata,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec![options.page.text.to_owned()],
                    deadline: original.required_deadline()?,
                },
            )
            .await;
        capture.recheck()?;
        Some(result?)
    } else {
        None
    };
    let page = capture
        .search(
            &identity,
            &inputs,
            KnowledgeVectorSearchQuery {
                page: options.page,
                mode: options.mode,
                vector: output.as_ref().map(|output| output.vectors[0].as_slice()),
                metric: options.metric,
            },
            options.limits,
            options.preview,
        )
        .await?;
    capture.recheck()?;
    Ok(SearchOutcome::Vector(Some(page)))
}

/// Borrow existing configuration and model owners; this carries no authority.
pub(crate) enum ImportedRerankOwner<'a> {
    Source {
        hook: Option<&'a crate::source_server::project_hook::HookPolicy>,
        installation: &'a graphit_core::project_paths::InstallationPaths,
        local_models: &'a crate::source_server::LocalModelControlSlot,
    },
    Browser(&'a crate::task_server::TaskServer),
}
pub(crate) struct ImportedRerankContext<'a> {
    pub(crate) auth: &'a graphit_daemon::auth_startup::DaemonAuth,
    pub(crate) owner: ImportedRerankOwner<'a>,
    pub(crate) verifiers: Option<&'a graphit_daemon::broker_verifiers::BrokerVerifierCache>,
}
impl ImportedRerankContext<'_> {
    /// Resolve configuration only after a nonempty page, before capturing AI authority.
    pub(crate) fn configuration(
        &self,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
        input: &super::search::Input,
        candidates: usize,
    ) -> Result<Option<graphit_core::config::ConfigSnapshot>, KnowledgeSemanticError> {
        if input.rerank == Some(false) || candidates == 0 {
            return Ok(None);
        }
        let config = match self.owner {
            ImportedRerankOwner::Source {
                hook, installation, ..
            } => hook
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
                .selected_configuration(installation, None, original),
            ImportedRerankOwner::Browser(server) => server
                .live_configuration(None, original)
                .map(|(_, config, _)| config),
        };
        let config = match config {
            Ok(config) => config,
            Err(error) if error.kind() == io::ErrorKind::Unsupported && input.rerank.is_none() => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        if match input.rerank {
            Some(value) => value,
            None => crate::search_rerank::configured(&config)?,
        } {
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }
    pub(crate) async fn score(
        &self,
        admission: &ImportedKnowledgeReadAdmission<'_>,
        config: &graphit_core::config::ConfigSnapshot,
        input: &super::search::Input,
        generation: &graphit_data::knowledge_index::KnowledgeGeneration,
        slugs: &[&str],
        selected: &graphit_data::knowledge_search::KnowledgeSelectedDocuments,
    ) -> Result<crate::search_rerank::RerankScores, KnowledgeSemanticError> {
        let original = admission.request().snapshot();
        let texts = super::search::rerank_texts(selected, generation, slugs)
            .map_err(|error| KnowledgeSemanticError::Search(Box::new(error)))?;
        let mut service = self
            .auth
            .issuer()
            .capture_imported_knowledge_rerank_service(admission)?;
        let control = if service.mode()? == graphit_auth::ServiceMode::LocalAuthorized {
            let slot = match self.owner {
                ImportedRerankOwner::Source { local_models, .. } => Arc::clone(local_models),
                ImportedRerankOwner::Browser(server) => server.local_model_control()?,
            };
            slot.read()
                .map_err(|_| io::Error::other("model control observation failed"))?
                .clone()
        } else {
            None
        };
        let policy =
            graphit_hub::policy::HubResourcePolicy::resolve(config).map_err(io::Error::other)?;
        crate::search_rerank::run(
            original,
            &mut service,
            control
                .as_ref()
                .map(|control| (control, self.auth.runtime().as_ref())),
            config,
            &policy,
            self.verifiers,
            &input.query,
            &texts,
        )
        .await
        .map_err(|error| KnowledgeSemanticError::Inference(Box::new(error)))
    }
    pub(crate) async fn copied(
        &self,
        admission: &ImportedKnowledgeReadAdmission<'_>,
        reader: &CapturedImportedKnowledge<'_>,
        input: &super::search::Input,
        limits: graphit_data::knowledge_index::KnowledgeIndexReadLimits,
        outcome: SearchOutcome,
    ) -> Result<SearchOutcome, KnowledgeSemanticError> {
        let Some((generation, slugs)) = outcome.rerank_selection() else {
            return Ok(outcome);
        };
        let Some(config) =
            self.configuration(admission.request().snapshot(), input, slugs.len())?
        else {
            return Ok(outcome);
        };
        let capture = reader
            .capture_source(limits)
            .await
            .map_err(imported_source)?;
        let selected = capture
            .read_selected_documents(&slugs)
            .await
            .map_err(imported_source)?;
        let result = self
            .score(admission, &config, input, generation, &slugs, &selected)
            .await;
        capture.recheck().map_err(imported_source)?;
        let scores = result?;
        Ok(SearchOutcome::Reranked(Box::new(outcome), scores))
    }
    pub(crate) async fn snapshot(
        &self,
        admission: &ImportedKnowledgeReadAdmission<'_>,
        capture: &graphit_data::knowledge_embeddings::KnowledgeEmbeddingSnapshotCapture,
        input: &super::search::Input,
        outcome: SearchOutcome,
        mut guard: impl FnMut()
            -> Result<(), graphit_data::knowledge_embeddings::KnowledgeEmbeddingError>,
    ) -> Result<SearchOutcome, KnowledgeSemanticError> {
        let original = admission.request().snapshot();
        let Some((generation, slugs)) = outcome.rerank_selection() else {
            return Ok(outcome);
        };
        let Some(config) = self.configuration(original, input, slugs.len())? else {
            return Ok(outcome);
        };
        let selected = capture
            .read_selected_documents_guarded(original, &slugs, &mut guard)
            .await
            .map_err(|error| KnowledgeSemanticError::Search(Box::new(error)))?;
        let result = self
            .score(admission, &config, input, generation, &slugs, &selected)
            .await;
        capture.recheck_guarded(original, &mut guard)?;
        admission.recheck()?;
        let scores = result?;
        Ok(SearchOutcome::Reranked(Box::new(outcome), scores))
    }
}
fn imported_source(
    error: graphit_knowledge::imported_knowledge::ImportedKnowledgeSourceError,
) -> KnowledgeSemanticError {
    use graphit_knowledge::imported_knowledge::ImportedKnowledgeSourceError as E;
    match error {
        E::Reader(error) => error.into(),
        E::Embedding(error) => error.into(),
        E::Query(error) => KnowledgeSemanticError::Search(Box::new(error)),
    }
}
