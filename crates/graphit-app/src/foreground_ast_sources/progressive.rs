//! Local FILE publication, separate from portable graph exports and inference.
use super::*;
use graphit_ast::live_graph::LiveGraph;
use graphit_data::graph_writer::{GraphEdge, GraphNode};
use std::sync::{Mutex, Weak};
use tokio::sync::Mutex as AsyncMutex;

pub(crate) struct FilePublication {
    pub path: String,
    pub source_sha256: [u8; 32],
    pub raw_version: u64,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub definitions: Vec<graphit_ast::semantic_graph::scip_document::AstScipDocumentDefinition>,
    pub references: Vec<graphit_ast::semantic_graph::scip_document::AstScipDocumentReference>,
    pub scip_document: Option<Vec<u8>>,
    pub pending_compiler: Option<String>,
    pub scip_compiler: Option<String>,
    pub oracle: Vec<graphit_ast::semantic_graph::oracle::AstOracleGraphRows>,
    pub oracle_file: Option<graphit_ast::semantic_graph::AstGraphNode>,
    pub search: Vec<RecordBatch>,
    pub projections: Vec<RecordBatch>,
    pub identity_changes: Vec<graphit_ast::semantic_graph::scip_document::AstGraphIdentityChange>,
}
impl FilePublication {
    pub fn new(path: String, source_sha256: [u8; 32], raw_version: u64) -> Self {
        Self {
            path,
            source_sha256,
            raw_version,
            nodes: Vec::new(),
            edges: Vec::new(),
            search: Vec::new(),
            projections: Vec::new(),
            identity_changes: Vec::new(),
            definitions: Vec::new(),
            references: Vec::new(),
            scip_document: None,
            pending_compiler: None,
            scip_compiler: None,
            oracle: Vec::new(),
            oracle_file: None,
        }
    }
}

/// One live database owner, shared only while a producer or reader needs it.
/// Authorization stays with each operation.
pub(crate) struct LiveIndex {
    pub graph: Arc<LiveGraph>,
    pub fence: Arc<AsyncMutex<()>>,
}

impl LiveIndex {
    pub async fn write_fence(
        &self,
        original: &SnapshotContext,
    ) -> io::Result<tokio::sync::OwnedMutexGuard<()>> {
        write_fence(&self.fence, original).await
    }
}

pub(super) async fn write_fence(
    fence: &Arc<AsyncMutex<()>>,
    original: &SnapshotContext,
) -> io::Result<tokio::sync::OwnedMutexGuard<()>> {
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
        guard = Arc::clone(fence).lock_owned() => {
            original.check()?;
            Ok(guard)
        },
    }
}

#[derive(Default)]
pub(crate) struct LiveAstRegistry(
    Mutex<BTreeMap<PathBuf, Arc<LiveSlot>>>,
    #[cfg(test)] pub(crate) Mutex<Option<Arc<tokio::sync::Barrier>>>,
);

#[derive(Default)]
struct LiveSlot {
    entry: tokio::sync::Mutex<Option<LiveEntry>>,
    fence: Arc<AsyncMutex<()>>,
}

struct LiveEntry {
    graph: Weak<LiveGraph>,
    closed: tokio::sync::watch::Receiver<()>,
}

pub(super) struct CapturedCohort {
    pub(super) root: PathBuf,
    namespace: [u8; 32],
    slot: Arc<LiveSlot>,
}

// This stable PROJECT/version locator owns the live graph directory only.
// It is not a search/Source/embedding publication filter.
const COHORT_FORMAT: &str = "ast-icebug-file-1";

pub(crate) fn location(
    project: graphit_auth::ProjectSelection<'_>,
) -> io::Result<(PathBuf, [u8; 32])> {
    let graphit_core::artifact_store::StoreOwner::Project { project_id } = project.scope.owner()
    else {
        return Err(io::ErrorKind::InvalidInput.into());
    };
    let namespace: [u8; 32] = Sha256::digest(
        serde_json::to_vec(&(
            COHORT_FORMAT,
            project_id.to_string(),
            project.version.version(),
        ))
        .map_err(err)?,
    )
    .into();
    let segment = namespace
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok((
        project
            .scope
            .runtime_root()
            .join("ast")
            .join("live")
            .join(segment),
        namespace,
    ))
}

impl LiveAstRegistry {
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn hold_native_close_for_test(&self, root: &Path) -> tokio::sync::watch::Sender<()> {
        let slot = {
            let registry = self.0.lock().unwrap();
            Arc::clone(registry.get(root).unwrap())
        };
        let mut locked = slot.entry.try_lock().unwrap();
        let entry = locked.as_mut().unwrap();
        assert!(entry.graph.upgrade().is_none());
        assert!(entry.closed.has_changed().is_err());
        let (sender, closed) = tokio::sync::watch::channel(());
        entry.closed = closed;
        sender
    }

    pub async fn capture(
        &self,
        project: graphit_auth::ProjectSelection<'_>,
        original: &Arc<SnapshotContext>,
        create: bool,
    ) -> io::Result<Option<(Arc<LiveIndex>, PathBuf)>> {
        let Some(cohort) = self.capture_cohort(project, original, create)? else {
            return Ok(None);
        };
        let Some(index) = cohort.capture_index(original, create).await? else {
            return Ok(None);
        };
        Ok(Some((index, cohort.root)))
    }

    pub(super) fn capture_cohort(
        &self,
        project: graphit_auth::ProjectSelection<'_>,
        original: &Arc<SnapshotContext>,
        create: bool,
    ) -> io::Result<Option<CapturedCohort>> {
        original.check()?;
        let (root, namespace) = location(project)?;
        if !create && !root.try_exists()? {
            return Ok(None);
        }
        if create {
            std::fs::create_dir_all(&root)?;
        }
        if !std::fs::metadata(&root)?.is_dir() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        original.check()?;
        // The map only locates a stable cohort slot. Icebug catalogue/index IO
        // must not prevent another PROJECT/version from capturing its graph.
        let slot = {
            let mut registry = self
                .0
                .lock()
                .map_err(|_| io::Error::other("live AST registry poisoned"))?;
            Arc::clone(registry.entry(root.clone()).or_default())
        };
        Ok(Some(CapturedCohort {
            root,
            namespace,
            slot,
        }))
    }
}

impl CapturedCohort {
    pub(super) fn fence(&self) -> &Arc<AsyncMutex<()>> {
        &self.slot.fence
    }

    async fn entry(
        &self,
        original: &SnapshotContext,
    ) -> io::Result<tokio::sync::MutexGuard<'_, Option<LiveEntry>>> {
        original.check()?;
        let deadline = async {
            match original.deadline() {
                Some(deadline) => {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await
                }
                None => std::future::pending::<()>().await,
            }
        };
        let locked = tokio::select! {
            biased;
            () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = deadline => return Err(io::ErrorKind::TimedOut.into()),
            entry = self.slot.entry.lock() => entry,
        };
        original.check()?;

        Ok(locked)
    }

    async fn capture_index(
        &self,
        original: &Arc<SnapshotContext>,
        create: bool,
    ) -> io::Result<Option<Arc<LiveIndex>>> {
        let mut locked = self.entry(original).await?;
        loop {
            if let Some(entry) = locked.as_ref() {
                if let Some(graph) = entry.graph.upgrade() {
                    return Ok(Some(Arc::new(LiveIndex {
                        graph,
                        fence: Arc::clone(&self.slot.fence),
                    })));
                }
                // The temporary lookup index closes without acquiring this slot.
                // Keep same-cohort openers serialized while that close finishes.
                if entry.closed.has_changed().is_ok() {
                    wait_for_close(entry.closed.clone(), original).await?;

                    continue;
                }
            }
            let present = self.root.join("graph/icebug.json").try_exists()?;
            if !create && !present {
                return Ok(None);
            }
            // Reproducible derived data is rebuilt only by an explicit producer.
            let initialize = create && !present;
            let graph = Arc::new(LiveGraph::open_path_owned(
                self.root.join("graph"),
                original,
                self.namespace,
                initialize,
            )?);

            original.check()?;
            *locked = Some(LiveEntry {
                graph: Arc::downgrade(&graph),
                closed: graph.close_notification(),
            });
            return Ok(Some(Arc::new(LiveIndex {
                graph,
                fence: Arc::clone(&self.slot.fence),
            })));
        }
    }
}

/// The watch carries no updates; sender drop acknowledges completed index close.
/// Waiting here never retries a query or retains the database being closed.
async fn wait_for_close(
    mut closed: tokio::sync::watch::Receiver<()>,
    original: &SnapshotContext,
) -> io::Result<()> {
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
        () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
        () = deadline => return Err(io::ErrorKind::TimedOut.into()),
        _ = closed.changed() => {},
    }
    original.check()
}

mod corpus;
pub(crate) use corpus::ProgressivePublication;
pub(super) use corpus::{clean_corpus_history, file_rows, snapshot as corpus_snapshot};
