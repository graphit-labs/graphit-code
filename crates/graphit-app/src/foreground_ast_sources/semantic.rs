//! Immediate per-FILE structural publication and independent prepared NN inference.
use super::*;
use crate::foreground_memory_jobs::{MemoryJobResources, background_embedding_request};
use arrow_array::{Array, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, UInt64Array};
use graphit_ast::{
    antlr_engine::generic_model::ParsedGenericAntlrDocument,
    antlr_engine::model::{AntlrProjection, ParsedAntlrDocument},
    antlr_engine::preprocessing::{detect_cobol_format, prepare_cobol},
    catalog::{EffectiveProfile, Selection},
    indexed_source::MissingSourceReason,
    native_nn::{
        NativeNnAntlrGenericProjection, NativeNnSource, native_nn_antlr_generic_projections,
    },
    native_nn_physical_source::capture_native_nn_physical_source,
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    semantic_graph::antlr_generic::{AstGraphAntlrGenericInput, assemble_graph_antlr_generic},
    semantic_graph::{
        AstGraphAntlrInput, AstGraphJsonInput, AstGraphTsInput, assemble_graph_antlr,
        assemble_graph_json, assemble_graph_ts,
    },
    semantic_graph::{AstGraphGenericInput, assemble_graph_generic},
    semantic_model::{JsonProjection, ParsedJsonDocument},
    source_index_locator::{SourceIndexPresence, open_local_source_index},
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_generic_model::PreparedGenericTreeSitterModel,
    tree_sitter_model::{ParsedTreeSitterDocument, TsProjection},
};
use graphit_daemon::model_service::EmbedInput;
use std::collections::HashMap;
use tokio_stream::StreamExt;
#[path = "generic.rs"]
pub(super) mod generic;
#[path = "semantic_reuse.rs"]
mod reuse;
use reuse::{FileEmbedding, embedding_key, file_embeddings};
#[path = "semantic_checkpoint.rs"]
pub(crate) mod checkpoint;

/// Structure was committed successfully; only the independent embedding work
/// failed. The watcher must retain that error without parsing the FILEs again.
#[derive(Debug)]
pub(crate) struct InferenceFailure {
    error: io::Error,
    pending_files: BTreeSet<String>,
    model_reconciled: bool,
}
impl std::fmt::Display for InferenceFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}
impl std::error::Error for InferenceFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn only_inference_failed(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|cause| cause.is::<InferenceFailure>())
}

pub(crate) fn inference_model_reconciled(error: &io::Error) -> bool {
    error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<InferenceFailure>())
        .is_some_and(|failure| failure.model_reconciled)
}

pub(crate) fn inference_pending_files(error: &io::Error) -> Option<&BTreeSet<String>> {
    error
        .get_ref()?
        .downcast_ref::<InferenceFailure>()
        .map(|failure| &failure.pending_files)
}

#[derive(Debug, Default)]
pub(crate) struct AstSemanticProgress {
    pub version: Option<u64>,
    pub pending_files: BTreeSet<String>,
    pub model_reconciled: bool,
}

impl ForegroundAstSources {
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) async fn index_semantic(
        self: &Arc<Self>,
        project: ProjectPaths,
        selected: ProjectContext,
        resources: Option<Arc<MemoryJobResources>>,
        selection: Option<BTreeSet<String>>,
        priority_paths: BTreeSet<String>,
        model_refresh: bool,
        background_cancellation: Cancellation,
        cancellation: Cancellation,
    ) -> io::Result<AstSemanticProgress> {
        if cancellation.is_cancelled() {
            return Ok(AstSemanticProgress::default());
        }
        let owner = Arc::clone(self);
        let runtime = tokio::runtime::Handle::current();
        // Syn/Rc and all parser borrows stay local. Await the owned join even on
        // cancellation; cooperative checks stop ingress, then SDK effects finish.
        tokio::task::spawn_blocking(move || {
            runtime.block_on(owner.semantic_job(
                &project,
                &selected,
                resources.as_deref(),
                selection,
                priority_paths,
                None,
                model_refresh,
                &background_cancellation,
                &cancellation,
                None,
            ))
        })
        .await
        .map_err(err)?
    }

    pub(super) async fn semantic_job(
        &self,
        project: &ProjectPaths,
        selected: &ProjectContext,
        resources: Option<&MemoryJobResources>,
        selection: Option<BTreeSet<String>>,
        priority_paths: BTreeSet<String>,
        compiler_restore_scope: Option<BTreeSet<String>>,
        model_refresh: bool,
        background_cancellation: &Cancellation,
        cancellation: &Cancellation,
        external_pending: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> io::Result<AstSemanticProgress> {
        let request =
            graphit_hub::policy::RequestContext::service(self.policy.content(), cancellation)
                .map_err(err)?;
        let original = request.snapshot();
        let auth_error = |error| super::request_auth_error(error, &request);
        let publication_started = std::cell::Cell::new(false);
        let pending_files = std::cell::RefCell::new(BTreeSet::new());
        let model_reconciled = std::cell::Cell::new(!model_refresh);
        let result = Box::pin(async {
            if project.root() != selected.root() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let config = project_config(&self.installation, project, &self.base, original)?;
            let modules = ModuleSettings::resolve(&config)?;
            if !modules.enabled(Module::Ast) {
                return Ok(None);
            }
            let captured =
                CapturedPublicationScope::project(project, selected.id, self.scope, original)?;
            let record = captured.binding()?;
            let project_capture = ProjectCapture {
                scope: ScopeCapture {
                    live: &captured,
                    record: &record,
                },
                version: &selected.version,
            };
            let runtime_key = self.auth.runtime().check(original).map_err(err)?;
            let credential =
                std::str::from_utf8(runtime_key.secret(original).map_err(err)?).map_err(err)?;
            let caller = self
                .auth
                .issuer()
                .authenticate_runtime(credential, &request)
                .map_err(auth_error)?;
            let authority = ProjectRecordAuthority::new(self.auth.issuer());
            let scope = ScopeInput {
                installation: self.auth.installation(),
                project: Some(project_capture),
                providers: &[],
                binding: self.binding,
                global_version: "",
            };
            let read = authority
                .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Ast)
                .map_err(auth_error)?;
            let write = authority
                .admit_authenticated_write(&caller, &request, scope, ProjectWriteOperation::Ast)
                .map_err(auth_error)?;
            let recheck = || {
                request.check()?;
                read.recheck().map_err(auth_error)?;
                write.recheck().map_err(auth_error)?;
                Ok(())
            };
            recheck()?;
            let raw = match open_local_source_index(
                self.auth.installation(),
                project_capture,
                original,
                self.native,
            )
            .await
            .map_err(err)?
            {
                SourceIndexPresence::Absent => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Raw source corpus is absent",
                    ));
                }
                SourceIndexPresence::Available(raw) => raw,
            };
            // Watcher deltas are ordered before the remaining files from a
            // yielded full scan. Resolve directory events through the Raw path
            // index; no source bodies or corpus-wide query are needed.
            let priority_paths = if priority_paths.is_empty() { priority_paths } else {
                source_paths_under(raw.snapshot(original).map_err(err)?, priority_paths, original).await?
            };
            let profiles = self
                .profiles
                .load(&self.installation, project, &config, original)?;
            let mut progressive = super::progressive::ProgressivePublication::open(
                self.auth.installation(), project_capture, self.native,
                Arc::clone(original), Arc::clone(&self.live), &recheck,
            ).await?;
            let (pending_sender, pending_receiver) = match external_pending {
                Some(sender) => (sender, None),
                None => { let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<String>(); (sender, Some(receiver)) },
            };
            let mut published_version = None;
            let mut selected_paths = Vec::new();
            match selection {
                Some(roots) => for path in {
                    let index = progressive.capture_index().await?;

                    let mut paths = source_paths_under(raw.snapshot(original).map_err(err)?, roots.iter().cloned(), original).await?;
                    for root in roots {
                        paths.extend(index.graph.files_under(&root, original)?.into_iter().map(|file| file.path));
                    }
                    paths
                } {
                    recheck()?;
                    let indexed = raw.read(&path, original).await.map_err(err)?;
                    if indexed.missing_reason() == Some(MissingSourceReason::FileAbsent) {
                        publication_started.set(true);
                        published_version = Some(progressive.remove_file(&path, &recheck).await?);
                    } else {
                        selected_paths.push(path);
                    }
                },
                None => {
                    selected_paths = raw.paths(original).await.map_err(err)?;
                    let retained = selected_paths.iter().map(String::as_str).collect::<BTreeSet<_>>();
                    progressive.retain_paths(&retained, &recheck).await?;
                    let previous_files = {
                        let index = progressive.capture_index().await?;

                        index.graph.files(original)?
                    };
                    for previous in previous_files {
                        recheck()?;
                        if !retained.contains(previous.path.as_str()) {
                            publication_started.set(true);
                            published_version = Some(progressive.remove_file(&previous.path, &recheck).await?);
                        }
                    }
                },
            }
            let parsed_paths = selected_paths.iter().cloned().collect::<BTreeSet<_>>();
            let mut groups: Vec<(&EffectiveProfile, Vec<String>)> = Vec::new();
            let mut native_files = Vec::new();
            for path in selected_paths {
                recheck()?;
                let Selection::Candidates(candidates) =
                    profiles.resolve(Path::new(&path), original)?
                else {
                    if let Some(family) = profiles.native_scip_compiler(Path::new(&path), original)? {
                        let image = family.image(&config)?;
                        let previous = {
                            let index = progressive.capture_index().await?;

                            index.graph.file_observation(&path, original)?
                        };
                        if previous.as_ref().is_some_and(|file| file.scip_compiler.as_deref() != Some(image.as_str())) {
                            publication_started.set(true);
                            published_version = Some(progressive.remove_file(&path, &recheck).await?);
                        }
                        native_files.push((family, path));
                    } else {
                        publication_started.set(true);
                        published_version = Some(progressive.remove_file(&path, &recheck).await?);
                    }
                    continue;
                };
                for profile in candidates {
                    if let Some((_, paths)) = groups
                        .iter_mut()
                        .find(|(existing, _)| std::ptr::eq(*existing, profile))
                    {
                        paths.push(path.clone());
                    } else {
                        groups.push((profile, vec![path.clone()]));
                    }
                }
            }
            native_files.sort_by(|(_, left), (_, right)| {
                priority_paths.contains(right).cmp(&priority_paths.contains(left))
                    .then_with(|| left.cmp(right))
            });
            let mut compiler_families = BTreeMap::<super::scip::ScipCompiler, BTreeSet<String>>::new();
            for (profile, paths) in &groups {
                for path in paths {
                    if let Some(family) = profiles.scip_compiler(Path::new(path), Some(profile.language().value()), original)? {
                        compiler_families.entry(family).or_default().insert(path.clone());
                    }
                }
            }
            for (family, path) in &native_files {
                compiler_families.entry(family.clone()).or_default().insert(path.clone());
            }
            let mut restored_files = BTreeMap::new();
            let compiler_available = self.scip.docker_command(project.root()).is_ok();
            // Only unfinished compiler work participates. Missing Docker does
            // not read prepared payloads or expand an ordinary syntax delta.
            if compiler_available {
                let index = progressive.capture_index().await?;

                recheck()?;
                for observation in index.graph.pending_compiler_files(original)? {
                    if parsed_paths.contains(&observation.path) { continue; }
                    if compiler_restore_scope.as_ref().is_some_and(|roots| !roots.iter().any(|root| {
                        Path::new(&observation.path).starts_with(Path::new(root))
                    })) { continue; }
                    if let Some(compiler) = profiles.selected_scip_compiler(Path::new(&observation.path), original)? {
                        // A pending parser snapshot belongs to its selected compiler.
                        // Configuration changes must reparse, not replay another producer.
                        let image = compiler.image(&config)?;
                        if observation.scip_compiler.as_deref() == Some(image.as_str()) {
                            compiler_families.entry(compiler).or_default().insert(observation.path.clone());
                            restored_files.insert(observation.path.clone(), observation);
                        }
                    }
                }
            }
            let mut compiler_images = BTreeMap::<String, String>::new();
            for (compiler, paths) in &compiler_families {
                let image = compiler.image(&config)?;
                for path in paths { compiler_images.insert(path.clone(), image.clone()); }
            }
            let mut compiler_families = compiler_families.into_iter().collect::<Vec<_>>();
            compiler_families.sort_by(|(left_family, left), (right_family, right)| {
                right.iter().any(|path| priority_paths.contains(path))
                    .cmp(&left.iter().any(|path| priority_paths.contains(path)))
                    .then_with(|| left_family.cmp(right_family))
            });
            let native_priority = native_files.iter().any(|(_, path)| priority_paths.contains(path));
            let deferred_compiler_families = if native_priority {
                let boundary = compiler_families.partition_point(|(_, paths)| paths.iter().any(|path| priority_paths.contains(path)));
                compiler_families.split_off(boundary)
            } else { Vec::new() };
            let compiler = async |compiler_families: Vec<(super::scip::ScipCompiler, BTreeSet<String>)>| {
                let mut scip_outputs = Vec::new();
                if !compiler_available { return Ok(scip_outputs); }
            for (family, paths) in compiler_families {
                if background_cancellation.is_cancelled() && !cancellation.is_cancelled() {
                    recheck()?;
                    continue;
                }
                let paths = paths.into_iter().collect::<Vec<_>>();
                let inputs = super::scip::CapturedInputs::capture(
                    &raw, project.root(), family.clone(), &paths, original,
                ).await?;
                let output = self.scip.index(
                    family.clone(), selected, &self.installation, &config, &self.policy,
                    background_cancellation, &recheck,
                ).await;
                let output = match output {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted
                        && background_cancellation.is_cancelled()
                        && !cancellation.is_cancelled() => {
                        recheck()?;
                        None
                    }
                    result => result?,
                };
                if background_cancellation.is_cancelled() && !cancellation.is_cancelled() {
                    recheck()?;
                    continue;
                }
                if let Some(output) = output {
                    inputs.recheck(original)?;
                    scip_outputs.push((family, inputs, output));
                }
            }
                Ok::<_, io::Error>(scip_outputs)
            };
            recheck()?;
            Box::pin(async {
                let max = self.policy.content().max_input_bytes;
                let identity = identity_limits(&self.policy)?;
                let mut files = BTreeMap::<String, Vec<&EffectiveProfile>>::new();
                for (profile, paths) in groups {
                    for path in paths { files.entry(path).or_default().push(profile); }
                }
                eprintln!("Graphit AST structural job for {} version {:?}: Raw generation {}; {} selected files",
                    selected.id, selected.version.version(), raw.version(), files.len() + native_files.len());
                let producer = Box::pin(async {
                    // Reuse the temporary writer locators across this compiler
                    // batch; drop them when production ends, independently of NN.
                    let _graph_owner = progressive.capture_index().await?;
                    let result = Box::pin(async {
                        let graph =
                            graphit_data::graph_writer::IcebugWriter::new(Arc::clone(original))?;
                        let scip_index = super::graph::ScipIndex::new(&graph, selected)?;
                        let mut prioritized = Vec::new();
                        for path in &priority_paths {
                            if let Some(profiles) = files.remove(path) {
                                prioritized.push((path.clone(), profiles));
                            }
                        }
                        let mut files = prioritized.into_iter().chain(files).collect::<Vec<_>>();
                        let deferred_files = if native_priority {
                            let boundary = files.partition_point(|(path, _)| priority_paths.contains(path));
                            files.split_off(boundary)
                        } else { Vec::new() };
                        let priority_native_files = if native_priority {
                            let boundary = native_files.partition_point(|(_, path)| priority_paths.contains(path));
                            native_files.drain(..boundary).collect::<Vec<_>>()
                        } else { Vec::new() };
                        let syntax = async |mut graph: graphit_data::graph_writer::IcebugWriter,
                            files: Vec<(String, Vec<&EffectiveProfile>)>,
                            progressive: &mut super::progressive::ProgressivePublication<'_>,
                            published_version: &mut Option<u64>| {
                        let mut revisions = BTreeMap::new();
                        let mut embedded_models = generic::Models::new(&self.parsers, profiles.profiles(original)?);
                        let mut files = files.into_iter();
                        while let Some((path, file_profiles)) = files.next() {
                            // A new watcher batch may stop only between complete
                            // FILE publications. Lifetime/auth/deadline remain
                            // terminal and are checked before yielding ingress.
                            recheck()?;
                            if background_cancellation.is_cancelled() {
                                let mut pending = pending_files.borrow_mut();
                                pending.insert(path);
                                pending.extend(files.map(|(path, _)| path));
                                break;
                            }
                            let indexed = raw.read(&path, original).await.map_err(err)?;
                            let (next_graph, file) = SyntaxPreparation {
                                owner: self, project_capture, profiles: &profiles,
                                original, identity,
                                embedding: modules.enabled(Module::Embedding), scip_index: &scip_index,
                            }.prepare(graph, &path, file_profiles, indexed, &mut embedded_models,
                                compiler_images.get(&path).map(String::as_str), &recheck).await?;
                            graph = next_graph;
                            publication_started.set(true);
                            let revision = progressive.publish_file(file, &recheck).await?;
                            revisions.insert(path.clone(), revision);
                            *published_version = Some(revision);
                            let _ = pending_sender.send(path.clone());
                            #[cfg(all(test, target_os = "linux"))]
                            self.after_file_publication(&path, original).await?;
                        }
                        Ok::<_, io::Error>((graph, revisions))
                        };
                        let import_compiled = async |graph: &graphit_data::graph_writer::IcebugWriter,
                            outputs: &[(super::scip::ScipCompiler, super::scip::CapturedInputs, super::scip::ScipOutput)],
                            progressive: &super::progressive::ProgressivePublication<'_>| {
                        for (family, inputs, output) in outputs {
                            let image = family.image(&config)?;
                            recheck()?;
                            if background_cancellation.is_cancelled() { continue; }
                            if let Err(error) = scip_index.import_compiler(
                                graph, &image, output, inputs, &self.policy, original,
                            ) {
                                recheck()?;
                                inputs.recheck(original)?;
                                eprintln!("AST SCIP {} output unavailable; using syntax SCIP: {error}", family.as_str());
                            } else {
                                let live_index = progressive.capture_index().await?;
                                let _fence = live_index.write_fence(original).await?;
                                recheck()?;
                                scip_index.publish_externals(graph, &image, &live_index.graph, original)?;
                                recheck()?;
                            }
                        }
                            Ok::<_, io::Error>(())
                        };
                        let publish_native = async |mut graph: graphit_data::graph_writer::IcebugWriter,
                            native_files: Vec<(super::scip::ScipCompiler, String)>,
                            progressive: &mut super::progressive::ProgressivePublication<'_>,
                            published_version: &mut Option<u64>| {
                        for (compiler, path) in native_files {
                            recheck()?;
                            if background_cancellation.is_cancelled() {
                                pending_files.borrow_mut().insert(path);
                                continue;
                            }
                            let indexed = raw.read(&path, original).await.map_err(err)?;
                            let components = path.split('/').collect::<Vec<_>>();
                            let physical = match indexed.source(original).map_err(err)? {
                                IndexedSource::Available(_) => None,
                                IndexedSource::Missing if indexed.missing_reason() == Some(MissingSourceReason::TextNotStored) => Some(
                                    capture_native_nn_physical_source(project_capture,&components,self.policy.copy(),self.policy.retained(),
                                        SourceReadLimits::new(max,64*1024,max as u64)?,original).map_err(err)?
                                ),
                                _ => return Err(io::Error::new(io::ErrorKind::InvalidData,"SCIP path has no Raw source association")),
                            };
                            let text = match &physical {
                                Some(capture) => capture.source(original).map_err(err)?,
                                None => match indexed.source(original).map_err(err)? {
                                    IndexedSource::Available(text) => text,
                                    _ => unreachable!("source checked above"),
                                },
                            };
                            let source_sha256: [u8;32] = Sha256::digest(text.as_bytes()).into();
                            let mut file = super::progressive::FilePublication::new(path.clone(), source_sha256, indexed.version());
                            file.scip_compiler = Some(compiler.image(&config)?);
                            let batch = {
                                let document = scip_index.compiler_document(&graph,&path,original)?
                                    .ok_or_else(||io::Error::new(io::ErrorKind::Unsupported,
                                        format!("no SCIP compiler document or fallback grammar for {path}")))?;
                                graphit_ast::semantic_graph::scip_document::assemble_scip_source(
                                    &document,
                                    match &physical {
                                        Some(capture) => NativeNnSource::Physical { capture, raw:&indexed },
                                        None => NativeNnSource::Indexed(&indexed),
                                    }, identity, original,
                                ).map_err(err)?
                            };
                            graph = super::graph::materialize(graph,batch,None,&scip_index,text,
                                modules.enabled(Module::Embedding).then_some((match &physical {
                                    Some(capture) => NativeNnSource::Physical { capture, raw: &indexed },
                                    None => NativeNnSource::Indexed(&indexed),
                                }, identity)),&mut file,original)?;
                            indexed.source(original).map_err(err)?;
                            if let Some(source) = physical { source.revalidate(original).map_err(err)?; }
                            recheck()?;
                            publication_started.set(true);
                            *published_version = Some(progressive.publish_file(file, &recheck).await?);
                            let _ = pending_sender.send(path.clone());
                            #[cfg(all(test, target_os = "linux"))]
                            self.after_file_publication(&path, original).await?;
                        }
                            Ok::<_, io::Error>(graph)
                        };
                        // A compiler-only delta is published after its own required
                        // families, before unrelated compiler or syntax tail work.
                        // Both phases reuse the same parsers, staging and FILE publisher.
                        let (compiled, parsed) = tokio::join!(
                            compiler(compiler_families),
                            syntax(graph, files, &mut progressive, &mut published_version),
                        );
                        let (mut graph, mut syntax_revisions) = parsed?;
                        let mut scip_outputs = compiled?;
                        import_compiled(&graph, &scip_outputs, &progressive).await?;
                        if !priority_native_files.is_empty() {
                            graph = publish_native(graph, priority_native_files, &mut progressive, &mut published_version).await?;
                        }
                        if !deferred_files.is_empty() || !deferred_compiler_families.is_empty() {
                            let (compiled, parsed) = tokio::join!(
                                compiler(deferred_compiler_families),
                                syntax(graph, deferred_files, &mut progressive, &mut published_version),
                            );
                            let (next_graph, revisions) = parsed?;
                            graph = next_graph;
                            syntax_revisions.extend(revisions);
                            let outputs = compiled?;
                            import_compiled(&graph, &outputs, &progressive).await?;
                            scip_outputs.extend(outputs);
                        }
                        // Load one prepared FILE only after its compiler has
                        // supplied facts. Its original parser owner is gone;
                        // stored rows retain the real Source and node identities.
                        for (path, expected) in &restored_files {
                            if scip_index.compiler_document(&graph, path, original)?.is_none() { continue; }
                            let pending = {
                                let index = progressive.capture_index().await?;

                                recheck()?;
                                index.graph.pending_compiler_file(path, original)?
                            };
                            let Some((observed, payload)) = pending else { continue; };
                            if observed.file_revision != expected.file_revision
                                || observed.raw_version != expected.raw_version
                                || observed.source_sha256 != expected.source_sha256
                                || observed.scip_compiler != expected.scip_compiler {
                                continue;
                            }
                            scip_index.restore_parser_file(&graph, &observed, &payload, original)?;
                        }
                        // Replay only the real parser rows staged by this job. The parser
                        // owners and source text were released after each FILE publication.
                        for path in scip_index.staged_paths(&graph, original)? {
                            recheck()?;
                            if background_cancellation.is_cancelled() { continue; }
                            let compiler_enriched = scip_index.compiler_document(&graph, &path, original)?.is_some();
                            if !compiler_enriched { continue; }
                            let indexed = match restored_files.get(&path) {
                                Some(observation) => raw.read_version(&path, observation.raw_version, original).await,
                                None => raw.read(&path, original).await,
                            }.map_err(err)?;
                            let components = path.split('/').collect::<Vec<_>>();
                            let physical = match indexed.source(original).map_err(err)? {
                                IndexedSource::Available(_) => None,
                                IndexedSource::Missing if indexed.missing_reason() == Some(MissingSourceReason::TextNotStored) => Some(
                                    capture_native_nn_physical_source(project_capture, &components, self.policy.copy(), self.policy.retained(),
                                        SourceReadLimits::new(max, 64*1024, max as u64)?, original).map_err(err)?),
                                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "staged AST FILE has no Raw source association")),
                            };
                            let text = match &physical {
                                Some(source) => source.source(original).map_err(err)?,
                                None => match indexed.source(original).map_err(err)? {
                                    IndexedSource::Available(text) => text,
                                    _ => unreachable!("source checked above"),
                                },
                            };
                            let source_sha256: [u8;32] = Sha256::digest(text.as_bytes()).into();
                            let mut file = super::progressive::FilePublication::new(path.clone(), source_sha256, indexed.version());
                            file.scip_compiler = compiler_images.get(&path).cloned();
                            for (batch, oracle_rows) in scip_index.staged_file(&graph, &path, original)? {
                                if batch.association().path() != path
                                    || batch.association().source_sha256() != &source_sha256 {
                                    return Err(io::ErrorKind::InvalidData.into());
                                }
                                graph = super::graph::materialize(graph, batch, oracle_rows,
                                    &scip_index, text,
                                    modules.enabled(Module::Embedding).then_some((match &physical {
                                        Some(capture) => NativeNnSource::Physical { capture, raw: &indexed },
                                        None => NativeNnSource::Indexed(&indexed),
                                    }, identity)), &mut file, original)?;
                            }
                            indexed.source(original).map_err(err)?;
                            if let Some(source) = physical { source.revalidate(original).map_err(err)?; }
                            recheck()?;
                            if compiler_enriched {
                                let revision = syntax_revisions.get(&path).copied()
                                    .or_else(|| restored_files.get(&path).map(|observation| observation.file_revision))
                                    .ok_or(io::ErrorKind::InvalidData)?;
                                if let Some(revision) = progressive.enrich_file(file, revision, &recheck).await? {
                                    published_version = Some(revision);
                                    let _ = pending_sender.send(path.clone());
                                }
                            }
                        }
                        publish_native(graph, native_files, &mut progressive, &mut published_version).await?;
                        raw.revalidate(original).map_err(err)?;
                        profiles.revalidate(original)?;
                        recheck()?;
                        for (_, inputs, _) in &scip_outputs { inputs.recheck(original)?; }
                        Ok::<_, io::Error>(())
                    }).await;
                    drop(pending_sender);
                    result
                });
                let inference = Box::pin(async {
                    match pending_receiver {
                        Some(receiver) => Box::pin(self.embed_pending_loop(project, selected,
                            resources.filter(|_| modules.enabled(Module::Embedding)), receiver, model_refresh, priority_paths.clone(), &model_reconciled, background_cancellation)).await,
                        None => Ok(None),
                    }
                });
                let (produced, inference) = tokio::join!(producer, inference);
                produced?;
                let structural = published_version;
                // A newer watcher batch yields only optional background work.
                // Parsed FILEs and durable NN projections remain committed; the
                // next admitted batch resumes them without reparsing intact files.
                if background_cancellation.is_cancelled()
                    && !cancellation.is_cancelled()
                    && inference.as_ref().is_err_and(|error| error.kind() == io::ErrorKind::Interrupted)
                {
                    recheck()?;
                    return Ok(structural);
                }
                inference
                    .map(|version| version.or(structural))
                    .map_err(|error| io::Error::new(error.kind(), InferenceFailure {
                        error, pending_files: pending_files.borrow().clone(), model_reconciled: model_reconciled.get(),
                    }))
            }).await
        }).await;
        if result.is_err() && cancellation.is_cancelled() && !publication_started.get() {
            // All work so far was captured input or disposable staging. No SDK
            // publication was entered; shutdown can join this cancelled job.
            return Ok(AstSemanticProgress::default());
        }
        result.map(|version| AstSemanticProgress {
            version,
            pending_files: pending_files.into_inner(),
            model_reconciled: model_reconciled.get(),
        })
    }
}

/// A partial successful batch cannot discard a previously yielded structural
/// tail. Only complete Raw coverage can resolve an unbounded recovery selection.
pub(crate) fn merge_pending_selection(
    pending: &mut Option<BTreeSet<String>>,
    incoming: BTreeSet<String>,
    full_raw: bool,
) {
    if full_raw {
        // Every selected FILE was reconciled by the progressive Raw producer;
        // its remaining native-only set replaces an older syntax tail.
        *pending = Some(incoming);
    } else if let Some(paths) = pending.as_mut() {
        paths.extend(incoming);
    }
}

pub(super) fn identity_limits(policy: &HubResourcePolicy) -> io::Result<IdentityLimits> {
    let max = policy.content().max_input_bytes;
    IdentityLimits::new(IdentityLimitsInput {
        source_bytes: max,
        path_components: usize::try_from(policy.limit(ResourceSetting::SnapshotDepth).value)
            .map_err(err)?,
        path_component_bytes: graphit_core::native_path::MAX_PATH_BYTES,
        identity_text_bytes: policy.content().max_scalar_bytes,
        owner_depth: usize::try_from(policy.limit(ResourceSetting::ContentDepth).value)
            .map_err(err)?,
        uid_bytes: max,
    })
    .map_err(err)
}

pub(super) fn inference_failure(
    error: io::Error,
    pending_files: BTreeSet<String>,
    model_reconciled: bool,
) -> io::Error {
    io::Error::new(
        error.kind(),
        InferenceFailure {
            error,
            pending_files,
            model_reconciled,
        },
    )
}

pub(super) struct SyntaxPreparation<'a, 'scope> {
    pub owner: &'a ForegroundAstSources,
    pub project_capture: ProjectCapture<'scope>,
    pub profiles: &'a LoadedProfiles,
    pub original: &'a Arc<SnapshotContext>,
    pub identity: IdentityLimits,
    pub embedding: bool,
    pub scip_index: &'a super::graph::ScipIndex,
}
impl<'profile, 'scope> SyntaxPreparation<'profile, 'scope> {
    pub async fn prepare(
        &self,
        mut graph: graphit_data::graph_writer::IcebugWriter,
        path: &str,
        file_profiles: Vec<&'profile EffectiveProfile>,
        indexed: graphit_ast::indexed_source::IndexedSourceSnapshot,
        embedded_models: &mut generic::Models<'profile>,
        compiler_image: Option<&str>,
        recheck: &impl Fn() -> io::Result<()>,
    ) -> io::Result<(
        graphit_data::graph_writer::IcebugWriter,
        super::progressive::FilePublication,
    )> {
        let Self {
            owner,
            project_capture,
            profiles,
            original,
            identity,
            embedding,
            scip_index,
        } = self;
        let owner = *owner;
        let profiles = *profiles;
        let original = *original;
        let scip_index = *scip_index;
        let project_capture = *project_capture;
        let identity = *identity;
        let embedding = *embedding;
        let max = owner.policy.content().max_input_bytes;
        let mut nn_file = GenericNnFile::new(original, embedding);
        let components: Vec<_> = path.split('/').collect();
        let physical = match indexed.source(original).map_err(err)? {
            IndexedSource::Available(_) => None,
            IndexedSource::Missing
                if indexed.missing_reason() == Some(MissingSourceReason::TextNotStored) =>
            {
                Some(
                    capture_native_nn_physical_source(
                        project_capture,
                        &components,
                        owner.policy.copy(),
                        owner.policy.retained(),
                        SourceReadLimits::new(max, 64 * 1024, max as u64)?,
                        original,
                    )
                    .map_err(err)?,
                )
            }
            IndexedSource::Missing => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Raw path has no source association",
                ));
            }
        };
        let text = match &physical {
            Some(source) => source.source(original).map_err(err)?,
            None => match indexed.source(original).map_err(err)? {
                IndexedSource::Available(text) => text,
                _ => unreachable!("source checked above"),
            },
        };
        let span = ByteSpan {
            start: 0,
            end: text.len() as u64,
        };
        let segment = (!text.is_empty()).then_some(OriginSegment::Direct {
            prepared: span,
            original: span,
        });
        let input = PreparedInput::new(
            text.as_bytes(),
            text.as_bytes(),
            OriginPlan {
                segments: segment.as_slice(),
                removed_original: &[],
                anchors: &[],
            },
            original,
        )
        .map_err(err)?;
        let source_sha256: [u8; 32] = Sha256::digest(text.as_bytes()).into();
        let mut file = super::progressive::FilePublication::new(
            path.to_owned(),
            source_sha256,
            indexed.version(),
        );
        for profile in file_profiles {
            recheck()?;
            let contexts = matches!(
                profile.language().value().as_str(),
                "rust" | "go" | "javascript" | "typescript" | "tsx" | "python"
            )
            .then(|| owner.parsers.contexts(profile, original))
            .transpose()?;
            let json_factory = (profile.language().value() == "json")
                .then(|| owner.parsers.contexts(profile, original))
                .transpose()?;
            let generic_factory = (*profile.parser().value()
                == graphit_ast::profiles::ProfileParser::TreeSitter
                && contexts.is_none()
                && json_factory.is_none())
            .then(|| {
                PreparedGenericTreeSitterModel::prepare(
                    profile,
                    owner.parsers.contexts(profile, original)?,
                    original,
                )
                .map_err(err)
            })
            .transpose()?;
            let antlr_model = matches!(
                (
                    profile.language().value().as_str(),
                    profile.grammar().value().as_str()
                ),
                ("db2", "antlr-db2") | ("plsql", "antlr-plsql")
            );
            let antlr_generic = *profile.parser().value()
                == graphit_ast::profiles::ProfileParser::Antlr4
                && !antlr_model;
            let postgresql_body = if profile.grammar().value() == "antlr-postgresql" {
                let body_profile = profiles
                    .profiles(original)?
                    .iter()
                    .find(|candidate| candidate.language().value() == "plpgsql")
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "PostgreSQL requires its shipped PL/pgSQL body profile",
                        )
                    })?;
                Some(owner.parsers.profile(body_profile, original)?)
            } else {
                None
            };
            recheck()?;
            if let Some(factory) = &contexts {
                let parsed =
                    ParsedTreeSitterDocument::collect(factory, &input, &components, identity)
                        .map_err(err)?;
                let document = parsed.assemble().map_err(err)?;
                graph = super::graph::append(
                    graph,
                    assemble_graph_ts(
                        AstGraphTsInput {
                            document: &document,
                            source: match &physical {
                                Some(capture) => NativeNnSource::Physical {
                                    capture,
                                    raw: &indexed,
                                },
                                None => NativeNnSource::Indexed(&indexed),
                            },
                        },
                        original,
                    )
                    .map_err(err)?,
                    scip_index,
                    text,
                    &mut file,
                    original,
                )?;
                nn_file.ts_document(
                    &document,
                    &match &physical {
                        Some(capture) => NativeNnSource::Physical {
                            capture,
                            raw: &indexed,
                        },
                        None => NativeNnSource::Indexed(&indexed),
                    },
                )?;
            } else if let Some(factory) = &generic_factory {
                let parsed = factory
                    .collect(&input, &components, identity)
                    .map_err(err)?;
                let document = parsed.assemble().map_err(err)?;
                let native_source = match &physical {
                    Some(capture) => NativeNnSource::Physical {
                        capture,
                        raw: &indexed,
                    },
                    None => NativeNnSource::Indexed(&indexed),
                };
                let batch = assemble_graph_generic(
                    AstGraphGenericInput {
                        document: &document,
                        source: match &native_source {
                            NativeNnSource::Physical { capture, raw } => {
                                NativeNnSource::Physical { capture, raw }
                            }
                            NativeNnSource::Indexed(raw) => NativeNnSource::Indexed(raw),
                        },
                    },
                    original,
                )
                .map_err(err)?;
                nn_file.generic_document(&document, &native_source)?;
                let batch = embedded_models
                    .append(
                        &parsed,
                        &document,
                        batch,
                        &native_source,
                        &components,
                        identity,
                        original,
                        Some(&mut nn_file),
                    )
                    .map_err(err)?;
                graph = super::graph::append(graph, batch, scip_index, text, &mut file, original)?;
            } else if let Some(factory) = &json_factory {
                let parsed = ParsedJsonDocument::collect(factory, &input, &components, identity)
                    .map_err(err)?;
                let document = parsed.assemble().map_err(err)?;
                graph = super::graph::append(
                    graph,
                    assemble_graph_json(
                        AstGraphJsonInput {
                            document: &document,
                            source: match &physical {
                                Some(capture) => NativeNnSource::Physical {
                                    capture,
                                    raw: &indexed,
                                },
                                None => NativeNnSource::Indexed(&indexed),
                            },
                        },
                        original,
                    )
                    .map_err(err)?,
                    scip_index,
                    text,
                    &mut file,
                    original,
                )?;
                nn_file.json_document(
                    &document,
                    &match &physical {
                        Some(capture) => NativeNnSource::Physical {
                            capture,
                            raw: &indexed,
                        },
                        None => NativeNnSource::Indexed(&indexed),
                    },
                )?;
            } else if antlr_generic {
                let prepared = (profile.grammar().value() == "antlr-cobol85")
                    .then(|| prepare_cobol(text, detect_cobol_format(text), original).map_err(err))
                    .transpose()?;
                let document = if let Some(prepared) = &prepared {
                    prepared.with_origin_plan(|plan| {
                        let input = PreparedInput::new(
                            text.as_bytes(),
                            prepared.text().as_bytes(),
                            plan,
                            original,
                        )
                        .map_err(err)?;
                        ParsedGenericAntlrDocument::collect(profile, &input, &components, identity)
                            .map_err(err)
                    })?
                } else if let Some(body) = &postgresql_body {
                    ParsedGenericAntlrDocument::collect_postgresql(
                        profile,
                        body,
                        &input,
                        &components,
                        identity,
                        graphit_ast::tree_sitter_engine::ParseControl {
                            cancellation: None,
                            deadline: original.deadline(),
                        },
                    )
                    .map_err(err)?
                } else {
                    ParsedGenericAntlrDocument::collect(profile, &input, &components, identity)
                        .map_err(err)?
                };
                let source = match &physical {
                    Some(capture) => NativeNnSource::Physical {
                        capture,
                        raw: &indexed,
                    },
                    None => NativeNnSource::Indexed(&indexed),
                };
                let batch = assemble_graph_antlr_generic(
                    AstGraphAntlrGenericInput {
                        document: &document,
                        source: generic::source_copy(&source),
                    },
                    original,
                )
                .map_err(err)?;
                graph = super::graph::append(graph, batch, scip_index, text, &mut file, original)?;
                nn_file.antlr_generic_document(&document, &source)?;
            } else if antlr_model {
                let parsed = ParsedAntlrDocument::collect(profile, &input, &components, identity)
                    .map_err(err)?;
                let document = parsed.assemble().map_err(err)?;
                let batch = assemble_graph_antlr(
                    AstGraphAntlrInput {
                        document: &document,
                        source: match &physical {
                            Some(capture) => NativeNnSource::Physical {
                                capture,
                                raw: &indexed,
                            },
                            None => NativeNnSource::Indexed(&indexed),
                        },
                    },
                    original,
                )
                .map_err(err)?;

                let rows = graphit_ast::semantic_graph::oracle::collect_oracle_graph_rows(
                    &document, &batch, original,
                )
                .map_err(err)?;
                graph = super::graph::append_oracle(
                    graph, batch, rows, scip_index, text, &mut file, original,
                )?;
                nn_file.antlr_document(
                    &document,
                    &match &physical {
                        Some(capture) => NativeNnSource::Physical {
                            capture,
                            raw: &indexed,
                        },
                        None => NativeNnSource::Indexed(&indexed),
                    },
                )?;
            }
            recheck()?;
        }
        indexed.source(original).map_err(err)?;
        if let Some(source) = physical {
            source.revalidate(original).map_err(err)?;
        }
        recheck()?;
        file.projections = nn_file.finish();
        file.scip_compiler = compiler_image.map(str::to_owned);
        if compiler_image.is_some() {
            file.pending_compiler = Some(scip_index.prepared_parser_file(&graph, path, original)?);
        }
        Ok((graph, file))
    }
}

/// Owned projections for one physical FILE, including its embedded models.
/// Parser owners are released before inference; no model discovery occurs here.
pub(super) struct GenericNnFile<'a> {
    enabled: bool,
    original: &'a Arc<SnapshotContext>,
    batches: Vec<RecordBatch>,
}

impl<'a> GenericNnFile<'a> {
    fn new(original: &'a Arc<SnapshotContext>, enabled: bool) -> Self {
        Self {
            enabled,
            original,
            batches: Vec::new(),
        }
    }

    fn finish(self) -> Vec<RecordBatch> {
        self.batches
    }

    pub(super) fn ts_document(
        &mut self,
        document: &graphit_ast::tree_sitter_model::TreeSitterSemanticDocument<'_, '_, '_, '_, '_>,
        source: &NativeNnSource<'_>,
    ) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.original.check()?;
        let indices = document
            .code_projections()
            .map_err(err)?
            .iter()
            .filter_map(|projection| match projection {
                TsProjection::Available(projection) => Some(projection.entity_index),
                _ => None,
            })
            .collect::<Vec<_>>();
        let batch = graphit_ast::native_nn::prepare_native_nn_ts(
            document,
            generic::source_copy(source),
            &indices,
            self.original,
        )
        .map_err(err)?;
        self.batches.push(batch);
        Ok(())
    }

    pub(super) fn json_document(
        &mut self,
        document: &graphit_ast::semantic_model::JsonSemanticDocument<'_, '_, '_, '_, '_>,
        source: &NativeNnSource<'_>,
    ) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.original.check()?;
        let indices = document
            .code_projections()
            .map_err(err)?
            .iter()
            .filter_map(|projection| match projection {
                JsonProjection::Available(projection) => Some(projection.entity_index),
                _ => None,
            })
            .collect::<Vec<_>>();
        let batch = graphit_ast::native_nn::prepare_native_nn_json(
            document,
            generic::source_copy(source),
            &indices,
            self.original,
        )
        .map_err(err)?;
        self.batches.push(batch);
        Ok(())
    }

    pub(super) fn antlr_document(
        &mut self,
        document: &graphit_ast::antlr_engine::model::AntlrSemanticDocument<'_, '_, '_, '_, '_>,
        source: &NativeNnSource<'_>,
    ) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.original.check()?;
        let indices = document
            .code_projections()
            .map_err(err)?
            .iter()
            .filter_map(|projection| match projection {
                AntlrProjection::Available(projection) => Some(projection.entity_index),
                _ => None,
            })
            .collect::<Vec<_>>();
        let batch = graphit_ast::native_nn::prepare_native_nn_antlr(
            document,
            generic::source_copy(source),
            &indices,
            self.original,
        )
        .map_err(err)?;
        self.batches.push(batch);
        Ok(())
    }

    pub(super) fn antlr_generic_document(
        &mut self,
        document: &ParsedGenericAntlrDocument<'_, '_, '_, '_>,
        source: &NativeNnSource<'_>,
    ) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.original.check()?;
        let indices = native_nn_antlr_generic_projections(document, self.original)
            .map_err(err)?
            .iter()
            .filter_map(|projection| match projection {
                NativeNnAntlrGenericProjection::Available { entity_index, .. } => {
                    Some(*entity_index)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let batch = graphit_ast::native_nn::prepare_native_nn_antlr_generic(
            document,
            generic::source_copy(source),
            &indices,
            self.original,
        )
        .map_err(err)?;
        self.batches.push(batch);
        Ok(())
    }

    pub(super) fn generic_document(
        &mut self,
        document: &graphit_ast::tree_sitter_generic_model::GenericSemanticDocument<
            '_,
            '_,
            '_,
            '_,
            '_,
        >,
        source: &NativeNnSource<'_>,
    ) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.original.check()?;
        let indices = document
            .code_projections()
            .map_err(err)?
            .iter()
            .filter_map(|projection| match projection {
                graphit_ast::tree_sitter_generic_model::GenericCodeProjection::Available {
                    entity_index,
                    ..
                } => Some(*entity_index),
                _ => None,
            })
            .collect::<Vec<_>>();
        let batch = graphit_ast::native_nn::prepare_native_nn_generic(
            document,
            generic::source_copy(source),
            &indices,
            self.original,
        )
        .map_err(err)?;
        self.batches.push(batch);
        Ok(())
    }
}

/// Infer an already admitted, current FILE's durable projections. The caller
/// owns current authorization/Source-SHA checks and publication CAS; no parser,
/// discovery or graph export is retained while this operation runs.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn embed_prepared_file<F, Fut>(
    resources: &MemoryJobResources,
    model: &graphit_ast::native_nn::NativeNnModel<'_>,
    selected: &ProjectContext,
    cancellation: &Cancellation,
    original: &Arc<SnapshotContext>,
    path: &str,
    batches: &[RecordBatch],
    snapshots: &[&BranchSnapshot],
    recheck: &F,
    sender: &tokio::sync::mpsc::Sender<Result<RecordBatch, DataFusionError>>,
) -> io::Result<bool>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = io::Result<()>>,
{
    let mut existing = HashMap::new();
    for snapshot in snapshots {
        existing.extend(file_embeddings(Some(snapshot), path, model, original).await?);
    }
    if batches.is_empty() {
        let pending = RecordBatch::new_empty(graphit_ast::native_nn::native_nn_projection_schema());
        let batch =
            graphit_ast::native_nn::complete_native_nn(&pending, model, None, &[], original)
                .map_err(err)?;
        recheck().await?;
        return Ok(sender.send(Ok(batch)).await.is_ok());
    }
    for pending in batches {
        recheck().await?;
        graphit_ast::native_nn::validate_native_nn_projection(pending, original).map_err(err)?;
        let string_column = |name: &str| {
            pending
                .column_by_name(name)
                .and_then(|column| column.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
        };
        let paths = string_column("path")?;
        let uid = string_column("uid")?;
        let key = string_column("data_key")?;
        let projections = string_column("projection")?;
        let mut texts = Vec::with_capacity(pending.num_rows());
        let mut reused = Vec::with_capacity(pending.num_rows());
        for row in 0..pending.num_rows() {
            original.check()?;
            if paths.value(row) != path {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let text = projections.value(row);
            texts.push(text);
            reused.push(existing.remove(&embedding_key(uid.value(row), key.value(row), text)));
        }
        if !embed_file(
            (resources, model),
            (selected, cancellation, original),
            &texts,
            reused,
            recheck,
            sender,
            |range, generation, vectors| {
                graphit_ast::native_nn::complete_native_nn(
                    &pending.slice(range.start, range.len()),
                    model,
                    generation,
                    vectors,
                    original,
                )
                .map_err(err)
            },
        )
        .await?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn embed_file<F, Check, Fut>(
    (resources, model): (
        &MemoryJobResources,
        &graphit_ast::native_nn::NativeNnModel<'_>,
    ),
    (selected, cancellation, original): (&ProjectContext, &Cancellation, &SnapshotContext),
    texts: &[&str],
    mut reused: Vec<Option<FileEmbedding>>,
    recheck: &Check,
    sender: &tokio::sync::mpsc::Sender<Result<RecordBatch, DataFusionError>>,
    mut encode: F,
) -> io::Result<bool>
where
    F: FnMut(std::ops::Range<usize>, Option<u64>, &[Vec<f32>]) -> io::Result<RecordBatch>,
    Check: Fn() -> Fut,
    Fut: std::future::Future<Output = io::Result<()>>,
{
    let limits = resources.rpc_limits();
    let metadata = crate::embedding_service::EmbeddingMetadata {
        vector_id: model.vector_id,
        dimensions: model.dimensions,
        model: model.model_id.to_owned(),
        revision: model.revision.to_owned(),
        local_generation: None,
    };
    let dimensions = usize::try_from(model.dimensions).map_err(err)?;
    if dimensions == 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let max_rows = limits.max_texts.min(limits.max_output_values / dimensions);
    if max_rows == 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let inference_rows = max_rows;
    if texts.is_empty() {
        recheck().await?;
        return Ok(sender.send(Ok(encode(0..0, None, &[])?)).await.is_ok());
    }
    let mut offset = 0;
    while offset < texts.len() {
        recheck().await?;
        if let Some(cached) = &reused[offset] {
            let generation = cached.generation;
            let mut end = offset + 1;
            while end < texts.len()
                && end - offset < max_rows
                && reused[end]
                    .as_ref()
                    .is_some_and(|value| value.generation == generation)
            {
                end += 1;
            }
            let vectors: Vec<_> = reused[offset..end]
                .iter_mut()
                .map(|value| value.take().expect("contiguous cached embeddings").vector)
                .collect();
            let batch = encode(offset..end, generation, &vectors)?;
            if sender.send(Ok(batch)).await.is_err() {
                return Ok(false);
            }
            offset = end;
            continue;
        }
        let mut end = offset;
        let mut bytes = 0usize;
        while end < texts.len() && end - offset < inference_rows && reused[end].is_none() {
            let next = bytes
                .checked_add(texts[end].len())
                .ok_or(io::ErrorKind::InvalidInput)?;
            if next > limits.max_input_bytes {
                break;
            }
            bytes = next;
            end += 1;
        }
        if end == offset {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "one code projection exceeds embedding RPC input",
            ));
        }
        let request = background_embedding_request(&resources.policy, original, cancellation)?;
        let key = resources
            .auth
            .runtime()
            .check(request.snapshot())
            .map_err(err)?;
        let credential =
            std::str::from_utf8(key.secret(request.snapshot()).map_err(err)?).map_err(err)?;
        let caller = resources
            .auth
            .issuer()
            .authenticate_runtime(credential, &request)
            .map_err(err)?;
        let mut service = resources
            .auth
            .issuer()
            .capture_service(&caller, graphit_auth::ServicePurpose::Embedding)
            .map_err(err)?;
        let output = resources.embed(
            &mut service,
            &metadata,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::CodeDocument,
                texts: texts[offset..end].iter().map(|text| (*text).to_owned()).collect(),
                deadline: request.snapshot().required_deadline()?,
            },
        )            .await
            .inspect_err(|cause| {
                use std::io::Write;
                let local_error = match cause {
                    crate::embedding_service::EmbeddingServiceError::Local(error) => Some(error),
                    _ => None,
                };
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "AST NN embedding RPC failed: project_id={} project_version={:?} input_bytes={} error_kind={:?} local_error={:?}",
                    selected.id,
                    selected.version.version(),
                    bytes,
                    cause.kind(),
                    local_error
                );
            })
            .map_err(crate::embedding_service::EmbeddingServiceError::into_io)?;
        recheck().await?;
        let batch = encode(
            offset..end,
            output.metadata.local_generation,
            &output.vectors,
        )?;
        drop(output);
        if sender.send(Ok(batch)).await.is_err() {
            return Ok(false);
        }
        offset = end;
    }
    Ok(true)
}
