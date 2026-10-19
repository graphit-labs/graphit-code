//! Real Source supervisors and HTTP/MCP consumers with the provisioned native GGUF child.
use super::*;
use futures_util::FutureExt;
use graphit_ast::native_nn_index_locator::{NativeNnIndexPresence, open_local_native_nn_index};
use graphit_core::native_path::NativePath;
use graphit_daemon::model_service::{ModelErrorKind, ModelLimitReason, RuntimeCredential};
use rmcp::{
    model::{CallToolRequestParams, ProtocolVersion},
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::panic::AssertUnwindSafe;

#[allow(dead_code)]
use crate::rerank_test_http as embedding_wire;

#[tokio::test]
async fn direct_memory_index_and_query_use_the_real_dispatcher_without_local_resources() {
    use graphit_auth::profile::DirectProtocol;
    use graphit_daemon::model_service::{EmbedOperation, MemoryEmbeddingIndexRead};
    use graphit_records::memory_store::MemoryEmbeddingAttachOutcome;
    let brand = Brand::default();
    let config = ConfigSnapshot::new(
        &brand,
        [ConfigDocument::new(
            &brand,
            ConfigSource::Global,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let policy = HubResourcePolicy::resolve(&config).unwrap();
    let cancel = Cancellation::default();
    let request = policy.request_context(Instant::now(), &cancel).unwrap();
    let http = embedding_wire::fixture_with_deadline(
        vec![
            embedding_wire::reply(json!({"object":"list","model":"memory-direct","data":[
            {"object":"embedding","index":0,"embedding":[1.,0.,0.,0.]}
        ],"usage":{"prompt_tokens":8}})),
            embedding_wire::reply(json!({"object":"list","model":"memory-direct","data":[
            {"object":"embedding","index":0,"embedding":[1.,0.,0.,0.]}
        ],"usage":{"prompt_tokens":2}})),
        ],
        request.snapshot().required_deadline().unwrap(),
    )
    .await;
    let mut fixture = Fixture::with_ai_services(
        AiService::Direct {
            protocol: DirectProtocol::OpenAi,
            endpoint: format!("{}/v1/embeddings", http.origin),
            model: "memory-direct".into(),
            model_revision: "weights-r1".into(),
            recipe: "retrieval-r1".into(),
            dimensions: 4,
            request_dimensions: None,
            batch_endpoint: None,
        },
        AiService::Disabled,
    );
    fixture.projects.truncate(1);
    fixture.config = config;
    fixture.policy = policy;
    let memories = fixture.seed_memories().await;
    let provider = active_provider(&fixture.installation, &fixture.config, &request)
        .unwrap()
        .unwrap();
    let foreground = ForegroundSource::start_with_inventory(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        &request,
        Some(&provider),
    )
    .unwrap();
    let modules = foreground
        .server
        .as_ref()
        .unwrap()
        .project_modules(&request)
        .unwrap();
    let mut jobs = foreground
        .create_memory_jobs(
            &fixture.installation,
            &fixture.config,
            Some(&provider),
            &request,
            &modules,
        )
        .unwrap()
        .unwrap();
    let resources = jobs
        .start(
            &fixture.installation,
            Vec::new(),
            foreground.scope,
            foreground.native,
            NativeBinding::KiroV3,
            &request,
        )
        .await
        .unwrap()
        .unwrap();
    assert!(resources.local.is_none());
    assert!(jobs.child.is_none());
    let metadata = resources.metadata(&request).await.unwrap();
    assert_eq!(metadata.local_generation, None);
    assert_eq!(metadata.dimensions, 4);
    let (project, id, version) = &foreground.projects.as_ref().unwrap()[0];
    let context = graphit_daemon::supervisor::ProjectContext {
        id: *id,
        version: version.clone(),
    };
    let indexed = resources.index(project, &context, &cancel).await.unwrap();
    let MemoryEmbeddingIndexRead::Complete {
        generation,
        last_attachment,
    } = indexed
    else {
        panic!("existing PROJECT corpus must be indexed")
    };
    assert_eq!(generation.project_id, *id);
    assert_eq!(generation.project_version, version.version());
    assert!(matches!(
        last_attachment.as_deref(),
        Some(MemoryEmbeddingAttachOutcome::Published(_))
    ));
    let repeated = resources.index(project, &context, &cancel).await.unwrap();
    assert!(matches!(
        repeated,
        MemoryEmbeddingIndexRead::Complete {
            last_attachment: None,
            ..
        }
    ));
    let captured =
        CapturedPublicationScope::project(project, *id, foreground.scope, request.snapshot())
            .unwrap();
    let binding = captured.binding().unwrap();
    let scope = ScopeInput {
        installation: foreground.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &binding,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let key = foreground.auth.runtime().check(request.snapshot()).unwrap();
    let issuer = foreground.auth.issuer();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(request.snapshot()).unwrap()).unwrap(),
            &request,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let read = authority
        .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Memory)
        .unwrap();
    let identity =
        crate::foreground_memory_jobs::MemoryJobResources::memory_identity(&metadata).unwrap();
    let expected_document = prepare_memory_embedding_document(&memories[0], &identity)
        .unwrap()
        .into_text();
    let model =
        MemoryEmbeddingModel::new(request.snapshot(), identity, Duration::from_secs(30)).unwrap();
    let found = model
        .query(
            &read,
            "Paris capital".into(),
            MemorySemanticSearchOptions {
                filter: MemoryListFilter::default(),
                include_deleted_history: false,
                top_k: NonZeroUsize::new(1).unwrap(),
            },
            read_limits(&foreground),
            |operation: EmbedOperation| {
                let resources = &resources;
                let metadata = &metadata;
                let original = request.snapshot();
                async move { resources.embed_batch(original, metadata, operation).await }
            },
        )
        .await
        .unwrap();
    let MemorySemanticSearchRead::Published {
        generation: queried,
        total_hits,
        hits,
    } = found
    else {
        panic!("remote vector must be queried from the actual published corpus")
    };
    assert_eq!(queried.project_id, *id);
    assert_eq!(queried.project_version, version.version());
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current.id, memories[0].id);
    assert_eq!(hits[0].current.revision, memories[0].revision);
    assert!(hits[0].distance.abs() < 1e-6);
    let requests = http.server.await.unwrap();
    assert_eq!(
        requests.len(),
        2,
        "covered index must not send another inference"
    );
    assert_eq!(requests[0].body["input"], json!([expected_document]));
    assert_eq!(requests[1].body["input"], json!(["Paris capital"]));
    for request in requests {
        assert!(request.head.starts_with("POST /v1/embeddings HTTP/1.1"));
        assert!(request.head.contains("Bearer fixture-direct-embedding"));
        assert!(!request.head.contains("fixture-local-only"));
        assert_eq!(request.body["model"], "memory-direct");
    }
    assert!(jobs.child.is_none());
    assert!(
        ConfigStore::global(&fixture.installation)
            .unwrap()
            .load()
            .unwrap()
            .get::<model_api::ModelSettings>(&ConfigKey::new("daemon.models").unwrap())
            .unwrap()
            .is_none()
    );
    jobs.shutdown().await.unwrap();
    foreground.shutdown().await.unwrap();
}

pub(super) fn install_gguf(fixture: &Fixture) -> tempfile::TempDir {
    use crate::model_provision::{MODEL_BYTES, MODEL_FILE, MODEL_ID, MODEL_REVISION};
    let asset = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL").expect("provisioned pinned BF16 asset"),
    );
    assert_eq!(fs::metadata(&asset).unwrap().len(), MODEL_BYTES);
    let bundle = tempfile::tempdir_in(asset.parent().unwrap()).unwrap();
    // A private inode preserves captured ctime while other fixtures use the pinned asset.
    fs::copy(&asset, bundle.path().join(MODEL_FILE)).unwrap();
    fs::write(
        bundle.path().join("model.json"),
        crate::models::pinned_manifest(768).unwrap(),
    )
    .unwrap();
    let settings = model_api::ModelSettings {
        slots: vec![model_api::SlotSettings {
            name: crate::models::DEFAULT_SLOT.into(),
            selection: model_api::SelectionSettings {
                id: MODEL_ID.into(),
                revision: MODEL_REVISION.into(),
                task: model_api::TaskKind::Embedding,
            },
            bundle: bundle.path().to_owned(),
            manifest_relative: "model.json".into(),
            runtime: crate::models::default_runtime().unwrap(),
        }],
    };
    ConfigStore::global(&fixture.installation)
        .unwrap()
        .set(
            ConfigKey::new("daemon.models").unwrap(),
            serde_json::to_value(settings).unwrap(),
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
    bundle
}

async fn source_call(
    fixture: &Fixture,
    native: graphit_core::native_document_file::NativeDocumentFileLimits,
    name: &str,
    arguments: serde_json::Value,
    cancellation: &Cancellation,
) -> io::Result<serde_json::Value> {
    let original = fixture
        .policy
        .request_context(Instant::now(), cancellation)
        .map_err(io::Error::other)?;
    let mut output = Vec::new();
    crate::graph_entry::run(
        &fixture.installation,
        &reqwest::Client::new(),
        CallToolRequestParams::new(name.to_owned())
            .with_arguments(arguments.as_object().unwrap().clone()),
        (native, fixture.policy.content()),
        &original,
        &mut output,
    )
    .await?;
    serde_json::from_slice(&output).map_err(io::Error::other)
}

async fn wait_nn(
    fixture: &Fixture,
    auth: &graphit_daemon::auth_startup::DaemonAuth,
    project: &ProjectPaths,
    id: ProjectId,
    version: &VersionContext,
    native: graphit_core::native_document_file::NativeDocumentFileLimits,
) {
    let cancellation = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let captured = CapturedPublicationScope::project(
        project,
        id,
        ScopeLimits::new(16, 65536, 256).unwrap(),
        request.snapshot(),
    )
    .unwrap();
    let record = captured.binding().unwrap();
    let project = ProjectCapture {
        scope: ScopeCapture {
            live: &captured,
            record: &record,
        },
        version,
    };
    let until = request.snapshot().required_deadline().unwrap();
    tokio::time::timeout_at(tokio::time::Instant::from_std(until), async {
        loop {
            request.check().unwrap();
            if let NativeNnIndexPresence::Available(index) =
                open_local_native_nn_index(auth.installation(), project, request.snapshot(), native)
                    .await
                    .unwrap()
            {
                assert!(index.version() > 0);
                return;
            }
            // Only publication metadata is retried; no embedding query is replayed here.
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Source supervisor did not publish the genuine native NN index");
}

/// One fresh Source publication and its actual native GGUF consumer, without a corpus campaign.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires provisioned BF16 GGUF and actual GRAPHIT_TEST_MODEL_SERVER"]
async fn native_gguf_source_one_file_commits_nn_and_reads_matching_identity() {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;

    let mut fixture = Fixture::new();
    fixture.projects.truncate(1);
    let _bundle = install_gguf(&fixture);
    let text = "function capital() { return \"Paris\"; }\n";
    fs::write(fixture.projects[0].root().join("calls.js"), text).unwrap();
    let cancellation = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &request,
    )
    .await
    .unwrap();
    let native = foreground.native;
    let address = foreground.address();
    let auth = Arc::clone(&foreground.auth);
    let projects = foreground.projects.clone().unwrap();
    let resources = foreground
        .server
        .as_ref()
        .unwrap()
        .semantic_model()
        .unwrap()
        .unwrap();
    let child_address = resources.local.as_ref().unwrap().address;
    let service = Cancellation::default();
    let running_signal = service.clone();
    let running = tokio::spawn(async move { foreground.run(&running_signal).await });
    let exercise = AssertUnwindSafe(async {
        assert_eq!(resources.local.as_ref().unwrap().expected.dimensions(), Some(768));
        assert_eq!(resources.local.as_ref().unwrap().expected.model(), crate::model_provision::MODEL_ID);
        assert_eq!(resources.local.as_ref().unwrap().expected.revision(), crate::model_provision::MODEL_REVISION);
        assert!(resources.local.as_ref().unwrap().expected.embedding_is_scalar());
        let (project, id, version) = &projects[0];
        wait_nn(&fixture, &auth, project, *id, version, native).await;
        let path = json!(NativePath::new(project.root().to_owned()).unwrap());
        let graph = source_call(&fixture, native, "graphit_ast_query", json!({
            "project_dir":path,"project_version":version.version(),
            "query":"MATCH (n:Function) WHERE n.name = 'capital' RETURN n.uid, n.data_key, n.path, n.source_sha256, n.raw_version"
        }), &cancellation).await.unwrap();
        let graph_row = &graph["rows"].as_array().unwrap()[0];
        assert_eq!(graph["rows"].as_array().unwrap().len(), 1);
        assert_eq!(graph_row[2]["value"], "calls.js");
        let mut source_sha256 = String::with_capacity(64);
        for byte in Sha256::digest(text.as_bytes()) {
            write!(&mut source_sha256, "{byte:02x}").unwrap();
        }
        assert_eq!(graph_row[3]["value"], source_sha256);
        let original_version = graph_row[4]["value"].as_str().expect("Graph raw_version string")
            .parse::<u64>().expect("Graph raw_version unsigned Original");
        assert!(original_version > 0);
        let original = fixture.policy.request_context(Instant::now(), &cancellation).unwrap();
        let source_address = crate::source_entry::address(&fixture.installation, native, &original)
            .await.unwrap();
        let source_connection = crate::ast_source::connect_source(
            &fixture.installation, source_address, &reqwest::Client::new(), native, &original, (),
        ).await.unwrap();
        let source = source_connection.peer().call_tool(
            CallToolRequestParams::new("graphit_ast_source".to_owned()).with_arguments(
                json!({"project_dir":project.root().to_str().unwrap(),"project_version":version.version(),"path":"calls.js"})
                    .as_object().unwrap().clone()),
        ).await;
        source_connection.cancel().await.unwrap();
        let source = source.unwrap();
        assert_ne!(source.is_error, Some(true), "{source:?}");
        assert_eq!(source.content.iter().filter_map(|part| part.as_text())
            .map(|part| part.text.as_str()).collect::<String>(), text);
        let nn = source_call(&fixture, native, "graphit_ast_search", json!({
            "project_dir":path,"project_version":version.version(),
            "query":"function returning Paris","mode":"semantic","k":3,"metric":"cosine"
        }), &cancellation).await.unwrap();
        println!("source_gguf_one_file_response={}", json!({
            "status":nn["status"],
            "graph_original_version":original_version,
            "model":{"id":nn["model"]["id"],"revision":nn["model"]["revision"],"dimensions":nn["model"]["dimensions"]},
            "results":nn["results"].as_array().map(|rows|rows.iter().map(|hit|json!({
                "uid":hit["uid"],"data_key":hit["data_key"],"name":hit["name"],"path":hit["path"],
                "raw_version":hit["raw_version"],"source_sha256":hit["source_sha256"],"distance":hit["distance"]
            })).collect::<Vec<_>>())
        }));
        assert_eq!(nn["status"], "available");
        assert_eq!(nn["model"]["id"], crate::model_provision::MODEL_ID);
        assert_eq!(nn["model"]["revision"], crate::model_provision::MODEL_REVISION);
        assert_eq!(nn["model"]["dimensions"], 768);
        for field in ["search_generation", "graph_generation", "raw_generation", "source_original_generation", "nn_generation", "query_model_generation"] {
            assert!(nn.get(field).is_none(), "obsolete field {field}");
        }
        let hits = nn["results"].as_array().unwrap();
        assert!(hits.iter().all(|hit| hit["path"] == "calls.js"
            && hit["distance"].as_f64().is_some_and(f64::is_finite)));
        let hit = hits.iter().find(|hit| hit["name"] == "capital").unwrap();
        assert_eq!(hit["uid"], graph_row[0]["value"]);
        assert_eq!(hit["data_key"], graph_row[1]["value"]);
        println!("source_gguf_one_file matching_uid=true dimensions=768");

        let cancelled = Cancellation::default();
        let original = fixture.policy.request_context(Instant::now(), &cancelled).unwrap();
        cancelled.cancel();
        let mut output = Vec::new();
        let error = crate::graph_entry::run(
            &fixture.installation,
            &reqwest::Client::new(),
            CallToolRequestParams::new("graphit_ast_search".to_owned()).with_arguments(
                json!({"project_dir":path,"project_version":version.version(),
                    "query":"capital","mode":"semantic"}).as_object().unwrap().clone()),
            (native, fixture.policy.content()),
            &original,
            &mut output,
        ).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(output.is_empty());
    }).catch_unwind().await;
    service.cancel();
    let stopped = running.await;
    let child_closed = TcpStream::connect(child_address).await.is_err();
    let source_closed = match address {
        Some(address) => TcpStream::connect(address).await.is_err(),
        None => false,
    };
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    stopped.unwrap().unwrap();
    assert!(child_closed);
    assert!(source_closed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires pinned BGE F16 GGUF and actual GRAPHIT_TEST_MODEL_SERVER"]
async fn native_ast_fts_local_bge_rerank_without_embedding_preserves_the_published_page() {
    let mut fixture = Fixture::with_services(false, true);
    fixture.projects.truncate(1);
    fixture.install_rerank_model().await;
    let global = ConfigStore::global(&fixture.installation).unwrap();
    let cancellation = Cancellation::default();
    for (key, value) in [
        (Module::Embedding.key(), json!(false)),
        (Module::Memory.key(), json!(false)),
        (Module::Ast.key(), json!(true)),
    ] {
        global
            .set(key, value, Duration::from_secs(5), &cancellation)
            .unwrap();
    }
    fixture.config =
        ConfigSnapshot::new(fixture.installation.brand(), [global.load().unwrap()]).unwrap();
    fixture.policy = HubResourcePolicy::resolve(&fixture.config).unwrap();
    for (path, text) in [
        (
            "paris.js",
            "function capital() { return \"Paris is the capital of France\"; }\n",
        ),
        (
            "color.js",
            "function capital() { return \"uppercase text color\"; }\n",
        ),
    ] {
        fs::write(fixture.projects[0].root().join(path), text).unwrap();
    }
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let provider = active_provider(&fixture.installation, &fixture.config, &request)
        .unwrap()
        .unwrap();
    // Use the existing browser AST fixture boundary: one actual Raw/Graph
    // publisher completes before requests capture its immutable published Copy.
    let mut foreground = ForegroundSource::start_with_inventory(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        &request,
        Some(&provider),
    )
    .unwrap();
    foreground.project_control.as_mut().unwrap().environment = fixture.environment();
    assert!(foreground.memory_job_resources.upgrade().is_none());
    assert!(
        foreground
            .memory_jobs
            .as_ref()
            .is_none_or(|jobs| jobs.child.is_none())
    );
    let source = foreground.server.as_ref().unwrap().clone();
    assert!(source.semantic_model().unwrap().is_none());
    let native = foreground.native;
    let selected = foreground.projects.as_ref().unwrap()[0].clone();
    let ast = Arc::new(
        ForegroundAstSources::new(
            Arc::clone(&foreground.auth),
            ForegroundAstSourceConfig {
                live: source.graph_server().live_ast(),
                installation: fixture.installation.clone(),
                base: fixture.config.clone(),
                policy: foreground.policy.clone(),
                scope: foreground.scope,
                native,
                binding: NativeBinding::KiroV3,
                environment: fixture.environment(),
            },
            request.snapshot(),
        )
        .unwrap(),
    );
    let context = graphit_daemon::supervisor::ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    assert!(
        ast.index(
            &selected.0,
            &context,
            graphit_daemon::watcher::ChangeBatch {
                rescan: true,
                ..Default::default()
            },
            &cancellation,
        )
        .await
        .unwrap()
        .is_some(),
        "the single real publisher creates Raw before Graph"
    );
    assert!(
        ast.index_semantic(
            selected.0.clone(),
            context,
            None,
            None,
            std::collections::BTreeSet::new(),
            false,
            cancellation.clone(),
            cancellation.clone()
        )
        .await
        .unwrap()
        .pending_files
        .is_empty(),
        "embedding-disabled publication finishes its source work; Graph is checked by the real query"
    );
    let local_models = Arc::clone(source.ast_search_context().unwrap().local_models);
    // An opted-out request must succeed even without access to the real control.
    let control = local_models.write().unwrap().take().unwrap();
    let service = Cancellation::default();
    let run_signal = service.clone();
    let running = tokio::spawn(async move { foreground.run(&run_signal).await });
    let exercise = AssertUnwindSafe(async {
        let arguments = json!({
            "project_dir":NativePath::new(selected.0.root().to_owned()).unwrap(),
            "project_version":selected.2.version(), "query":"capital", "mode":"fts", "k":2,
        });
        let baseline = source_call(&fixture, native, "graphit_ast_search", arguments.clone(), &cancellation).await.unwrap();
        assert_eq!(baseline["status"], "available");
        let original = baseline["results"].as_array().unwrap();
        assert_eq!(original.len(), 2);
        assert!(original.iter().all(|row| row.get("rerank_score").is_none()));
        let mut explicit = arguments.clone();
        explicit["rerank"] = json!(false);
        let disabled = source_call(&fixture, native, "graphit_ast_search", explicit, &cancellation).await.unwrap();
        assert_eq!(disabled, baseline);
        *local_models.write().unwrap() = Some(control);
        let mut enabled = arguments;
        enabled["rerank"] = json!(true);
        let ranked = source_call(&fixture, native, "graphit_ast_search", enabled, &cancellation).await.unwrap();
        assert!(source.semantic_model().unwrap().is_none());
        let rows = ranked["results"].as_array().unwrap();
        assert_eq!(rows.len(), original.len());
        let mut remaining = original.clone();
        for (rank, row) in rows.iter().enumerate() {
            assert!(row["rerank_score"].as_f64().is_some_and(f64::is_finite));
            assert_eq!(row["rerank_rank"], rank + 1);
            let mut observed = row.clone();
            observed.as_object_mut().unwrap().remove("rerank_score");
            observed.as_object_mut().unwrap().remove("rerank_rank");
            let found = remaining.iter().position(|candidate| *candidate == observed).expect("same original candidate, scores, source and identity");
            remaining.swap_remove(found);
        }
        assert!(remaining.is_empty());
        let mut original_page = baseline;
        let mut ranked_page = ranked;
        original_page.as_object_mut().unwrap().remove("results");
        ranked_page.as_object_mut().unwrap().remove("results");
        assert_eq!(ranked_page, original_page, "same published pins and page metadata");
        println!("native_ast_fts_local_bge_rerank embedding=false candidates=2 opt_out_without_control=true original_page_preserved=true");
    }).catch_unwind().await;
    service.cancel();
    let stopped = running.await;
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    stopped.unwrap().unwrap();
}

#[tokio::test]
#[ignore = "requires provisioned BF16 GGUF and actual GRAPHIT_TEST_MODEL_SERVER"]
async fn native_gguf_source_two_projects_and_long_document() {
    let fixture = Fixture::new();
    let _bundle = install_gguf(&fixture);
    for (index, project) in fixture.projects.iter().enumerate() {
        fs::write(project.root().join(".astignore"), "docs/\n").unwrap();
        fs::write(
            project.root().join(format!("project_{index}.rs")),
            format!("pub fn project_{index}_capital() -> &'static str {{ \"Paris\" }}\n"),
        )
        .unwrap();
        fs::create_dir(project.root().join("docs")).unwrap();
        fs::write(project.root().join(format!("docs/guide_{index}.md")),
            format!("---\nid: guide_{index}\n---\n# Paris project {index}\nParis is the capital of France.\n")).unwrap();
    }
    let memories = fixture.seed_memories().await;
    let cancellation = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &request,
    )
    .await
    .unwrap();
    let native = foreground.native;
    let address = foreground.address();
    let auth = Arc::clone(&foreground.auth);
    let projects = foreground.projects.clone();
    let resources = foreground
        .server
        .as_ref()
        .map(|server| server.semantic_model());
    let mut child_address = None;
    let service = Cancellation::default();
    let running_signal = service.clone();
    let running = tokio::spawn(async move { foreground.run(&running_signal).await });
    let exercise = AssertUnwindSafe(async {
        let address = address.as_ref().unwrap();
        let projects = projects.as_ref().unwrap();
        let resources = resources.unwrap().unwrap().unwrap();
        child_address = Some(resources.local.as_ref().unwrap().address);
        assert_eq!(resources.local.as_ref().unwrap().expected.dimensions(), Some(768));
        assert_eq!(resources.local.as_ref().unwrap().expected.model(), crate::model_provision::MODEL_ID);
        assert_eq!(resources.local.as_ref().unwrap().expected.revision(), crate::model_provision::MODEL_REVISION);
        assert!(resources.local.as_ref().unwrap().expected.embedding_is_scalar());

        for (index, (project, id, version)) in projects.iter().enumerate() {
            wait_nn(&fixture, &auth, project, *id, version, native).await;
            let path = serde_json::to_value(NativePath::new(project.root().to_owned()).unwrap()).unwrap();
            let ast = source_call(&fixture, native, "graphit_ast_search", json!({
                "project_dir":path,"project_version":version.version(),
                "query":"Which function returns the capital of France?","mode":"semantic","k":8,"metric":"cosine"
            }), &cancellation).await.unwrap();
            assert_eq!(ast["status"], "available");
            assert_eq!(ast["model"]["id"], crate::model_provision::MODEL_ID);
            assert_eq!(ast["model"]["revision"], crate::model_provision::MODEL_REVISION);
            assert_eq!(ast["model"]["dimensions"], 768);
            for field in ["search_generation", "graph_generation", "raw_generation", "source_original_generation", "nn_generation", "query_model_generation"] {
                assert!(ast.get(field).is_none(), "obsolete field {field}");
            }
            let hits = ast["results"].as_array().unwrap();
            assert!(hits.iter().any(|hit| hit["name"] == format!("project_{index}_capital")));
            assert!(hits.iter().all(|hit| hit["path"] != format!("project_{}.rs", 1-index)));
            assert!(hits.iter().all(|hit| hit["distance"].as_f64().unwrap().is_finite()));

            // The existing handlers check index readiness before invoking the model.
            // Retries below observe unfinished publication; only the ready call infers.
            let until = Instant::now() + Duration::from_millis(fixture.policy.limit(
                graphit_hub::policy::ResourceSetting::RequestDeadlineMs).value);
            let memory = tokio::time::timeout_at(until.into(), async {
                loop {
                    let value = source_call(&fixture, native, "graphit_memory_search", json!({
                        "project_dir":path,"project_version":version.version(),
                        "text":"Paris is the capital of France.","top_k":4,"mode":"semantic"
                    }), &cancellation).await.unwrap();
                    if value["kind"] == "published" { break value; }
                    assert!(value["kind"] == "unavailable" || value["kind"] == "no_corpus");
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }).await.expect("Source Memory initial native publication");
            assert_eq!(memory["generation"]["project_id"], json!(id));
            assert_eq!(memory["generation"]["project_version"], version.version());
            assert!(memory["hits"].as_array().unwrap().iter().any(|hit|
                hit["current"]["id"] == json!(memories[index].id)
                && hit["distance"].as_f64().is_some_and(f64::is_finite)));
            let until = Instant::now() + Duration::from_millis(fixture.policy.limit(
                graphit_hub::policy::ResourceSetting::RequestDeadlineMs).value);
            let knowledge = tokio::time::timeout_at(until.into(), async {
                loop {
                    let value = source_call(&fixture, native, "graphit_knowledge_search", json!({
                        "project_dir":path,"project_version":version.version(),
                        "search":{"query":"Paris capital France","mode":"semantic","top_k":4,"page_size":4}
                    }), &cancellation).await.unwrap();
                    if value["native"]["state"] == "hits" { break value; }
                    assert!(matches!(value["native"]["state"].as_str(), Some("no_corpus"|"unavailable"|"published_empty")));
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }).await.expect("Source Knowledge initial native publication");
            assert!(knowledge["native"]["semantic_generation"]["version"].as_u64().unwrap() > 0);
            let hits = knowledge["native"]["hits"].as_array().unwrap();
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0]["focused_source"]["slug"], format!("guide_{index}"));
            assert!(hits[0]["semantic_distance"].as_f64().unwrap().is_finite());
            println!("source_gguf_project_index={index} version={} knowledge_nn={} memory_hits={} dimensions=768",
                version.version(),
                knowledge["native"]["semantic_generation"]["version"], memory["hits"].as_array().unwrap().len());
        }
        let wrong = source_call(&fixture, native, "graphit_ast_search", json!({
            "project_dir":NativePath::new(projects[0].0.root().to_owned()).unwrap(),
            "project_version":"unmounted-version","query":"capital","mode":"semantic"
        }), &cancellation).await;
        assert!(wrong.is_err(), "selectors must not authorize an unmounted version");
        // The MCP catalogue is public metadata. Authorization is enforced on
        // this same valid data operation, before parsing or model execution.
        let denied_request = fixture.policy.request_context(Instant::now(), &cancellation).unwrap();
        let denied_deadline = tokio::time::Instant::from_std(
            denied_request.snapshot().required_deadline().unwrap());
        let unauthenticated = tokio::time::timeout_at(denied_deadline,
            ().serve_with_lifecycle(
                StreamableHttpClientTransport::with_client(reqwest::Client::new(),
                    StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"))),
                ClientLifecycleMode::Discover { preferred_versions:vec![ProtocolVersion::LATEST] },
            )).await.expect("Source MCP initialization within original deadline").unwrap();
        let denied = tokio::time::timeout_at(denied_deadline,
            unauthenticated.call_tool(CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
                "project_dir":NativePath::new(projects[0].0.root().to_owned()).unwrap(),
                "project_version":projects[0].2.version(),
                "query":"Which function returns the capital of France?","mode":"semantic","k":8,"metric":"cosine"
            }).as_object().unwrap().clone()))).await;
        unauthenticated.cancel().await.unwrap();
        let denied = denied.expect("Source data operation within original deadline").unwrap();
        denied_request.check().unwrap();
        assert_eq!(denied.is_error, Some(true));
        assert!(denied.structured_content.is_none());
        assert_eq!(denied.content.len(), 1);
        assert_eq!(denied.content[0].as_text().unwrap().text,
            "AST search requires native credentials");

        // The long case uses this Source's actual child once, independently of
        // publication. It does not claim that this long text was indexed by Source.
        let long = " a".repeat(8193);
        let original = fixture.policy.request_context(Instant::now(), &cancellation).unwrap();
        let issuer = auth.issuer();
        let runtime = auth.runtime().check(original.snapshot()).unwrap();
        let secret = runtime.secret(original.snapshot()).unwrap();
        let caller = issuer.authenticate_runtime(std::str::from_utf8(secret).unwrap(), &original).unwrap();
        let mut binding = issuer.capture_service(&caller, ServicePurpose::Embedding).unwrap();
        let lease = binding.local().unwrap();
        let credential = RuntimeCredential::from_secret(secret).unwrap();
        let transport = model_api::client_transport(TcpStream::connect(resources.local.as_ref().unwrap().address).await.unwrap(),
            ChildOptions::standard().unwrap().rpc).unwrap();
        let client = model_api::ModelRpcClient::new(Default::default(), transport);
        let dispatch = tokio::spawn(client.dispatch);
        let mut rpc_context = tarpc::context::current();
        rpc_context.deadline = original.snapshot().required_deadline().unwrap();
        let output = client.client.embed(rpc_context, credential,
            crate::models::DEFAULT_SLOT.into(), resources.local.as_ref().unwrap().expected.config_id(),
            EmbedInput::Query, vec![long.clone()]).await;
        drop(client.client);
        dispatch.await.unwrap().unwrap();
        let error = output.unwrap().unwrap_err();
        assert_eq!(error.kind, ModelErrorKind::Limit);
        assert_eq!(error.limit, Some(ModelLimitReason::NativeTokenLimit));
        lease.check().unwrap();
        original.check().unwrap();
        // A new RPC gets its own original deadline from the unchanged product policy.
        drop(binding); drop(caller); drop(runtime);
        let original = fixture.policy.request_context(Instant::now(), &cancellation).unwrap();
        let runtime = auth.runtime().check(original.snapshot()).unwrap();
        let caller = issuer.authenticate_runtime(std::str::from_utf8(runtime.secret(original.snapshot()).unwrap()).unwrap(), &original).unwrap();
        let mut binding = issuer.capture_service(&caller, ServicePurpose::Embedding).unwrap();
        let lease = binding.local().unwrap();
        let output = resources.local.as_ref().unwrap().client.embed(&lease, auth.runtime(), &resources.local.as_ref().unwrap().expected,
            model_api::EmbedOperation { input:EmbedInput::Document, texts:vec![long],
                deadline:original.snapshot().required_deadline().unwrap() },
            TcpStream::connect(resources.local.as_ref().unwrap().address)).await.unwrap();
        assert_eq!(output.config_id, resources.local.as_ref().unwrap().expected.config_id());
        assert_eq!(output.dimensions, 768);
        assert_eq!(output.vectors.len(), 1);
        assert!(output.vectors[0].iter().all(|n| n.is_finite()));
        assert!((output.vectors[0].iter().map(|n| n*n).sum::<f32>().sqrt()-1.0).abs() < 1e-5);
        println!("source_gguf_long_query_native_token_limit=true document_vectors=1 dimensions=768 default_max_tokens=8192 model_generation={}", output.generation);
        drop(binding); drop(caller); drop(runtime);
        let cancel = Cancellation::default();
        let original = fixture.policy.request_context(Instant::now(), &cancel).unwrap();
        let arguments = json!({
            "project_dir":NativePath::new(projects[0].0.root().to_owned()).unwrap(),
            "project_version":projects[0].2.version(),"query":"capital","mode":"semantic"
        });
        cancel.cancel();
        let mut output = Vec::new();
        let error = crate::graph_entry::run(
            &fixture.installation,
            &reqwest::Client::new(),
            CallToolRequestParams::new("graphit_ast_search".to_owned())
                .with_arguments(arguments.as_object().unwrap().clone()),
            (native, fixture.policy.content()),
            &original,
            &mut output,
        ).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(output.is_empty());
    }).catch_unwind().await;
    service.cancel();
    let stopped = running.await;
    // Cleanup runs before any shutdown assertion or propagation of the original panic.
    let child_closed = match child_address {
        Some(address) => TcpStream::connect(address).await.is_err(),
        None => true,
    };
    let source_closed = match address {
        Some(address) => TcpStream::connect(address).await.is_err(),
        None => true,
    };
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    stopped.unwrap().unwrap();
    assert!(child_closed);
    assert!(source_closed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_child_exit_retires_local_resources_keeps_task_and_allows_normal_refresh() {
    use graphit_auth::profile::DirectProtocol;
    for local_embedding in [true, false] {
        let mut fixture = if local_embedding {
            Fixture::new()
        } else {
            Fixture::with_ai_services(
                AiService::Direct {
                    protocol: DirectProtocol::OpenAi,
                    endpoint: "http://127.0.0.1:9/v1/embeddings".into(),
                    model: "remote-kept".into(),
                    model_revision: "r1".into(),
                    recipe: "retrieval-r1".into(),
                    dimensions: 4,
                    request_dimensions: None,
                    batch_endpoint: None,
                },
                AiService::Local(LocalExecution {
                    device: LocalDevice::Cpu,
                    device_id: 0,
                }),
            )
        };
        fixture.projects.truncate(1);
        if local_embedding {
            fixture.install_model();
        } else {
            let brand = Brand::default();
            fixture.config = ConfigSnapshot::new(
                &brand,
                [ConfigDocument::new(
                    &brand,
                    ConfigSource::Global,
                    [(
                        ConfigKey::new("broker.allow_loopback_http").unwrap(),
                        json!(true),
                    )],
                )
                .unwrap()],
            )
            .unwrap();
            fixture.policy = HubResourcePolicy::resolve(&fixture.config).unwrap();
            // The real lightweight control process can remain alive after all
            // slots have been explicitly retired. No rerank model is prepared.
            ConfigStore::global(&fixture.installation)
                .unwrap()
                .set(
                    ConfigKey::new("daemon.models").unwrap(),
                    json!({"slots":[]}),
                    Duration::from_secs(5),
                    &Cancellation::default(),
                )
                .unwrap();
        }
        let cancellation = Cancellation::default();
        let request = fixture
            .policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let mut foreground = ForegroundSource::start_and_publish(
            &fixture.installation,
            &fixture.inputs(),
            0,
            &fixture.config,
            NativeBinding::KiroV3,
            fixture.environment(),
            &request,
        )
        .await
        .unwrap();
        let server = foreground.server.as_ref().unwrap().clone();
        let resources = server.semantic_model().unwrap().unwrap();
        assert_eq!(resources.local.is_some(), local_embedding);
        let remote_identity = if local_embedding {
            None
        } else {
            Some(resources.metadata(&request).await.unwrap())
        };
        let (client, address) = foreground
            .memory_jobs
            .as_mut()
            .unwrap()
            .ensure_process(&fixture.installation, fixture.environment(), &request)
            .await
            .unwrap();
        let auth = Arc::clone(&foreground.auth);
        let selected = foreground.projects.as_ref().unwrap()[0].clone();
        let scope = foreground.scope;
        let source_address = foreground.address().unwrap();
        let child_lifetime = foreground
            .memory_jobs
            .as_ref()
            .unwrap()
            .child_lifetime_for_test();
        let mut exits = foreground.model_child_exits.subscribe();
        let mut refreshes = foreground.model_refreshes.subscribe();
        let service = Cancellation::default();
        let signal = service.clone();
        let remote_refresh = if local_embedding {
            None
        } else {
            client
                .shutdown(
                    auth.runtime(),
                    request.snapshot(),
                    TcpStream::connect(address),
                )
                .await
                .unwrap();
            child_lifetime.cancel();
            tokio::time::timeout(
                Duration::from_secs(10),
                foreground
                    .memory_jobs
                    .as_mut()
                    .unwrap()
                    .child
                    .as_mut()
                    .unwrap()
                    .completed(),
            )
            .await
            .unwrap();
            let operation = fixture
                .policy
                .request_context(Instant::now(), &cancellation)
                .unwrap();
            let result = foreground
                .refresh_model_selection(
                    &fixture.installation,
                    &fixture.config,
                    &fixture.environment(),
                    &operation,
                )
                .await;
            Some((result, server.semantic_model().unwrap()))
        };
        let running = tokio::spawn(async move { foreground.run(&signal).await });
        let exercise = AssertUnwindSafe(async {
            if let Some((result, retained)) = &remote_refresh {
                let error = result.as_ref().expect_err("the observed child has exited");
                assert_eq!(
                    error
                        .get_ref()
                        .and_then(|cause| cause.downcast_ref::<model_api::ModelServiceError>())
                        .map(model_api::ModelServiceError::kind),
                    Some(model_api::ModelErrorKind::Transport)
                );
                assert!(
                    retained.as_ref().is_some_and(|resources| resources.local.is_none()),
                    "a failed local refresh must retain the remote binding"
                );
            }
            // End the actual compute child independently of the Source owner.
            if local_embedding {
                client
                    .shutdown(
                        auth.runtime(),
                        request.snapshot(),
                        TcpStream::connect(address),
                    )
                    .await
                    .unwrap();
                // The shutdown RPC drains native workers, while the independent
                // ProcessHost lifetime actually terminates the compute process.
                child_lifetime.cancel();
            }
            tokio::time::timeout(Duration::from_secs(10), exits.changed())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                *exits.borrow_and_update(),
                Some(true),
                "native cleanup completed"
            );
            assert!(TcpStream::connect(address).await.is_err());
            assert!(
                !running.is_finished(),
                "compute death must not stop the control plane"
            );
            if local_embedding {
                assert!(server.semantic_model().unwrap().is_none());
            } else {
                let current = server.semantic_model().unwrap().unwrap();
                assert!(current.local.is_none());
                let observation = fixture
                    .policy
                    .request_context(Instant::now(), &cancellation)
                    .unwrap();
                assert!(
                    &current.metadata(&observation).await.unwrap()
                        == remote_identity.as_ref().unwrap(),
                    "a local rerank/control child does not own remote embedding resources"
                );
            }
            let original = fixture
                .policy
                .request_context(Instant::now(), &cancellation)
                .unwrap();
            let role = sdk_task_role(&auth, &selected, scope, &original);
            let task_client =
                sdk_task_client(source_address, &role, fixture.policy.content(), &original).await;
            let tasks = live_task_call(
                &task_client,
                "graphit_task_query",
                json!({"input":{"selection":{"table":"tasks"},"columns":["id","status"],"page_size":1,"top_k":1}}),
                false,
            )
            .await;
            task_client.cancel().await.unwrap();
            {
                let issuer = auth.issuer();
                let key = auth.runtime().check(original.snapshot()).unwrap();
                let raw =
                    std::str::from_utf8(key.secret(original.snapshot()).unwrap()).unwrap();
                let caller = issuer.authenticate_runtime(raw, &original).unwrap();
                let parent = issuer.admin_parent(&caller, &role.0).unwrap();
                issuer.close_run(&parent, &role.1).unwrap();
            }
            assert_eq!(
                tasks,
                json!({"operation":"query","result":{"state":"no_corpus"}})
            );
            // A Task read is not model demand and must not silently restart it.
            if local_embedding {
                assert!(server.semantic_model().unwrap().is_none());
                // A refresh interrupted by the earlier process stop is not
                // the outcome of the configuration change below.
                drop(refreshes.borrow_and_update());
                ConfigStore::global(&fixture.installation)
                    .unwrap()
                    .set(
                        ConfigKey::new("models.idle_timeout_seconds").unwrap(),
                        json!(111),
                        Duration::from_secs(5),
                        &cancellation,
                    )
                    .unwrap();
                let fresh = tokio::time::timeout(Duration::from_secs(30), async {
                    refreshes.changed().await.unwrap();
                    match *refreshes.borrow_and_update() {
                        Some(Ok(())) => (),
                        Some(Err(kind)) => {
                            panic!("ordinary configuration refresh failed: {kind:?}");
                        }
                        None => panic!("configuration refresh has no outcome"),
                    }
                    server
                        .semantic_model()
                        .unwrap()
                        .expect("successful local configuration refresh publishes its resources")
                })
                .await
                .unwrap();
                assert!(!Arc::ptr_eq(&resources, &fresh));
                assert_eq!(
                    resources.local.as_ref().unwrap().expected.vector_id(),
                    fresh.local.as_ref().unwrap().expected.vector_id()
                );
                let operation = fixture
                    .policy
                    .request_context(Instant::now(), &cancellation)
                    .unwrap();
                let metadata = fresh.metadata(&operation).await.unwrap();
                let local = fresh.local.as_ref().unwrap();
                let config_id = local.expected.config_id();
                let ack = local
                    .client
                    .refresh(
                        auth.runtime(),
                        operation.snapshot(),
                        [(crate::models::DEFAULT_SLOT.to_owned(), config_id)]
                            .into_iter()
                            .collect(),
                        TcpStream::connect(local.address),
                    )
                    .await
                    .unwrap();
                assert_eq!(ack.config_ids.get(crate::models::DEFAULT_SLOT), Some(&config_id));
                assert_eq!(
                    metadata.vector_id,
                    fresh.local.as_ref().unwrap().expected.vector_id()
                );
            }
        })
        .catch_unwind()
        .await;
        service.cancel();
        let stopped = running.await;
        if let Err(panic) = exercise {
            std::panic::resume_unwind(panic);
        }
        stopped.unwrap().unwrap();
    }
}
