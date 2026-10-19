//! Durable FILE progress and retry through the actual small-model App producer.
use super::*;
use crate::foreground_ast_sources::semantic::checkpoint::{FileInput, publish_file};
use crate::foreground_ast_sources::{ForegroundAstSourceConfig, ForegroundAstSources};
use crate::foreground_memory_jobs::{ForegroundMemoryJobs, MemoryJobResources};
use arrow_array::{
    Array, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, RecordBatch, StringArray,
    UInt64Array,
};
use futures_util::TryStreamExt;
use graphit_ast::native_nn::validate_native_nn;
use graphit_daemon::supervisor::ProjectContext;
use graphit_data::lance_branch_store::BranchSnapshot;
use graphit_data::lance_module_store::{DataFusionError, RecordBatchStreamAdapter};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use tokio::sync::{mpsc, oneshot};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
type Selected = (ProjectPaths, ProjectId, VersionContext);
type RowKey = (String, String, String);

#[derive(Clone, Debug, PartialEq)]
struct Row {
    language: String,
    raw_version: u64,
    projection_sha256: Vec<u8>,
    vector: Vec<f32>,
    generation: u64,
}

fn column<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> &'a T {
    batch
        .column_by_name(name)
        .unwrap()
        .as_any()
        .downcast_ref::<T>()
        .unwrap()
}

fn rows(batches: &[RecordBatch]) -> BTreeMap<RowKey, Row> {
    let mut result = BTreeMap::new();
    for batch in batches {
        let paths = column::<StringArray>(batch, "path");
        let uids = column::<StringArray>(batch, "uid");
        let keys = column::<StringArray>(batch, "data_key");
        let languages = column::<StringArray>(batch, "language");
        let versions = column::<UInt64Array>(batch, "raw_version");
        let hashes = column::<FixedSizeBinaryArray>(batch, "projection_sha256");
        let generations = column::<UInt64Array>(batch, "model_generation");
        let vectors = column::<FixedSizeListArray>(batch, "vector");
        for i in 0..batch.num_rows() {
            assert!(!generations.is_null(i));
            let vector = vectors.value(i);
            let vector = vector.as_any().downcast_ref::<Float32Array>().unwrap();
            assert!(
                result
                    .insert(
                        (
                            paths.value(i).into(),
                            uids.value(i).into(),
                            keys.value(i).into()
                        ),
                        Row {
                            language: languages.value(i).into(),
                            raw_version: versions.value(i),
                            projection_sha256: hashes.value(i).to_vec(),
                            vector: vector.values().to_vec(),
                            generation: generations.value(i),
                        },
                    )
                    .is_none()
            );
        }
    }
    result
}

fn context(selected: &Selected) -> ProjectContext {
    ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    }
}

async fn refresh(
    ast: &Arc<ForegroundAstSources>,
    selected: &Selected,
    cancel: &Cancellation,
) -> TestResult {
    ast.index(
        &selected.0,
        &context(selected),
        graphit_daemon::watcher::ChangeBatch {
            rescan: true,
            ..Default::default()
        },
        cancel,
    )
    .await?;
    Ok(())
}

fn nn_uri(source: &crate::source_server::SourceServer, selected: &Selected) -> std::path::PathBuf {
    source
        .policy()
        .installation
        .root()
        .join("projects")
        .join(selected.1.to_string())
        .join("ast")
        .join("corpus.lance")
}

async fn live(
    source: &crate::source_server::SourceServer,
    selected: &Selected,
    resources: &MemoryJobResources,
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> TestResult<(BranchSnapshot, Vec<RecordBatch>)> {
    let model =
        crate::ast_semantic::expected_native_nn_model(&resources.local.as_ref().unwrap().expected)?;
    let branch = selected.2.lance_branch();
    let snapshot = BranchSnapshot::capture_observed(
        nn_uri(source, selected).to_str().unwrap(),
        (!branch.is_empty()).then_some(branch.as_str()),
        None,
    )
    .await?;
    graphit_ast::corpus::validate_schema(snapshot.schema())?;
    let mut stream = snapshot.scan().try_into_stream().await?;
    let mut batches = Vec::new();
    while let Some(batch) = stream.try_next().await? {
        let batch = graphit_ast::corpus::vectors(&batch, &model, original)?;
        validate_native_nn(&batch, &model, original)?;
        batches.push(batch);
    }
    Ok((snapshot, batches))
}

/// Make selected existing entities pending without deleting their lexical rows.
async fn withdraw_vectors(
    pin: &BranchSnapshot,
    path: &str,
    selected: &[RecordBatch],
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> TestResult<BranchSnapshot> {
    let selected = selected
        .iter()
        .flat_map(|batch| {
            let ids = column::<StringArray>(batch, "uid");
            let keys = column::<StringArray>(batch, "data_key");
            (0..batch.num_rows())
                .map(|row| (ids.value(row).to_owned(), keys.value(row).to_owned()))
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>();
    let mut scan = pin.scan();
    scan.filter_expr(
        graphit_data::lance_branch_store::col("path")
            .eq(graphit_data::lance_branch_store::lit(path)),
    );
    let batches = scan
        .try_into_stream()
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    let schema = Arc::new(pin.schema().clone());
    let mut patches = Vec::new();
    for batch in batches {
        let ids = column::<StringArray>(&batch, "uid");
        let keys = column::<StringArray>(&batch, "data_key");
        let indices = arrow_array::UInt32Array::from(
            (0..batch.num_rows())
                .map(|row| {
                    (!selected.contains(&(ids.value(row).to_owned(), keys.value(row).to_owned())))
                        .then_some(row as u32)
                })
                .collect::<Vec<_>>(),
        );
        let mut arrays = batch.columns().to_vec();
        for name in ["vector", "vector_id", "model_generation"] {
            let index = schema.index_of(name)?;
            arrays[index] = arrow_select::take::take(arrays[index].as_ref(), &indices, None)?;
        }
        patches.push(RecordBatch::try_new(schema.clone(), arrays)?);
    }
    let stream = Box::pin(RecordBatchStreamAdapter::new(
        schema,
        tokio_stream::iter(patches.into_iter().map(Ok)),
    ));
    let prepared = pin
        .prepare_replace_paths_stream(
            &[path.to_owned()],
            stream,
            vec!["uid".into(), "data_key".into()],
        )
        .await?;
    original.check()?;
    Ok(prepared.commit().await?)
}

async fn raw_version(
    source: &crate::source_server::SourceServer,
    selected: &Selected,
) -> TestResult<u64> {
    let uri = source
        .policy()
        .installation
        .root()
        .join("projects")
        .join(selected.1.to_string())
        .join("ast")
        .join("source.lance");
    let branch = selected.2.lance_branch();
    Ok(
        LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
            .await?
            .capture_target((!branch.is_empty()).then_some(branch.as_str()))
            .await?
            .version(),
    )
}

fn for_path(batches: &[RecordBatch], path: &str) -> Vec<RecordBatch> {
    batches
        .iter()
        .flat_map(|batch| {
            let paths = column::<StringArray>(batch, "path");
            (0..batch.num_rows())
                .filter(|&i| paths.value(i) == path)
                .map(|i| batch.slice(i, 1))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn assert_reused(before: &BTreeMap<RowKey, Row>, after: &BTreeMap<RowKey, Row>, path: &str) {
    let old: BTreeMap<_, _> = before.iter().filter(|(k, _)| k.0 == path).collect();
    let new: BTreeMap<_, _> = after.iter().filter(|(k, _)| k.0 == path).collect();
    assert!(!old.is_empty());
    assert_eq!(
        old.keys().collect::<Vec<_>>(),
        new.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        new.values()
            .map(|r| r.language.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["db2", "sql"])
    );
    for (key, old) in old {
        let new = new[&key];
        assert_eq!(
            new.raw_version, old.raw_version,
            "intact FILE retains its parser-associated historical Raw pin"
        );
        assert_eq!(new.projection_sha256, old.projection_sha256);
        assert_eq!(new.vector, old.vector);
        assert_eq!(new.generation, old.generation);
    }
}

fn shared_selector(stock: &str) -> String {
    // Keep the shipped grammar/query contracts; only the fixture route is shared.
    assert_eq!(
        stock
            .lines()
            .filter(|line| line.starts_with("extensions:"))
            .count(),
        1
    );
    assert!(
        stock
            .lines()
            .filter(|line| *line == "exclusive: true")
            .count()
            <= 1
    );
    stock
        .lines()
        .map(|line| {
            if line.starts_with("extensions:") {
                "extensions: [\".checkpoint\"]"
            } else if line == "exclusive: true" {
                "exclusive: false"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

type Fixture = (
    super::super::foreground_memory_jobs_tests::Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    String,
    Selected,
    Arc<ForegroundAstSources>,
    Arc<MemoryJobResources>,
);

async fn fixture(tables: usize) -> TestResult<Fixture> {
    let (storage, mut owner, tasks, credential, _) =
        super::super::foreground_memory_jobs_tests::browser_search_fixture().await;
    let resources = owner.memory_job_resources.upgrade().unwrap();
    owner
        .server
        .as_mut()
        .unwrap()
        .attach_semantic_model(Some(Arc::clone(&resources)))?;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let source = owner.server.as_ref().unwrap();
    let cancel = Cancellation::default();
    ConfigStore::project(&selected.0)?.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancel,
    )?;
    let queries = selected.0.brand_directory().join("ast/queries");
    std::fs::create_dir_all(&queries)?;
    for (name, stock) in [
        (
            "db2.yaml",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../graphit-ast/profiles/queries/db2.yaml"
            )),
        ),
        (
            "sql.yaml",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../graphit-ast/profiles/queries/sql.yaml"
            )),
        ),
    ] {
        std::fs::write(queries.join(name), shared_selector(stock))?;
    }
    let body = (0..tables)
        .map(|i| format!("CREATE TABLE CHECKPOINT_A_{i}(C_{i} INTEGER);\n"))
        .collect::<String>();
    std::fs::write(selected.0.root().join("a.checkpoint"), body)?;
    std::fs::write(
        selected.0.root().join("removed.checkpoint"),
        "CREATE TABLE CHECKPOINT_REMOVED(C INTEGER);",
    )?;
    let request = owner.policy.request_context(Instant::now(), &cancel)?;
    let base = ConfigSnapshot::new(source.policy().installation.brand(), [])?;
    let config = crate::ast_profile_loader::project_config(
        &source.policy().installation,
        &selected.0,
        &base,
        request.snapshot(),
    )?;
    let loader =
        crate::ast_profile_loader::AstProfileLoader::new(owner.policy.clone(), request.snapshot())?;
    let profiles = loader.load(
        &source.policy().installation,
        &selected.0,
        &config,
        request.snapshot(),
    )?;
    let graphit_ast::catalog::Selection::Candidates(candidates) =
        profiles.resolve(std::path::Path::new("a.checkpoint"), request.snapshot())?
    else {
        panic!("fixture must select both mature parser profiles");
    };
    assert_eq!(
        candidates
            .iter()
            .map(|profile| profile.language().value().as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["db2", "sql"])
    );
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
            environment: Vec::new(),
        },
        request.snapshot(),
    )?);
    refresh(&ast, &selected, &cancel).await?;
    ast.index_semantic(
        selected.0.clone(),
        context(&selected),
        Some(Arc::clone(&resources)),
        None,
        std::collections::BTreeSet::new(),
        false,
        cancel.clone(),
        cancel.clone(),
    )
    .await?;
    Ok((storage, owner, tasks, credential, selected, ast, resources))
}

async fn final_mcp(
    owner: &mut ForegroundSource,
    selected: &Selected,
    credential: &str,
    _version: u64,
) -> TestResult {
    use rmcp::{
        RoleClient,
        service::{ClientLifecycleMode, ClientServiceExt},
        transport::{
            StreamableHttpClientTransport,
            streamable_http_client::StreamableHttpClientTransportConfig,
        },
    };
    let cancel = Cancellation::default();
    let installation = owner.server.as_ref().unwrap().policy().installation.clone();
    let config = ConfigSnapshot::new(
        installation.brand(),
        [ConfigStore::global(&installation)?.load()?],
    )?;
    let request = owner.policy.request_context(Instant::now(), &cancel)?;
    let provider = crate::foreground_source::active_provider(&installation, &config, &request)?
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let expected = crate::foreground_memory_jobs::expected_model(
        &installation,
        Some(&provider),
        request.snapshot(),
        &owner.server.as_ref().unwrap().project_modules(&request)?,
    )?
    .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let mut jobs = ForegroundMemoryJobs::new(
        Arc::clone(&owner.auth),
        (owner.policy.clone()).clone(),
        Some(expected),
        true,
        None,
        crate::foreground_memory_jobs::test_broker_cache(&(owner.policy.clone())),
    )?;
    let executable =
        std::path::PathBuf::from(std::env::var_os("GRAPHIT_TEST_MODEL_SERVER").unwrap());
    let resources = jobs
        .start(
            &installation,
            vec![(
                "PATH".into(),
                executable.parent().unwrap().as_os_str().to_owned(),
            )],
            owner.scope,
            owner.native,
            NativeBinding::KiroV3,
            &request,
        )
        .await?
        .expect("configured embedding resources");
    owner.memory_jobs = Some(jobs);
    owner.memory_job_resources = Arc::downgrade(&resources);
    owner
        .server
        .as_mut()
        .unwrap()
        .attach_semantic_model(Some(resources))?;
    let source = owner.server.as_ref().unwrap().clone();
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &cancel,
    )?;
    let uri = format!("http://{}/mcp", listener.address());
    let serving_source = source.clone();
    let serving = tokio::spawn(async move { serving_source.serve(listener).await });
    let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {credential}"))?;
    bearer.set_sensitive(true);
    let headers = HashMap::from([
        (axum::http::header::AUTHORIZATION, bearer),
        (
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static("runtime"),
        ),
    ]);
    let client: rmcp::service::RunningService<RoleClient, ()> = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::new(),
                StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
            },
        )
        .await?;
    let args = json!({"project_dir":graphit_core::native_path::NativePath::new(selected.0.root().to_owned())?,"project_version":selected.2.version(),"query":"checkpoint table","mode":"semantic","k":3,"ai_optimized":false});
    let response = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("graphit_ast_search")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await?;
    assert_eq!(response.is_error, Some(false));
    let payload = response.structured_content.unwrap();
    assert_eq!(payload["status"], "available");
    assert!(payload.get("raw_generation").is_none());
    let hits = payload["results"].as_array().unwrap();
    assert!(!hits.is_empty());
    for hit in hits {
        assert_eq!(hit["path"], "a.checkpoint");
        assert!(hit["distance"].as_f64().unwrap().is_finite());
    }
    client.cancel().await?;
    source.shutdown().await?;
    serving.await??;
    Ok(())
}

async fn live_mcp(
    source: &crate::source_server::SourceServer,
    selected: &Selected,
    credential: &str,
    mode: &str,
    text: &str,
) -> TestResult<Value> {
    let headers = axum::http::HeaderMap::from_iter([
        (
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_str(&format!("Bearer {credential}"))?,
        ),
        (
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static("runtime"),
        ),
    ]);
    let result = source.graph_server().call_search_mcp(source,
        rmcp::model::CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
            "project_dir":graphit_core::native_path::NativePath::new(selected.0.root().to_owned())?,
            "project_version":selected.2.version(),"query":text,"mode":mode,"rerank":false,"k":3,
        }).as_object().unwrap().clone()), headers, tokio_util::sync::CancellationToken::new(),
    ).await;
    assert_ne!(result.is_error, Some(true), "{result:?}");
    let payload = result.structured_content.unwrap();
    assert_eq!(payload["status"], "available");
    Ok(payload)
}

async fn refresh_paths(
    ast: &Arc<ForegroundAstSources>,
    selected: &Selected,
    paths: &[&str],
    cancel: &Cancellation,
) -> TestResult {
    ast.index(
        &selected.0,
        &context(selected),
        graphit_daemon::watcher::ChangeBatch {
            paths: paths
                .iter()
                .map(|path| selected.0.root().join(path))
                .collect(),
            ..Default::default()
        },
        cancel,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn partial_file_checkpoint_survives_stream_failure_without_pruning_and_reuses_on_retry()
-> TestResult {
    let (_storage, mut owner, tasks, credential, selected, ast, resources) = fixture(65).await?;
    let source = owner.server.as_ref().unwrap().clone();
    let cancel = Cancellation::default();
    let request = owner.policy.request_context(Instant::now(), &cancel)?;
    let original = request.snapshot();
    let (_, baseline_batches) = live(&source, &selected, &resources, original).await?;
    let baseline = rows(&baseline_batches);
    let a = for_path(&baseline_batches, "a.checkpoint");
    assert!(a.len() > 64);
    let model =
        crate::ast_semantic::expected_native_nn_model(&resources.local.as_ref().unwrap().expected)?;
    for fail_stream in [true, false] {
        let (initial, _) = live(&source, &selected, &resources, original).await?;
        let initial = withdraw_vectors(&initial, "a.checkpoint", &a[..64], original).await?;
        let (_, partial_batches) = live(&source, &selected, &resources, original).await?;
        let partial_rows = rows(&partial_batches);
        assert_eq!(partial_rows.len(), baseline.len() - 64);
        let partial = live_mcp(
            &source,
            &selected,
            &credential,
            "semantic",
            "checkpoint table",
        )
        .await?;
        let hits = partial["results"].as_array().unwrap();
        assert!(
            !hits.is_empty(),
            "completed embeddings are immediately queryable"
        );
        for hit in hits {
            let key = (
                hit["path"].as_str().unwrap().to_owned(),
                hit["uid"].as_str().unwrap().to_owned(),
                hit["data_key"].as_str().unwrap().to_owned(),
            );
            assert!(
                partial_rows.contains_key(&key),
                "MCP must not read embeddings absent from the current progressive snapshot"
            );
        }
        let (tx, rx) = mpsc::channel(4);
        let (complete_tx, complete_rx) = oneshot::channel();
        let (committed_tx, committed_rx) = oneshot::channel();
        let mut committed_tx = Some(committed_tx);
        let mut commits = Vec::new();
        let mut on_commit =
            |publication: &graphit_ast::native_nn_publication::NativeNnPublication| {
                assert!(publication.completion_error().is_none());
                commits.push(publication.version());
                if let Some(tx) = committed_tx.take() {
                    tx.send(()).unwrap();
                }
            };
        let sending = async {
            for batch in a.iter().take(64) {
                tx.send(Ok(batch.clone())).await.map_err(io::Error::other)?;
            }
            committed_rx.await?;
            if fail_stream {
                tx.send(Err(DataFusionError::IoError(io::Error::other(
                    "later FILE stream failed",
                ))))
                .await
                .map_err(io::Error::other)?;
            }
            drop(complete_tx);
            drop(tx);
            Ok::<(), Box<dyn std::error::Error>>(())
        };
        let recheck = || async { original.check() };
        let fence = Arc::new(tokio::sync::Mutex::new(()));
        let writing = publish_file(
            &initial,
            FileInput {
                path: "a.checkpoint".into(),
                batches: rx,
                completed: complete_rx,
            },
            &model,
            original,
            &recheck,
            &fence,
            &mut on_commit,
        );
        let (sent, result) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(sending, writing)
        })
        .await?;
        sent?;
        assert!(result.is_err());
        assert!(!commits.is_empty());
        assert!(commits[0] > initial.version());
        let (_, observed) = live(&source, &selected, &resources, original).await?;
        assert_eq!(
            rows(&observed),
            baseline,
            "partial upsert preserves the other identities/profile and other FILE"
        );
    }
    owner.memory_jobs.as_mut().unwrap().shutdown().await?;
    assert!(
        tokio::net::TcpStream::connect(resources.local.as_ref().unwrap().address)
            .await
            .is_err()
    );
    std::fs::remove_file(selected.0.root().join("removed.checkpoint"))?;
    refresh_paths(&ast, &selected, &["removed.checkpoint"], &cancel).await?;
    let version = raw_version(&source, &selected).await?;
    assert!(baseline.values().all(|row| row.raw_version < version));
    ast.index_semantic(
        selected.0.clone(),
        context(&selected),
        Some(Arc::clone(&resources)),
        Some(BTreeSet::from(["removed.checkpoint".into()])),
        std::collections::BTreeSet::new(),
        false,
        cancel.clone(),
        cancel.clone(),
    )
    .await?;
    let (_, final_batches) = live(&source, &selected, &resources, original).await?;
    let final_rows = rows(&final_batches);
    assert!(final_rows.keys().all(|key| key.0 == "a.checkpoint"));
    assert_reused(&baseline, &final_rows, "a.checkpoint");
    final_mcp(&mut owner, &selected, &credential, version).await?;
    tasks.shutdown().await?;
    owner.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_failure_preserves_intact_file_pins_and_deleted_pending_does_not_load_model()
-> TestResult {
    let (_storage, mut owner, tasks, credential, selected, ast, resources) = fixture(1).await?;
    let source = owner.server.as_ref().unwrap().clone();
    let cancel = Cancellation::default();
    let request = owner.policy.request_context(Instant::now(), &cancel)?;
    let original = request.snapshot();
    let (_, batches) = live(&source, &selected, &resources, original).await?;
    let before = rows(&batches);
    owner.memory_jobs.as_mut().unwrap().shutdown().await?;
    assert!(
        tokio::net::TcpStream::connect(resources.local.as_ref().unwrap().address)
            .await
            .is_err()
    );
    std::fs::write(
        selected.0.root().join("b.checkpoint"),
        "CREATE TABLE CHECKPOINT_B(C INTEGER);",
    )?;
    refresh_paths(&ast, &selected, &["b.checkpoint"], &cancel).await?;
    let failed_version = raw_version(&source, &selected).await?;
    assert!(
        ast.index_semantic(
            selected.0.clone(),
            context(&selected),
            Some(Arc::clone(&resources)),
            Some(BTreeSet::from(["b.checkpoint".into()])),
            std::collections::BTreeSet::new(),
            false,
            cancel.clone(),
            cancel.clone()
        )
        .await
        .is_err()
    );
    let (_, after_failure) = live(&source, &selected, &resources, original).await?;
    let after_failure = rows(&after_failure);
    assert_reused(&before, &after_failure, "a.checkpoint");
    assert!(!after_failure.keys().any(|key| key.0 == "b.checkpoint"));
    assert!(
        !live_mcp(&source, &selected, &credential, "fts", "CHECKPOINT_B").await?["results"]
            .as_array()
            .unwrap()
            .is_empty(),
        "model failure must not hide the newly parsed FILE from lexical search"
    );
    for file in ["b.checkpoint", "removed.checkpoint"] {
        std::fs::remove_file(selected.0.root().join(file))?;
    }
    refresh_paths(
        &ast,
        &selected,
        &["b.checkpoint", "removed.checkpoint"],
        &cancel,
    )
    .await?;
    let version = raw_version(&source, &selected).await?;
    assert!(version > failed_version);
    ast.index_semantic(
        selected.0.clone(),
        context(&selected),
        Some(Arc::clone(&resources)),
        Some(BTreeSet::from([
            "b.checkpoint".into(),
            "removed.checkpoint".into(),
        ])),
        std::collections::BTreeSet::new(),
        false,
        cancel.clone(),
        cancel.clone(),
    )
    .await?;
    let (_, final_batches) = live(&source, &selected, &resources, original).await?;
    let final_rows = rows(&final_batches);
    assert!(final_rows.keys().all(|key| key.0 == "a.checkpoint"));
    assert_reused(&before, &final_rows, "a.checkpoint");
    final_mcp(&mut owner, &selected, &credential, version).await?;
    tasks.shutdown().await?;
    owner.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_rows_flush_at_interval_and_idle_never_commits() -> TestResult {
    let (_storage, mut owner, tasks, _credential, selected, _ast, resources) = fixture(1).await?;
    let source = owner.server.as_ref().unwrap().clone();
    let cancel = Cancellation::default();
    let request = owner.policy.request_context(Instant::now(), &cancel)?;
    let (initial, batches) = live(&source, &selected, &resources, request.snapshot()).await?;
    let before = rows(&batches);
    let batch = for_path(&batches, "a.checkpoint")
        .into_iter()
        .next()
        .unwrap();
    owner.memory_jobs.as_mut().unwrap().shutdown().await?;
    assert!(
        tokio::net::TcpStream::connect(resources.local.as_ref().unwrap().address)
            .await
            .is_err()
    );
    let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(3600),
        Cancellation::default(),
    ));
    let initial = withdraw_vectors(
        &initial,
        "a.checkpoint",
        std::slice::from_ref(&batch),
        &original,
    )
    .await?;
    let version = initial.version();
    let (tx, rx) = mpsc::channel(1);
    let (complete_tx, complete_rx) = oneshot::channel();
    let (committed_tx, mut committed_rx) = mpsc::unbounded_channel();
    let writer_resources = Arc::clone(&resources);
    let writer = tokio::spawn(async move {
        let model = crate::ast_semantic::expected_native_nn_model(
            &writer_resources.local.as_ref().unwrap().expected,
        )
        .map_err(io::Error::other)?;
        let recheck = || async { original.check() };
        let mut on_commit =
            |publication: &graphit_ast::native_nn_publication::NativeNnPublication| {
                assert!(publication.completion_error().is_none());
                committed_tx.send(publication.version()).unwrap();
            };
        let fence = Arc::new(tokio::sync::Mutex::new(()));
        publish_file(
            &initial,
            FileInput {
                path: "a.checkpoint".into(),
                batches: rx,
                completed: complete_rx,
            },
            &model,
            &original,
            &recheck,
            &fence,
            &mut on_commit,
        )
        .await
    });
    let sent_at = tokio::time::Instant::now();
    tx.send(Ok(batch)).await?;
    // Reserving the single slot proves the writer has consumed the pending row.
    drop(tx.reserve().await?);
    tokio::select! {
        commit = committed_rx.recv() => panic!("checkpoint arrived before its interval: {commit:?}"),
        () = tokio::time::sleep_until(sent_at + Duration::from_secs(59)) => {}
    }
    // This timeout bounds this fixture's wait, not the production commit's latency.
    let first = tokio::time::timeout(Duration::from_secs(60), committed_rx.recv())
        .await?
        .ok_or_else(|| io::Error::from(io::ErrorKind::BrokenPipe))?;
    assert!(sent_at.elapsed() >= Duration::from_secs(60));
    assert!(first > version);
    tokio::select! {
        commit = committed_rx.recv() => panic!("flushed FILE without new rows committed again: {commit:?}"),
        () = tokio::time::sleep(Duration::from_secs(61)) => {}
    }
    complete_tx.send(()).unwrap();
    drop(tx);
    let final_publication = writer
        .await??
        .ok_or_else(|| io::Error::other("checkpoint missing"))?;
    assert!(final_publication.completion_error().is_none());
    assert_eq!(
        final_publication.version(),
        first,
        "completion has no redundant FILE replay commit"
    );
    assert!(matches!(
        committed_rx.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    // The timed observation outlives the initial read request; capture a fresh read.
    let request = owner.policy.request_context(Instant::now(), &cancel)?;
    let (_, final_batches) = live(&source, &selected, &resources, request.snapshot()).await?;
    let final_rows = rows(&final_batches);
    assert_reused(&before, &final_rows, "a.checkpoint");
    assert_reused(&before, &final_rows, "removed.checkpoint");
    source.shutdown().await?;
    tasks.shutdown().await?;
    owner.shutdown().await?;
    Ok(())
}
