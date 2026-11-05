//! Real Native graph/raw publication consumed through the shipped Source MCP/CLI.
use super::*;
use crate::ast_source::{SourceArguments, call_source_result, run_source};
use arrow_array::{ArrayRef, RecordBatch, StringArray};
use graphit_ast::indexed_source::source_schema;
use graphit_data::lance_module_store::LanceModuleStore;

const MULTILINE_C: &str = "int\nmain(void){}\n";

pub(super) async fn publish_entity_payload(
    f: &source_fixture::Fixture,
    server: &GraphServer,
    version: &str,
    text: Option<&str>,
) -> std::path::PathBuf {
    let raw = graph_payload(f, version, "main", false);
    let graph = raw.join("graph.icebug");
    parquet(
        &graph.join("Function.parquet"),
        "message graph { required int64 id; required binary name (UTF8); required binary path (UTF8); required int64 line_number; optional int64 start_line; required int64 end_line; required boolean available; required binary extent_status (UTF8); }",
        &[
            Values::Ints(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]),
            Values::Strings(&[
                "hello",
                "dup",
                "dup",
                "HELLO",
                "ΑLPHA",
                "wrong",
                "reversed",
                "zero",
                "negative",
                "null",
                "blank",
                "main",
                "unavailable",
                "nonexact",
            ]),
            Values::Strings(&[
                "main.rs", "main.rs", "main.rs", "main.rs", "main.rs", "main.rs", "main.rs",
                "main.rs", "main.rs", "main.rs", "main.rs", "main.c", "main.rs", "main.rs",
            ]),
            // line_number points to the name, while start_line is the full declaration.
            Values::Ints(&[2, 2, 2, 5, 2, 9, 4, 1, 1, 1, 1, 2, 2, 2]),
            Values::NullableInts(
                &[2, 2, 2, 5, 2, 9, 4, 0, -1, 1, 1, 2, 2],
                &[1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1],
            ),
            Values::Ints(&[4, 4, 4, 5, 4, 10, 2, 2, 2, 2, 1, 2, 4, 4]),
            Values::Bools(&[
                true, true, true, true, true, true, true, true, true, true, true, true, false, true,
            ]),
            Values::Strings(&[
                "Exact", "Exact", "Exact", "Exact", "Exact", "Exact", "Exact", "Exact", "Exact",
                "Exact", "Exact", "Exact", "Exact", "Conflict",
            ]),
        ],
    );
    parquet(
        &graph.join("Method.parquet"),
        "message graph { required int64 id; required binary name (UTF8); required binary path (UTF8); required int64 line_number; required int64 start_line; required int64 end_line; required boolean available; required binary extent_status (UTF8); }",
        &[
            Values::Ints(&[1]),
            Values::Strings(&["hello"]),
            Values::Strings(&["main.rs"]),
            Values::Ints(&[2]),
            Values::Ints(&[2]),
            Values::Ints(&[4]),
            Values::Bools(&[true]),
            Values::Strings(&["Exact"]),
        ],
    );
    parquet(
        &graph.join("Broken.parquet"),
        "message graph { required int64 id; required binary name (UTF8); required binary path (UTF8); required int64 line_number; required int64 start_line; required binary end_line (UTF8); required boolean available; required binary extent_status (UTF8); }",
        &[
            Values::Ints(&[1]),
            Values::Strings(&["broken"]),
            Values::Strings(&["main.rs"]),
            Values::Ints(&[2]),
            Values::Ints(&[2]),
            Values::Strings(&["4"]),
            Values::Bools(&[true]),
            Values::Strings(&["Exact"]),
        ],
    );
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(graph.join("icebug.json")).unwrap()).unwrap();
    let columns = json!([
        {"name":"id", "type":"INT64"}, {"name":"name", "type":"STRING"},
        {"name":"path", "type":"STRING"}, {"name":"line_number", "type":"INT64"},
        {"name":"start_line", "type":"INT64"}, {"name":"end_line", "type":"INT64"},
        {"name":"available", "type":"BOOL"}, {"name":"extent_status", "type":"STRING"}
    ]);
    manifest["node_tables"][1]["rows"] = json!(14);
    manifest["node_tables"][1]["shards"][0]["rows"] = json!(14);
    manifest["relationships"][0]["members"][0]["target_rows"] = json!(14);
    manifest["node_tables"][1]["columns"] = columns.clone();
    manifest["node_tables"].as_array_mut().unwrap().push(json!({
        "label":"Method", "shards":[{"file":"Method.parquet","row_base":0,"rows":1}], "rows":1,
        "primary_key":"id", "columns":columns, "lang_counts":[]
    }));
    manifest["node_tables"].as_array_mut().unwrap().push(json!({
        "label":"Broken", "shards":[{"file":"Broken.parquet","row_base":0,"rows":1}], "rows":1,
        "primary_key":"id", "columns":[
            {"name":"id", "type":"INT64"}, {"name":"name", "type":"STRING"},
            {"name":"path", "type":"STRING"}, {"name":"line_number", "type":"INT64"},
            {"name":"start_line", "type":"INT64"}, {"name":"end_line", "type":"STRING"},
            {"name":"available", "type":"BOOL"}, {"name":"extent_status", "type":"STRING"}
        ], "lang_counts":[]
    }));
    fs::write(
        graph.join("icebug.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec!["main.rs", "main.c"])) as ArrayRef,
            Arc::new(StringArray::from(vec![text, Some(MULTILINE_C)])) as ArrayRef,
        ],
    )
    .unwrap();
    LanceModuleStore::create(
        graph.join("source.lance").to_str().unwrap(),
        source_schema(),
        vec![batch],
    )
    .await
    .unwrap();
    publish_graph_payload(f, server, version, &raw)
}

fn arguments(version: &str) -> SourceArguments {
    serde_json::from_value(json!({
        "path":"main.rs", "context":format!("actual-graph@{version}"),
        "entity":"hello", "entity_type":"Function"
    }))
    .unwrap()
}
async fn source_socket(
    f: &source_fixture::Fixture,
) -> (
    std::net::SocketAddr,
    tokio::task::JoinHandle<io::Result<()>>,
) {
    let listener = tcp_listener::reserve(
        Options {
            address: "127.0.0.1".parse().unwrap(),
            ports: PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let address = listener.address();
    let server = f.server.clone();
    (
        address,
        tokio::spawn(async move { server.serve(listener).await }),
    )
}
fn text(result: rmcp::model::CallToolResult) -> String {
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result
        .content
        .iter()
        .map(|c| c.as_text().unwrap().text.as_str())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_entity_source_mcp_cli_windows_types_and_raw_binding() {
    let (f, graph) = fixture(true);
    let first = "// header\npub fn hello() {\n    let value = \"α\";\n}\n// exact upper\n// tail\n";
    publish_entity_payload(&f, &graph, "entity-1", Some(first)).await;
    publish_entity_payload(
        &f,
        &graph,
        "entity-2",
        Some("// header\npub fn hello() {\n    let newer = 2;\n}\n// exact upper\n// tail\n"),
    )
    .await;
    publish_entity_payload(&f, &graph, "entity-null", None).await;
    publish_entity_payload(&f, &graph, "entity-empty", Some("")).await;
    let (address, served) = source_socket(&f).await;
    let client = source_client();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        content(),
        &cancel,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap();
    let limits = graph.0.policy.index;
    let mut args = arguments("entity-1");
    let result = call_source_result(&f.paths, address, &client, &args, limits, &original)
        .await
        .unwrap();
    assert_eq!(text(result), "pub fn hello() {\n    let value = \"α\";\n}");
    let c_arguments: SourceArguments = serde_json::from_value(json!({
        "path":"main.c", "context":"actual-graph@entity-1", "entity":"main",
        "entity_type":"Function", "line_numbers":true
    }))
    .unwrap();
    assert_eq!(
        text(
            call_source_result(&f.paths, address, &client, &c_arguments, limits, &original)
                .await
                .unwrap()
        ),
        "   1: int\n   2: main(void){}",
        "the declaration starts before the name's line_number"
    );
    let mut c_output = Vec::new();
    run_source(
        &f.paths,
        address,
        &client,
        &c_arguments,
        limits,
        &original,
        &mut c_output,
    )
    .await
    .unwrap();
    assert_eq!(c_output, b"   1: int\n   2: main(void){}");
    args.source.head = Some(2);
    args.source.tail = Some(1);
    args.source.line_numbers = true;
    let mut output = Vec::new();
    run_source(
        &f.paths,
        address,
        &client,
        &args,
        limits,
        &original,
        &mut output,
    )
    .await
    .unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "   3:     let value = \"α\";"
    );
    args = arguments("entity-1");
    args.source.start_line = Some(2);
    args.source.end_line = Some(2);
    args.source.line_numbers = true;
    assert_eq!(
        text(
            call_source_result(&f.paths, address, &client, &args, limits, &original)
                .await
                .unwrap()
        ),
        "   3:     let value = \"α\";"
    );
    args = arguments("entity-1");
    args.source.pattern = Some("let.*α".into());
    args.source.regex = true;
    let matched = text(
        call_source_result(&f.paths, address, &client, &args, limits, &original)
            .await
            .unwrap(),
    );
    assert!(matched.contains("α"));
    assert!(!matched.contains("// header"));
    args = arguments("entity-2");
    args.source.pattern = Some("newer".into());
    args.source.before = Some(1);
    args.source.after = Some(1);
    let value = text(
        call_source_result(&f.paths, address, &client, &args, limits, &original)
            .await
            .unwrap(),
    );
    assert!(value.contains("newer = 2"));
    assert!(!value.contains("value ="));
    assert!(!value.contains("// tail"));
    args = arguments("entity-1");
    args.source.entity = Some("HELLO".into());
    assert_eq!(
        text(
            call_source_result(&f.paths, address, &client, &args, limits, &original)
                .await
                .unwrap()
        ),
        "// exact upper"
    );
    args.source.entity = Some("HeLLo".into());
    let result = call_source_result(&f.paths, address, &client, &args, limits, &original)
        .await
        .unwrap();
    assert_eq!(
        result.is_error,
        Some(true),
        "folded two candidates must be ambiguous"
    );
    args.source.entity = Some("αlpha".into());
    assert_eq!(
        text(
            call_source_result(&f.paths, address, &client, &args, limits, &original)
                .await
                .unwrap()
        ),
        "pub fn hello() {\n    let value = \"α\";\n}"
    );
    for (version, name, kind) in [
        ("entity-1", "dup", Some("Function")),
        ("entity-1", "hello", None),
        ("entity-1", "absent", Some("Function")),
        ("entity-1", "hello", Some("AbsentType")),
        ("entity-1", "wrong", Some("Function")),
        ("entity-1", "reversed", Some("Function")),
        ("entity-1", "zero", Some("Function")),
        ("entity-1", "negative", Some("Function")),
        ("entity-1", "null", Some("Function")),
        ("entity-1", "unavailable", Some("Function")),
        ("entity-1", "nonexact", Some("Function")),
        ("entity-1", "broken", Some("Broken")),
        ("entity-1", "hello' OR true", Some("Function")),
        ("entity-null", "hello", Some("Function")),
        ("entity-empty", "hello", Some("Function")),
    ] {
        args = arguments(version);
        args.source.entity = Some(name.into());
        args.source.entity_type = kind.map(str::to_owned);
        let result = call_source_result(&f.paths, address, &client, &args, limits, &original)
            .await
            .unwrap();
        assert_eq!(
            result.is_error,
            Some(true),
            "{version}/{name}/{kind:?}: {result:?}"
        );
    }
    args = arguments("entity-empty");
    args.source.entity = Some("blank".into());
    assert_eq!(
        text(
            call_source_result(&f.paths, address, &client, &args, limits, &original)
                .await
                .unwrap()
        ),
        ""
    );
    args = arguments("entity-1");
    args.source.entity = None;
    args.source.entity_type = None;
    assert_eq!(
        text(
            call_source_result(&f.paths, address, &client, &args, limits, &original)
                .await
                .unwrap()
        ),
        first
    );
    f.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    graph.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_entity_source_same_original_auth_and_origin_guards() {
    for phase in [
        "cancel",
        "expired",
        "graph_changed",
        "root_changed",
        "auth_changed",
        "local",
        "denied",
    ] {
        let (f, server) = fixture(phase != "denied");
        let installed = publish_entity_payload(
            &f,
            &server,
            phase,
            Some("// header\npub fn hello() {\n    let value = 1;\n}\n"),
        )
        .await;
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(async {
                use graphit_ast::imported_graph::{
                    ImportedGraphInput, open_imported_graph_local, open_imported_graph_runtime,
                };
                let cancel = Cancellation::default();
                let deadline =
                    Instant::now() + Duration::from_secs(if phase == "expired" { 2 } else { 30 });
                let original = RequestContext::new(content(), &cancel, deadline).unwrap();
                if phase == "local" {
                    publish_local(&f.auth, &f.paths, &original);
                }
                let projection = server.0.policy.registry.limits().projection();
                let id = ArtifactId::new("actual-graph", projection).unwrap();
                let version = ConcreteVersion::new(phase, projection).unwrap();
                let input = ImportedGraphInput {
                    registry: &server.0.policy.registry,
                    id: &id,
                    version: &version,
                    installation: f.auth.installation(),
                    file_policy: &server.0.policy.file,
                    read_limits: server.0.policy.read,
                    graph_policy: server.0.policy.native_install.graph,
                    directory_limits: server.0.policy.index,
                    request: &original,
                };
                if phase == "denied" {
                    fs::write(
                        installed.join("graph.icebug/icebug.json"),
                        "malformed graph that must not be read",
                    )
                    .unwrap();
                    let denied = open_imported_graph_runtime(f.auth.issuer(), &f.raw, input);
                    assert!(matches!(
                        denied,
                        Err(graphit_ast::imported_graph::ImportedGraphError::Authentication(_))
                    ));
                    f.server.shutdown().await.unwrap();
                    server.shutdown().await.unwrap();
                    return;
                }
                let graph = if phase == "local" {
                    open_imported_graph_local(f.auth.issuer(), "test-local-key", input).unwrap()
                } else {
                    open_imported_graph_runtime(f.auth.issuer(), &f.raw, input).unwrap()
                };
                match phase {
                    "cancel" => cancel.cancel(),
                    "expired" => std::thread::sleep(
                        deadline.saturating_duration_since(Instant::now())
                            + Duration::from_millis(5),
                    ),
                    "graph_changed" => {
                        fs::rename(installed.join("graph.icebug"), installed.join("old-graph"))
                            .unwrap();
                        fs::create_dir(installed.join("graph.icebug")).unwrap();
                    }
                    "root_changed" => {
                        fs::rename(&installed, installed.with_extension("old")).unwrap();
                        fs::create_dir(&installed).unwrap();
                    }
                    "auth_changed" => {
                        let removed = f.auth.remove_runtime(original.snapshot());
                        assert!(removed.failure().is_none());
                    }
                    "local" => (),
                    _ => unreachable!(),
                }
                let result = graph
                    .entity_source(
                        &runtime,
                        "main.rs",
                        "hello",
                        Some("Function"),
                        &graphit_core::text_slice::Request::default(),
                        server.0.policy.graph_query,
                    )
                    .await;
                if phase == "local" {
                    let inspected = result.unwrap();
                    assert_eq!(
                        inspected.origin,
                        graphit_ast::source_inspection::SourceOrigin::Indexed
                    );
                    assert_eq!(
                        inspected.selection.source,
                        "pub fn hello() {\n    let value = 1;\n}"
                    );
                    assert_eq!(inspected.selection.total_lines, 5);
                    assert_eq!(inspected.selection.start_line, Some(2));
                    assert_eq!(inspected.selection.end_line, Some(4));
                } else {
                    let error = result.unwrap_err();
                    if matches!(phase, "cancel" | "expired") {
                        let expected = if phase == "cancel" {
                            io::ErrorKind::Interrupted
                        } else {
                            io::ErrorKind::TimedOut
                        };
                        let mut cause: &(dyn std::error::Error + 'static) = &error;
                        loop {
                            if let Some(io) = cause.downcast_ref::<io::Error>() {
                                assert_eq!(io.kind(), expected);
                                break;
                            }
                            cause = cause
                                .source()
                                .expect("actual original I/O cause must be retained");
                        }
                    }
                }
                drop(graph);
                f.server.shutdown().await.unwrap();
                server.shutdown().await.unwrap();
            });
        })
        .await
        .unwrap();
    }
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_entity_source_mcp_and_cli_consume_live_graph_and_bound_raw() {
    let (f, graph) = super::web_source_fixture(true).await;
    let (address, served) = source_socket(&f).await;
    let client = source_client();
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        content(),
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let mut args: SourceArguments =
        serde_json::from_value(json!({"path":"main.rs","entity":"hello","entity_type":"Function"}))
            .unwrap();
    let result = call_source_result(
        &f.paths,
        address,
        &client,
        &args,
        graph.0.policy.index,
        &original,
    )
    .await
    .unwrap();
    assert_eq!(text(result), "fn hello() {\n  work();\n}");
    args.source.start_line = Some(2);
    args.source.end_line = Some(2);
    args.source.line_numbers = true;
    let mut output = Vec::new();
    run_source(
        &f.paths,
        address,
        &client,
        &args,
        graph.0.policy.index,
        &original,
        &mut output,
    )
    .await
    .unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "   3:   work();");
    args.source.entity = Some("duplicate".into());
    let result = call_source_result(
        &f.paths,
        address,
        &client,
        &args,
        graph.0.policy.index,
        &original,
    )
    .await
    .unwrap();
    assert_eq!(result.is_error, Some(true));
    let (paths, _, _) = &f.projects[0];
    let index = super::live_project_graph(&f, &graph, original.snapshot()).await;
    let (_, id, version) = &f.projects[0];
    let scope =
        CapturedPublicationScope::project(paths, *id, scopes(), original.snapshot()).unwrap();
    let binding = scope.binding().unwrap();
    let (directory, _) = crate::foreground_ast_sources::progressive::location(
        f.auth.installation(),
        graphit_auth::ProjectCapture {
            scope: ScopeCapture {
                live: &scope,
                record: &binding,
            },
            version,
        },
    )
    .unwrap();
    drop(index);
    std::fs::remove_dir_all(directory).unwrap();
    graphit_core::config::ConfigStore::project(paths)
        .unwrap()
        .set(
            graphit_core::config::ConfigKey::new("modules.ast").unwrap(),
            json!(false),
            Duration::from_secs(5),
            original.cancellation(),
        )
        .unwrap();
    std::fs::write(
        paths.root().join("arbitrary.unsupported"),
        "disk fallback\n",
    )
    .unwrap();
    args.source = serde_json::from_value(json!({"path":"arbitrary.unsupported","head":1})).unwrap();
    let result = call_source_result(
        &f.paths,
        address,
        &client,
        &args,
        graph.0.policy.index,
        &original,
    )
    .await
    .unwrap();
    assert_eq!(text(result), "disk fallback");
    f.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    graph.shutdown().await.unwrap();
}
