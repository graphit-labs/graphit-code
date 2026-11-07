//! Installed remote readers keep the real admission, lease and SDK generation together.
pub(crate) mod semantic;

use crate::{
    native_storage_credentials::{BrokerStorage, broker_credentials},
    remote_native_mount::{ErrorKind, Mount, ReadAdmission, Selection},
};
use graphit_ast::{
    indexed_source::read_indexed_source,
    source_inspection::{IndexedSource, SourceInspection, inspect_source},
};
use graphit_auth::ServiceCredentialLease;
use graphit_core::text_slice;
use graphit_data::{
    knowledge_index::{
        KnowledgeGeneration, KnowledgeHeadRead, KnowledgeHeadReadLimits, KnowledgeIndexError,
        KnowledgeIndexReadLimits,
    },
    knowledge_search::{KnowledgeLexicalQuery, KnowledgeSearchLimits},
    knowledge_snapshot::{
        KnowledgeSnapshotPage, read_knowledge_heads_snapshot_guarded,
        read_knowledge_snapshot_guarded, search_knowledge_snapshot_guarded,
    },
};
use graphit_hub::{ArtifactId, ConcreteVersion, remote_metadata::MetadataLimits};
use graphit_knowledge::preview::MarkdownPreviewLimits;
use std::sync::Arc;

pub(crate) struct KnowledgePage {
    pub(crate) generation: KnowledgeGeneration,
    pub(crate) page: KnowledgeSnapshotPage,
    pub(crate) focused_source:
        Option<graphit_data::knowledge_reference_query::KnowledgeReferenceSource>,
}

pub(crate) struct Opened<'a> {
    pub(crate) selection: Selection<'a>,
    pub(crate) mount: Mount,
    storage: BrokerStorage,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn search_admitted(
    state: &super::State,
    read: &graphit_auth::ImportedKnowledgeReadAdmission<'_>,
    id: &ArtifactId,
    version: &ConcreteVersion,
    input: &crate::foreground_knowledge::search::Input,
    limits: KnowledgeSearchLimits,
    session: &str,
) -> Result<Option<crate::foreground_knowledge::search::SearchOutcome>, ErrorKind> {
    use crate::foreground_knowledge::search;
    let Some(selection) = Selection::capture(
        ReadAdmission::Knowledge(read),
        &state.policy.registry,
        id,
        version,
        state.policy.index,
    )?
    else {
        return Ok(None);
    };
    let mut service = state
        .auth
        .issuer()
        .capture_imported_knowledge_hub_service(read, selection.record().provider_name())?;
    let verifier = if service.mode()? == graphit_auth::ServiceMode::BrokerNative {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotConnected))?
                .get(&mut service)
                .map_err(|error| ErrorKind::Operation(std::io::Error::other(error)))?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => service.native_credential(verifier).await?,
        None => service.credential()?,
    };
    let metadata = crate::native_publication::metadata_limits(
        state.policy.content,
        state.policy.registry.limits().projection(),
    );
    let opened = open(
        selection,
        &lease,
        &state.native_http,
        state.native_allow_loopback_http,
        metadata,
    )
    .await?;
    let preview = search::preview_limits(state.policy.content).map_err(ErrorKind::Knowledge)?;
    let rerank = crate::foreground_knowledge::imported_search::ImportedRerankContext {
        auth: &state.auth,
        owner: crate::foreground_knowledge::imported_search::ImportedRerankOwner::Source {
            hook: state.hook.as_deref(),
            installation: &state.policy.installation,
            local_models: &state.local_models,
        },
        verifiers: state.native_verifiers.as_deref(),
    };
    let outcome = if input.mode == search::Mode::Fts {
        opened
            .search(
                &lease,
                input.native(session),
                limits,
                preview,
                read,
                input,
                &rerank,
            )
            .await?
    } else {
        let resources = state.semantic_model()?;
        let binding = serde_json::to_string(&(session, input.mode, input.metric))
            .map_err(|error| ErrorKind::Operation(std::io::Error::other(error)))?;
        semantic::query(
            &opened,
            &lease,
            read,
            resources.as_deref(),
            &rerank,
            input,
            crate::knowledge_semantic::KnowledgeSemanticSearchOptions {
                page: input.native(&binding),
                mode: input.vector_mode(),
                metric: input.metric.native(),
                limits,
                preview,
            },
        )
        .await?
    };
    opened.check(&lease)?;
    Ok(Some(outcome))
}

/// Native CLI/MCP Source entry. Only a local backend returns `None`.
#[allow(clippy::too_many_arguments)]
pub(super) async fn inspect_with_auth(
    state: &super::State,
    id: &ArtifactId,
    version: &ConcreteVersion,
    source: &crate::ast_source::SourceParameters,
    fallback: Option<graphit_auth::ProjectCapture<'_>>,
    request: &graphit_hub::policy::RequestContext<'_>,
    credential: &str,
    local: bool,
) -> Result<Option<SourceInspection>, ErrorKind> {
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, request)
    } else {
        issuer.authenticate_runtime(credential, request)
    }
    .map_err(ErrorKind::Auth)?;
    let read = issuer
        .admit_authenticated_imported_ast_read(&caller, state.auth.installation())
        .map_err(ErrorKind::Auth)?;
    let Some(selection) = Selection::capture(
        ReadAdmission::Ast(&read),
        &state.policy.registry,
        id,
        version,
        state.policy.index,
    )?
    else {
        return Ok(None);
    };
    let mut service = issuer.capture_hub_service(&caller, selection.record().provider_name())?;
    let verifier = if service.mode()? == graphit_auth::ServiceMode::BrokerNative {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotConnected))?
                .get(&mut service)
                .map_err(|error| ErrorKind::Operation(std::io::Error::other(error)))?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => service.native_credential(verifier).await?,
        None => service.credential()?,
    };
    let limits = crate::native_publication::metadata_limits(
        state.policy.content,
        state.policy.registry.limits().projection(),
    );
    let opened = open(
        selection,
        &lease,
        &state.native_http,
        state.native_allow_loopback_http,
        limits,
    )
    .await?;
    let selectors = source.selection();
    let result = if source.has_entity() {
        opened
            .entity(
                &lease,
                source,
                &selectors,
                state.policy.native_install.graph,
                state.policy.graph_query,
            )
            .await?
    } else if let Some(result) = opened.source(&lease, &source.path, &selectors).await? {
        result
    } else {
        use graphit_core::artifact_store::{CapturedSourceReader, StoreOwner};
        let project = fallback.ok_or_else(|| {
            ErrorKind::Inspection(
                graphit_ast::source_inspection::SourceInspectionError::PhysicalUnavailable,
            )
        })?;
        if !matches!(project.scope.live.owner(), StoreOwner::Project { project_id } if *project_id == opened.selection.source().producer())
        {
            return Err(ErrorKind::InvalidBinding);
        }
        let authority = graphit_auth::ProjectRecordAuthority::new(issuer);
        let project_read = authority
            .admit_authenticated_read(
                &caller,
                request,
                graphit_auth::ScopeInput {
                    installation: state.auth.installation(),
                    project: Some(project),
                    providers: &[],
                    binding: state.policy.binding,
                    global_version: "",
                },
                graphit_auth::ProjectReadModule::Ast,
            )
            .map_err(ErrorKind::Auth)?;
        project_read.recheck().map_err(ErrorKind::Auth)?;
        let physical = CapturedSourceReader::capture_project(
            project.scope.live,
            std::path::Path::new(&source.path),
            graphit_fs::NodeKind::File,
            &state.policy.file,
            state.policy.read,
            request.snapshot(),
        )?;
        let result = inspect_source(
            IndexedSource::Missing,
            Some(graphit_ast::source_inspection::PhysicalSource {
                reader: &physical,
                relative: std::path::Path::new(""),
            }),
            &selectors,
            request.snapshot(),
        )
        .map_err(ErrorKind::Inspection)?;
        project_read.recheck().map_err(ErrorKind::Auth)?;
        result
    };
    opened.check(&lease)?;
    read.recheck().map_err(ErrorKind::Auth)?;
    Ok(Some(result))
}

/// Open the exact remote selection captured before acquiring its credentials.
pub(crate) async fn open<'a>(
    selection: Selection<'a>,
    lease: &ServiceCredentialLease<'_>,
    client: &reqwest::Client,
    allow_loopback_http: bool,
    limits: MetadataLimits,
) -> Result<Opened<'a>, ErrorKind> {
    selection.check(lease)?;
    let storage = broker_credentials(
        lease,
        selection.source().producer(),
        client,
        allow_loopback_http,
    )
    .await
    .map_err(ErrorKind::Credentials)?;
    selection.check(lease)?;
    let record = selection.record();
    if record.bucket() != storage.bucket
        || record.region() != storage.region
        || record.prefix() != storage.prefix
        || record.endpoint().map(|value| value.trim_end_matches('/'))
            != storage
                .endpoint
                .as_deref()
                .map(|value| value.trim_end_matches('/'))
    {
        return Err(ErrorKind::InvalidBinding);
    }
    let mount = Mount::open(&selection, lease, storage.store.clone(), limits).await?;
    let opened = Opened {
        selection,
        mount,
        storage,
    };
    opened.check(lease)?;
    Ok(opened)
}

impl Opened<'_> {
    pub(crate) fn check(&self, lease: &ServiceCredentialLease<'_>) -> Result<(), ErrorKind> {
        self.selection.check(lease)?;
        self.storage.check().map_err(ErrorKind::Credentials)
    }
    fn data_check(&self, lease: &ServiceCredentialLease<'_>) -> Result<(), KnowledgeIndexError> {
        self.check(lease)
            .map_err(|error| KnowledgeIndexError::Request(std::io::Error::other(error)))
    }
    /// An explicit source miss permits the caller's separately admitted disk fallback.
    pub(crate) async fn source(
        &self,
        lease: &ServiceCredentialLease<'_>,
        path: &str,
        selectors: &text_slice::Request,
    ) -> Result<Option<SourceInspection>, ErrorKind> {
        self.check(lease)?;
        let Some(snapshot) = self.mount.source(&self.selection, lease).await? else {
            return Ok(None);
        };
        let original = self.selection.request().snapshot();
        let source = read_indexed_source(&snapshot, path, original)
            .await
            .map_err(ErrorKind::Indexed)?;
        self.check(lease)?;
        let indexed = source.source(original).map_err(ErrorKind::Indexed)?;
        if matches!(indexed, IndexedSource::Missing) {
            return Ok(None);
        }
        let inspected =
            inspect_source(indexed, None, selectors, original).map_err(ErrorKind::Inspection)?;
        self.check(lease)?;
        Ok(Some(inspected))
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn entity(
        &self,
        lease: &ServiceCredentialLease<'_>,
        source: &crate::ast_source::SourceParameters,
        selectors: &text_slice::Request,
        graph_policy: graphit_data::LocalGraphPolicy,
        options: graphit_ast::icebug_query::QueryOptions,
    ) -> Result<SourceInspection, ErrorKind> {
        self.check(lease)?;
        let graph = self
            .mount
            .graph(&self.selection, lease, graph_policy)
            .await?;
        let original = Arc::clone(self.selection.request().snapshot());
        let runtime = tokio::runtime::Handle::current();
        let input = source.clone();
        let query_original = Arc::clone(&original);
        // Always join the owned query. Its native interrupt observes this Original;
        // dropping the consumer does not detach a blocking query onto remote storage.
        let window = tokio::task::spawn_blocking(move || {
            input.resolve_remote_window(&graph, &query_original, &runtime, options)
        })
        .await
        .map_err(|error| ErrorKind::Operation(std::io::Error::other(error)))?
        .map_err(ErrorKind::Entity)?;
        self.check(lease)?;
        self.entity_at(lease, &source.path, selectors, window).await
    }
    /// Apply an extent resolved from this mount's exact graph on its owned query job.
    pub(crate) async fn entity_at(
        &self,
        lease: &ServiceCredentialLease<'_>,
        path: &str,
        selectors: &text_slice::Request,
        window: (usize, usize),
    ) -> Result<SourceInspection, ErrorKind> {
        self.check(lease)?;
        let original = Arc::clone(self.selection.request().snapshot());
        let snapshot = self
            .mount
            .source(&self.selection, lease)
            .await?
            .ok_or_else(|| {
                ErrorKind::Entity(graphit_ast::imported_graph::EntitySourceError::Unavailable)
            })?;
        let indexed = read_indexed_source(&snapshot, path, &original)
            .await
            .map_err(ErrorKind::Indexed)?;
        self.check(lease)?;
        let IndexedSource::Available(text) =
            indexed.source(&original).map_err(ErrorKind::Indexed)?
        else {
            return Err(ErrorKind::Entity(
                graphit_ast::imported_graph::EntitySourceError::Unavailable,
            ));
        };
        let selection = graphit_ast::entity_source::select_entity_source(
            text, window.0, window.1, selectors, &original,
        )
        .map_err(ErrorKind::Inspection)?;
        self.check(lease)?;
        Ok(SourceInspection {
            origin: graphit_ast::source_inspection::SourceOrigin::Indexed,
            selection,
        })
    }
    pub(crate) async fn page(
        &self,
        lease: &ServiceCredentialLease<'_>,
        slug: &str,
        limits: KnowledgeIndexReadLimits,
    ) -> Result<KnowledgePage, ErrorKind> {
        self.check(lease)?;
        let snapshot = self.mount.wiki(&self.selection, lease).await?;
        let page = read_knowledge_snapshot_guarded(
            self.selection.request().snapshot(),
            &snapshot,
            slug,
            limits,
            || self.data_check(lease),
        )
        .await
        .map_err(ErrorKind::Knowledge)?;
        self.check(lease)?;
        let generation = generation(&snapshot, self.selection.request().snapshot()).await?;
        let focused_source = match &page {
            KnowledgeSnapshotPage::Found { focused_source, .. } => {
                focused_source.as_deref().cloned()
            }
            _ => None,
        };
        self.check(lease)?;
        Ok(KnowledgePage {
            generation,
            page,
            focused_source,
        })
    }
    pub(crate) async fn head_by_slug(
        &self,
        lease: &ServiceCredentialLease<'_>,
        slug: &str,
        limits: graphit_data::knowledge_index::KnowledgeHeadReadLimits,
    ) -> Result<Option<graphit_data::knowledge_index::KnowledgeHead>, ErrorKind> {
        self.check(lease)?;
        let snapshot = self.mount.wiki(&self.selection, lease).await?;
        let mut failure = None;
        let result =
            graphit_data::knowledge_snapshot::read_knowledge_head_by_slug_snapshot_guarded(
                self.selection.request().snapshot(),
                &snapshot,
                slug,
                limits,
                || match self.check(lease) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        failure = Some(error);
                        Err(KnowledgeIndexError::InvalidField("external_guard"))
                    }
                },
            )
            .await;
        if let Some(error) = failure {
            return Err(error);
        }
        let result = result.map_err(ErrorKind::Knowledge)?;
        self.check(lease)?;
        Ok(result)
    }

    pub(crate) async fn head_by_source_path(
        &self,
        lease: &ServiceCredentialLease<'_>,
        source_path: &str,
        limits: graphit_data::knowledge_index::KnowledgeHeadReadLimits,
    ) -> Result<Option<graphit_data::knowledge_index::KnowledgeHead>, ErrorKind> {
        self.check(lease)?;
        let snapshot = self.mount.wiki(&self.selection, lease).await?;
        let mut failure = None;
        let result =
            graphit_data::knowledge_snapshot::read_knowledge_head_by_source_path_snapshot_guarded(
                self.selection.request().snapshot(),
                &snapshot,
                source_path,
                limits,
                || match self.check(lease) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        failure = Some(error);
                        Err(KnowledgeIndexError::InvalidField("external_guard"))
                    }
                },
            )
            .await;
        if let Some(error) = failure {
            return Err(error);
        }
        let result = result.map_err(ErrorKind::Knowledge)?;
        self.check(lease)?;
        Ok(result)
    }

    pub(crate) async fn heads(
        &self,
        lease: &ServiceCredentialLease<'_>,
        limits: KnowledgeHeadReadLimits,
    ) -> Result<KnowledgeHeadRead, ErrorKind> {
        self.check(lease)?;
        let snapshot = self.mount.wiki(&self.selection, lease).await?;
        let heads = read_knowledge_heads_snapshot_guarded(
            self.selection.request().snapshot(),
            &snapshot,
            limits,
            || self.data_check(lease),
        )
        .await
        .map_err(ErrorKind::Knowledge)?;
        self.check(lease)?;
        Ok(KnowledgeHeadRead::Published {
            generation: generation(&snapshot, self.selection.request().snapshot()).await?,
            heads,
        })
    }
    fn search_binding(
        &self,
        caller_binding: &str,
        limits: KnowledgeSearchLimits,
        preview: MarkdownPreviewLimits,
    ) -> Result<String, ErrorKind> {
        if caller_binding.len() > limits.max_query_bytes.get() {
            return Err(ErrorKind::Search(
                graphit_data::knowledge_search::KnowledgeSearchError::InvalidQuery("query_bytes"),
            ));
        }
        let source = self.selection.source();
        let binding = serde_json::to_string(&(
            source.producer(),
            source.id().as_str(),
            source.version().as_str(),
            self.selection.record(),
            caller_binding,
        ))
        .map_err(|error| ErrorKind::Operation(std::io::Error::other(error)))?;
        Ok(graphit_knowledge::search::preview_binding(
            &binding, preview,
        ))
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "service lease, imported admission, pinned query and rerank context remain explicit"
    )]
    pub(crate) async fn search(
        &self,
        lease: &ServiceCredentialLease<'_>,
        query: KnowledgeLexicalQuery<'_>,
        limits: KnowledgeSearchLimits,
        preview: MarkdownPreviewLimits,
        admission: &graphit_auth::ImportedKnowledgeReadAdmission<'_>,
        input: &crate::foreground_knowledge::search::Input,
        rerank: &crate::foreground_knowledge::imported_search::ImportedRerankContext<'_>,
    ) -> Result<crate::foreground_knowledge::search::SearchOutcome, ErrorKind> {
        self.check(lease)?;
        let snapshot = self.mount.wiki(&self.selection, lease).await?;
        let binding = self.search_binding(query.caller_binding, limits, preview)?;
        let query = KnowledgeLexicalQuery {
            caller_binding: &binding,
            ..query
        };
        let original = self.selection.request().snapshot();
        let page = search_knowledge_snapshot_guarded(
            original,
            &snapshot,
            &generation(&snapshot, self.selection.request().snapshot()).await?,
            query,
            limits,
            || self.data_check(lease),
            |body, summary, remaining| {
                graphit_knowledge::search::render_preview(
                    original, body, summary, preview, remaining,
                )
            },
        )
        .await
        .map_err(ErrorKind::Search)?;
        self.check(lease)?;
        let outcome = crate::foreground_knowledge::search::SearchOutcome::Lexical(page);
        // Disabled/empty pages must not capture or validate an embedding cursor.
        let Some((generation, slugs)) = outcome.rerank_selection() else {
            return Ok(outcome);
        };
        let Some(config) = rerank
            .configuration(original, input, slugs.len())
            .map_err(|error| ErrorKind::Semantic(Box::new(error)))?
        else {
            return Ok(outcome);
        };
        let guard = || {
            self.data_check(lease)
                .map_err(graphit_data::knowledge_embeddings::KnowledgeEmbeddingError::from)
        };
        let capture = graphit_data::knowledge_embeddings::capture_native_knowledge_embedding_snapshots_guarded(
            original, snapshot, None, limits.read, guard,
        ).await.map_err(|error| ErrorKind::Embedding(Box::new(error)))?;
        let selected = capture
            .read_selected_documents_guarded(original, &slugs, guard)
            .await
            .map_err(ErrorKind::Search)?;
        let result = rerank
            .score(admission, &config, input, generation, &slugs, &selected)
            .await;
        capture
            .recheck_guarded(original, guard)
            .map_err(|error| ErrorKind::Embedding(Box::new(error)))?;
        self.check(lease)?;
        let scores = result.map_err(|error| ErrorKind::Semantic(Box::new(error)))?;
        Ok(crate::foreground_knowledge::search::SearchOutcome::Reranked(Box::new(outcome), scores))
    }
}

async fn generation(
    snapshot: &graphit_data::lance_module_store::ModuleSnapshot,
    original: &graphit_core::file_io::SnapshotContext,
) -> Result<KnowledgeGeneration, ErrorKind> {
    let observed = snapshot.native_metadata(original).await?;
    Ok(KnowledgeGeneration {
        corpus_uri: observed.uri().to_owned(),
        version: snapshot.version(),
        physical_branch: None,
        inherited_target: None,
    })
}

#[cfg(test)]
mod tests;
