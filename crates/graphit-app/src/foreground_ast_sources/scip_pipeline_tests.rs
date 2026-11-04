//! Source consumer journeys with controlled process fixtures and an opt-in real Docker container.
use super::{ForegroundSource, foreground_memory_jobs_tests as support};
use crate::{
    foreground_ast_sources::{ForegroundAstSourceConfig, ForegroundAstSources},
    foreground_memory_jobs::MemoryJobResources,
    graph_entry::{Command, Query, Selection},
};
use arrow_array::{
    Array, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, StringArray, UInt64Array,
};
use axum::http::{HeaderMap, HeaderValue, header};
use futures_util::{FutureExt, TryStreamExt};
use graphit_core::native_path::NativePath;
use graphit_core::{
    config::{ConfigKey, ConfigSnapshot, ConfigStore},
    file_lock::Cancellation,
};
use graphit_daemon::{supervisor::ProjectContext, watcher::ChangeBatch};
use graphit_hub::NativeBinding;
use protobuf::Message;
use rmcp::model::CallToolRequestParams;
use scip::types::{
    Document, Index, Metadata, Occurrence, PositionEncoding, SymbolInformation, TextEncoding,
    ToolInfo,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
type VectorRows = BTreeMap<(String, String), (Vec<f32>, Option<u64>)>;

async fn live_vectors(
    owner: &ForegroundSource,
    selected: &(
        graphit_core::project_paths::ProjectPaths,
        graphit_core::project_identity::ProjectId,
        graphit_core::version_context::VersionContext,
    ),
    resources: &MemoryJobResources,
) -> TestResult<VectorRows> {
    use graphit_ast::native_nn_index_locator::{NativeNnIndexPresence, open_local_native_nn_index};
    use graphit_auth::{ProjectCapture, ScopeCapture};
    use graphit_core::artifact_store::CapturedPublicationScope;
    use graphit_data::lance_branch_store::{col, lit};
    let cancellation = Cancellation::default();
    let request = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
    let original = request.snapshot();
    let capture =
        CapturedPublicationScope::project(&selected.0, selected.1, owner.scope, original)?;
    let record = capture.binding()?;
    let project = ProjectCapture {
        scope: ScopeCapture {
            live: &capture,
            record: &record,
        },
        version: &selected.2,
    };
    let NativeNnIndexPresence::Available(index) =
        open_local_native_nn_index(owner.auth.installation(), project, original, owner.native)
            .await?
    else {
        return Ok(BTreeMap::new());
    };
    let model = crate::ast_semantic::expected_native_nn_model(
        &resources
            .local
            .as_ref()
            .ok_or("local model fixture missing")?
            .expected,
    )?;
    graphit_ast::corpus::validate_schema(index.schema())?;
    if let Ok(field) = index.schema().field_with_name("vector") {
        assert_eq!(
            field,
            graphit_ast::corpus::vector_fields(&model)?.field(0),
            "observed corpus uses the selected model"
        );
    }
    let mut scan = index.snapshot(original)?.scan();
    scan.filter_expr(col("path").eq(lit("code.rs")));
    let mut rows = scan.try_into_stream().await?;
    let mut output = BTreeMap::new();
    while let Some(batch) = rows.try_next().await? {
        let batch = graphit_ast::corpus::vectors(&batch, &model, original)?;
        graphit_ast::native_nn::validate_native_nn(&batch, &model, original)?;
        let strings = |name| {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
        };
        let vectors = batch
            .column_by_name("vector")
            .unwrap()
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .unwrap();
        let generations = batch
            .column_by_name("model_generation")
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        for row in 0..batch.num_rows() {
            let vector = vectors.value(row);
            let vector = vector.as_any().downcast_ref::<Float32Array>().unwrap();
            output.insert(
                (
                    strings("uid").value(row).to_owned(),
                    strings("data_key").value(row).to_owned(),
                ),
                (
                    vector.values().to_vec(),
                    (!generations.is_null(row)).then(|| generations.value(row)),
                ),
            );
        }
    }
    Ok(output)
}
const RUST: &str = "/// café Ω\npub fn alpha(x: i32) -> i32 { if x > 0 { beta() } else { 0 } }\npub fn beta() -> i32 { 7 }\n";
const SQL: &str = "CREATE TABLE FALLBACK_SQL(C INTEGER); SELECT C FROM FALLBACK_SQL;\n";
const JAVASCRIPT: &str =
    "function js_caller() { return js_target(); }\nfunction js_target() { return 7; }\n";
const COMPILER_ALPHA: &str = "scip-rust cargo scip-default-fixture 1 alpha().";
const COMPILER_BETA: &str = "scip-rust cargo scip-default-fixture 1 beta().";
const COMPILER_ONLY: &str = "scip-rust cargo scip-default-fixture 1 compiler_only#";
const QUERIES: [&str; 5] = [
    "MATCH (n:Function) WHERE n.name = 'alpha' RETURN n.uid, n.data_key, n.path, n.metadata_json, n.scip_symbols_json",
    "MATCH (a:Function)-[:CALLS]->(b:Function) WHERE a.name = 'alpha' RETURN a.name, b.name",
    "MATCH (a:Function)-[:CALLS]->(b:Function) WHERE a.name = 'js_caller' RETURN a.name, b.name, a.uid, a.data_key",
    "MATCH (n:`Table`) WHERE n.name = 'FALLBACK_SQL' RETURN n.uid, n.data_key, n.path, n.scip_symbols_json",
    "MATCH (n:AstFileObservation) WHERE n.path = 'code.rs' RETURN n.resolution_outcomes_json",
];

#[derive(Clone, Copy, Debug)]
enum Docker {
    Compiler,
    Absent,
    Failed,
}

fn compiler_index() -> TestResult<Vec<u8>> {
    let lines: Vec<_> = RUST.lines().collect();
    let beta_call = i32::try_from(lines[1].find("beta").unwrap())?;
    let primitive = i32::try_from(lines[1].find("i32").unwrap())?;
    let document = Document {
        relative_path: "code.rs".into(),
        language: "rust".into(),
        text: RUST.into(),
        position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
        occurrences: vec![
            Occurrence {
                range: vec![1, primitive, primitive + 3],
                enclosing_range: vec![1, primitive, 1, primitive + 3],
                symbol: COMPILER_ONLY.into(),
                symbol_roles: 1,
                ..Default::default()
            },
            Occurrence {
                range: vec![1, 7, 12],
                enclosing_range: vec![1, 0, i32::try_from(lines[1].len())?],
                symbol: COMPILER_ALPHA.into(),
                symbol_roles: 1,
                ..Default::default()
            },
            Occurrence {
                range: vec![2, 7, 11],
                enclosing_range: vec![2, 0, i32::try_from(lines[2].len())?],
                symbol: COMPILER_BETA.into(),
                symbol_roles: 1,
                ..Default::default()
            },
            Occurrence {
                range: vec![1, beta_call, beta_call + 4],
                enclosing_range: vec![1, 0, i32::try_from(lines[1].len())?],
                symbol: COMPILER_BETA.into(),
                ..Default::default()
            },
        ],
        symbols: vec![
            SymbolInformation {
                symbol: COMPILER_ONLY.into(),
                kind: scip::types::symbol_information::Kind::Type.into(),
                display_name: "compiler_only_type".into(),
                ..Default::default()
            },
            SymbolInformation {
                symbol: COMPILER_ALPHA.into(),
                display_name: "compiler_alpha_witness".into(),
                documentation: vec!["compiler fixture café Ω".into()],
                ..Default::default()
            },
            SymbolInformation {
                symbol: COMPILER_BETA.into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    Ok(Index {
        metadata: Some(Metadata {
            project_root: "file:///source".into(),
            text_document_encoding: TextEncoding::UTF8.into(),
            tool_info: Some(ToolInfo {
                name: "fixture-compiler".into(),
                version: "1".into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        })
        .into(),
        documents: vec![document],
        ..Default::default()
    }
    .write_to_bytes()?)
}

fn docker_environment(
    directory: &Path,
    mode: Docker,
) -> TestResult<Vec<(std::ffi::OsString, std::ffi::OsString)>> {
    fs::write(directory.join("compiler.scip"), compiler_index()?)?;
    if matches!(mode, Docker::Compiler) {
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            directory.join("release-compiler"),
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )?;
    }
    if !matches!(mode, Docker::Absent) {
        let executable = directory.join("docker");
        fs::write(
            &executable,
            r#"#!/bin/sh
case "$1" in
run)
  printf '%s\n' "$@" >> "$SCIP_TEST_DIR/argv"
  if [ "$SCIP_TEST_MODE" = failed ]; then exit 42; fi
  for arg in "$@"; do
    case "$arg" in *graphit-scip-typescript:*) exit 42 ;; esac
  done
  if [ -p "$SCIP_TEST_DIR/release-compiler" ]; then
    IFS= read -r release < "$SCIP_TEST_DIR/release-compiler"
  fi
  for arg in "$@"; do
    case "$arg" in
      'type=bind,"source='*',target=/output')
        output=${arg#type=bind,\"source=}; output=${output%\",target=/output}
        /bin/cp "$SCIP_TEST_DIR/compiler.scip" "$output/index.scip" ;;
    esac
  done ;;
ps) printf '%s\n' aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ;;
rm) : > "$SCIP_TEST_DIR/removed" ;;
*) exit 91 ;;
esac
"#,
        )?;
        fs::set_permissions(executable, fs::Permissions::from_mode(0o700))?;
    }
    Ok(vec![
        ("PATH".into(), directory.as_os_str().to_owned()),
        ("SCIP_TEST_DIR".into(), directory.as_os_str().to_owned()),
        (
            "SCIP_TEST_MODE".into(),
            if matches!(mode, Docker::Failed) {
                "failed"
            } else {
                "compiler"
            }
            .into(),
        ),
    ])
}

fn observe(
    owner: &ForegroundSource,
    source: &crate::source_server::SourceServer,
    selected: &(
        graphit_core::project_paths::ProjectPaths,
        graphit_core::project_identity::ProjectId,
        graphit_core::version_context::VersionContext,
    ),
    credential: &str,
    resources: &MemoryJobResources,
    mode: Docker,
) -> TestResult<Vec<Value>> {
    let cancellation = Cancellation::default();
    let request = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
    let base = ConfigSnapshot::new(source.policy().installation.brand(), [])?;
    let live = source.live_projects();
    let runtime = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        crate::project_graph::with_exported_graph(
            &source.graph_server(),
            crate::project_graph::ReadInput {
                auth: &owner.auth,
                policy: source.policy(),
                base: &base,
                live: &live,
                selected: (&selected.0, selected.1, &selected.2),
                credential,
                local: false,
                request: &request,
            },
            |graph, _| -> TestResult<Vec<Value>> {
                let reader = graph.source_reader(request.snapshot())?;
                assert!(
                    reader
                        .nodes()
                        .any(|node| node.relative() == Path::new("index.scip"))
                );
                let bytes = fs::read(reader.source_path(request.snapshot())?.join("index.scip"))?;
                graph.revalidate(request.snapshot())?;
                let index = Index::parse_from_bytes(&bytes)?;
                assert!(index.metadata.is_some());
                for path in ["code.rs", "model.tsql", "calls.js"] {
                    let document = index
                        .documents
                        .iter()
                        .find(|doc| doc.relative_path == path)
                        .ok_or("missing published SCIP document")?;
                    assert_ne!(document.position_encoding.value(), 0);
                    assert!(
                        !document.occurrences.is_empty(),
                        "real syntax/compiler occurrences: {path}"
                    );
                }
                let rust = index
                    .documents
                    .iter()
                    .find(|doc| doc.relative_path == "code.rs")
                    .unwrap();
                let compiler = rust.occurrences.iter().any(|o| o.symbol == COMPILER_ALPHA);
                assert_eq!(compiler, matches!(mode, Docker::Compiler));
                let mut observations = Vec::new();
                for (query_index, query) in QUERIES.into_iter().enumerate() {
                    let result = graphit_ast::icebug_query::execute(
                        graph,
                        request.snapshot(),
                        &runtime,
                        query,
                        &Default::default(),
                        source.policy().graph_query,
                    )?;
                    if query_index == 1 && !matches!(mode, Docker::Compiler) {
                        assert!(
                            result.rows.is_empty(),
                            "Rust fallback must not invent local CALLS"
                        );
                    } else {
                        assert!(!result.rows.is_empty(), "preserved graph consumer: {query}");
                    }
                    observations.push(serde_json::to_value(result)?);
                }
                let alpha = &observations[0]["rows"][0];
                let metadata: Value = serde_json::from_str(
                    alpha[3]["value"]
                        .as_str()
                        .ok_or("native metadata missing")?,
                )?;
                assert!(
                    metadata.to_string().contains("café"),
                    "native documentation survives SCIP consumption"
                );
                assert!(
                    metadata["contributions"]
                        .as_array()
                        .ok_or("native contributions missing")?
                        .iter()
                        .any(|c| c["callable"]["complexity"]
                            .as_str()
                            .is_some_and(|s| s.starts_with("Score { value: 2,"))),
                    "native branch complexity survives SCIP consumption"
                );
                let symbols = alpha[4]["value"]
                    .as_str()
                    .ok_or("SCIP graph overlay missing")?;
                assert_eq!(
                    symbols.contains(COMPILER_ALPHA),
                    matches!(mode, Docker::Compiler)
                );
                if matches!(mode, Docker::Compiler) {
                    assert!(
                        observations[1]["rows"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|row| row[0]["value"] == "alpha" && row[1]["value"] == "beta")
                    );
                }
                assert!(observations[2]["rows"].as_array().unwrap().iter()
                    .any(|row| row[0]["value"] == "js_caller" && row[1]["value"] == "js_target"));
                assert_eq!(observations[3]["rows"][0][2]["value"], "model.tsql");
                assert!(
                    observations[4]["rows"][0][0]["value"]
                        .as_str()
                        .is_some_and(|value| value.contains("CALLS")),
                    "native Rust relation observations are preserved"
                );

                let search = runtime
                    .block_on(graphit_ast::entity_search::EntitySearch::open(
                        Arc::clone(graph),
                        Arc::clone(request.snapshot()),
                    ))?
                    .ok_or("search sidecar missing")?;
                let local = resources
                    .local
                    .as_ref()
                    .ok_or("actual local SCIP NN resources missing")?;
                let model = crate::ast_semantic::expected_native_nn_model(&local.expected)?;
                let nn = runtime
                    .block_on(search.nn(&model))?
                    .ok_or("NN clone missing")?;
                let files = graphit_ast::icebug_query::execute(
                    graph,
                    request.snapshot(),
                    &runtime,
                    "MATCH (n:AstFileObservation) RETURN n.path,n.raw_version",
                    &Default::default(),
                    source.policy().graph_query,
                )?;
                let file_versions = files
                    .rows
                    .iter()
                    .map(|row| {
                        Ok((
                            row[0]["value"]
                                .as_str()
                                .ok_or("FILE path missing")?
                                .to_owned(),
                            row[1]["value"]
                                .as_str()
                                .ok_or("FILE Raw version missing")?
                                .parse::<u64>()?,
                        ))
                    })
                    .collect::<TestResult<BTreeMap<_, _>>>()?;
                let raw = runtime
                    .block_on(graph.open_lance_sidecar(
                        "source.lance",
                        graphit_ast::indexed_source::source_schema(),
                        request.snapshot(),
                    ))?
                    .ok_or("exported Source absent")?;
                let mut rows = runtime.block_on(nn.scan().try_into_stream())?;
                let mut observed_paths = BTreeSet::new();
                let mut observed_identities = BTreeSet::new();
                while let Some(batch) = runtime.block_on(rows.try_next())? {
                    let batch = graphit_ast::corpus::vectors(&batch, &model, request.snapshot())?;
                    graphit_ast::native_nn::validate_native_nn(&batch, &model, request.snapshot())?;
                    let strings = |name| {
                        batch
                            .column_by_name(name)
                            .unwrap()
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap()
                    };
                    let versions = batch
                        .column_by_name("raw_version")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<UInt64Array>()
                        .unwrap();
                    let vectors = batch
                        .column_by_name("vector")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<FixedSizeListArray>()
                        .unwrap();
                    let hashes = batch
                        .column_by_name("full_source_sha256")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<FixedSizeBinaryArray>()
                        .unwrap();
                    for row in 0..batch.num_rows() {
                        let path = strings("path").value(row);
                        assert_eq!(
                            Some(&versions.value(row)),
                            file_versions.get(path),
                            "NN retains this FILE's genuine Raw observation"
                        );
                        if !matches!(path, "code.rs" | "model.tsql" | "calls.js") {
                            continue;
                        }
                        let source =
                            runtime.block_on(graphit_ast::indexed_source::read_indexed_source(
                                &raw,
                                path,
                                request.snapshot(),
                            ))?;
                        let graphit_ast::source_inspection::IndexedSource::Available(text) =
                            source.source(request.snapshot())?
                        else {
                            return Err("exported Source missing".into());
                        };
                        assert!(text.contains(strings("projection").value(row)));
                        let hash: [u8; 32] = Sha256::digest(text.as_bytes()).into();
                        assert_eq!(hashes.value(row), hash.as_slice());
                        let graph_identity = match (path, strings("name").value(row)) {
                            ("code.rs", "alpha") => Some((&observations[0]["rows"][0], 0, 1)),
                            ("model.tsql", "FALLBACK_SQL") => {
                                Some((&observations[3]["rows"][0], 0, 1))
                            }
                            ("calls.js", "js_caller") => Some((&observations[2]["rows"][0], 2, 3)),
                            _ => None,
                        };
                        if let Some((graph_row, uid_column, key_column)) = graph_identity {
                            assert_eq!(
                                strings("uid").value(row),
                                graph_row[uid_column]["value"].as_str().unwrap()
                            );
                            assert_eq!(
                                strings("data_key").value(row),
                                graph_row[key_column]["value"].as_str().unwrap()
                            );
                            observed_identities.insert(path.to_owned());
                        }
                        let vector = vectors.value(row);
                        let values = vector.as_any().downcast_ref::<Float32Array>().unwrap();
                        assert!(values.values().iter().all(|v| v.is_finite()));
                        observed_paths.insert(path.to_owned());
                    }
                }
                assert!(
                    observed_paths.contains("code.rs")
                        && observed_paths.contains("model.tsql")
                        && observed_paths.contains("calls.js")
                );
                assert_eq!(
                    observed_identities, observed_paths,
                    "native NN and graph retain the same UID/data_key"
                );
                let raw = runtime
                    .block_on(graph.open_lance_sidecar(
                        "source.lance",
                        graphit_ast::indexed_source::source_schema(),
                        request.snapshot(),
                    ))?
                    .ok_or("captured Source clone missing")?;
                let changed_javascript = format!("{JAVASCRIPT}// changed B\n");
                for (path, expected) in [
                    ("code.rs", RUST),
                    ("model.tsql", SQL),
                    (
                        "calls.js",
                        if matches!(mode, Docker::Compiler) {
                            changed_javascript.as_str()
                        } else {
                            JAVASCRIPT
                        },
                    ),
                ] {
                    let source =
                        runtime.block_on(graphit_ast::indexed_source::read_indexed_source(
                            &raw,
                            path,
                            request.snapshot(),
                        ))?;
                    let graphit_ast::source_inspection::IndexedSource::Available(text) =
                        source.source(request.snapshot())?
                    else {
                        return Err("captured Source text missing".into());
                    };
                    assert_eq!(text, expected);
                }
                graph.revalidate(request.snapshot())?;
                Ok(observations)
            },
        )?
    })
}

async fn scenario(mode: Docker) -> TestResult {
    let (fixture, mut owner, tasks, credential, _) = support::browser_search_fixture().await;
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let source = owner.server.as_ref().unwrap().clone();
    let cancellation = Cancellation::default();
    let installation = source.policy().installation.clone();
    let settings = ConfigStore::project(&selected.0)?;
    for (key, value) in [
        ("modules.ast", json!(true)),
        ("ast.grammar", json!(".tsql=antlr-tsql")),
    ] {
        settings.set(
            ConfigKey::new(key)?,
            value,
            Duration::from_secs(5),
            &cancellation,
        )?;
    }
    assert!(
        fixture
            .config()
            .get::<bool>(&ConfigKey::new("ast.scip.enabled")?)?
            .is_none()
    );
    fs::write(selected.0.root().join("code.rs"), RUST)?;
    fs::write(selected.0.root().join("model.tsql"), SQL)?;
    fs::write(selected.0.root().join("calls.js"), JAVASCRIPT)?;
    fs::write(
        selected.0.root().join("Cargo.toml"),
        "[package]\nname='scip-default-fixture'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    let docker = tempfile::tempdir()?;
    let environment = docker_environment(docker.path(), mode)?;
    let request = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
    let ast = Arc::new(ForegroundAstSources::new(
        Arc::clone(&owner.auth),
        ForegroundAstSourceConfig {
            live: source.graph_server().live_ast(),
            installation,
            base: ConfigSnapshot::new(source.policy().installation.brand(), [])?,
            policy: owner.policy.clone(),
            scope: owner.scope,
            native: owner.native,
            binding: NativeBinding::KiroV3,
            environment,
        },
        request.snapshot(),
    )?);
    let resources = owner
        .memory_job_resources
        .upgrade()
        .ok_or("CPU child resources missing")?;
    let context = ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    let outcome = std::panic::AssertUnwindSafe(async {
        assert!(
            ast.index(
                &selected.0,
                &context,
                ChangeBatch {
                    rescan: true,
                    ..Default::default()
                },
                &cancellation
            )
            .await?
            .is_some()
        );
        if matches!(mode, Docker::Compiler) {
            let (first, release_parser) = ast.hold_after_file_publication("code.rs");
            let producer = Arc::clone(&ast);
            let project = selected.0.clone();
            let execution = Arc::clone(&resources);
            let job_cancellation = cancellation.clone();
            let background = Cancellation::default();
            let job_background = background.clone();
            let job = tokio::spawn(async move {
                producer.index_semantic(project, context, Some(execution), None, std::collections::BTreeSet::new(), false, job_background, job_cancellation).await
            });
            tokio::time::timeout(Duration::from_secs(30), first).await??;
            assert!(!job.is_finished(), "SCIP compiler is held before completion");
            let headers = HeaderMap::from_iter([
                (header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {credential}"))?),
                (header::HeaderName::from_static("x-graphit-auth-origin"), HeaderValue::from_static("runtime")),
            ]);
            let selection = json!({
                "project_dir":NativePath::new(selected.0.root().to_owned())?,
                "project_version":selected.2.version(),
                "query":"MATCH (n:Function) WHERE n.name='alpha' RETURN n.uid,n.data_key,n.raw_version,n.source_sha256,n.scip_symbols_json",
            });
            let syntax = source.graph_server().call_mcp(
                CallToolRequestParams::new("graphit_ast_query").with_arguments(selection.as_object().unwrap().clone()),
                headers.clone(), CancellationToken::new(),
            ).await;
            assert_ne!(syntax.is_error, Some(true), "{syntax:?}");
            let syntax = syntax.structured_content.unwrap();
            assert_eq!(syntax["rows"].as_array().unwrap().len(), 1);
            assert!(!serde_json::to_string(&syntax)?.contains(COMPILER_ALPHA), "compiler facts have not been released");
            let search = source.graph_server().call_search_mcp(&source,
                CallToolRequestParams::new("graphit_ast_search").with_arguments(json!({
                    "project_dir":NativePath::new(selected.0.root().to_owned())?,
                    "project_version":selected.2.version(),"query":"alpha","mode":"fts","rerank":false,
                }).as_object().unwrap().clone()), headers.clone(), CancellationToken::new(),
            ).await;
            assert_ne!(search.is_error, Some(true), "{search:?}");
            assert!(!search.structured_content.as_ref().unwrap()["results"].as_array().unwrap().is_empty());
            let parser_vectors=tokio::time::timeout(Duration::from_secs(60),async {
                loop {
                    let vectors=live_vectors(&owner,&selected,&resources).await?;
                    if !vectors.is_empty() {break TestResult::Ok(vectors);}
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }).await??;
            assert!(parser_vectors.keys().all(|(_,key)|key!="scip_definitions"),"compiler is still held");
            // A new batch preempts the compiler, without throwing away A's
            // parser facts or completed vectors. The next batch changes B only.
            background.cancel();
            release_parser.send(()).unwrap();
            let progress = job.await??;
            assert!(progress.version.is_some());
            let mut pending_files = progress.pending_files;
            assert!(pending_files.contains("model.tsql"), "the yielded structural tail must remain pending");
            fs::remove_file(docker.path().join("release-compiler"))?;
            fs::write(selected.0.root().join("calls.js"), format!("{JAVASCRIPT}// changed B\n"))?;
            let context = ProjectContext { id:selected.1, version:selected.2.clone() };
            ast.index(&selected.0, &context, ChangeBatch {
                paths:BTreeSet::from([selected.0.root().join("calls.js")]), ..Default::default()
            }, &cancellation).await?;
            pending_files.insert("calls.js".into());
            ast.index_semantic(selected.0.clone(), context, Some(Arc::clone(&resources)),
                Some(pending_files), BTreeSet::from(["calls.js".into()]), false,
                cancellation.clone(), cancellation.clone()).await
                .map_err(|error| format!("SCIP resumed producer: {error:?}"))?;
            let enriched = source.graph_server().call_mcp(
                CallToolRequestParams::new("graphit_ast_query").with_arguments(selection.as_object().unwrap().clone()),
                headers, CancellationToken::new(),
            ).await;
            assert_ne!(enriched.is_error, Some(true), "{enriched:?}");
            let enriched = enriched.structured_content.unwrap();
            assert!(serde_json::to_string(&enriched)?.contains(COMPILER_ALPHA));
            for column in 0..4 {
                assert_eq!(syntax["rows"][0][column], enriched["rows"][0][column], "resumed SCIP enrichment keeps A's UID/data_key and historical Source without reparsing it");
            }
            let compiled_vectors=live_vectors(&owner,&selected,&resources).await
                .map_err(|error| format!("SCIP enriched NN observation: {error:?}"))?;
            assert!(compiled_vectors.keys().any(|(_,key)|key=="scip_definitions"),"new compiler definition receives a real NN vector");
            for (identity,vector) in parser_vectors {
                assert_eq!(compiled_vectors.get(&identity),Some(&vector),"compiler enrichment reuses every parser vector and generation");
            }
        } else {
            assert!(ast.index_semantic(selected.0.clone(), context, Some(Arc::clone(&resources)), None, std::collections::BTreeSet::new(), false,
                cancellation.clone(), cancellation.clone()).await?.version.is_some());
        }
        let expected = observe(&owner, &source, &selected, &credential, &resources, mode)
            .map_err(|error| format!("SCIP explicit export consumer: {error:?}"))?;
        if matches!(mode, Docker::Absent) {
            assert!(!docker.path().join("argv").exists());
        } else {
            let args = fs::read_to_string(docker.path().join("argv"))?;
            assert!(
                args.contains("graphit-scip-rust:"),
                "default compiler family was selected"
            );
            assert!(
                args.contains("graphit-scip-typescript:"),
                "JS compiler family is explicitly attempted and falls back"
            );
            assert!(
                docker.path().join("removed").exists(),
                "real fixture cleanup completed"
            );
        }
        let commands: Vec<_> = QUERIES
            .iter()
            .map(|query| {
                Command::Query(Query {
                    selection: Selection {
                        project_dir: Some(selected.0.root().to_owned()),
                        project_version: Some(selected.2.version().into()),
                        ..Default::default()
                    },
                    query: (*query).into(),
                    parameters: Vec::new(),
                    string_parameters: Vec::new(),
                })
            })
            .collect();
        let results = fixture
            .run_ast_graph_commands(&mut owner, &source, &commands, &[])
            .await;
        assert_eq!(results.len(), expected.len());
        for (actual, expected) in results.iter().zip(&expected) {
            assert_eq!(
                actual["rows"], expected["rows"],
                "CLI->MCP reads same captured graph"
            );
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .catch_unwind()
    .await;
    let compiler_cleanup = ast.shutdown().await;
    let task_cleanup = tasks.shutdown().await;
    drop(resources);
    drop(ast);
    let source_cleanup = owner.shutdown().await;
    match outcome {
        Err(panic) => std::panic::resume_unwind(panic),
        Ok(result) => result?,
    }
    compiler_cleanup?;
    task_cleanup?;
    source_cleanup?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_scip_source_cli_mcp_preserves_compiler_and_both_syntax_fallbacks() -> TestResult {
    for mode in [Docker::Compiler, Docker::Absent, Docker::Failed] {
        scenario(mode).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_only_delta_is_visible_before_unrelated_compiler_and_syntax_tail() -> TestResult {
    native_only_journey(false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn productive_native_only_is_queryable_while_model_rpc_is_unanswered() -> TestResult {
    native_only_journey(true).await
}

async fn native_only_journey(hold_model: bool) -> TestResult {
    async fn release_compiler(path: &Path, line: &[u8]) -> std::io::Result<()> {
        use rustix::fs::{CWD, Mode, OFlags, openat};
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match openat(
                    CWD,
                    path,
                    OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
                    Mode::empty(),
                ) {
                    Ok(fd) => return std::io::Write::write_all(&mut fs::File::from(fd), line),
                    Err(rustix::io::Errno::NXIO) => {
                        tokio::time::sleep(Duration::from_millis(20)).await
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        })
        .await
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "compiler did not open its release FIFO",
            )
        })?
    }
    let (fixture, owner, tasks, credential, _) = if hold_model {
        support::browser_search_fixture().await
    } else {
        support::browser_syntax_fixture().await
    };
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let source = owner.server.as_ref().unwrap().clone();
    let cancellation = Cancellation::default();
    let settings = ConfigStore::project(&selected.0)?;
    for (key, value) in [("modules.ast", true), ("modules.embedding", hold_model)] {
        settings.set(
            ConfigKey::new(key)?,
            json!(value),
            Duration::from_secs(5),
            &cancellation,
        )?;
    }
    // Reuse the existing CPU service identity and RPC client; only this
    // fixture's endpoint is held before its first response. No account, model
    // download, provider binary or container daemon is needed for this contract.
    let mut request_seen = None;
    let mut model_release = None;
    let mut model_endpoint = None;
    let mut held_resources = None;
    let background = Cancellation::default();
    let source_preemption = Cancellation::default();
    if hold_model {
        let resources = owner.memory_job_resources.upgrade().unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        let (requested, observed) = tokio::sync::oneshot::channel();
        let (release, held) = tokio::sync::oneshot::channel();
        model_endpoint = Some(tokio::spawn(async move {
            let mut held = held;
            let (socket, _) = tokio::select! {
                socket = listener.accept() => socket?,
                _ = &mut held => return Ok::<(), std::io::Error>(()),
            };
            let _ = requested.send(());
            let _ = held.await;
            drop(socket);
            Ok::<(), std::io::Error>(())
        }));
        model_release = Some(release);
        request_seen = Some(observed);
        let mut execution = MemoryJobResources::remote(
            Arc::clone(&resources.auth),
            resources.http.clone(),
            Arc::clone(&resources.brokers),
            resources.policy.clone(),
            resources.scope,
            resources.native,
            resources.binding,
        )?;
        let local = resources.local.as_ref().unwrap();
        execution.local = Some(crate::foreground_memory_jobs::OwnedLocalEmbedding {
            client: Arc::clone(&local.client),
            expected: Arc::clone(&local.expected),
            address,
        });
        held_resources = Some(Arc::new(execution));
    }
    const VB: &str = "Public Class NativeWitness\nEnd Class\n";
    fs::write(selected.0.root().join("z.vb"), VB)?;
    for (path, name) in [("b.rs", "beta"), ("c.rs", "gamma")] {
        fs::write(
            selected.0.root().join(path),
            format!("pub fn {name}() {{}}\n"),
        )?;
    }
    fs::write(
        selected.0.root().join("Cargo.toml"),
        "[package]\nname='tail'\nversion='0.1.0'\n",
    )?;
    let docker = tempfile::tempdir()?;
    let environment = docker_environment(docker.path(), Docker::Compiler)?;
    let symbol = "scip-dotnet nuget native-priority-fixture 1 NativeWitness#";
    let start = i32::try_from(VB.find("NativeWitness").unwrap())?;
    let index = Index {
        metadata: Some(Metadata {
            project_root: "file:///source".into(),
            text_document_encoding: TextEncoding::UTF8.into(),
            tool_info: Some(ToolInfo {
                name: "fixture-compiler".into(),
                version: "1".into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        })
        .into(),
        documents: vec![Document {
            relative_path: "z.vb".into(),
            language: "visual-basic".into(),
            position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
            occurrences: vec![Occurrence {
                range: vec![0, start, start + 13],
                enclosing_range: vec![0, 0, 1, 9],
                symbol: symbol.into(),
                symbol_roles: 1,
                ..Default::default()
            }],
            symbols: vec![SymbolInformation {
                symbol: symbol.into(),
                display_name: "NativeWitness".into(),
                kind: scip::types::symbol_information::Kind::Type.into(),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    fs::write(docker.path().join("compiler.scip"), index.write_to_bytes()?)?;
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        docker.path().join("release-rust"),
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;
    let executable = docker.path().join("docker");
    let script = fs::read_to_string(&executable)?;
    let insertion = r#"  for arg in "$@"; do
    case "$arg" in
      *graphit-scip-dotnet:*) : > "$SCIP_TEST_DIR/dotnet-started" ;;
      *graphit-scip-rust:*)
        : > "$SCIP_TEST_DIR/rust-started"
        IFS= read -r release < "$SCIP_TEST_DIR/release-rust"
        : > "$SCIP_TEST_DIR/rust-finished"
        exit 42 ;;
    esac
  done
"#;
    let anchor = "  if [ \"$SCIP_TEST_MODE\" = failed ]; then exit 42; fi\n";
    assert!(script.contains(anchor));
    fs::write(
        &executable,
        script.replace(anchor, &format!("{insertion}{anchor}")),
    )?;
    let request = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
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
            environment,
        },
        request.snapshot(),
    )?);
    let headers = HeaderMap::from_iter([
        (
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {credential}"))?,
        ),
        (
            header::HeaderName::from_static("x-graphit-auth-origin"),
            HeaderValue::from_static("runtime"),
        ),
    ]);
    let query = async |cypher: &str| {
        let response = source
            .graph_server()
            .call_mcp(
                CallToolRequestParams::new("graphit_ast_query").with_arguments(
                    json!({"project_dir":NativePath::new(selected.0.root().to_owned())?,
                "project_version":selected.2.version(),"query":cypher})
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
                headers.clone(),
                CancellationToken::new(),
            )
            .await;
        assert_ne!(response.is_error, Some(true), "{response:?}");
        Ok::<_, Box<dyn std::error::Error>>(response.structured_content.unwrap())
    };
    let mut job = None;
    let result=std::panic::AssertUnwindSafe(async {
        let context=ProjectContext { id:selected.1,version:selected.2.clone() };
        if !hold_model {
            ast.index(&selected.0,&context,ChangeBatch{rescan:true,..Default::default()},&cancellation).await?;
        }
        let (published,release)=ast.hold_after_file_publication("z.vb");
        let producer=Arc::clone(&ast);let project=selected.0.clone();let lifetime=cancellation.clone();
        let execution = held_resources.take();
        let job_background = background.clone();
        let job_source_preemption = source_preemption.clone();
        let raw_error = ast.observe_next_raw_error();
        job = Some(tokio::spawn(async move {
            if let Some(execution) = execution {
                producer.index_productive(&project, &context, ChangeBatch {
                    rescan: true, paths: BTreeSet::from([project.root().join("z.vb")]), ..Default::default()
                }, Some(execution), Some(BTreeSet::new()), false, &job_source_preemption, &job_background, &lifetime).await
            } else {
                producer.index_semantic(project,context,None,None,
                    BTreeSet::from(["z.vb".into()]),false,job_source_preemption,lifetime).await
            }
        }));
        tokio::select! {
            biased;
            kind = raw_error => return Err(std::io::Error::new(kind?, "Raw producer failed before SCIP started").into()),
            ended = job.as_mut().unwrap() => {
                job.take();
                return Err(std::io::Error::other(format!("producer ended before SCIP started: {ended:?}")).into());
            }
            started = tokio::time::timeout(Duration::from_secs(30), async {
                while !docker.path().join("dotnet-started").exists() { tokio::time::sleep(Duration::from_millis(20)).await; }
            }) => started.map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "native-only compiler did not reach its release gate"))?,
        }
        release_compiler(&docker.path().join("release-compiler"), b"ready\n").await?;
        tokio::time::timeout(Duration::from_secs(30), published).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "native-only FILE was not published after compiler release"))??;
        // The compiler publishes and queues the native-only FILE before NN
        // may request its first vector. Waiting for the RPC sooner can deadlock
        // this fixture against its own compiler gate.
        if let Some(observed) = request_seen.take() {
            tokio::time::timeout(Duration::from_secs(30), observed).await
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "model RPC did not reach the fixture after FILE publication"))??;
        }
        assert!(!docker.path().join("rust-started").exists(),"the unrelated compiler has not delayed the native-only delta");
        assert_eq!(query("MATCH(n) WHERE n.name='NativeWitness' RETURN count(n)").await?["rows"][0][0]["value"],"1");
        assert_eq!(query("MATCH(n:AstFileObservation) WHERE n.path='b.rs' OR n.path='c.rs' RETURN count(DISTINCT n.path)").await?["rows"][0][0]["value"],if hold_model { "2" } else { "0" });
        let found=source.graph_server().call_search_mcp(&source,CallToolRequestParams::new("graphit_ast_search")
            .with_arguments(json!({"project_dir":NativePath::new(selected.0.root().to_owned())?,
                "project_version":selected.2.version(),"query":"NativeWitness","mode":"fts","rerank":false}).as_object().unwrap().clone()),
            headers.clone(),CancellationToken::new()).await;
        assert_ne!(found.is_error,Some(true),"{found:?}");
        assert!(!found.structured_content.unwrap()["results"].as_array().unwrap().is_empty());
        if hold_model {
            assert!(!model_endpoint.as_ref().unwrap().is_finished(), "model metadata/inference has never replied");
            assert!(!job.as_ref().unwrap().is_finished(), "compiler/native-only publication must proceed independently of NN");
        }
        release.send(()).map_err(|_|std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        tokio::time::timeout(Duration::from_secs(30),async {
            loop {
                if docker.path().join("rust-started").exists() && query("MATCH(n:AstFileObservation) WHERE n.path='b.rs' OR n.path='c.rs' RETURN count(DISTINCT n.path)").await?["rows"][0][0]["value"]=="2" {break;}
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok::<_,Box<dyn std::error::Error>>(())
        }).await??;
        assert_eq!(query("MATCH(n:Function) WHERE n.name='beta' OR n.name='gamma' RETURN count(n)").await?["rows"][0][0]["value"],"2");
        assert!(!job.as_ref().unwrap().is_finished(),"the unrelated compiler is still blocked, while syntax tail and native delta are queryable");
        release_compiler(&docker.path().join("release-rust"), b"finish\n").await?;
        if hold_model {
            tokio::time::timeout(Duration::from_secs(30), async {
                while !docker.path().join("rust-finished").exists() { tokio::time::sleep(Duration::from_millis(20)).await; }
            }).await?;
            assert!(!model_endpoint.as_ref().unwrap().is_finished());
            assert_eq!(query("MATCH(n) WHERE n.name='NativeWitness' RETURN count(n)").await?["rows"][0][0]["value"], "1");
            // Local metadata is derived from the selected model. This endpoint
            // is still holding the actual inference RPC after all FILE writes,
            // so no NN check or publisher waiting for its batches may pin Native.
            let captured = graphit_core::artifact_store::CapturedPublicationScope::project(
                &selected.0, selected.1, owner.scope, request.snapshot(),
            )?;
            let record = captured.binding()?;
            let project = graphit_auth::ProjectCapture {
                scope: graphit_auth::ScopeCapture { live: &captured, record: &record },
                version: &selected.2,
            };
            let (index, directory) = source.graph_server().live_ast()
                .capture(owner.auth.installation(), project, owner.native,
                    request.snapshot(), false).await?
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
            let weak = Arc::downgrade(&index.graph);
            let mut closed = index.graph.close_notification();
            drop((index, directory));
            tokio::time::timeout(Duration::from_secs(30), async {
                tokio::select! {
                    biased;
                    ended = job.as_mut().unwrap() => {
                        job.take();
                        Err(std::io::Error::other(format!(
                            "Source ended before Native closed during blocked inference: {ended:?}")))
                    }
                    result = closed.changed() => {
                        assert!(result.is_err(), "Native close notification must terminate");
                        Ok(())
                    }
                }
            }).await??;
            tokio::task::yield_now().await;
            assert!(!job.as_ref().unwrap().is_finished(), "closing Native must not depend on the inference ending");
            assert!(weak.upgrade().is_none(), "NN must not retain the Native database during inference");
            assert!(!model_endpoint.as_ref().unwrap().is_finished(),
                "Native must close while the inference RPC is still unanswered");
            // Stop the outstanding fixture RPC cooperatively, preserving the
            // already completed FILEs and their pending embedding projections.
            background.cancel();
        }
        let joined = tokio::time::timeout(Duration::from_secs(30), job.as_mut().unwrap()).await?;
        job.take();
        let progress = joined??;
        assert!(progress.pending_files.is_empty());
        if !hold_model {
            // Failed Rust enrichment left real, durable compiler work behind.
            // A later productive Lua delta must not restore those other FILEs
            // or start their compiler merely because their pending rows exist.
            let captured = graphit_core::artifact_store::CapturedPublicationScope::project(
                &selected.0, selected.1, owner.scope, request.snapshot(),
            )?;
            let record = captured.binding()?;
            let project = graphit_auth::ProjectCapture {
                scope: graphit_auth::ScopeCapture { live: &captured, record: &record },
                version: &selected.2,
            };
            let registry = source.graph_server().live_ast();
            let pending = || async {
                let (index, _directory) = registry.capture(owner.auth.installation(), project, owner.native, request.snapshot(), false).await?
                    .ok_or(std::io::ErrorKind::NotFound)?;
                Ok::<_, std::io::Error>(index.graph.pending_compiler_files(request.snapshot())?
                    .into_iter().map(|observation| (observation.path.clone(), observation))
                    .collect::<BTreeMap<_, _>>())
            };
            let before_pending = pending().await?;
            assert!(before_pending.contains_key("b.rs") && before_pending.contains_key("c.rs"));
            let compiler_calls = fs::read(docker.path().join("argv"))?;
            let lua = selected.0.root().join("only.lua");
            fs::write(&lua, "function isolated_delta() return 11 end\n")?;
            let delta_background = Cancellation::default();
            let context = ProjectContext { id:selected.1, version:selected.2.clone() };
            let mut delta = Box::pin(ast.index_productive(&selected.0, &context,
                ChangeBatch { paths: BTreeSet::from([lua]), ..Default::default() },
                None, Some(BTreeSet::new()), false, &delta_background, &delta_background, &cancellation));
            match tokio::time::timeout(Duration::from_secs(30), delta.as_mut()).await {
                Ok(result) => { result?; }
                Err(error) => {
                    delta_background.cancel();
                    let _ = tokio::time::timeout(Duration::from_secs(30), delta.as_mut()).await;
                    return Err(error.into());
                }
            }
            assert_eq!(fs::read(docker.path().join("argv"))?, compiler_calls,
                "an explicit Lua delta must not enter the unrelated pending compiler");
            assert_eq!(pending().await?, before_pending,
                "unrelated durable compiler work remains available for full/resume");
            assert_eq!(query("MATCH(n:Function) WHERE n.name='isolated_delta' RETURN count(n)").await?["rows"][0][0]["value"], "1");
            let found = source.graph_server().call_search_mcp(&source,
                CallToolRequestParams::new("graphit_ast_search").with_arguments(
                    json!({"project_dir":NativePath::new(selected.0.root().to_owned())?,
                        "project_version":selected.2.version(),"query":"isolated_delta",
                        "mode":"fts","rerank":false}).as_object().unwrap().clone()),
                headers.clone(), CancellationToken::new()).await;
            assert_ne!(found.is_error, Some(true), "{found:?}");
            assert!(!found.structured_content.unwrap()["results"].as_array().unwrap().is_empty());
        }
        Ok::<(),Box<dyn std::error::Error>>(())
    }).catch_unwind().await;
    // Cancel a compiler held at its FIFO before taking its service lock.
    source_preemption.cancel();
    cancellation.cancel();
    background.cancel();
    if let Some(release) = model_release.take() {
        let _ = release.send(());
    }
    if let Some(endpoint) = model_endpoint.take() {
        endpoint.await??;
    }
    let producer_cleanup = if let Some(task) = job.as_mut() {
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "SCIP producer did not join after cancellation",
                )
            })
            .and_then(|joined| joined.map(|_| ()).map_err(std::io::Error::other))
    } else {
        Ok(())
    };
    let compiler_cleanup = ast.shutdown().await;
    tasks.shutdown().await?;
    owner.shutdown().await.map_err(|failure| failure.error)?;
    compiler_cleanup?;
    producer_cleanup?;
    drop(fixture);
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn custom_image_routes_full_and_delta_without_profile_and_retires_changed_producer()
-> TestResult {
    custom_image_journey(false).await
}

/// Requires only the local stock image and Docker. No registry access or accounts.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Linux Docker and local ghcr.io/graphit-labs/graphit-scip-rust:v1"]
async fn real_docker_custom_image_full_delta_source_and_mount_contract() -> TestResult {
    custom_image_journey(true).await
}

struct RealDockerImages([String; 2]);
impl Drop for RealDockerImages {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["image", "rm"])
            .args(&self.0)
            .output();
    }
}

fn build_real_docker_images(directory: &Path, images: [String; 2]) -> TestResult<RealDockerImages> {
    // Declare cleanup before building, including partial build/tag failures.
    let owned = RealDockerImages(images);
    fs::write(
        directory.join("Dockerfile"),
        "FROM ghcr.io/graphit-labs/graphit-scip-rust:v1\nCOPY compiler.scip /opt/graphit-fixture/index.scip\nCOPY compiler-delta.scip /opt/graphit-fixture/delta.scip\nCOPY graphit-scip-run /usr/local/bin/graphit-scip-run\nRUN chmod 755 /usr/local/bin/graphit-scip-run\n",
    )?;
    fs::write(
        directory.join("graphit-scip-run"),
        r#"#!/bin/sh
set -eu
[ "$PWD" = /workspace ]
[ -r /workspace/code.rs ] && [ -r /workspace/unknown.exm ]
if touch /source/.compiler-must-not-write 2>/dev/null; then
  echo '/source was writable' >&2
  exit 71
fi
printf 'workspace-writable\n' > /workspace/.compiler-overlay-only
printf 'readonly-source writable-workspace writable-output\n' > /output/contract.txt
printf 'invoked\n' >> /cache/target/real-contract
[ ! -e /workspace/.compiler-fixture-fail ] || exit 42
if [ -e /workspace/.compiler-fixture-delta ]; then
  cp /opt/graphit-fixture/delta.scip /output/index.scip
else
  cp /opt/graphit-fixture/index.scip /output/index.scip
fi
[ -s /output/index.scip ]
"#,
    )?;
    let result = std::process::Command::new("docker")
        .args([
            "build",
            "--pull=false",
            "--network=none",
            "--tag",
            &owned.0[0],
        ])
        .arg(directory)
        .output()?;
    if !result.status.success() {
        return Err(std::io::Error::other(format!(
            "local SCIP fixture build failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ))
        .into());
    }
    let result = std::process::Command::new("docker")
        .args(["image", "tag", &owned.0[0], &owned.0[1]])
        .output()?;
    if !result.status.success() {
        return Err(std::io::Error::other(format!(
            "local SCIP fixture tag failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ))
        .into());
    }
    Ok(owned)
}

async fn custom_image_journey(real_docker: bool) -> TestResult {
    let (fixture, owner, tasks, credential, _) = support::browser_syntax_fixture().await;
    assert!(owner.memory_jobs.is_none());
    assert!(
        owner.memory_job_resources.upgrade().is_none(),
        "syntax fixture must not create model resources"
    );
    let selected = owner.projects.as_ref().unwrap()[0].clone();
    let source = owner.server.as_ref().unwrap().clone();
    let cancellation = Cancellation::default();
    let settings = ConfigStore::project(&selected.0)?;
    let image_name = format!(
        "graphit-scip-contract-{}",
        selected.1.to_string().to_ascii_lowercase()
    );
    let images = if real_docker {
        [format!("{image_name}:one"), format!("{image_name}:two")]
    } else {
        [
            "example.test/custom-parser:one".into(),
            "example.test/custom-parser:two".into(),
        ]
    };
    for (key, value) in [
        ("modules.ast", json!(true)),
        ("modules.embedding", json!(false)),
        (
            "ast.scip.parsers",
            json!(format!("rs={},exm={}", images[0], images[0])),
        ),
    ] {
        settings.set(
            ConfigKey::new(key)?,
            value,
            Duration::from_secs(5),
            &cancellation,
        )?;
    }
    fs::write(selected.0.root().join("code.rs"), RUST)?;
    fs::write(selected.0.root().join("unknown.exm"), RUST)?;
    let docker = tempfile::tempdir()?;
    let mut environment = if real_docker {
        std::env::vars_os().collect()
    } else {
        docker_environment(docker.path(), Docker::Compiler)?
    };
    if real_docker {
        environment.retain(|(key, _)| key != "GRAPHIT_AST_SCIP_PULL_POLICY");
        environment.push(("GRAPHIT_AST_SCIP_PULL_POLICY".into(), "never".into()));
    } else {
        fs::remove_file(docker.path().join("release-compiler"))?;
    }
    let mut index = Index::parse_from_bytes(&compiler_index()?)?;
    let mut custom_document = index.documents[0].clone();
    custom_document.relative_path = "unknown.exm".into();
    index.documents.push(custom_document);
    fs::write(docker.path().join("compiler.scip"), index.write_to_bytes()?)?;
    if real_docker {
        let mut delta = index.clone();
        for document in &mut delta.documents {
            if document.relative_path == "unknown.exm" {
                document.text = format!("{RUST}// incremental δ\n");
            }
        }
        fs::write(
            docker.path().join("compiler-delta.scip"),
            delta.write_to_bytes()?,
        )?;
    }
    let _real_images = if real_docker {
        Some(build_real_docker_images(docker.path(), images.clone())?)
    } else {
        let executable = docker.path().join("docker");
        let script = fs::read_to_string(&executable)?;
        fs::write(&executable, script.replace("  if [ \"$SCIP_TEST_MODE\" = failed ]; then exit 42; fi",
        "  for arg in \"$@\"; do [ \"$arg\" = example.test/custom-parser:two ] && exit 42; done\n  if [ \"$SCIP_TEST_MODE\" = failed ]; then exit 42; fi"))?;
        None
    };
    let request = owner
        .policy
        .request_context(Instant::now(), &cancellation)?;
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
            environment,
        },
        request.snapshot(),
    )?);
    let headers = HeaderMap::from_iter([
        (
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {credential}"))?,
        ),
        (
            header::HeaderName::from_static("x-graphit-auth-origin"),
            HeaderValue::from_static("runtime"),
        ),
    ]);
    let query = async |cypher: &str| -> TestResult<Value> {
        let response = source
            .graph_server()
            .call_mcp(
                CallToolRequestParams::new("graphit_ast_query").with_arguments(
                    json!({"project_dir":NativePath::new(selected.0.root().to_owned())?,
                "project_version":selected.2.version(),"query":cypher})
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
                headers.clone(),
                CancellationToken::new(),
            )
            .await;
        assert_ne!(response.is_error, Some(true), "{response:?}");
        Ok(response.structured_content.unwrap())
    };
    let (mcp_client, mcp_server) = if real_docker {
        use rmcp::service::{ClientLifecycleMode, ClientServiceExt};
        use rmcp::transport::{
            StreamableHttpClientTransport,
            streamable_http_client::StreamableHttpClientTransportConfig,
        };
        let listener = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &cancellation,
        )?;
        let uri = format!("http://{}/mcp", listener.address());
        let serving = source.clone();
        let server = tokio::spawn(async move { serving.serve(listener).await });
        let client = ()
            .serve_with_lifecycle(
                StreamableHttpClientTransport::with_client(
                    reqwest::Client::new(),
                    StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(
                        headers
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect(),
                    ),
                ),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
                },
            )
            .await?;
        (Some(client), Some(server))
    } else {
        (None, None)
    };
    let outcome = std::panic::AssertUnwindSafe(async {
        let context = ProjectContext {
            id: selected.1,
            version: selected.2.clone(),
        };
        ast.index_productive(&selected.0, &context, ChangeBatch { rescan:true, ..Default::default() },
            None, None, false, &cancellation, &cancellation, &cancellation).await?;
        if real_docker {
            let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
            let image_hash = hex(&Sha256::digest(images[0].as_bytes()));
            let version_hash = hex(&Sha256::digest(selected.2.version().as_bytes()));
            let target = source.policy().installation.global_path(&[
                "cache",
                "scip",
                "images",
                &image_hash,
                "targets",
                &selected.1.to_string(),
                &version_hash,
            ])?;
            assert_eq!(
                fs::read_to_string(target.join("real-contract"))?
                    .lines()
                    .count(),
                1,
                "one real container indexes both mapped extensions"
            );
            assert!(!selected.0.root().join(".compiler-must-not-write").exists());
            assert!(
                !selected.0.root().join(".compiler-overlay-only").exists(),
                "writable workspace overlay must not modify the checkout"
            );
        } else {
            let argv = fs::read_to_string(docker.path().join("argv"))?;
            assert_eq!(
                argv.lines()
                    .filter(|line| *line == "example.test/custom-parser:one")
                    .count(),
                1,
                "extensions sharing an image run together exactly once"
            );
            assert!(
                !argv.contains("graphit-scip-rust:"),
                "custom route overrides the stock family"
            );
        }
        let rows =
            query("MATCH(n:AstFileObservation) RETURN DISTINCT n.path ORDER BY n.path").await?;
        let rows = serde_json::to_string(&rows)?;
        assert!(rows.contains("code.rs") && rows.contains("unknown.exm"));
        assert!(
            serde_json::to_string(
                &query("MATCH(n:Function) WHERE n.name='alpha' RETURN n.scip_symbols_json").await?
            )?
            .contains(COMPILER_ALPHA)
        );
        if real_docker {
            let delta = format!("{RUST}// incremental δ\n");
            fs::write(selected.0.root().join("unknown.exm"), &delta)?;
            fs::write(selected.0.root().join(".compiler-fixture-delta"), "delta")?;
            ast.index_productive(&selected.0, &context, ChangeBatch {
                paths:BTreeSet::from([selected.0.root().join("unknown.exm")]), ..Default::default()
            }, None, Some(BTreeSet::new()), false, &cancellation, &cancellation, &cancellation).await?;
            assert!(serde_json::to_string(&query("MATCH(n) WHERE n.path='unknown.exm' RETURN n.scip_symbols_json").await?)?.contains(COMPILER_ALPHA));
        }
        if real_docker {
            fs::write(
                selected.0.root().join(".compiler-fixture-fail"),
                "fixture failure",
            )?;
        }
        if let Some(client) = &mcp_client {
            for path in ["code.rs", "unknown.exm"] {
                let text = client.call_tool(CallToolRequestParams::new("graphit_ast_source").with_arguments(
                    json!({"project_dir":NativePath::new(selected.0.root().to_owned())?,
                        "project_version":selected.2.version(),"path":path}).as_object().unwrap().clone())).await?;
                assert_ne!(text.is_error, Some(true), "{text:?}");
                let expected = if path == "unknown.exm" { format!("{RUST}// incremental δ\n") } else { RUST.to_owned() };
                assert_eq!(text.content[0].as_text().ok_or("Source text content missing")?.text, expected,
                    "MCP Source uses original captured text after full/delta, including the extension without a profile");
            }
        }
        // Reuse the exact same Source. A changed Docker association must bypass
        // Raw's unchanged shortcut even without an explicit controls_changed flag.
        settings.set(
            ConfigKey::new("ast.scip.parsers")?,
            json!(format!("rs={},exm={}", images[1], images[1])),
            Duration::from_secs(5),
            &cancellation,
        )?;
        let result = ast.index_productive(&selected.0, &context, ChangeBatch {
            paths:BTreeSet::from([selected.0.root().join("code.rs"), selected.0.root().join("unknown.exm")]),
            ..Default::default()
        }, None, Some(BTreeSet::new()), false, &cancellation, &cancellation, &cancellation).await;
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Unsupported);
        let fallback =
            query("MATCH(n:Function) WHERE n.name='alpha' RETURN n.scip_symbols_json").await?;
        assert!(
            !serde_json::to_string(&fallback)?.contains(COMPILER_ALPHA),
            "obsolete compiler facts withdrawn; syntax remains live: {fallback}"
        );
        let missing =
            query("MATCH(n:AstFileObservation) WHERE n.path='unknown.exm' RETURN count(n)").await?;
        assert_eq!(
            missing["rows"][0][0]["value"], "0",
            "profile-free old producer is not served as current"
        );
        if !real_docker {
            assert!(
                fs::read_to_string(docker.path().join("argv"))?
                    .contains("example.test/custom-parser:two")
            );
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .catch_unwind()
    .await;
    let client_cleanup = match mcp_client {
        Some(client) => client
            .cancel()
            .await
            .map(|_| ())
            .map_err(std::io::Error::other),
        None => Ok(()),
    };
    let compiler_cleanup = ast.shutdown().await;
    let task_cleanup = tasks.shutdown().await;
    let source_cleanup = owner.shutdown().await;
    let server_cleanup = match mcp_server {
        Some(server) => server.await.map_err(std::io::Error::other)?,
        None => Ok(()),
    };
    drop(fixture);
    client_cleanup?;
    server_cleanup?;
    compiler_cleanup?;
    task_cleanup?;
    source_cleanup?;
    match outcome {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
