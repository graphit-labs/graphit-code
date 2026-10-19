//! Infer missing vectors from current rows of the single AST Lance corpus.
use super::progressive::{corpus_snapshot, file_rows, write_fence};
use super::*;
use crate::foreground_memory_jobs::{MemoryJobResources, background_embedding_request};
use arrow_array::Array;
use futures_util::TryStreamExt;
use graphit_ast::corpus;

impl ForegroundAstSources {
    pub(crate) async fn embed_pending_loop(
        &self,
        project: &ProjectPaths,
        selected: &ProjectContext,
        resources: Option<&MemoryJobResources>,
        mut pending: tokio::sync::mpsc::UnboundedReceiver<String>,
        model_refresh: bool,
        priority_paths: BTreeSet<String>,
        model_reconciled: &std::cell::Cell<bool>,
        cancellation: &Cancellation,
    ) -> io::Result<Option<u64>> {
        let Some(resources) = resources else {
            while pending.recv().await.is_some() {
                if cancellation.is_cancelled() {
                    return Err(io::ErrorKind::Interrupted.into());
                }
            }
            return Ok(None);
        };
        let request =
            graphit_hub::policy::RequestContext::service(self.policy.content(), cancellation)
                .map_err(err)?;
        let original = request.snapshot();
        let config = project_config(&self.installation, project, &self.base, original)?;
        if !ModuleSettings::resolve(&config)?.enabled(Module::Embedding) {
            while pending.recv().await.is_some() {
                original.check()?;
            }
            return Ok(None);
        }
        let mut ready = BTreeSet::new();
        let mut inspect_existing = true;
        let mut metadata = None;
        let mut initialized = false;
        let mut version = None;
        let mut corpus_location = None;
        loop {
            let ended = if !inspect_existing && ready.is_empty() {
                // Retain the resolved corpus only while work is ready. Idle
                // jobs release directory handles and recapture the next route.
                corpus_location = None;
                match next_pending(&mut pending, original).await? {
                    Some(path) => {
                        ready.insert(path);
                        false
                    }
                    None => true,
                }
            } else {
                false
            };
            let _completed_path = {
                let capture =
                    CapturedPublicationScope::project(project, selected.id, self.scope, original)?;
                let record = capture.binding()?;
                let project_capture = ProjectCapture {
                    scope: ScopeCapture {
                        live: &capture,
                        record: &record,
                    },
                    version: &selected.version,
                };
                let key = self.auth.runtime().check(original).map_err(err)?;
                let credential =
                    std::str::from_utf8(key.secret(original).map_err(err)?).map_err(err)?;
                let caller = self
                    .auth
                    .issuer()
                    .authenticate_runtime(credential, &request)
                    .map_err(err)?;
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
                    .map_err(err)?;
                let write = authority
                    .admit_authenticated_write(&caller, &request, scope, ProjectWriteOperation::Ast)
                    .map_err(err)?;
                let recheck = || -> io::Result<()> {
                    original.check()?;
                    read.recheck().map_err(err)?;
                    write.recheck().map_err(err)?;
                    Ok(())
                };
                if ended {
                    // EOF releases the last operation, but does not bypass its
                    // final current PROJECT/version authorization check.
                    recheck()?;
                    return Ok(version);
                }
                recheck()?;
                let cohort = self
                    .live
                    .capture_cohort(
                        self.auth.installation(),
                        project_capture,
                        self.native,
                        original,
                        true,
                    )?
                    .ok_or(io::ErrorKind::NotFound)?;
                if corpus_location.is_none() {
                    let relative = graphit_core::local_corpus::resolve(
                        project_capture.scope.live,
                        selected.version.version(),
                        "ast",
                        corpus::DIRECTORY,
                        self.native,
                        original,
                    )?;
                    let path = project_capture.scope.live.runtime_root().join(relative);
                    let directory =
                        CapturedDocumentDirectory::capture(&path, self.native, original)?;
                    corpus_location = Some((path, directory));
                }
                let (corpus_path, corpus_directory) =
                    corpus_location.as_ref().ok_or(io::ErrorKind::InvalidData)?;
                let recheck = || {
                    recheck()?;
                    corpus_directory.revalidate(original)
                };
                let fence = cohort.fence();
                let recheck = &recheck;
                if inspect_existing {
                    let corpus =
                        corpus_snapshot(corpus_path, corpus_directory, project_capture, original)
                            .await?;
                    ready = pending_paths(&corpus, None, model_refresh, None, original).await?;
                    inspect_existing = false;
                    if ready.is_empty() {
                        model_reconciled.set(true);
                        continue;
                    }
                }
                let Some(path) = take_ready_path(&mut ready, &mut pending, &priority_paths) else {
                    recheck()?;
                    continue;
                };
                if metadata.is_none() && !model_refresh {
                    let corpus =
                        corpus_snapshot(corpus_path, corpus_directory, project_capture, original)
                            .await?;
                    if !pending_paths(&corpus, None, false, Some(&path), original)
                        .await?
                        .contains(&path)
                    {
                        continue;
                    }
                }
                if metadata.is_none() {
                    let model_request =
                        background_embedding_request(&resources.policy, original, cancellation)?;
                    metadata = Some(
                        resources
                            .metadata(&model_request)
                            .await
                            .map_err(crate::embedding_service::EmbeddingServiceError::into_io)?,
                    );
                }
                let model = crate::ast_semantic::native_nn_model(
                    metadata.as_ref().expect("model resolved for pending FILE"),
                )
                .map_err(err)?;
                if !initialized {
                    let publication = write_fence(fence, original).await?;
                    recheck()?;
                    let mut corpus =
                        corpus_snapshot(corpus_path, corpus_directory, project_capture, original)
                            .await?;
                    if corpus.inherited_target().is_some() {
                        corpus = corpus.materialize().await.map_err(err)?;
                    }
                    let wanted = corpus::vector_fields(&model)?;
                    if let Ok(actual) = corpus.schema().field_with_name(corpus::VECTOR)
                        && actual != wanted.field(0)
                    {
                        corpus = corpus
                            .drop_vector(corpus::VECTOR, original)
                            .await
                            .map_err(err)?;
                        version = Some(corpus.version());
                        schema_postcheck(corpus.version(), recheck())?;
                    }
                    let before = corpus.version();
                    corpus = corpus
                        .add_nullable_columns(wanted, original)
                        .await
                        .map_err(err)?;
                    if corpus.version() != before {
                        version = Some(corpus.version());
                        schema_postcheck(corpus.version(), recheck())?;
                    }
                    // The scan owns an immutable Lance snapshot. Only the schema
                    // mutation needs exclusion; each selected FILE is rechecked below.
                    drop(publication);
                    ready
                        .extend(pending_paths(&corpus, Some(&model), false, None, original).await?);
                    ready.remove(&path);
                    model_reconciled.set(true);
                    initialized = true;
                }
                let captured = {
                    let corpus =
                        corpus_snapshot(corpus_path, corpus_directory, project_capture, original)
                            .await?;
                    if !pending_paths(&corpus, Some(&model), false, Some(&path), original)
                        .await?
                        .contains(&path)
                    {
                        continue;
                    }
                    let rows = file_rows(&corpus, &path, original).await?;
                    let prepared = corpus::projections(&rows, original)?;
                    (prepared, corpus)
                };
                let (prepared, snapshot) = captured;
                if prepared.num_rows() == 0 {
                    continue;
                }
                // The SDK conditional update matches each entity/content fingerprint.
                // Unrelated graph and Source commits do not invalidate this work.
                let current = || async { recheck() };
                let (sender, receiver) = tokio::sync::mpsc::channel(1);
                let (complete, completed) = tokio::sync::oneshot::channel();
                let batches = vec![prepared];
                let reuse = vec![&snapshot];
                let producer = Box::pin(async {
                    let result = Box::pin(super::semantic::embed_prepared_file(
                        resources,
                        &model,
                        selected,
                        cancellation,
                        original,
                        &path,
                        &batches,
                        &reuse,
                        &current,
                        &sender,
                    ))
                    .await;
                    if matches!(result, Ok(true)) {
                        let _ = complete.send(());
                    }
                    drop(sender);
                    result
                });
                let publish = Box::pin(async {
                    let input = super::semantic::checkpoint::FileInput {
                        path: path.clone(),
                        batches: receiver,
                        completed,
                    };
                    let mut seen =
                        |saved: &graphit_ast::native_nn_publication::NativeNnPublication| {
                            version = Some(saved.version());
                        };
                    super::semantic::checkpoint::publish_file(
                        &snapshot, input, &model, original, &current, fence, &mut seen,
                    )
                    .await
                });
                let (produced, published) = tokio::join!(producer, publish);
                recheck()?;
                if !produced? {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                published?;
                path
            };
            #[cfg(all(test, target_os = "linux"))]
            self.after_nn_file(&_completed_path, ready.len(), original)
                .await?;
        }
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn hold_after_nn_file(
        &self,
        path: &str,
    ) -> (
        tokio::sync::oneshot::Receiver<usize>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (started, observed) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let mut gate = self.nn_file_gate.lock().unwrap();
        assert!(gate.is_none());
        *gate = Some((path.into(), started, wait));
        (observed, release)
    }

    #[cfg(all(test, target_os = "linux"))]
    async fn after_nn_file(
        &self,
        path: &str,
        remaining: usize,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<()> {
        let gate = {
            let mut gate = self.nn_file_gate.lock().unwrap();
            if gate
                .as_ref()
                .is_some_and(|(expected, _, _)| expected == path)
            {
                gate.take()
            } else {
                None
            }
        };
        if let Some((_, started, wait)) = gate {
            let _ = started.send(remaining);
            tokio::select! {
                () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                result = wait => result.map_err(err)?,
            }
        }
        original.check()
    }
}

/// Caller holds the FILE writer fence. Source revision alone does not detect
/// compiler definitions added to this same Source while inference was running.
/// Drain committed FILE notifications before choosing from a previous backlog.
/// Keep event roots intact: a directory can gain more ready FILEs later.
fn take_ready_path(
    ready: &mut BTreeSet<String>,
    pending: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
    priority_paths: &BTreeSet<String>,
) -> Option<String> {
    while let Ok(path) = pending.try_recv() {
        ready.insert(path);
    }
    let prioritized = ready
        .iter()
        .find(|path| {
            priority_paths
                .iter()
                .any(|root| Path::new(path.as_str()).starts_with(root))
        })
        .cloned();
    match prioritized {
        Some(path) => ready.take(&path),
        None => ready.pop_first(),
    }
}

async fn next_pending(
    pending: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
    original: &SnapshotContext,
) -> io::Result<Option<String>> {
    original.check()?;
    let deadline = async {
        match original.deadline() {
            Some(deadline) => {
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await
            }
            None => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        biased;
        () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = deadline => Err(io::ErrorKind::TimedOut.into()),
        path = pending.recv() => { original.check()?; Ok(path) },
    }
}

async fn pending_paths(
    snapshot: &BranchSnapshot,
    model: Option<&graphit_ast::native_nn::NativeNnModel<'_>>,
    all: bool,
    path: Option<&str>,
    original: &SnapshotContext,
) -> io::Result<BTreeSet<String>> {
    let mut scan = snapshot.scan();
    scan.project(&["path"]).map_err(err)?;
    let mut predicate = col("projection").is_not_null();
    if !all && snapshot.schema().field_with_name(corpus::VECTOR).is_ok() {
        let mut missing = col(corpus::VECTOR).is_null();
        if let Some(model) = model {
            let id = lance::deps::datafusion::common::ScalarValue::FixedSizeBinary(
                32,
                Some(model.vector_id.to_vec()),
            );
            missing = missing
                .or(col("vector_id").is_null())
                .or(col("vector_id").not_eq(lit(id)));
        }
        predicate = predicate.and(missing);
    }
    if let Some(path) = path {
        predicate = predicate.and(col("path").eq(lit(path)));
        scan.limit(Some(1), None).map_err(err)?;
    }
    scan.filter_expr(predicate);
    let mut stream = scan.try_into_stream().await.map_err(err)?;
    let mut paths = BTreeSet::new();
    while let Some(batch) = stream.try_next().await.map_err(err)? {
        original.check()?;
        let values = batch
            .column_by_name("path")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .ok_or(io::ErrorKind::InvalidData)?;
        for value in values.iter() {
            paths.insert(value.ok_or(io::ErrorKind::InvalidData)?.to_owned());
        }
    }
    Ok(paths)
}
#[derive(Debug)]
struct SchemaChanged {
    version: u64,
    cause: io::Error,
}
impl std::fmt::Display for SchemaChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AST corpus schema committed at generation {}: {}",
            self.version, self.cause
        )
    }
}
impl std::error::Error for SchemaChanged {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
fn schema_postcheck(version: u64, result: io::Result<()>) -> io::Result<()> {
    result.map_err(|cause| io::Error::new(cause.kind(), SchemaChanged { version, cause }))
}
