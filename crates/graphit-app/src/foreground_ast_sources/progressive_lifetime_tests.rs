//! Native owners must end at FILE boundaries even while a full or NN backlog continues.
use super::foreground_memory_jobs_tests as support;
use super::*;
use crate::foreground_ast_sources::progressive::LiveIndex;
use arrow_array::RecordBatch;
use futures_util::{FutureExt, TryStreamExt};
use graphit_ast::native_nn_index_locator::{NativeNnIndexPresence, open_local_native_nn_index};
use graphit_auth::{ProjectCapture, ScopeCapture};
use graphit_core::{
    artifact_store::CapturedPublicationScope, config::ConfigStore,
    native_document_file::CapturedDocumentDirectory,
};
use graphit_daemon::{supervisor::ProjectContext, watcher::ChangeBatch};
use graphit_data::lance_branch_store::{AST_INDEX_BATCH, BranchSnapshot, col, lit};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

async fn pending_snapshot(root: &std::path::Path) -> TestResult<BranchSnapshot> {
    Ok(BranchSnapshot::capture_observed(
        root.parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("corpus.lance")
            .to_str()
            .unwrap(),
        None,
        None,
    )
    .await?)
}

async fn file_rows(snapshot: &BranchSnapshot, path: &str) -> TestResult<Vec<RecordBatch>> {
    let mut scan = snapshot.scan();
    let mut predicate = col("path")
        .eq(lit(path))
        .and(col("projection").is_not_null());
    if snapshot.schema().field_with_name("vector").is_ok() {
        predicate = predicate.and(col("vector").is_null());
    }
    scan.filter_expr(predicate);
    Ok(scan.try_into_stream().await?.try_collect().await?)
}

fn assert_last_native_owner(index: Arc<LiveIndex>, directory: CapturedDocumentDirectory) {
    assert_eq!(
        Arc::strong_count(&index.graph),
        1,
        "the suspended producer/NN backlog must not retain the native database"
    );
    let weak = Arc::downgrade(&index.graph);
    let closed = index.graph.close_notification();
    drop((index, directory));
    assert!(weak.upgrade().is_none());
    assert!(
        closed.has_changed().is_err(),
        "native teardown must finish before the next cold capture"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_and_nn_backlog_release_native_owner_between_files() -> TestResult {
    let (_fixture, owner, tasks, _, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let resources = owner.memory_job_resources.upgrade().unwrap();
    let cancellation = Cancellation::default();
    let config_store = ConfigStore::project(&selected.0)?;
    for (key, value) in [
        ("modules.ast", json!(true)),
        ("modules.embedding", json!(true)),
        ("ast.query.buffer_pool_bytes", json!(67_108_864)),
    ] {
        config_store.set(
            ConfigKey::new(key)?,
            value,
            Duration::from_secs(5),
            &cancellation,
        )?;
    }
    let first_text = "function alpha_source() return 7 end\n";
    std::fs::write(selected.0.root().join("a.lua"), first_text)?;
    std::fs::write(
        selected.0.root().join("b.lua"),
        "function beta_source() return 3 end\n",
    )?;
    let source = owner.server.as_ref().unwrap().clone();
    let original = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
    let no_docker = tempfile::tempdir()?;
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation: source.policy().installation.clone(),
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::KiroV3,
            environment: vec![("PATH".into(), no_docker.path().as_os_str().to_owned())],
        },
        original.snapshot(),
    )?);
    let context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let mut job = None;
    let result = std::panic::AssertUnwindSafe(async {
        let capture = CapturedPublicationScope::project(
            &selected.0, selected.1, owner.scope, original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture { live: &capture, record: &record },
            version: &selected.2,
        };
        let registry = source.graph_server().live_ast();
        let (graph_root, _) = crate::foreground_ast_sources::progressive::location(owner.auth.installation(), project)?;
        let open = || async {
            registry.capture(owner.auth.installation(), project, owner.native,
                original.snapshot(), false).await?
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        };
        let (first, release) = ast.hold_after_file_publication("a.lua");
        let producer = Arc::clone(&ast);
        let paths = selected.0.clone();
        let captured_context = context.clone();
        let lifetime = cancellation.clone();
        job = Some(tokio::spawn(async move {
            producer.index_productive(&paths, &captured_context,
                ChangeBatch { rescan: true, ..Default::default() }, None,
                Some(BTreeSet::new()), false, &lifetime, &lifetime, &lifetime).await
        }));
        tokio::time::timeout(Duration::from_secs(30), first).await??;
        assert!(!job.as_ref().unwrap().is_finished());
        let (index, directory) = open().await?;
        let first_source = index.graph.file_observation("a.lua", original.snapshot())?
            .ok_or("first committed FILE absent")?;
        assert_eq!(first_source.source_sha256, <[u8; 32]>::from(Sha256::digest(first_text)));
        assert!(index.graph.file_observation("b.lua", original.snapshot())?.is_none());
        let search = BranchSnapshot::capture_observed(
            graph_root.parent().unwrap().parent().unwrap().join("corpus.lance").to_str().unwrap(), None, None,
        ).await?;
        assert!(file_rows(&search, "a.lua").await?.iter().any(|batch| batch.num_rows() > 0));
        drop(search);
        // A live FILE batch retains its disk locator index for the next FILE.
        // Read access still works during publication; it owns no Ladybug pool.
        drop((index, directory));
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(30), job.as_mut().unwrap()).await???;
        job.take();
        let (index, directory) = open().await?;
        assert_eq!(index.graph.file_observation("a.lua", original.snapshot())?, Some(first_source.clone()));
        assert!(index.graph.file_observation("b.lua", original.snapshot())?.is_some());
        let pending = pending_snapshot(&graph_root).await?;
        for path in ["a.lua", "b.lua"] {
            assert!(file_rows(&pending, path).await?.iter().any(|batch| batch.num_rows() > 0));
        }
        drop(pending);
        assert_last_native_owner(index, directory);

        // Give the idle, private corpus real obsolete SDK versions without
        // changing any row, Source association or pending projection.
        let corpus_path = graph_root.parent().unwrap().parent().unwrap().join("corpus.lance");
        let corpus_uri = corpus_path.to_str().ok_or("corpus path is not Unicode")?;
        let mut history = pending_snapshot(&graph_root).await?;
        let batches = history.scan().try_into_stream().await?.try_collect::<Vec<_>>().await?;
        assert!(batches.iter().any(|batch| batch.num_rows() > 0));
        for _ in 0..AST_INDEX_BATCH {
            history = history.prepare_overwrite(batches.clone()).await?.commit().await?;
        }
        drop(batches);
        drop(history);
        let before_cleanup = lance::Dataset::open(corpus_uri).await?.versions().await?
            .into_iter().map(|version| version.version).collect::<BTreeSet<_>>();
        assert!(before_cleanup.len() >= AST_INDEX_BATCH);

        // The real NN consumer starts with both durable FILEs ready. Its next
        // FILE is already known while the first operation's owners are gone.
        let (first_nn, release_nn) = ast.hold_after_nn_file("a.lua");
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let reconciled = AtomicBool::new(false);
        let mut consumer = Box::pin(ast.embed_pending_loop(
            &selected.0, &context, Some(&resources), receiver, false,
            BTreeSet::from(["a.lua".into()]), &reconciled, &cancellation,
        ));
        let remaining = tokio::time::timeout(Duration::from_secs(60), async {
            tokio::select! {
                ready = first_nn => ready.map_err(io::Error::other),
                result = &mut consumer => Err(io::Error::other(format!("NN ended before FILE boundary: {result:?}"))),
            }
        }).await??;
        assert!(remaining > 0, "the second FILE must still be in ready");
        assert!(reconciled.load(Ordering::Relaxed));
        let (index, directory) = open().await?;
        assert_eq!(index.graph.file_observation("a.lua", original.snapshot())?, Some(first_source.clone()));
        let pending = pending_snapshot(&graph_root).await?;
        assert!(file_rows(&pending, "a.lua").await?.iter().all(|batch| batch.num_rows() == 0));
        assert!(file_rows(&pending, "b.lua").await?.iter().any(|batch| batch.num_rows() > 0));
        let after_cleanup = lance::Dataset::open(corpus_uri).await?.versions().await?
            .into_iter().map(|version| version.version).collect::<BTreeSet<_>>();
        assert!(after_cleanup.len() < before_cleanup.len(),
            "NN FILE cleanup must prune real SDK history before its ready tail completes: {before_cleanup:?} -> {after_cleanup:?}");
        assert!(!after_cleanup.contains(before_cleanup.first().unwrap()),
            "an obsolete version must be gone while b.lua remains pending");
        assert_last_native_owner(index, directory);
        // Both skip paths must release their capture and leave the valid tail
        // ready: an acknowledged FILE has no pending rows, and a stale notice
        // can name a FILE which is absent from the current graph.
        sender.send("a.lua".into())?;
        sender.send("0-absent.lua".into())?;
        drop(sender);
        release_nn.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(60), consumer.as_mut()).await??;
        drop(consumer);
        let (index, directory) = open().await?;
        assert_eq!(index.graph.file_observation("a.lua", original.snapshot())?, Some(first_source));
        let pending = pending_snapshot(&graph_root).await?;
        let NativeNnIndexPresence::Available(nn) = open_local_native_nn_index(
            owner.auth.installation(), project, original.snapshot(), owner.native,
        ).await? else { return Err("NN publication absent".into()); };
        for path in ["a.lua", "b.lua"] {
            assert!(file_rows(&pending, path).await?.iter().all(|batch| batch.num_rows() == 0));
            let mut scan=nn.snapshot(original.snapshot())?.scan();
            scan.filter_expr(col("path").eq(lit(path)).and(col("vector").is_not_null()));
            assert!(scan.count_rows().await? > 0);
        }
        assert_last_native_owner(index, directory);
        Ok::<(), Box<dyn std::error::Error>>(())
    }).catch_unwind().await;
    cancellation.cancel();
    if let Some(job) = job {
        let _ = job.await;
    }
    let compiler_cleanup = ast.shutdown().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
