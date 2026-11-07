//! The actual local publication/EOF caller of bounded Icebug maintenance.
use super::*;
use crate::foreground_ast_sources::progressive::{
    FilePublication, LiveAstRegistry, ProgressivePublication,
};
use futures_util::{FutureExt, TryStreamExt};
use graphit_ast::live_graph::{LiveFileInput, LiveGraph};
use graphit_auth::{ProjectCapture, ScopeCapture};
use graphit_data::graph_writer::{GraphNode, GraphValue};
use graphit_data::lance_branch_store::{BranchSnapshot, col, lit};
#[cfg(unix)]
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[path = "foreground_ast_source_maintenance_tests.rs"]
mod source_maintenance;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Fixture {
    _temporary: tempfile::TempDir,
    _empty_path: tempfile::TempDir,
    project: ProjectPaths,
    policy: HubResourcePolicy,
    owner: ForegroundSource,
    ast: Arc<ForegroundAstSources>,
    selected: graphit_daemon::supervisor::ProjectContext,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let (temporary, installation, project, config, policy) = ast_fixture();
        let lifetime = Cancellation::default();
        let startup = policy.request_context(Instant::now(), &lifetime)?;
        let mut owner = ForegroundSource::start(
            &installation,
            &[ProjectBootstrapInput {
                project: &project,
                explicit_version: "",
                pin: None,
                detached_base: "",
            }],
            0,
            &config,
            NativeBinding::Codex,
            &startup,
        )?;
        // Finite startup reserves a loopback socket even before publication.
        // These local callers never serve HTTP or need that reserved socket.
        drop(owner.listener.take());
        let selected = owner.projects.as_ref().unwrap()[0].clone();
        let selected = graphit_daemon::supervisor::ProjectContext {
            id: selected.1,
            version: selected.2,
        };
        let empty_path = tempfile::tempdir()?;
        let ast = Arc::new(ForegroundAstSources::new(
            Arc::clone(&owner.auth),
            ForegroundAstSourceConfig {
                live: owner.server.as_ref().unwrap().graph_server().live_ast(),
                installation: installation.clone(),
                base: config,
                policy: policy.clone(),
                scope: owner.scope,
                native: owner.native,
                binding: NativeBinding::Codex,
                environment: vec![("PATH".into(), empty_path.path().as_os_str().to_owned())],
            },
            startup.snapshot(),
        )?);
        drop(startup);
        Ok(Self {
            _temporary: temporary,
            _empty_path: empty_path,
            project,
            policy,
            owner,
            ast,
            selected,
        })
    }

    fn registry(&self) -> Arc<LiveAstRegistry> {
        self.owner
            .server
            .as_ref()
            .unwrap()
            .graph_server()
            .live_ast()
    }

    async fn index(&self, paths: &[&str], rescan: bool) -> TestResult {
        let lifetime = Cancellation::default();
        let preemption = Cancellation::default();
        let background = Cancellation::default();
        self.ast
            .index_productive(
                &self.project,
                &self.selected,
                graphit_daemon::watcher::ChangeBatch {
                    rescan,
                    paths: paths
                        .iter()
                        .map(|path| self.project.root().join(path))
                        .collect(),
                    ..Default::default()
                },
                None,
                Some(BTreeSet::new()),
                false,
                &preemption,
                &background,
                &lifetime,
            )
            .await?;
        Ok(())
    }

    async fn shutdown(self, result: std::thread::Result<TestResult>) -> TestResult {
        let compiler = self.ast.shutdown().await;
        let owner = self.owner.shutdown().await.map_err(|failure| failure.error);
        compiler?;
        owner?;
        match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
}

fn node(path: &str, name: &str) -> GraphNode {
    GraphNode {
        label: "Function".into(),
        key: path.into(),
        properties: BTreeMap::from([
            ("uid".into(), GraphValue::String(path.into())),
            ("path".into(), GraphValue::String(path.into())),
            ("name".into(), GraphValue::String(name.into())),
        ]),
    }
}

fn seed(
    graph: &LiveGraph,
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> io::Result<()> {
    for ordinal in 0..257 {
        let path = format!("f{ordinal:03}.lua");
        let nodes = [node(&path, "before")];
        graph.replace_file(
            LiveFileInput {
                path: &path,
                source_sha256: [3; 32],
                raw_version: 1,
                nodes: &nodes,
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            original,
        )?;
    }
    Ok(())
}

fn row_bases(manifest: &serde_json::Value) -> BTreeMap<String, Vec<serde_json::Value>> {
    manifest["live"]["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, file)| {
            (
                path.clone(),
                file["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|part| part["row_base"].clone())
                    .collect(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_eof_maintains_shards_and_preserves_incremental_source_and_reopen() -> TestResult {
    let fixture = Fixture::new()?;
    let result = std::panic::AssertUnwindSafe(async {
        for (path, source) in [
            ("a.lua", "function alpha() return 1 end\n"),
            ("b.lua", "function beta() return 2 end\n"),
        ] {
            std::fs::write(fixture.project.root().join(path), source)?;
        }
        fixture.index(&[], true).await?;
        let cancel = Cancellation::default();
        let original = fixture.policy.request_context(Instant::now(), &cancel)?;
        let capture = CapturedPublicationScope::project(
            &fixture.project,
            fixture.selected.id,
            fixture.owner.scope,
            original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &capture,
                record: &record,
            },
            version: &fixture.selected.version,
        };
        let registry = fixture.registry();
        let graph_root = crate::foreground_ast_sources::progressive::location(
            fixture.owner.auth.installation(),
            project,
        )?
        .0
        .join("graph");
        let source_path = fixture
            .project
            .project_runtime(fixture.selected.id, &["ast", "source.lance"])?;
        let (index, directory) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                false,
            )
            .await?
            .ok_or("EOF graph missing")?;
        let before = index.graph.files(original.snapshot())?;
        assert_eq!(before.len(), 2);
        let publication = ProgressivePublication::open(
            fixture.owner.auth.installation(),
            project,
            fixture.owner.native,
            Arc::clone(original.snapshot()),
            Arc::clone(&registry),
            &|| original.check(),
        )
        .await?;
        let manifest = std::fs::read(graph_root.join("icebug.json"))?;
        let report = publication
            .maintain_shards_batch(&|| original.check(), None)
            .await?
            .ok_or("graph maintenance missing")?;
        assert_eq!(
            report.consolidated_files, 0,
            "the real Raw/structural EOF already performed consolidation"
        );
        assert!(!report.more);
        assert_eq!(
            std::fs::read(graph_root.join("icebug.json"))?,
            manifest,
            "no useful delta means no manifest rewrite"
        );
        assert_eq!(index.graph.files(original.snapshot())?, before);
        assert_eq!(
            observed_text(&source_path, "a.lua", original.snapshot())
                .await?
                .as_deref(),
            Some("function alpha() return 1 end\n")
        );
        let corpus =
            graphit_ast::corpus::project_directory(fixture.owner.auth.installation(), project)?
                .join("corpus.lance");
        let search = BranchSnapshot::capture_observed(corpus.to_str().unwrap(), None, None).await?;
        let mut scan = search.scan();
        scan.filter_expr(col("path").eq(lit("a.lua")));
        let rows: Vec<_> = scan.try_into_stream().await?.try_collect().await?;
        assert!(
            rows.iter().any(|batch| batch.num_rows() != 0),
            "FILE search facts remain queryable"
        );
        drop((rows, search));
        let old_b = index.graph.file_observation("b.lua", original.snapshot())?;
        std::fs::write(
            fixture.project.root().join("a.lua"),
            "function changed_alpha() return 9 end\n",
        )?;
        fixture.index(&["a.lua"], false).await?;
        assert_eq!(
            index.graph.file_observation("b.lua", original.snapshot())?,
            old_b
        );
        assert_eq!(
            observed_text(&source_path, "a.lua", original.snapshot())
                .await?
                .as_deref(),
            Some("function changed_alpha() return 9 end\n")
        );
        std::fs::remove_file(fixture.project.root().join("b.lua"))?;
        fixture.index(&["b.lua"], false).await?;
        assert!(
            index
                .graph
                .file_observation("b.lua", original.snapshot())?
                .is_none()
        );
        assert!(
            observed_text(&source_path, "b.lua", original.snapshot())
                .await?
                .is_none()
        );
        let remaining = index.graph.files(original.snapshot())?;
        drop((publication, index, directory));
        let (reopened, _) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                false,
            )
            .await?
            .ok_or("cold graph missing")?;
        assert_eq!(reopened.graph.files(original.snapshot())?, remaining);
        let queried = reopened.graph.query(
            "MATCH (n:AstFileObservation) RETURN n.path ORDER BY n.path",
            &BTreeMap::new(),
            graphit_ast::icebug_query::QueryOptions {
                buffer_pool_bytes: 64 * 1024 * 1024,
                query_threads: 1,
                max_rows: 8,
                max_bytes: 4096,
                max_depth: 16,
            },
            original.snapshot(),
        )?;
        assert_eq!(queried.rows.len(), 1);
        assert_eq!(queried.rows[0][0]["value"], "a.lua");
        Ok(())
    })
    .catch_unwind()
    .await;
    fixture.shutdown(result).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maintenance_interleaves_foreground_and_resumes_after_preemption() -> TestResult {
    let fixture = Fixture::new()?;
    let result = std::panic::AssertUnwindSafe(async {
        let cancel = Cancellation::default();
        let original = fixture.policy.request_context(Instant::now(), &cancel)?;
        let capture = CapturedPublicationScope::project(
            &fixture.project,
            fixture.selected.id,
            fixture.owner.scope,
            original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &capture,
                record: &record,
            },
            version: &fixture.selected.version,
        };
        let registry = fixture.registry();
        let mut background = ProgressivePublication::open(
            fixture.owner.auth.installation(),
            project,
            fixture.owner.native,
            Arc::clone(original.snapshot()),
            Arc::clone(&registry),
            &|| original.check(),
        )
        .await?;
        let mut foreground = ProgressivePublication::open(
            fixture.owner.auth.installation(),
            project,
            fixture.owner.native,
            Arc::clone(original.snapshot()),
            Arc::clone(&registry),
            &|| original.check(),
        )
        .await?;
        let (index, directory) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                true,
            )
            .await?
            .ok_or("graph missing")?;
        let writer = Arc::clone(&index);
        let context = Arc::clone(original.snapshot());
        tokio::task::spawn_blocking(move || seed(&writer.graph, &context)).await??;
        let observations = index.graph.files(original.snapshot())?;
        let revision = index.graph.revision()?;
        let preemption = Cancellation::default();
        let check = || original.check();
        let held = Arc::clone(&index.fence).lock_owned().await;
        let mut maintenance = Box::pin(async {
            let report = background
                .maintain_shards_batch(&check, Some(&preemption))
                .await?
                .ok_or("batch missing")?;
            assert!(report.consolidated_files > 0 && report.consolidated_files <= 128);
            assert!(
                report.more,
                "the bounded producer leaves real remaining work"
            );
            background
                .finish_maintenance(&check, Some(&preemption))
                .await?;
            Ok::<_, Box<dyn std::error::Error>>(report)
        });
        assert!(futures_util::poll!(maintenance.as_mut()).is_pending());
        let mut update = FilePublication::new("f000.lua".into(), [4; 32], 2);
        update.nodes.push(node("f000.lua", "foreground_changed"));
        let mut foreground_work = Box::pin(async {
            foreground.publish_file(update, &check).await?;
            preemption.cancel();
            Ok::<_, Box<dyn std::error::Error>>(())
        });
        assert!(futures_util::poll!(foreground_work.as_mut()).is_pending());
        drop(held);
        let (foreground_result, maintenance_result) =
            tokio::join!(biased; foreground_work, maintenance);
        foreground_result?;
        maintenance_result?;
        assert_eq!(
            index.graph.revision()?,
            revision + 2,
            "one atomic batch then the foreground FILE, with no preempted second batch"
        );
        let current = index.graph.files(original.snapshot())?;
        for observation in &observations {
            if observation.path != "f000.lua" {
                assert!(
                    current.contains(observation),
                    "maintenance preserves the exact FILE observation"
                );
            }
        }
        assert_eq!(
            index
                .graph
                .file_observation("f000.lua", original.snapshot())?
                .unwrap()
                .source_sha256,
            [4; 32]
        );
        let graph_root = crate::foreground_ast_sources::progressive::location(
            fixture.owner.auth.installation(),
            project,
        )?
        .0
        .join("graph");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(graph_root.join("icebug.json"))?)?;
        let bases = row_bases(&manifest);
        drop((background, foreground, index, directory));
        let mut resumed = ProgressivePublication::open(
            fixture.owner.auth.installation(),
            project,
            fixture.owner.native,
            Arc::clone(original.snapshot()),
            Arc::clone(&registry),
            &check,
        )
        .await?;
        resumed.finish_maintenance(&check, None).await?;
        let final_report = resumed
            .maintain_shards_batch(&check, None)
            .await?
            .ok_or("resumed graph missing")?;
        assert_eq!(final_report.consolidated_files, 0);
        assert!(!final_report.more);
        let (reopened, _) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                false,
            )
            .await?
            .ok_or("reopened graph missing")?;
        assert_eq!(reopened.graph.files(original.snapshot())?, current);
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(graph_root.join("icebug.json"))?)?;
        assert_eq!(
            row_bases(&manifest),
            bases,
            "cold resume changes physical slices, never logical row addresses"
        );
        Ok(())
    })
    .catch_unwind()
    .await;
    fixture.shutdown(result).await
}

// Windows directory bindings intentionally deny renaming an open corpus.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn graph_maintenance_precedes_unavailable_lance_indices() -> TestResult {
    let fixture = Fixture::new()?;
    let result = std::panic::AssertUnwindSafe(async {
        let sources = [
            ("a.lua", "function alpha() return 1 end\n"),
            ("b.lua", "function beta() return 2 end\n"),
        ];
        for (path, source) in sources {
            std::fs::write(fixture.project.root().join(path), source)?;
        }
        let cancel = Cancellation::default();
        let raw_version = fixture
            .ast
            .index(
                &fixture.project,
                &fixture.selected,
                graphit_daemon::watcher::ChangeBatch {
                    rescan: true,
                    ..Default::default()
                },
                &cancel,
            )
            .await?
            .ok_or("Raw publication missing")?;
        let original = fixture.policy.request_context(Instant::now(), &cancel)?;
        let capture = CapturedPublicationScope::project(
            &fixture.project,
            fixture.selected.id,
            fixture.owner.scope,
            original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &capture,
                record: &record,
            },
            version: &fixture.selected.version,
        };
        let registry = fixture.registry();
        let check = || original.check();
        let mut publication = ProgressivePublication::open(
            fixture.owner.auth.installation(),
            project,
            fixture.owner.native,
            Arc::clone(original.snapshot()),
            Arc::clone(&registry),
            &check,
        )
        .await?;
        for (path, source) in sources {
            let mut file = FilePublication::new(
                path.into(),
                Sha256::digest(source.as_bytes()).into(),
                raw_version,
            );
            file.nodes.push(node(path, path));
            publication.publish_file(file, &check).await?;
        }
        let (index, _) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                false,
            )
            .await?
            .ok_or("graph missing")?;
        let before = index.graph.files(original.snapshot())?;
        let revision = index.graph.revision()?;
        let corpus =
            graphit_ast::corpus::project_directory(fixture.owner.auth.installation(), project)?
                .join("corpus.lance");
        let unavailable = corpus.with_file_name("unavailable-corpus.lance");
        std::fs::rename(&corpus, &unavailable)?;
        let lance_failure = publication.maintain_indices(&check).await;
        let result = publication.finish_maintenance(&check, None).await;
        let queried = index.graph.query(
            "MATCH (n:Function) RETURN n.path ORDER BY n.path",
            &BTreeMap::new(),
            graphit_ast::icebug_query::QueryOptions {
                buffer_pool_bytes: 64 * 1024 * 1024,
                query_threads: 1,
                max_rows: 8,
                max_bytes: 4096,
                max_depth: 16,
            },
            original.snapshot(),
        );
        let preemption = Cancellation::default();
        let held = Arc::clone(&index.fence).lock_owned().await;
        let mut waiting = Box::pin(publication.finish_maintenance(&check, Some(&preemption)));
        assert!(futures_util::poll!(waiting.as_mut()).is_pending());
        preemption.cancel();
        let preempted = tokio::time::timeout(Duration::from_secs(5), waiting).await;
        drop(held);
        // Restore the real corpus before assertions or fixture shutdown.
        std::fs::rename(&unavailable, &corpus)?;
        preempted??;
        let queried = queried?;
        assert_eq!(queried.rows.len(), 2);
        assert_eq!(queried.rows[0][0]["value"], "a.lua");
        assert_eq!(queried.rows[1][0]["value"], "b.lua");
        let lance_failure = lance_failure.expect_err("the real Lance corpus is unavailable");
        let failure = result.expect_err("the independent Lance error must remain visible");
        assert_eq!(failure.kind(), lance_failure.kind());
        assert_eq!(
            index.graph.revision()?,
            revision + 1,
            "Icebug consolidates before the independent Lance capture fails"
        );
        assert_eq!(index.graph.files(original.snapshot())?, before);
        let report = publication
            .maintain_shards_batch(&check, None)
            .await?
            .ok_or("graph maintenance missing")?;
        assert_eq!(report.consolidated_files, 0);
        assert!(!report.more);
        let source_path = fixture
            .project
            .project_runtime(fixture.selected.id, &["ast", "source.lance"])?;
        for (path, source) in sources {
            assert_eq!(
                observed_text(&source_path, path, original.snapshot())
                    .await?
                    .as_deref(),
                Some(source)
            );
        }
        Ok(())
    })
    .catch_unwind()
    .await;
    fixture.shutdown(result).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_maintenance_wait_preserves_manifest_and_empty_cohort() -> TestResult {
    let fixture = Fixture::new()?;
    let result = std::panic::AssertUnwindSafe(async {
        let cancel = Cancellation::default();
        let original = fixture.policy.request_context(Instant::now(), &cancel)?;
        let capture = CapturedPublicationScope::project(
            &fixture.project,
            fixture.selected.id,
            fixture.owner.scope,
            original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &capture,
                record: &record,
            },
            version: &fixture.selected.version,
        };
        let registry = fixture.registry();
        let publication = ProgressivePublication::open(
            fixture.owner.auth.installation(),
            project,
            fixture.owner.native,
            Arc::clone(original.snapshot()),
            Arc::clone(&registry),
            &|| original.check(),
        )
        .await?;
        let graph_root = crate::foreground_ast_sources::progressive::location(
            fixture.owner.auth.installation(),
            project,
        )?
        .0
        .join("graph");
        assert!(
            publication
                .maintain_shards_batch(&|| original.check(), None)
                .await?
                .is_none()
        );
        assert!(
            !graph_root.exists(),
            "maintenance never initializes an empty native graph"
        );
        let (index, _) = registry
            .capture(
                fixture.owner.auth.installation(),
                project,
                fixture.owner.native,
                original.snapshot(),
                true,
            )
            .await?
            .ok_or("graph missing")?;
        let manifest = std::fs::read(graph_root.join("icebug.json"))?;
        let revision = index.graph.revision()?;
        let held = Arc::clone(&index.fence).lock_owned().await;
        let check = || original.check();
        let mut waiting = Box::pin(publication.maintain_shards_batch(&check, None));
        assert!(futures_util::poll!(waiting.as_mut()).is_pending());
        cancel.cancel();
        let error = tokio::time::timeout(Duration::from_secs(5), waiting)
            .await?
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(std::fs::read(graph_root.join("icebug.json"))?, manifest);
        assert_eq!(index.graph.revision()?, revision);
        drop(held);
        Ok(())
    })
    .catch_unwind()
    .await;
    fixture.shutdown(result).await
}
