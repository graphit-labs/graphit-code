//! Per-PROJECT Raw publication; each selected FILE is captured and released before the next.
//! Opt-in tracing reports static publication boundaries and error kinds.
use crate::ast_parser_resources::AstParserResources;
use crate::ast_profile_loader::{AstProfileLoader, LoadedProfiles, project_config};
mod graph;
pub(crate) mod progressive;
pub(super) mod scip;
pub(crate) mod semantic;
mod semantic_background;
use arrow_array::{RecordBatch, StringArray};
use graphit_ast::{
    ignore_policy::AstIgnorePolicy,
    indexed_source::{branch_source_paths, read_branch_source, source_schema},
    source_inspection::IndexedSource,
};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ScopeCapture,
    ScopeInput,
};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, ProjectIgnoreCriteria,
        ProjectSelectionCriteria, ProjectSelectionLimits, ScopeLimits, SnapshotContext,
        SourceReadLimits,
    },
    config::{ConfigKey, ConfigSnapshot},
    file_lock::Cancellation,
    module_settings::{Module, ModuleSettings},
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::{auth_startup::DaemonAuth, supervisor::ProjectContext, watcher::ChangeBatch};
use graphit_data::{
    lance_branch_store::{BranchSnapshot, DeltaRecoveryKey, LanceBranchStore, col, lit},
    lance_module_store::{
        DataFusionError, LanceModuleStore, PreparedCreate, RecordBatchStreamAdapter, RecoveryKey,
        SendableRecordBatchStream,
    },
};
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, ResourceSetting},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tokio_stream::wrappers::ReceiverStream;

fn request_auth_error(
    error: graphit_auth::AuthError,
    request: &graphit_hub::policy::RequestContext<'_>,
) -> io::Error {
    if let Err(interrupted) = request.check() {
        io::Error::new(interrupted.kind(), error)
    } else {
        err(error)
    }
}

#[cfg(all(test, target_os = "linux"))]
type RawBatchObservation = (bool, BTreeSet<PathBuf>);
#[cfg(all(test, target_os = "linux"))]
type FilePublicationGate = (
    String,
    tokio::sync::oneshot::Sender<()>,
    tokio::sync::oneshot::Receiver<()>,
);
#[cfg(all(test, target_os = "linux"))]
type NnFileGate = (
    String,
    tokio::sync::oneshot::Sender<usize>,
    tokio::sync::oneshot::Receiver<()>,
);

pub(super) struct ForegroundAstSources {
    auth: Arc<DaemonAuth>,
    installation: InstallationPaths,
    base: ConfigSnapshot,
    policy: HubResourcePolicy,
    scope: ScopeLimits,
    native: NativeDocumentFileLimits,
    binding: NativeBinding,
    profiles: AstProfileLoader,
    parsers: AstParserResources,
    scip: scip::ScipService,
    live: Arc<progressive::LiveAstRegistry>,
    #[cfg(all(test, target_os = "linux"))]
    next_raw_error: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<io::ErrorKind>>>,
    #[cfg(all(test, target_os = "linux"))]
    next_raw_batch: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<RawBatchObservation>>>,
    #[cfg(all(test, target_os = "linux"))]
    next_background: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<Cancellation>>>,
    #[cfg(all(test, target_os = "linux"))]
    file_publication_gate: std::sync::Mutex<Option<FilePublicationGate>>,
    #[cfg(all(test, target_os = "linux"))]
    nn_file_gate: std::sync::Mutex<Option<NnFileGate>>,
}
pub(super) struct ForegroundAstSourceConfig {
    pub live: Arc<progressive::LiveAstRegistry>,
    pub installation: InstallationPaths,
    pub base: ConfigSnapshot,
    pub policy: HubResourcePolicy,
    pub scope: ScopeLimits,
    pub native: NativeDocumentFileLimits,
    pub binding: NativeBinding,
    pub environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
}
#[derive(Debug)]
pub(super) enum RawKey {
    Create(RecoveryKey),
    Delta(DeltaRecoveryKey),
}
impl fmt::Display for RawKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create(key) => write!(f, "Create {key:?}"),
            Self::Delta(key) => write!(f, "Delta {key:?}"),
        }
    }
}
#[derive(Debug, Default)]
pub(super) struct RawEffects {
    created_directories: Vec<PathBuf>,
    retained_staging: Option<PathBuf>,
    pub keys: Vec<RawKey>,
    materialized_branch: Option<String>,
    pub published_version: Option<u64>,
    pending_files: BTreeSet<String>,
}
#[derive(Debug)]
pub(super) struct RawPublicationFailure {
    pub effects: RawEffects,
    pub cause: io::Error,
}
impl fmt::Display for RawPublicationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Raw publication, created {:?}, retained staging {:?}, materialized {:?}, observed version {:?}",
            self.effects.created_directories,
            self.effects.retained_staging,
            self.effects.materialized_branch,
            self.effects.published_version
        )?;
        for key in &self.effects.keys {
            write!(f, ", {key}")?;
        }
        write!(f, ": {}", self.cause)
    }
}
impl Error for RawPublicationFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}
fn err(e: impl Error + Send + Sync + 'static) -> io::Error {
    io::Error::other(e)
}
pub(super) fn literal_path(path: &Path) -> io::Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        parts.push(part.to_str().ok_or(io::ErrorKind::InvalidData)?);
    }
    if parts.is_empty() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(parts.join("/"))
}
impl ForegroundAstSources {
    pub async fn shutdown(&self) -> io::Result<()> {
        self.scip.shutdown(&self.policy).await
    }

    pub fn external_controls(&self) -> io::Result<graphit_daemon::watcher::ExternalControls> {
        Ok(graphit_daemon::watcher::ExternalControls {
            files: vec![graphit_core::config::ConfigStore::global(&self.installation)?.path()],
            directories: vec![
                self.installation.global_path(&["ast", "queries"])?,
                self.installation
                    .framework_runtime(env!("GRAPHIT_COMPILED_BUILD_VERSION"))?
                    .join("ast/queries"),
            ],
        })
    }

    pub fn refresh_controls(
        &self,
        project: &ProjectPaths,
        cancellation: &Cancellation,
        controls: &graphit_daemon::watcher::ProjectWatchControls,
    ) -> io::Result<()> {
        let original =
            graphit_hub::policy::RequestContext::service(self.policy.content(), cancellation)
                .map_err(err)?;
        let config = project_config(&self.installation, project, &self.base, original.snapshot())?;
        controls.reconfigure(super::foreground_source::project_watch_config(
            project, &config, &original,
        )?)?;
        original.check()
    }
    pub fn new(
        auth: Arc<DaemonAuth>,
        config: ForegroundAstSourceConfig,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<Self> {
        let ForegroundAstSourceConfig {
            live,
            installation,
            base,
            policy,
            scope,
            native,
            binding,
            environment,
        } = config;
        let profiles = AstProfileLoader::new(policy.clone(), original)?;
        let parsers = AstParserResources::default();
        let scip = scip::ScipService::new(environment, base.brand())?;
        Ok(Self {
            auth,
            installation,
            base,
            policy,
            scope,
            native,
            binding,
            profiles,
            parsers,
            scip,
            live,
            #[cfg(all(test, target_os = "linux"))]
            next_raw_error: std::sync::Mutex::new(None),
            #[cfg(all(test, target_os = "linux"))]
            next_raw_batch: std::sync::Mutex::new(None),
            #[cfg(all(test, target_os = "linux"))]
            next_background: std::sync::Mutex::new(None),
            #[cfg(all(test, target_os = "linux"))]
            file_publication_gate: std::sync::Mutex::new(None),
            #[cfg(all(test, target_os = "linux"))]
            nn_file_gate: std::sync::Mutex::new(None),
        })
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn hold_after_file_publication(
        &self,
        path: &str,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (started, observed) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let mut gate = self.file_publication_gate.lock().unwrap();
        assert!(gate.is_none());
        *gate = Some((path.into(), started, wait));
        (observed, release)
    }
    #[cfg(all(test, target_os = "linux"))]
    async fn after_file_publication(
        &self,
        path: &str,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<()> {
        let gate = {
            let mut gate = self.file_publication_gate.lock().unwrap();
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
            let _ = started.send(());
            tokio::select! {
                () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                result = wait => result.map_err(err)?,
            }
        }
        original.check()
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn observe_next_raw_error(&self) -> tokio::sync::oneshot::Receiver<io::ErrorKind> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        *self.next_raw_error.lock().unwrap() = Some(sender);
        receiver
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn observe_next_raw_batch(
        &self,
    ) -> tokio::sync::oneshot::Receiver<RawBatchObservation> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        *self.next_raw_batch.lock().unwrap() = Some(sender);
        receiver
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn observe_next_background(&self) -> tokio::sync::oneshot::Receiver<Cancellation> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        *self.next_background.lock().unwrap() = Some(sender);
        receiver
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn observe_raw_batch(&self, batch: &ChangeBatch) {
        if let Some(sender) = self.next_raw_batch.lock().unwrap().take() {
            let _ = sender.send((batch.rescan, batch.paths.clone()));
        }
    }
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(project_id = %selected.id, project_version = selected.version.version())
    )]
    #[cfg(test)]
    pub async fn index(
        self: &Arc<Self>,
        project: &ProjectPaths,
        selected: &ProjectContext,
        batch: ChangeBatch,
        cancellation: &Cancellation,
    ) -> io::Result<Option<u64>> {
        let owner = Arc::clone(self);
        let project = project.clone();
        let selected = selected.clone();
        let cancellation = cancellation.clone();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(owner.index_progressive(
                &project,
                &selected,
                batch,
                &cancellation,
                None,
            ))
        })
        .await
        .map_err(err)?
        .map(|progress| progress.version)
    }

    /// Source updates can yield structure; record updates yield only inference.
    /// Lifetime cancellation still stops and joins the complete producer.
    pub async fn index_productive(
        self: &Arc<Self>,
        project: &ProjectPaths,
        selected: &ProjectContext,
        batch: ChangeBatch,
        resources: Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>,
        selection: Option<BTreeSet<String>>,
        model_refresh: bool,
        source_preemption: &Cancellation,
        background: &Cancellation,
        cancellation: &Cancellation,
    ) -> io::Result<semantic::AstSemanticProgress> {
        let owner = Arc::clone(self);
        let project = project.clone();
        let selected = selected.clone();
        let source_preemption = source_preemption.clone();
        let background = background.clone();
        #[cfg(all(test, target_os = "linux"))]
        if let Some(sender) = self.next_background.lock().unwrap().take() {
            let _ = sender.send(background.clone());
        }
        let cancellation = cancellation.clone();
        let runtime = tokio::runtime::Handle::current();
        // Parser/Rc borrows stay in this complete owned batch, as in
        // index_semantic. Cancellation stops ingress cooperatively and the
        // join is always awaited so native publications can settle.
        tokio::task::spawn_blocking(move || {
            runtime.block_on(owner.productive_job(
                &project,
                &selected,
                batch,
                resources.as_deref(),
                selection,
                model_refresh,
                &source_preemption,
                &background,
                &cancellation,
            ))
        })
        .await
        .map_err(err)?
    }

    async fn productive_job(
        &self,
        project: &ProjectPaths,
        selected: &ProjectContext,
        batch: ChangeBatch,
        resources: Option<&crate::foreground_memory_jobs::MemoryJobResources>,
        mut selection: Option<BTreeSet<String>>,
        model_refresh: bool,
        source_preemption: &Cancellation,
        background: &Cancellation,
        cancellation: &Cancellation,
    ) -> io::Result<semantic::AstSemanticProgress> {
        if batch.records_changed
            && !batch.rescan
            && !batch.controls_changed
            && batch.paths.is_empty()
        {
            let request =
                graphit_hub::policy::RequestContext::service(self.policy.content(), cancellation)
                    .map_err(err)?;
            let config =
                project_config(&self.installation, project, &self.base, request.snapshot())?;
            let modules = ModuleSettings::resolve(&config)?;
            if !modules.enabled(Module::Ast) || !modules.enabled(Module::Embedding) {
                return Ok(semantic::AstSemanticProgress::default());
            }
            // Resume only the already persisted inference tail interrupted by
            // the record notification; no file discovery, parser or compiler.
            let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
            drop(sender);
            let reconciled = std::cell::Cell::new(!model_refresh);
            let version = self
                .embed_pending_loop(
                    project,
                    selected,
                    resources,
                    receiver,
                    model_refresh,
                    BTreeSet::new(),
                    &reconciled,
                    background,
                )
                .await?;
            return Ok(semantic::AstSemanticProgress {
                version,
                pending_files: BTreeSet::new(),
                model_reconciled: reconciled.get(),
            });
        }
        let priority_paths = batch
            .paths
            .iter()
            .filter_map(|path| path.strip_prefix(project.root()).ok().map(literal_path))
            .collect::<io::Result<BTreeSet<_>>>()?;
        let full_raw =
            batch.rescan || batch.controls_changed || batch.paths.contains(project.root());
        // Compiler recovery follows the productive delta, independently of the
        // NN selection. An empty ingress is a resume of durable pending work.
        let compiler_restore_scope =
            (!full_raw && !batch.paths.is_empty()).then(|| priority_paths.clone());
        let model_reconciled = std::cell::Cell::new(!model_refresh);
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let inference_priority = priority_paths.clone();
        // These SDK-heavy branches have large concrete futures. Keep them on
        // the heap rather than copying both through join!/block_on stack frames.
        let publication = Box::pin(async move {
            let raw = Box::pin(self.index_progressive(
                project,
                selected,
                batch,
                cancellation,
                Some((
                    source_preemption,
                    sender.clone(),
                    std::cell::RefCell::new(Some(started)),
                )),
            ))
            .await?;
            semantic::merge_pending_selection(&mut selection, raw.pending_files, full_raw);
            // Compiler-only FILEs and enrichment join the same producer before
            // it closes the pending stream. Record changes preserve these facts;
            // the separate NN consumer observes background cancellation.
            let progress = Box::pin(self.semantic_job(
                project,
                selected,
                None,
                selection,
                priority_paths,
                compiler_restore_scope,
                false,
                source_preemption,
                cancellation,
                Some(sender),
            ))
            .await?;
            Ok::<_, io::Error>(semantic::AstSemanticProgress {
                version: progress.version.or(raw.version),
                pending_files: progress.pending_files,
                model_reconciled: false,
            })
        });
        let inference = Box::pin(async {
            // Readiness follows opening the real LiveIndex. An empty or disabled
            // producer closes this signal without discovery or model loading.
            if ready.await.is_err() {
                return Ok(None);
            }
            Box::pin(self.embed_pending_loop(
                project,
                selected,
                resources,
                receiver,
                model_refresh,
                inference_priority,
                &model_reconciled,
                background,
            ))
            .await
        });
        let (publication, inference) = tokio::join!(publication, inference);
        let mut publication = publication?;
        publication.model_reconciled = model_reconciled.get();
        match inference {
            Ok(_) => Ok(publication),
            Err(error)
                if error.kind() == io::ErrorKind::Interrupted
                    && background.is_cancelled()
                    && !cancellation.is_cancelled() =>
            {
                Ok(publication)
            }
            Err(error) => Err(semantic::inference_failure(
                error,
                publication.pending_files,
                publication.model_reconciled,
            )),
        }
    }

    async fn index_progressive(
        &self,
        project: &ProjectPaths,
        selected: &ProjectContext,
        batch: ChangeBatch,
        cancellation: &Cancellation,
        progressive: Option<(
            &Cancellation,
            tokio::sync::mpsc::UnboundedSender<String>,
            std::cell::RefCell<Option<tokio::sync::oneshot::Sender<()>>>,
        )>,
    ) -> io::Result<semantic::AstSemanticProgress> {
        let mut effects = RawEffects::default();
        tracing::debug!(phase = "capture", "AST Raw publication boundary");
        let result = async {
            // A native capture has one subtree plus explicit files. Split
            // independent directory events instead of widening them to the
            // complete project. Nested events are already covered by their parent.
            let mut directories = Vec::<PathBuf>::new();
            if !batch.rescan && !batch.controls_changed && batch.error.is_none() {
                for path in &batch.paths {
                    if path.is_dir() && !directories.iter().any(|parent| path.starts_with(parent)) {
                        directories.push(path.clone());
                    }
                }
            }
            if directories.len() < 2 {
                return self
                    .index_inner(
                        project,
                        selected,
                        batch,
                        cancellation,
                        &mut effects,
                        progressive.as_ref(),
                    )
                    .await;
            }
            let files = batch
                .paths
                .into_iter()
                .filter(|path| {
                    !directories
                        .iter()
                        .any(|directory| path.starts_with(directory))
                })
                .collect::<BTreeSet<_>>();
            let mut version = None;
            for paths in directories
                .into_iter()
                .map(|path| BTreeSet::from([path]))
                .chain((!files.is_empty()).then_some(files))
            {
                let current = self
                    .index_inner(
                        project,
                        selected,
                        ChangeBatch {
                            paths,
                            ..Default::default()
                        },
                        cancellation,
                        &mut effects,
                        progressive.as_ref(),
                    )
                    .await?;
                version = current.or(version);
            }
            Ok(version)
        }
        .await;
        match result {
            Ok(version) => Ok(semantic::AstSemanticProgress {
                version,
                pending_files: effects.pending_files,
                model_reconciled: false,
            }),
            Err(cause) => {
                tracing::debug!(error_kind = ?cause.kind(), "AST Raw publication failed");
                #[cfg(all(test, target_os = "linux"))]
                if let Some(sender) = self.next_raw_error.lock().unwrap().take() {
                    let _ = sender.send(cause.kind());
                }
                Err(io::Error::new(
                    cause.kind(),
                    RawPublicationFailure { effects, cause },
                ))
            }
        }
    }
    async fn index_inner(
        &self,
        project: &ProjectPaths,
        selected: &ProjectContext,
        batch: ChangeBatch,
        cancellation: &Cancellation,
        effects: &mut RawEffects,
        progressive_mode: Option<&(
            &Cancellation,
            tokio::sync::mpsc::UnboundedSender<String>,
            std::cell::RefCell<Option<tokio::sync::oneshot::Sender<()>>>,
        )>,
    ) -> io::Result<Option<u64>> {
        if let Some(error) = batch.error {
            return Err(error);
        }
        let request =
            graphit_hub::policy::RequestContext::service(self.policy.content(), cancellation)
                .map_err(err)?;
        let auth_error = |error| request_auth_error(error, &request);
        if project.root() != selected.root() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let config = project_config(&self.installation, project, &self.base, request.snapshot())?;
        if !ModuleSettings::resolve(&config)?.enabled(Module::Ast) {
            return Ok(None);
        }
        let captured = CapturedPublicationScope::project(
            project,
            selected.id,
            self.scope,
            request.snapshot(),
        )?;
        let record = captured.binding()?;
        let project_capture = ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version: &selected.version,
        };
        let scope = ScopeInput {
            installation: self.auth.installation(),
            project: Some(project_capture),
            providers: &[],
            binding: self.binding,
            global_version: "",
        };
        let runtime = self.auth.runtime().check(request.snapshot()).map_err(err)?;
        let raw =
            std::str::from_utf8(runtime.secret(request.snapshot()).map_err(err)?).map_err(err)?;
        let caller = self
            .auth
            .issuer()
            .authenticate_runtime(raw, &request)
            .map_err(auth_error)?;
        let authority = ProjectRecordAuthority::new(self.auth.issuer());
        let read = authority
            .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Ast)
            .map_err(auth_error)?;
        let write = authority
            .admit_authenticated_write(&caller, &request, scope, ProjectWriteOperation::Ast)
            .map_err(auth_error)?;
        let recheck = || -> io::Result<()> {
            request.check()?;
            read.recheck().map_err(auth_error)?;
            write.recheck().map_err(auth_error)?;
            Ok(())
        };
        recheck()?;
        let profiles =
            self.profiles
                .load(&self.installation, project, &config, request.snapshot())?;
        let ignore = AstIgnorePolicy::from_config(&config, request.snapshot()).map_err(err)?;
        let defaults: Vec<_> = ignore.defaults().iter().map(String::as_str).collect();
        let selectors: Vec<_> = profiles.selectors.iter().map(String::as_str).collect();
        let mut relative = BTreeSet::new();
        for path in &batch.paths {
            request.check()?;
            relative.insert(path.strip_prefix(project.root()).map_err(err)?.to_owned());
        }
        let query_dir = crate::ast_profile_loader::queries_directory(project, &config)?;
        let config_path = project.identity_file();
        let rescan = batch.rescan
            || batch.controls_changed
            || batch.paths.iter().any(|p| {
                p == project.root()
                    || p == &config_path
                    || p.starts_with(project.root().join(&query_dir))
            });
        let subtree = relative
            .iter()
            .find(|path| project.root().join(path).is_dir());
        let extras: Vec<_> = relative
            .iter()
            .filter(|path| subtree.is_none_or(|subtree| !path.starts_with(subtree)))
            .map(PathBuf::as_path)
            .collect();
        let max = usize::try_from(self.policy.limit(ResourceSetting::ContentInputBytes).value)
            .map_err(err)?;
        let index_source = config
            .get::<bool>(&ConfigKey::new("ast.index_source")?)?
            .unwrap_or(true);
        let corpus_relative = graphit_core::local_corpus::resolve(
            project_capture.scope.live,
            selected.version.version(),
            "ast",
            "source.lance",
            self.native,
            request.snapshot(),
        )?;
        let mapped = corpus_relative != Path::new("ast").join("source.lance");
        let selected_path = project_capture
            .scope
            .live
            .runtime_root()
            .join(corpus_relative);
        let selected_parent = selected_path.parent().ok_or(io::ErrorKind::InvalidData)?;
        let installation_root = self.auth.installation().live.owner_root();
        let components = selected_parent
            .strip_prefix(installation_root)
            .map_err(err)?;
        let mut path = installation_root.to_path_buf();
        let mut parent =
            CapturedDocumentDirectory::capture(&path, self.native, request.snapshot())?;
        parent.private_handle(request.snapshot())?;
        for component in components.components() {
            recheck()?;
            parent.revalidate(request.snapshot())?;
            path.push(component);
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
                Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
                Err(e) if e.kind() == io::ErrorKind::NotFound && !mapped => {
                    drop(graphit_fs::create_private_directory(&path)?);
                    effects.created_directories.push(path.clone());
                }
                Err(e) => return Err(e),
            }
            parent.revalidate(request.snapshot())?;
            parent = CapturedDocumentDirectory::capture(&path, self.native, request.snapshot())?;
            parent.private_handle(request.snapshot())?;
        }
        let path = selected_path;
        let presence = match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => true,
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(e) if e.kind() == io::ErrorKind::NotFound && !mapped => false,
            Err(e) => return Err(e),
        };
        recheck()?;
        parent.revalidate(request.snapshot())?;
        // Progressive publication can initialize an empty Raw corpus and admit
        // explicit FILEs before the retained full pass. Raw-only callers still
        // require the complete initial selected image.
        let rescan = rescan || (!presence && progressive_mode.is_none());
        let selection_limits = ProjectSelectionLimits {
            max_depth: usize::try_from(self.policy.limit(ResourceSetting::SnapshotDepth).value)
                .map_err(err)?,
            max_input_items: usize::try_from(
                self.policy
                    .limit(ResourceSetting::ContentCollectionEntries)
                    .value,
            )
            .map_err(err)?,
            max_input_bytes: max,
            max_observed_path_bytes: graphit_core::native_path::MAX_PATH_BYTES,
            max_selected_file_bytes: max as u64,
            max_control_file_bytes: max as u64,
        };
        let criteria = || ProjectSelectionCriteria {
            subtree: if rescan {
                Some(Path::new("."))
            } else {
                subtree.map(PathBuf::as_path)
            },
            extras: if rescan { &[] } else { &extras },
            include_readme: false,
            extensions: &selectors,
            ignore: ProjectIgnoreCriteria {
                protected_defaults: &defaults,
                custom_control: Some(ignore.filename()),
                git_control: Some(".gitignore"),
            },
        };
        let mut paths = graphit_core::artifact_store::selected_project_paths(
            &captured,
            criteria(),
            selection_limits,
            request.snapshot(),
        )?;
        let discovered_paths = paths.clone();
        paths.sort_by(|left, right| {
            let priority = |path: &Path| relative.iter().any(|root| path.starts_with(root));
            priority(right)
                .cmp(&priority(left))
                .then_with(|| left.cmp(right))
        });
        let branch_name = selected.version.lance_branch();
        let branch = (!branch_name.is_empty()).then_some(branch_name.as_str());
        tracing::debug!(phase = "prepare", "AST Raw publication boundary");
        if !presence {
            // Keep a dataset under construction outside the readable corpus locator.
            let mut staging_builder = tempfile::Builder::new();
            staging_builder.prefix(".source-");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                staging_builder.permissions(std::fs::Permissions::from_mode(0o700));
            }
            let staging = staging_builder.tempdir_in(parent.canonical_path())?;
            parent.revalidate(request.snapshot())?;
            let directory = CapturedDocumentDirectory::capture(
                staging.path(),
                self.native,
                request.snapshot(),
            )?;
            directory.private_handle(request.snapshot())?;
            let uri = staging.path().to_str().ok_or(io::ErrorKind::InvalidData)?;
            let stream = Box::pin(RecordBatchStreamAdapter::new(
                source_schema(),
                tokio_stream::iter([Ok(RecordBatch::new_empty(source_schema()))]),
            ));
            let prepared: PreparedCreate =
                LanceModuleStore::prepare_create_stream(uri, source_schema(), stream)
                    .await
                    .map_err(err)?;
            tracing::debug!(
                phase = "prepublish_recheck_begin",
                "AST Raw publication boundary"
            );
            profiles.revalidate(request.snapshot())?;
            recheck()?;
            directory.revalidate(request.snapshot())?;
            tracing::debug!(
                phase = "prepublish_recheck_end",
                "AST Raw publication boundary"
            );
            effects
                .keys
                .push(RawKey::Create(prepared.recovery_key().clone()));
            // Once commit starts its outcome may be unknown. Retain that exact SDK
            // locator on failure rather than deleting evidence of a possible commit.
            let staging = staging.keep();
            effects.retained_staging = Some(staging.clone());
            tracing::debug!(phase = "publish", "AST Raw publication boundary");
            let (created, _) = prepared.commit().await.map_err(err)?;
            let version = created.snapshot().version();
            recheck()?;
            profiles.revalidate(request.snapshot())?;
            directory.revalidate(request.snapshot())?;
            parent.revalidate(request.snapshot())?;
            graphit_fs::rename_exclusive(&staging, &path)?;
            effects.retained_staging = None;
            effects.created_directories.push(path.clone());
            effects.published_version = Some(version);
            drop(created);
            parent.revalidate(request.snapshot())?;
            let published =
                CapturedDocumentDirectory::capture(&path, self.native, request.snapshot())?;
            published.private_handle(request.snapshot())?;
            recheck()?;
            profiles.revalidate(request.snapshot())?;
            published.revalidate(request.snapshot())?;
        }
        let directory = CapturedDocumentDirectory::capture(&path, self.native, request.snapshot())?;
        directory.private_handle(request.snapshot())?;
        let uri = path.to_str().ok_or(io::ErrorKind::InvalidData)?;
        recheck()?;
        directory.revalidate(request.snapshot())?;
        let current = {
            let store = LanceBranchStore::open(uri, source_schema(), None)
                .await
                .map_err(err)?;
            recheck()?;
            directory.revalidate(request.snapshot())?;
            store.capture_target(branch).await.map_err(err)?
        };
        // This is a writer admission. Establish the branch before downstream
        // projections capture its origin, even if its Source matches Main.
        let mut current = if current.inherited_target().is_some() {
            recheck()?;
            let materialized = current.materialize().await.map_err(err)?;
            effects.materialized_branch = materialized.physical_branch().map(str::to_owned);
            materialized
        } else {
            current
        };
        recheck()?;
        directory.revalidate(request.snapshot())?;
        let mut previous = if rescan {
            branch_source_paths(&current, request.snapshot())
                .await
                .map_err(err)?
                .into_iter()
                .collect::<BTreeSet<_>>()
        } else {
            source_paths_under(
                &current,
                relative
                    .iter()
                    .map(|p| literal_path(p))
                    .collect::<io::Result<Vec<_>>>()?,
                request.snapshot(),
            )
            .await?
        };
        let mut live = if progressive_mode.is_some() {
            Some(
                progressive::ProgressivePublication::open(
                    self.auth.installation(),
                    project_capture,
                    self.native,
                    Arc::clone(request.snapshot()),
                    Arc::clone(&self.live),
                    &recheck,
                )
                .await?,
            )
        } else {
            None
        };
        // Keep the rebuildable writer lookup index for this FILE batch, then
        // release it before background inference or idle time. No native
        // Ladybug database/pool belongs to this owner.
        let graph_owner = match live.as_ref() {
            Some(live) => Some(live.capture_index().await?),
            None => None,
        };
        let mut initial_digests = BTreeMap::<String, ([u8; 32], Option<String>)>::new();
        if let Some(index) = graph_owner.as_ref() {
            let observations = if rescan {
                index.graph.files(request.snapshot())?
            } else {
                let mut observations = Vec::new();
                for path in &relative {
                    observations.extend(
                        index
                            .graph
                            .files_under(&literal_path(path)?, request.snapshot())?,
                    );
                }
                observations
            };
            for observation in observations {
                previous.insert(observation.path.clone());
                initial_digests.insert(
                    observation.path,
                    (observation.source_sha256, observation.scip_compiler),
                );
            }
        }
        if live.is_some()
            && let Some((_, _, started)) = progressive_mode
            && let Some(started) = started.borrow_mut().take()
        {
            let _ = started.send(());
        }
        let retained = paths
            .iter()
            .map(|path| literal_path(path))
            .collect::<io::Result<BTreeSet<_>>>()?;
        // An explicit FILE/subtree event also admits its missing previous FILEs.
        // Process those tombstones in the same per-FILE pipeline before a pending
        // full scan's tail; only broad membership pruning still requires EOF.
        paths.extend(previous.iter().filter_map(|path| {
            let path = Path::new(path);
            relative
                .iter()
                .any(|root| {
                    !root.as_os_str().is_empty() && root != Path::new(".") && path.starts_with(root)
                })
                .then(|| path.to_owned())
        }));
        paths.sort_by(|left, right| {
            let priority = |path: &Path| relative.iter().any(|root| path.starts_with(root));
            priority(right)
                .cmp(&priority(left))
                .then_with(|| left.cmp(right))
        });
        paths.dedup();
        // Raw-only callers do not initialize parsers or SQLite staging.
        let prepare_graph = live.is_some() && !paths.is_empty();
        let identity = prepare_graph
            .then(|| semantic::identity_limits(&self.policy))
            .transpose()?;
        let mut embedded_models = if prepare_graph {
            Some(semantic::generic::Models::new(
                &self.parsers,
                profiles.profiles(request.snapshot())?,
            ))
        } else {
            None
        };
        let mut graph = prepare_graph
            .then(|| graphit_data::graph_writer::IcebugWriter::new(Arc::clone(request.snapshot())))
            .transpose()?;
        let scip_index = graph
            .as_ref()
            .map(|graph| graph::ScipIndex::new(graph, selected))
            .transpose()?;
        let mut published = effects.published_version;
        for relative_path in paths {
            recheck()?;
            if rescan
                && progressive_mode
                    .is_some_and(|(source_preemption, _, _)| source_preemption.is_cancelled())
            {
                // The caller retains the unfinished full scan. No deletion is
                // inferred from a yielded or interrupted enumeration.
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "AST full publication yielded between FILEs",
                ));
            }
            let literal = literal_path(&relative_path)?;
            previous.remove(&literal);
            let extras = [relative_path.as_path()];
            let reader = CapturedSourceReader::capture_selected_project(
                &captured,
                ProjectSelectionCriteria {
                    subtree: None,
                    extras: &extras,
                    include_readme: false,
                    extensions: &selectors,
                    ignore: ProjectIgnoreCriteria {
                        protected_defaults: &defaults,
                        custom_control: Some(ignore.filename()),
                        git_control: Some(".gitignore"),
                    },
                },
                selection_limits,
                SourceReadLimits::new(max, 64 * 1024, max as u64)?,
                request.snapshot(),
            )?;
            let revalidate_file = || {
                #[cfg(unix)]
                {
                    reader.revalidate_bindings()
                }
                // Native observations currently expose change-time only on Unix.
                // Preserve byte validation elsewhere until an equivalent native
                // mutation observation is available.
                #[cfg(not(unix))]
                {
                    reader.revalidate_content()
                }
            };
            let affected = BTreeSet::from([relative_path]);
            let changed = !unchanged(
                &current,
                &reader,
                &profiles,
                &affected,
                false,
                index_source,
                request.snapshot(),
            )
            .await?;
            if changed {
                let replacement_paths = [literal.clone()];
                let prepared = prepare_stream(
                    &reader,
                    &profiles,
                    index_source,
                    request.snapshot(),
                    &recheck,
                    |stream| {
                        current.prepare_replace_paths_stream(
                            &replacement_paths,
                            stream,
                            vec!["path".into()],
                        )
                    },
                )
                .await?;
                revalidate_file()?;
                profiles.revalidate(request.snapshot())?;
                recheck()?;
                directory.revalidate(request.snapshot())?;
                effects
                    .keys
                    .push(RawKey::Delta(prepared.recovery_key().clone()));
                current = prepared.commit().await.map_err(err)?;
                effects.published_version = Some(current.version());
                current = index_source_paths(current, effects, &recheck).await?;
                published = Some(current.version());
            }
            if let Some(live) = live.as_mut() {
                // Unchanged Raw does not imply a missing/obsolete derived FILE
                // is healthy (for example after an interrupted first scan).
                let unchanged_source = !changed && !batch.controls_changed;
                let captured_digest = reader
                    .nodes()
                    .find(|node| node.kind() == graphit_fs::NodeKind::File)
                    .and_then(|node| node.digest());
                // This scope already captured owned FILE metadata. Reuse it
                // only for this no-write decision; newer compiler facts or a
                // concurrent deletion must not be replayed by unchanged Raw.
                let compiler_image = profiles
                    .selected_scip_compiler(Path::new(&literal), request.snapshot())?
                    .map(|compiler| compiler.image(&config))
                    .transpose()?;
                let unchanged_observed_file = unchanged_source
                    && initial_digests.remove(&literal).is_some_and(
                        |(digest, previous_compiler)| {
                            Some(digest) == captured_digest && previous_compiler == compiler_image
                        },
                    );
                // For this selected FILE, unchanged already verified Source
                // from the same retained Raw pin. Other paths still need the
                // existing presence check before retaining or deleting Native.
                if unchanged_observed_file
                    && reader.nodes().any(|node| {
                        node.kind() == graphit_fs::NodeKind::File
                            && node.relative() == Path::new(&literal)
                    })
                    && profiles.allows(Path::new(&literal), request.snapshot())?
                {
                    request.snapshot().check()?;
                    continue;
                }
                let indexed = read_branch_source(&current, &literal, request.snapshot())
                    .await
                    .map_err(err)?;
                if indexed.missing_reason()
                    == Some(graphit_ast::indexed_source::MissingSourceReason::FileAbsent)
                {
                    live.remove_file(&literal, &recheck).await?;
                    continue;
                }
                if unchanged_observed_file {
                    continue;
                }
                let observation = if unchanged_source {
                    let index = live.capture_index().await?;
                    index.graph.file_observation(&literal, request.snapshot())?
                } else {
                    None
                };
                if unchanged_source
                    && observation.as_ref().is_some_and(|observation| {
                        Some(observation.source_sha256) == captured_digest
                            && observation.scip_compiler == compiler_image
                    })
                {
                    continue;
                }
                if let graphit_ast::catalog::Selection::Candidates(file_profiles) =
                    profiles.resolve(Path::new(&literal), request.snapshot())?
                {
                    let (next, file) = semantic::SyntaxPreparation {
                        owner: self,
                        project_capture,
                        profiles: &profiles,
                        original: request.snapshot(),
                        identity: identity.ok_or(io::ErrorKind::InvalidData)?,
                        embedding: ModuleSettings::resolve(&config)?.enabled(Module::Embedding),
                        scip_index: scip_index.as_ref().ok_or(io::ErrorKind::InvalidData)?,
                    }
                    .prepare(
                        graph.take().ok_or(io::ErrorKind::InvalidData)?,
                        &literal,
                        file_profiles,
                        indexed,
                        embedded_models.as_mut().ok_or(io::ErrorKind::InvalidData)?,
                        compiler_image.as_deref(),
                        &recheck,
                    )
                    .await?;
                    graph = Some(next);
                    revalidate_file()?;
                    live.publish_file(file, &recheck).await?;
                    if let Some((_, sender, _)) = progressive_mode {
                        let _ = sender.send(literal.clone());
                    }
                    #[cfg(all(test, target_os = "linux"))]
                    self.after_file_publication(&literal, request.snapshot())
                        .await?;
                } else if profiles
                    .native_scip_compiler(Path::new(&literal), request.snapshot())?
                    .is_some()
                {
                    // The previous compiler no longer describes this Source/association.
                    // Keep Raw immediately readable; never serve obsolete semantic facts.
                    live.remove_file(&literal, &recheck).await?;
                    effects.pending_files.insert(literal.clone());
                } else {
                    live.remove_file(&literal, &recheck).await?;
                }
            }
            revalidate_file()?;
            directory.revalidate(request.snapshot())?;
        }
        // Only successful EOF with unchanged selection membership proves that a
        // formerly selected FILE disappeared. Recheck names and ignore controls,
        // never hash unrelated payloads merely to authorize pruning.
        recheck()?;
        if rescan
            && progressive_mode
                .is_some_and(|(source_preemption, _, _)| source_preemption.is_cancelled())
        {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "AST publication yielded before EOF pruning",
            ));
        }
        if graphit_core::artifact_store::selected_project_paths(
            &captured,
            criteria(),
            selection_limits,
            request.snapshot(),
        )? != discovered_paths
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "AST selection changed before EOF pruning",
            ));
        }
        if !previous.is_empty() {
            recheck()?;
            let deleted = previous.into_iter().collect::<Vec<_>>();
            let stream = Box::pin(RecordBatchStreamAdapter::new(
                source_schema(),
                tokio_stream::iter([Ok(RecordBatch::new_empty(source_schema()))]),
            ));
            let prepared = current
                .prepare_replace_paths_stream(&deleted, stream, vec!["path".into()])
                .await
                .map_err(err)?;
            profiles.revalidate(request.snapshot())?;
            directory.revalidate(request.snapshot())?;
            recheck()?;
            effects
                .keys
                .push(RawKey::Delta(prepared.recovery_key().clone()));
            current = prepared.commit().await.map_err(err)?;
            effects.published_version = Some(current.version());
            published = Some(current.version());
            if let Some(live) = live.as_mut() {
                for path in deleted {
                    live.remove_file(&path, &recheck).await?;
                }
            }
        }
        if rescan && let Some(live) = live.as_mut() {
            let retained = retained.iter().map(String::as_str).collect();
            live.retain_paths(&retained, &recheck).await?;
        }
        profiles.revalidate(request.snapshot())?;
        directory.revalidate(request.snapshot())?;
        recheck()?;
        Ok(published)
    }
}
async fn index_source_paths(
    snapshot: BranchSnapshot,
    effects: &mut RawEffects,
    recheck: &impl Fn() -> io::Result<()>,
) -> io::Result<BranchSnapshot> {
    recheck()?;
    let Some(prepared) = snapshot
        .prepare_scalar_append("path", "path_source")
        .await
        .map_err(err)?
    else {
        return Ok(snapshot);
    };
    recheck()?;
    effects
        .keys
        .push(RawKey::Delta(prepared.recovery_key().clone()));
    let snapshot = prepared.commit().await.map_err(err)?;
    effects.published_version = Some(snapshot.version());
    recheck()?;
    Ok(snapshot)
}

fn source_path_filter(
    paths: impl IntoIterator<Item = String>,
) -> Option<graphit_data::lance_branch_store::Expr> {
    paths
        .into_iter()
        .map(|path| {
            // Canonical paths use '/'. Its byte successor '0' closes the exact
            // descendant prefix range, preserving literal Unicode, '%' and '_'.
            let first_child = format!("{path}/");
            let after_children = format!("{path}0");
            col("path").eq(lit(path)).or(col("path")
                .gt_eq(lit(first_child))
                .and(col("path").lt(lit(after_children))))
        })
        .reduce(|left, right| left.or(right))
}

async fn source_paths_under(
    snapshot: &BranchSnapshot,
    paths: impl IntoIterator<Item = String>,
    original: &Arc<SnapshotContext>,
) -> io::Result<BTreeSet<String>> {
    use arrow_array::Array;
    use futures_util::TryStreamExt;
    original.check()?;
    let Some(filter) = source_path_filter(paths) else {
        return Ok(BTreeSet::new());
    };
    let mut scan = snapshot.scan();
    scan.project(&["path"]).map_err(err)?;
    scan.filter_expr(filter);
    let work = async {
        let mut stream = scan.try_into_stream().await.map_err(err)?;
        let mut paths = BTreeSet::new();
        while let Some(batch) = stream.try_next().await.map_err(err)? {
            original.check()?;
            let column = batch
                .column_by_name("path")
                .and_then(|column| column.as_any().downcast_ref::<StringArray>())
                .ok_or(io::ErrorKind::InvalidData)?;
            for row in 0..column.len() {
                if column.is_null(row) {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                paths.insert(column.value(row).to_owned());
            }
        }
        original.check()?;
        Ok(paths)
    };
    tokio::select! {
        biased;
        () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = async { match original.deadline() {
            Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
            None => std::future::pending().await,
        }} => Err(io::ErrorKind::TimedOut.into()),
        result = work => result,
    }
}
async fn unchanged(
    snapshot: &BranchSnapshot,
    reader: &CapturedSourceReader,
    profiles: &LoadedProfiles,
    affected: &BTreeSet<PathBuf>,
    rescan: bool,
    index_source: bool,
    original: &Arc<SnapshotContext>,
) -> io::Result<bool> {
    let mut selected = BTreeMap::new();
    for node in reader
        .nodes()
        .filter(|n| n.kind() == graphit_fs::NodeKind::File)
    {
        if profiles.allows(node.relative(), original)? {
            selected.insert(literal_path(node.relative())?, node);
        }
    }
    let previous = if rescan {
        branch_source_paths(snapshot, original)
            .await
            .map_err(err)?
            .into_iter()
            .collect()
    } else {
        let roots = affected
            .iter()
            .map(|p| literal_path(p))
            .collect::<io::Result<Vec<_>>>()?;
        source_paths_under(snapshot, roots, original).await?
    };
    if rescan
        && (selected.len() != previous.len() || selected.keys().any(|p| !previous.contains(p)))
    {
        return Ok(false);
    }
    for path in previous
        .iter()
        .chain(selected.keys().filter(|p| !previous.contains(*p)))
    {
        original.check()?;
        let old = read_branch_source(snapshot, path, original)
            .await
            .map_err(err)?;
        let node = selected.get(path);
        match (node, old.source(original).map_err(err)?) {
            (None, IndexedSource::Missing) => {}
            (Some(node), IndexedSource::Available(text))
                if index_source && profiles.allows(node.relative(), original)? =>
            {
                if let Some(captured) = reader.retained(node.relative()) {
                    if text.len() != captured.len() {
                        return Ok(false);
                    }
                    for (stored, captured) in text
                        .as_bytes()
                        .chunks(64 * 1024)
                        .zip(captured.chunks(64 * 1024))
                    {
                        original.check()?;
                        if stored != captured {
                            return Ok(false);
                        }
                    }
                } else {
                    let mut hash = Sha256::new();
                    for chunk in text.as_bytes().chunks(64 * 1024) {
                        original.check()?;
                        hash.update(chunk);
                    }
                    if Some(<[u8; 32]>::from(hash.finalize())) != node.digest() {
                        return Ok(false);
                    }
                }
            }
            // A NULL row cannot prove unchanged physical content; retain a real refresh.
            _ => return Ok(false),
        }
    }
    Ok(true)
}
pub(super) async fn prepare_stream<T, E, F, Fut>(
    reader: &CapturedSourceReader,
    profiles: &LoadedProfiles,
    index_source: bool,
    original: &Arc<SnapshotContext>,
    recheck: &impl Fn() -> io::Result<()>,
    prepare: F,
) -> io::Result<T>
where
    E: Error + Send + Sync + 'static,
    F: FnOnce(SendableRecordBatchStream) -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let schema = source_schema();
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let stream = Box::pin(RecordBatchStreamAdapter::new(
        Arc::clone(&schema),
        ReceiverStream::new(receiver),
    ));
    let producer = async {
        let mut produced = false;
        for node in reader
            .nodes()
            .filter(|n| n.kind() == graphit_fs::NodeKind::File)
        {
            let next = (|| -> io::Result<Option<RecordBatch>> {
                recheck()?;
                reader.check_request(original)?;
                if !profiles.allows(node.relative(), original)? {
                    return Ok(None);
                }
                let path = literal_path(node.relative())?;
                // A productive capture retains just this FILE. Reuse its
                // already hashed bytes instead of reopening and hashing them.
                let file;
                let range;
                let body = match reader.retained(node.relative()) {
                    Some(bytes) => {
                        reader.revalidate_bindings()?;
                        bytes
                    }
                    None => {
                        file = reader.open_file(node.relative())?;
                        range = file.read_range(0, usize::try_from(file.len()).map_err(err)?)?;
                        reader.validate_file_bytes(node.relative(), range.as_ref(), original)?;
                        range.as_ref()
                    }
                };
                let text = std::str::from_utf8(body).map_err(err)?;
                let batch = RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![
                        Arc::new(StringArray::from(vec![path.as_str()])),
                        Arc::new(StringArray::from(vec![index_source.then_some(text)])),
                    ],
                )
                .map_err(err)?;
                reader.revalidate_bindings()?;
                recheck()?;
                Ok(Some(batch))
            })();
            match next {
                Ok(Some(batch)) => {
                    produced = true;
                    if sender.send(Ok(batch)).await.is_err() {
                        return Ok(false);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let error = Arc::new(error);
                    let kind = error.kind();
                    let _ = sender
                        .send(Err(DataFusionError::IoError(io::Error::new(
                            kind,
                            Arc::clone(&error),
                        ))))
                        .await;
                    return Err(io::Error::new(kind, error));
                }
            }
        }
        if !produced
            && sender
                .send(Ok(RecordBatch::new_empty(schema)))
                .await
                .is_err()
        {
            return Ok(false);
        }
        drop(sender);
        tracing::debug!(phase = "stream_eof", "AST Raw publication boundary");
        Ok(true)
    };
    let sdk = prepare(stream);
    let preparing = async {
        let result = sdk.await;
        tracing::debug!(
            phase = "sdk_prepare_returned",
            "AST Raw publication boundary"
        );
        result
    };
    let (produced, prepared) = tokio::join!(producer, preparing);
    // Keep the original FILE/admission cause even if the SDK wraps its stream error.
    // A closed SDK consumer still reports its own failure rather than BrokenPipe.
    let produced = produced?;
    let prepared = prepared.map_err(err)?;
    if !produced {
        return Err(io::ErrorKind::BrokenPipe.into());
    }
    Ok(prepared)
}
