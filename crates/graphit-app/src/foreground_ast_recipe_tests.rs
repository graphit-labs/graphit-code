//! Normal recipe invalidation rebuilds FTS without repeating intact embeddings.
use super::super::foreground_memory_jobs_tests as support;
use super::*;
use crate::foreground_memory_jobs::{MemoryJobResources, OwnedLocalEmbedding};
use arrow_array::{Array, FixedSizeListArray, Float32Array, RecordBatch, StringArray};
use futures_util::{FutureExt, TryStreamExt};
use graphit_ast::native_nn_index_locator::{NativeNnIndexPresence, open_local_native_nn_index};
use graphit_auth::{ProjectCapture, ScopeCapture};
use graphit_daemon::{supervisor::ProjectContext, watcher::ChangeBatch};
use graphit_data::lance_branch_store::BranchSnapshot;
use std::collections::{BTreeMap, BTreeSet};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

async fn vectors(snapshot: &BranchSnapshot) -> TestResult<BTreeMap<String, Vec<f32>>> {
    let mut scan = snapshot.scan();
    scan.project(&["uid", "vector"])?;
    let mut stream = scan.try_into_stream().await?;
    let mut result = BTreeMap::new();
    while let Some(batch) = stream.try_next().await? {
        let uids = batch
            .column_by_name("uid")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let values = batch
            .column_by_name("vector")
            .unwrap()
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .unwrap();
        for row in 0..batch.num_rows() {
            if values.is_null(row) {
                continue;
            }
            result.insert(
                uids.value(row).to_owned(),
                values
                    .value(row)
                    .as_any()
                    .downcast_ref::<Float32Array>()
                    .unwrap()
                    .values()
                    .to_vec(),
            );
        }
    }
    Ok(result)
}

// Only this fixture's FTS metadata changes. The observed parser rows, Raw and
// genuine model vectors remain untouched; this is not a production converter.
async fn replace_fts_recipe(path: &std::path::Path, format: &str) -> TestResult {
    let snapshot = BranchSnapshot::capture_observed(path.to_str().unwrap(), None, None).await?;
    let mut metadata = snapshot.schema().metadata().clone();
    metadata.insert("graphit.ast.search.format".into(), format.into());
    let schema = Arc::new(arrow_schema::Schema::new_with_metadata(
        snapshot.schema().fields().clone(),
        metadata,
    ));
    let mut stream = snapshot.scan().try_into_stream().await?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.try_next().await? {
        rows.push(RecordBatch::try_new(
            Arc::clone(&schema),
            batch.columns().to_vec(),
        )?);
    }
    drop(stream);
    snapshot.prepare_overwrite(rows).await?.commit().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unchanged_file_keeps_source_and_vectors_and_foreign_recipe_is_rejected() -> TestResult {
    let (_fixture, owner, tasks, credential, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let resources = owner.memory_job_resources.upgrade().unwrap();
    let source = owner.server.as_ref().unwrap().clone();
    let cancel = Cancellation::default();
    ConfigStore::project(&selected.0)?.set(
        ConfigKey::new("modules.ast")?,
        json!(true),
        Duration::from_secs(5),
        &cancel,
    )?;
    std::fs::write(
        selected.0.root().join("a.rs"),
        "pub fn alpha() -> i32 { 7 }\n",
    )?;
    let no_docker = tempfile::tempdir()?;
    let original = owner.policy.request_context(Instant::now(), &cancel)?;
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
    let result = std::panic::AssertUnwindSafe(async {
        ast.index(
            &selected.0,
            &context,
            ChangeBatch {
                rescan: true,
                ..Default::default()
            },
            &cancel,
        )
        .await?;
        ast.index_semantic(
            selected.0.clone(),
            context.clone(),
            Some(Arc::clone(&resources)),
            None, std::collections::BTreeSet::new(),
            false,
            cancel.clone(),
            cancel.clone(),
        )
        .await?;
        let capture = CapturedPublicationScope::project(
            &selected.0,
            selected.1,
            owner.scope,
            original.snapshot(),
        )?;
        let record = capture.binding()?;
        let project = ProjectCapture {
            scope: ScopeCapture {
                live: &capture,
                record: &record,
            },
            version: &selected.2,
        };
        let NativeNnIndexPresence::Available(index) = open_local_native_nn_index(
            owner.auth.installation(),
            project,
            original.snapshot(),
            owner.native,
        )
        .await?
        else {
            return Err("initial NN missing".into());
        };
        let before = vectors(index.snapshot(original.snapshot())?).await?;
        assert_eq!(
            before.len(),
            1,
            "the CPU model must have inferred the actual parser entity"
        );
        let fts=graphit_ast::corpus::project_directory(owner.auth.installation(),project)?.join("corpus.lance");
        let (live, _) = source.graph_server().live_ast().capture(
            owner.auth.installation(), project,
            owner.native, original.snapshot(), false,
        ).await?.expect("the actual structural publisher exists");
        let previous_file = live.graph.file_observation("a.rs", original.snapshot())?.unwrap();

        let raw_path = owner
            .auth
            .installation()
            .live
            .owner_root()
            .join("projects")
            .join(selected.1.to_string())
            .join("ast")
            .join("source.lance");
        let raw_store =
            LanceBranchStore::open(raw_path.to_str().unwrap(), source_schema(), None).await?;
        let branch = selected.2.lance_branch();
        let raw_before = raw_store.capture_target((!branch.is_empty()).then_some(branch.as_str())).await?;
        let raw_rows = raw_before.scan().try_into_stream().await?.try_collect::<Vec<_>>().await?;
        let raw_current = raw_before.prepare_overwrite(raw_rows).await?.commit().await?;
        let raw_version = raw_current.version();
        assert!(raw_version > previous_file.raw_version, "an unrelated global Raw commit must not replace this FILE's historical association");
        // Local metadata comes from the actual ExpectedModel. The unreachable
        // endpoint makes any attempt to repeat inference an observable failure.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        drop(listener);
        let mut reuse_only = MemoryJobResources::remote(
            Arc::clone(&resources.auth),
            resources.http.clone(),
            Arc::clone(&resources.brokers),
            resources.policy.clone(),
            resources.scope,
            resources.native,
            resources.binding,
        )?;
        let local = resources.local.as_ref().unwrap();
        reuse_only.local = Some(OwnedLocalEmbedding {
            client: Arc::clone(&local.client),
            expected: Arc::clone(&local.expected),
            address,
        });
        ast.index_semantic(
            selected.0.clone(),
            context.clone(),
            Some(Arc::new(reuse_only)),
            Some(BTreeSet::new()), std::collections::BTreeSet::new(),
            false,
            cancel.clone(),
            cancel.clone(),
        )
        .await?;
        let current = BranchSnapshot::capture_observed(fts.to_str().unwrap(), None, None).await?;
        let rebuilt_file = live.graph.file_observation("a.rs", original.snapshot())?.unwrap();
        assert_eq!(rebuilt_file.file_revision,previous_file.file_revision,"unrelated global Raw commits do not republish an intact FILE");
        assert_eq!(rebuilt_file.raw_version, previous_file.raw_version);
        assert_eq!(rebuilt_file.source_sha256, previous_file.source_sha256);

        graphit_ast::corpus::validate_schema(current.schema())?;
        assert_eq!(current.schema().metadata().get("graphit.ast.search.format").map(String::as_str), Some("all-entity-2"));
        let mut scan = current.scan();
        scan.project(&["name"])?;
        let mut rows = scan.try_into_stream().await?;
        let mut names = Vec::new();
        while let Some(batch) = rows.try_next().await? {
            let column = batch
                .column_by_name("name")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            names.extend(column.iter().flatten().map(str::to_owned));
        }
        assert!(
            names.iter().any(|name| name == "alpha"),
            "the previously intact Raw FILE is structurally republished"
        );
        assert_eq!(
            raw_store.capture_target((!branch.is_empty()).then_some(branch.as_str())).await?.version(),
            raw_version
        );
        let headers = axum::http::HeaderMap::from_iter([
            (axum::http::header::AUTHORIZATION, axum::http::HeaderValue::from_str(&format!("Bearer {credential}"))?),
            (axum::http::HeaderName::from_static("x-graphit-auth-origin"), axum::http::HeaderValue::from_static("runtime")),
        ]);
        let lexical = source.graph_server().call_search_mcp(&source,
            rmcp::model::CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
                "project_dir": graphit_core::native_path::NativePath::new(selected.0.root().to_owned())?,
                "project_version": selected.2.version(), "query": "alpha", "mode": "fts", "k": 3, "rerank": false,
            }).as_object().unwrap().clone()), headers, CancellationToken::new()).await;
        assert_ne!(lexical.is_error, Some(true), "{lexical:?}");
        let lexical = lexical.structured_content.unwrap();
        assert!(!lexical["results"].as_array().unwrap().is_empty(), "canonical FTS serves its real MCP consumer");
        let NativeNnIndexPresence::Available(index) = open_local_native_nn_index(
            owner.auth.installation(),
            project,
            original.snapshot(),
            owner.native,
        )
        .await?
        else {
            return Err("rebuilt cohort NN missing".into());
        };
        assert_eq!(
            vectors(index.snapshot(original.snapshot())?).await?,
            before,
            "Source/model-compatible vectors are reused without inference"
        );
        drop(rows);
        drop(current);
        for format in ["all-entity-1","all-entity-unknown"] {
        replace_fts_recipe(&fts, format).await?;
        let unknown = BranchSnapshot::capture_observed(fts.to_str().unwrap(), None, None).await?;
        let unknown_version = unknown.version();
        let unknown_schema = unknown.schema().clone();
        let error = ast
            .index_semantic(
                selected.0.clone(),
                context.clone(),
                None,
                Some(BTreeSet::new()), std::collections::BTreeSet::new(),
                false,
                cancel.clone(),
                cancel.clone(),
            )
            .await
            .unwrap_err();
        assert!(
            !crate::foreground_ast_sources::semantic::only_inference_failed(&error),
            "unknown structure is not an NN-only failure"
        );
        let untouched = BranchSnapshot::capture_observed(fts.to_str().unwrap(), None, None).await?;
        assert_eq!(untouched.version(), unknown_version);
        assert_eq!(
            untouched.schema(),
            &unknown_schema,
            "foreign schemas must never be silently reset"
        );
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .catch_unwind()
    .await;
    let compiler_cleanup = ast.shutdown().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
