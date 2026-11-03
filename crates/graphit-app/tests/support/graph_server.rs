use super::*;
use crate::native_install_http::NativeInstallPolicy;
use graphit_auth::{
    Permission, ScopeCapture,
    profile::{
        AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
        ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, CopyLimits, CopyPolicy,
        RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotPolicy, SourceReadLimits,
        StoreOwner,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    config::ConfigSnapshot,
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    native_path::NativePath,
    project_paths::InstallationPaths,
    tcp_listener::{self, Options, PortSelection},
};
use graphit_hub::{
    ArtifactType, InstallationTarget, NativeBinding, SourceRef,
    native_package::{self, PackageKind},
    registry::{HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload},
};
use parquet_graph_fixture::{
    basic::Compression,
    data_type::{BoolType, ByteArray, ByteArrayType, Int64Type},
    file::{metadata::KeyValue, properties::WriterProperties, writer::SerializedFileWriter},
    schema::parser::parse_message_type,
};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

use crate::source_server::test_fixture as source_fixture;

#[cfg(all(test, target_os = "linux"))]
pub(crate) fn gate_next_native_job(
    graph: &GraphServer,
) -> (
    std::sync::mpsc::Receiver<Cancellation>,
    std::sync::mpsc::Sender<()>,
) {
    let (started, observed) = std::sync::mpsc::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    let previous = graph.0.gate.lock().unwrap().replace((started, waiting));
    assert!(previous.is_none(), "one native job gate at a time");
    (observed, release)
}

fn scopes() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 128).unwrap()
}
fn documents() -> AuthDocumentLimits {
    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn profiles() -> graphit_auth::profile::ProfileLimits {
    graphit_auth::profile::ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn fixture(read_permission: bool) -> (source_fixture::Fixture, GraphServer) {
    let permissions = if read_permission {
        vec![Permission::Read(Module::Ast)]
    } else {
        vec![]
    };
    fixture_with_permissions(&permissions, &[])
}
fn fixture_with_permissions(
    permissions: &[Permission],
    administrative: &[Permission],
) -> (source_fixture::Fixture, GraphServer) {
    let fixture = source_fixture::fixture_with_projects_and_authority(
        permissions,
        administrative,
        Brand::default(),
        NativeBinding::KiroV3,
        &["project"],
        &[],
    );
    fixture_from(fixture)
}
fn fixture_from(fixture: source_fixture::Fixture) -> (source_fixture::Fixture, GraphServer) {
    let retained = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
    let config = ConfigSnapshot::new(fixture.paths.brand(), []).unwrap();
    let cancel = Cancellation::default();
    let request =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let StoreOwner::Installation { installation_id } = fixture.auth.installation().live.owner()
    else {
        panic!("installation")
    };
    let policy = SourcePolicy {
        graph_query: graphit_ast::icebug_query::QueryOptions {
            buffer_pool_bytes: 128 * 1024 * 1024,
            query_threads: 2,
            max_rows: (content()).max_collection_entries,
            max_bytes: (content()).max_output_bytes,
            max_depth: (content()).max_depth,
        },
        installation: fixture.paths.clone(),
        native_install: NativeInstallPolicy::capture(&config, request.snapshot()).unwrap(),
        content: content(),
        timeout: Duration::from_secs(30),
        scope: scopes(),
        file: SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            CopyLimits::new(1048576, 128, 65536, 8).unwrap(),
            RetainedFileSet::new([], retained).unwrap(),
            retained,
        )
        .unwrap(),
        read: SourceReadLimits::new(65536, 4096, 1048576).unwrap(),
        path: retained,
        index: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
        binding: NativeBinding::KiroV3,
        registry: HubRegistry::global(
            &fixture.paths,
            installation_id,
            graphit_hub::policy::HubResourcePolicy::resolve(&config)
                .unwrap()
                .hub(),
        )
        .unwrap(),
    };
    let server =
        GraphServer::new(Arc::clone(&fixture.auth), policy, CancellationToken::new()).unwrap();
    (fixture, server)
}
fn content() -> graphit_hub::native::ContentLimits {
    graphit_hub::native::ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    }
}
enum Values<'a> {
    Bools(&'a [bool]),
    Strings(&'a [&'a str]),
    Ints(&'a [i64]),
    NullableInts(&'a [i64], &'a [i16]),
}
fn parquet(path: &Path, schema: &str, values: &[Values<'_>]) {
    let properties = Arc::new(
        WriterProperties::builder()
            .set_dictionary_enabled(false)
            .set_compression(Compression::ZSTD(Default::default()))
            .set_key_value_metadata(Some(vec![KeyValue::new(
                "icebug_disk_version".into(),
                Some("v1".into()),
            )]))
            .build(),
    );
    let mut writer = SerializedFileWriter::new(
        fs::File::create(path).unwrap(),
        Arc::new(parse_message_type(schema).unwrap()),
        properties,
    )
    .unwrap();
    let mut group = writer.next_row_group().unwrap();
    for values in values {
        let mut column = group.next_column().unwrap().unwrap();
        match values {
            Values::Bools(v) => {
                column
                    .typed::<BoolType>()
                    .write_batch(v, None, None)
                    .unwrap();
            }
            Values::Strings(v) => {
                column
                    .typed::<ByteArrayType>()
                    .write_batch(
                        &v.iter().map(|s| ByteArray::from(*s)).collect::<Vec<_>>(),
                        None,
                        None,
                    )
                    .unwrap();
            }
            Values::Ints(v) => {
                column
                    .typed::<Int64Type>()
                    .write_batch(v, None, None)
                    .unwrap();
            }
            Values::NullableInts(v, definitions) => {
                column
                    .typed::<Int64Type>()
                    .write_batch(v, Some(definitions), None)
                    .unwrap();
            }
        }
        column.close().unwrap();
    }
    group.close().unwrap();
    writer.close().unwrap();
}
fn install(
    f: &source_fixture::Fixture,
    s: &GraphServer,
    version: &str,
    name: &str,
) -> std::path::PathBuf {
    install_with_members(f, s, version, name, false)
}
fn install_with_members(
    f: &source_fixture::Fixture,
    s: &GraphServer,
    version: &str,
    name: &str,
    query_members: bool,
) -> std::path::PathBuf {
    let raw = graph_payload(f, version, name, query_members);
    publish_graph_payload(f, s, version, &raw)
}
fn graph_payload(
    f: &source_fixture::Fixture,
    version: &str,
    name: &str,
    query_members: bool,
) -> std::path::PathBuf {
    let raw = f.temp.path().join(format!("raw-{version}"));
    let graph = raw.join("graph.icebug");
    fs::create_dir_all(&graph).unwrap();
    fs::write(
        graph.join("schema.cypher"),
        "CREATE NODE TABLE File(path STRING, PRIMARY KEY(path));",
    )
    .unwrap();
    parquet(
        &graph.join("File.parquet"),
        "message graph { required binary path (UTF8); required binary name (UTF8); }",
        &[Values::Strings(&["main.rs"]), Values::Strings(&[name])],
    );
    parquet(
        &graph.join("Function.parquet"),
        "message graph { required int64 id; required binary name (UTF8); }",
        &[
            Values::Ints(&[9007199254740993, 2]),
            Values::Strings(&["α", "β"]),
        ],
    );
    parquet(
        &graph.join("a.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); required int64 weight; }",
        &[
            Values::Ints(&[0, 0, 1]),
            Values::Strings(&["u0", "u1", "u2"]),
            Values::Ints(&[9007199254740993, 2, 3]),
        ],
    );
    parquet(
        &graph.join("a.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Ints(&[0, 3])],
    );
    let mut manifest = json!({"manifest_version":3,"format":"icebug-canonical","storage":"direct-parquet","finished":true,"add_reverse_edges":false,"schema_file":"schema.cypher",
        "node_tables":[{"label":"File","rows":1,"shards":[{"file":"File.parquet","row_base":0,"rows":1}],"primary_key":"path","columns":[{"name":"path","type":"STRING"},{"name":"name","type":"STRING"}],"lang_counts":[]},{"label":"Function","rows":2,"shards":[{"file":"Function.parquet","row_base":0,"rows":2}],"primary_key":"id","columns":[{"name":"id","type":"INT64"},{"name":"name","type":"STRING"}],"lang_counts":[]}],
        "relationships":[{"type":"CONTAINS","members":[{"from":"File","to":"Function","table":"a","indices":"a.indices","indptr":"a.indptr","rows":3,"source_row_base":0,"source_rows":1,"target_row_base":0,"target_rows":2}]}],
        "relation_uids":true,"generation":version,"n_edges":3,"repaired_strings":0,"invariants":{"indptr_row_groups":1,"self_loops":"forward-once"}});
    if query_members {
        parquet(
            &graph.join("b.indices"),
            "message graph { required int64 target (UINT_64); required binary uid (UTF8); required int64 weight; }",
            &[
                Values::Ints(&[1]),
                Values::Strings(&["u3"]),
                Values::Ints(&[4]),
            ],
        );
        parquet(
            &graph.join("b.indptr"),
            "message graph { required int64 ptr (UINT_64); }",
            &[Values::Ints(&[0, 1])],
        );
        parquet(
            &graph.join("c.indices"),
            "message graph { required int64 target (UINT_64); required binary uid (UTF8); required int64 weight; }",
            &[
                Values::Ints(&[0, 0]),
                Values::Strings(&["u4", "u5"]),
                Values::Ints(&[5, 6]),
            ],
        );
        parquet(
            &graph.join("c.indptr"),
            "message graph { required int64 ptr (UINT_64); }",
            &[Values::Ints(&[0, 1, 2])],
        );
        manifest["relationships"][0]["members"].as_array_mut().unwrap().extend([
            json!({"from":"File","to":"Function","table":"b","indices":"b.indices","indptr":"b.indptr","rows":1,"source_row_base":0,"source_rows":1,"target_row_base":0,"target_rows":2}),
            json!({"from":"Function","to":"File","table":"c","indices":"c.indices","indptr":"c.indptr","rows":2,"source_row_base":0,"source_rows":2,"target_row_base":0,"target_rows":1}),
        ]);
        manifest["n_edges"] = json!(6);
    }
    fs::write(
        graph.join("icebug.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    raw
}
fn publish_graph_payload(
    f: &source_fixture::Fixture,
    s: &GraphServer,
    version: &str,
    raw: &Path,
) -> std::path::PathBuf {
    let cancel = Cancellation::default();
    let request =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let snapshot = &s.0.policy.file;
    let read = s.0.policy.read;
    let copy = CopyLimits::new(1048576, 128, 65536, 8).unwrap();
    let captured = CapturedSourceReader::capture(
        raw,
        graphit_fs::NodeKind::Directory,
        snapshot,
        read,
        Arc::clone(request.snapshot()),
    )
    .unwrap();
    let archive = f.temp.path().join(format!("{version}.zip"));
    native_package::export(&captured, PackageKind::Ast, &archive, request.snapshot()).unwrap();
    let zip = CapturedSourceReader::capture(
        &archive,
        graphit_fs::NodeKind::File,
        snapshot,
        read,
        Arc::clone(request.snapshot()),
    )
    .unwrap();
    let installed = f.temp.path().join(format!("installed-{version}"));
    native_package::stage_import(
        &zip,
        Some(PackageKind::Ast),
        &installed,
        copy,
        request.snapshot(),
    )
    .unwrap()
    .publish()
    .unwrap();
    assert!(!installed.join("package.json").exists());
    assert!(!installed.join("native.json").exists());
    let registry = &s.0.policy.registry;
    let projection = registry.limits().projection();
    let source = SourceRef::local_link(
        NativePath::new(installed.clone()).unwrap(),
        f.projects[0].1,
        ArtifactType::Ast,
        ArtifactId::new("actual-graph", projection).unwrap(),
        ConcreteVersion::new(version, projection).unwrap(),
        projection,
    )
    .unwrap();
    registry
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: ArtifactType::Ast,
            id: source.id().clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: source,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })
        .unwrap();
    installed
}
fn headers(raw: &str, local: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    bearer.set_sensitive(true);
    headers.insert(header::AUTHORIZATION, bearer);
    headers.insert(
        "x-graphit-auth-origin",
        axum::http::HeaderValue::from_static(if local { "local" } else { "runtime" }),
    );
    headers
}
async fn socket(server: &GraphServer) -> (String, tokio::task::JoinHandle<io::Result<()>>) {
    let listener = tcp_listener::reserve(
        Options {
            address: "127.0.0.1".parse().unwrap(),
            ports: PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let url = format!("http://{}", listener.address());
    let owner = server.clone();
    let handle = tokio::spawn(async move { owner.serve(listener).await });
    (url, handle)
}
#[tokio::test]
async fn installed_two_versions_real_packages_schema_sample_properties_and_cursor() {
    let (f, s) = fixture(true);
    install(&f, &s, "release-1", "primeira");
    install(&f, &s, "release-2", "segunda");
    let (url, served) = socket(&s).await;
    let client = reqwest::Client::new();
    for (version, name) in [("release-1", "primeira"), ("release-2", "segunda")] {
        let selection = json!({"id":"actual-graph","version":version});
        let schema = client
            .post(format!("{url}/api/graphs/schema"))
            .headers(headers(&f.raw, false))
            .json(&selection)
            .send()
            .await
            .unwrap();
        assert_eq!(schema.status(), StatusCode::OK);
        let schema: Value = schema.json().await.unwrap();
        assert_eq!(schema["nodes"][0]["primary_key"], "path");
        assert_eq!(schema["nodes"][1]["columns"][0], json!(["id", "INT64"]));
        assert_eq!(schema["relationships"][0]["rows"], "3");
        let sample = client
            .post(format!("{url}/api/graphs/sample"))
            .headers(headers(&f.raw, false))
            .json(&selection)
            .send()
            .await
            .unwrap();
        assert_eq!(sample.status(), StatusCode::OK);
        let sample: Value = sample.json().await.unwrap();
        assert_eq!(sample["nodes"][0]["name"], name);
        assert_eq!(sample["edges"].as_array().unwrap().len(), 3);
        assert_eq!(sample["generation"], schema["generation"]);
    }
    let mut input = json!({"id":"actual-graph","version":"release-1","anchor_label":"File","anchor_key":{"type":"string","value":"main.rs"},"relationship":"CONTAINS","other_label":"Function","direction":"outgoing","limit":1});
    let page = client
        .post(format!("{url}/api/graphs/neighborhood"))
        .headers(headers(&f.raw, false))
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let page: Value = page.json().await.unwrap();
    assert_eq!(page["edges"][0]["uid"], "u0");
    assert!(
        page["edges"][0]["properties"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "weight"
                && p["value"] == json!({"type":"int64","value":"9007199254740993"}))
    );
    input["cursor"] = page["next_cursor"].clone();
    let next: Value = client
        .post(format!("{url}/api/graphs/neighborhood"))
        .headers(headers(&f.raw, false))
        .json(&input)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(next["edges"][0]["uid"], "u1");
    input["version"] = json!("release-2");
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/neighborhood"))
            .headers(headers(&f.raw, false))
            .json(&input)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    input["version"] = json!("release-1");
    input["cursor"] = Value::Null;
    input["anchor_label"] = json!("Function");
    input["other_label"] = json!("File");
    input["anchor_key"] = json!({"type":"int64","value":"9007199254740993"});
    input["direction"] = json!("incoming");
    input["limit"] = json!(4);
    let incoming = client
        .post(format!("{url}/api/graphs/neighborhood"))
        .headers(headers(&f.raw, false))
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(incoming.status(), StatusCode::OK);
    assert_eq!(
        incoming.json::<Value>().await.unwrap()["edges"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    s.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn native_socket_auth_missing_versions_corrupt_schema_and_real_shared_route() {
    let (f, s) = fixture(true);
    let root = install(&f, &s, "release-1", "actual");
    let (url, served) = socket(&s).await;
    let client = reqwest::Client::new();
    let input = json!({"id":"actual-graph","version":"release-1"});
    for raw in ["bad", ""] {
        let response = client
            .post(format!("{url}/api/graphs/schema"))
            .headers(headers(raw, false))
            .json(&input)
            .send()
            .await
            .unwrap();
        assert!(matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED
        ));
    }
    let missing = json!({"id":"actual-graph","version":"missing"});
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/schema"))
            .headers(headers(&f.raw, false))
            .json(&missing)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    fs::remove_file(root.join("graph.icebug/schema.cypher")).unwrap();
    fs::create_dir(root.join("graph.icebug/schema.cypher")).unwrap();
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/schema"))
            .headers(headers(&f.raw, false))
            .json(&input)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    s.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    // Same production Source composition must expose the graph route, not only isolated Router.
    let listener = tcp_listener::reserve(
        Options {
            address: "127.0.0.1".parse().unwrap(),
            ports: PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let url = format!("http://{}", listener.address());
    let source = f.server.clone();
    let task = tokio::spawn(async move { source.serve(listener).await });
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/schema"))
            .headers(headers(&f.raw, false))
            .json(&missing)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    f.server.shutdown().await.unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn denied_read_before_unavailable_origin_and_captured_change_and_original_cancel() {
    let (f, s) = fixture(false);
    let root = install(&f, &s, "release-1", "denied");
    fs::remove_dir_all(root).unwrap();
    let cancel = Cancellation::default();
    let result = read(
        &s.0,
        &f.raw,
        false,
        &serde_json::to_vec(&json!({"id":"actual-graph","version":"release-1"})).unwrap(),
        Operation::Schema,
        &cancel,
        Instant::now() + Duration::from_secs(5),
    );
    assert!(matches!(
        result,
        Err(ReadFailure::Imported(ImportedGraphError::Authentication(_)))
    ));
    let (f, s) = fixture(true);
    let root = install(&f, &s, "release-1", "actual");
    let cancel = Cancellation::default();
    let request =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let projection = s.0.policy.registry.limits().projection();
    let id = ArtifactId::new("actual-graph", projection).unwrap();
    let version = ConcreteVersion::new("release-1", projection).unwrap();
    let graph = open_imported_graph_runtime(
        f.auth.issuer(),
        &f.raw,
        ImportedGraphInput {
            registry: &s.0.policy.registry,
            id: &id,
            version: &version,
            installation: f.auth.installation(),
            file_policy: &s.0.policy.file,
            read_limits: s.0.policy.read,
            graph_policy: s.0.policy.native_install.graph,
            directory_limits: s.0.policy.index,
            request: &request,
        },
    )
    .unwrap();
    assert!(graph.schema().is_ok());
    fs::write(root.join("graph.icebug/icebug.json"), "{}").unwrap();
    assert!(graph.schema().is_err());
    cancel.cancel();
    assert!(graph.generation().is_err());
}

#[tokio::test]
async fn local_key_admission_is_real_and_rechecked_before_delivery() {
    let (f, s) = fixture(true);
    install(&f, &s, "release-1", "local");
    let cancel = Cancellation::default();
    let request =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    publish_local(&f.auth, &f.paths, &request);
    let (url, served) = socket(&s).await;
    let input = json!({"id":"actual-graph","version":"release-1"});
    let response = reqwest::Client::new()
        .post(format!("{url}/api/graphs/sample"))
        .headers(headers("test-local-key", true))
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["nodes"][0]["name"],
        "local"
    );
    assert_eq!(
        reqwest::Client::new()
            .post(format!("{url}/api/graphs/sample"))
            .headers(headers("test-local-key", false))
            .json(&input)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    s.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn dropped_handler_cancels_only_its_job_and_shutdown_waiter_keeps_join_owned() {
    let (f, s) = fixture(true);
    install(&f, &s, "release-1", "sibling");
    let (started, observed) = std::sync::mpsc::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    *s.0.gate.lock().unwrap() = Some((started, waiting));
    let input = Bytes::from(
        serde_json::to_vec(&json!({"id":"actual-graph","version":"release-1"})).unwrap(),
    );
    let request = s.clone();
    let credential = f.raw.clone();
    let pending_input = input.clone();
    let pending = tokio::spawn(async move {
        request
            .read(
                headers(&credential, false),
                pending_input,
                Operation::Sample,
            )
            .await
    });
    let cancellation =
        tokio::task::spawn_blocking(move || observed.recv_timeout(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert!(cancellation.is_cancelled());
    let sibling = s
        .read(headers(&f.raw, false), input, Operation::Sample)
        .await;
    assert_eq!(sibling.status(), StatusCode::OK);
    let closing = s.clone();
    let shutdown = tokio::spawn(async move { closing.shutdown().await });
    tokio::task::yield_now().await;
    assert!(!shutdown.is_finished());
    shutdown.abort();
    assert!(shutdown.await.unwrap_err().is_cancelled());
    let closing = s.clone();
    let shutdown = tokio::spawn(async move { closing.shutdown().await });
    tokio::task::yield_now().await;
    assert!(!shutdown.is_finished());
    release.send(()).unwrap();
    shutdown.await.unwrap().unwrap();
    assert!(s.0.jobs.lock().await.is_empty());
}

#[tokio::test]
async fn original_deadline_expiry_cancels_queued_native_work_without_payload() {
    let (f, s) = fixture(true);
    install(&f, &s, "release-1", "actual");
    let mut policy = s.0.policy.clone();
    policy.timeout = Duration::from_millis(100);
    let server = GraphServer::new(Arc::clone(&f.auth), policy, CancellationToken::new()).unwrap();
    let (started, observed) = std::sync::mpsc::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    *server.0.gate.lock().unwrap() = Some((started, waiting));
    let owner = server.clone();
    let raw = f.raw.clone();
    let pending = tokio::spawn(async move {
        owner
            .read(
                headers(&raw, false),
                Bytes::from(
                    serde_json::to_vec(&json!({"id":"actual-graph","version":"release-1"}))
                        .unwrap(),
                ),
                Operation::Sample,
            )
            .await
    });
    let cancelled =
        tokio::task::spawn_blocking(move || observed.recv_timeout(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
    assert_eq!(pending.await.unwrap().status(), StatusCode::GATEWAY_TIMEOUT);
    assert!(cancelled.is_cancelled());
    release.send(()).unwrap();
    server.shutdown().await.unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
}

#[tokio::test]
async fn closed_wire_types_and_actual_input_output_byte_boundaries() {
    let (f, s) = fixture(true);
    install(&f, &s, "release-1", "actual");
    let (url, served) = socket(&s).await;
    let client = reqwest::Client::new();
    let malformed = json!({"id":"actual-graph","version":"release-1","anchor_label":"Function","anchor_key":{"type":"int64","value":9007199254740993_u64},"relationship":"CONTAINS","other_label":"File","direction":"incoming","limit":1});
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/neighborhood"))
            .headers(headers(&f.raw, false))
            .json(&malformed)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/schema"))
            .headers(headers(&f.raw, false))
            .json(&json!({"id":"actual-graph","version":"release-1","scope":"untrusted"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/schema"))
            .headers(headers(&f.raw, false))
            .body(vec![b' '; 65537])
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    s.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    let mut policy = s.0.policy.clone();
    policy.content.max_output_bytes = 16;
    let server = GraphServer::new(Arc::clone(&f.auth), policy, CancellationToken::new()).unwrap();
    let (url, served) = socket(&server).await;
    let response = client
        .post(format!("{url}/api/graphs/schema"))
        .headers(headers(&f.raw, false))
        .json(&json!({"id":"actual-graph","version":"release-1"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.text().await.unwrap(), "graph response unavailable");
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn mismatch_registry_replaced_origin_and_corrupt_parquet_are_causal() {
    let (f, s) = fixture(true);
    let root = install(&f, &s, "release-1", "actual");
    let (_other, foreign) = fixture(true);
    let cancel = Cancellation::default();
    let request =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let projection = s.0.policy.registry.limits().projection();
    let id = ArtifactId::new("actual-graph", projection).unwrap();
    let version = ConcreteVersion::new("release-1", projection).unwrap();
    let foreign_result = open_imported_graph_runtime(
        f.auth.issuer(),
        &f.raw,
        ImportedGraphInput {
            registry: &foreign.0.policy.registry,
            id: &id,
            version: &version,
            installation: f.auth.installation(),
            file_policy: &s.0.policy.file,
            read_limits: s.0.policy.read,
            graph_policy: s.0.policy.native_install.graph,
            directory_limits: s.0.policy.index,
            request: &request,
        },
    );
    assert!(matches!(
        foreign_result,
        Err(ImportedGraphError::InvalidInstallation)
    ));
    let captured = open_imported_graph_runtime(
        f.auth.issuer(),
        &f.raw,
        ImportedGraphInput {
            registry: &s.0.policy.registry,
            id: &id,
            version: &version,
            installation: f.auth.installation(),
            file_policy: &s.0.policy.file,
            read_limits: s.0.policy.read,
            graph_policy: s.0.policy.native_install.graph,
            directory_limits: s.0.policy.index,
            request: &request,
        },
    )
    .unwrap();
    let moved = root.with_extension("moved");
    fs::rename(&root, &moved).unwrap();
    fs::create_dir_all(root.join("graph.icebug")).unwrap();
    fs::write(root.join("graph.icebug/schema.cypher"), "replacement").unwrap();
    assert!(captured.generation().is_err());
    fs::remove_dir_all(&root).unwrap();
    fs::rename(&moved, &root).unwrap();
    fs::write(root.join("graph.icebug/a.indices"), b"not parquet").unwrap();
    let body = serde_json::to_vec(&json!({"id":"actual-graph","version":"release-1"})).unwrap();
    assert!(matches!(
        read(
            &s.0,
            &f.raw,
            false,
            &body,
            Operation::Sample,
            &cancel,
            Instant::now() + Duration::from_secs(5)
        ),
        Err(ReadFailure::Imported(ImportedGraphError::Graph(_)))
    ));
}

fn publish_local(auth: &DaemonAuth, paths: &InstallationPaths, ctx: &RequestContext<'_>) {
    let id = InstallationIdentityStore::new(paths)
        .unwrap()
        .load()
        .unwrap()
        .unit_id()
        .unwrap()
        .clone();
    let scope = CapturedPublicationScope::installation(
        paths,
        id,
        [paths.root().to_owned()],
        scopes(),
        ctx.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(scope, documents(), ctx.snapshot()).unwrap(),
        profiles(),
        ctx,
    )
    .unwrap();
    let before = store.load(ctx).unwrap();
    let result = store
        .mutate(before, ctx, |doc, ctx| {
            doc.add_provider(
                "local".into(),
                ProviderConfig {
                    kind: ProviderKind::Local {
                        allow_aws_credential_chain: false,
                        broker: None,
                    },
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: S3Source::LocalCredentials,
                    },
                },
                doc.revision(),
                1,
                ctx,
            )?;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("local").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(
                ProfileData {
                    name: "actual-local".into(),
                    provider: "local".into(),
                    username: "tester".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new("test-local-key".into())),
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expected,
                1,
                ctx,
            )?;
            doc.activate("actual-local", doc.revision(), ctx)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert!(result.failure().is_none());
    let ScopeCapture { live, record } = auth.installation();
    assert!(live.binding().unwrap() == *record);
}

#[tokio::test]
async fn query_http_native_members_versions_parameters_and_typed_entities() {
    let (f, s) = fixture(true);
    let local_cancel = Cancellation::default();
    let local_request = RequestContext::new(
        content(),
        &local_cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    publish_local(&f.auth, &f.paths, &local_request);
    install_with_members(&f, &s, "query-1", "primeira", true);
    install_with_members(&f, &s, "query-2", "segunda", true);
    let (url, served) = socket(&s).await;
    let client = reqwest::Client::new();
    for (version, name) in [("query-1", "primeira"), ("query-2", "segunda")] {
        let response=client.post(format!("{url}/api/graphs/query")).headers(headers(&f.raw,false))
            .json(&json!({"id":"actual-graph","version":version,
                "query":"MATCH (f:File) WHERE f.path=$path RETURN f.name,$record,NULL",
                "parameters":{"path":"main.rs","record":{"text":"α\0","count":9007199254740993i64}}}))
            .send().await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap_or_default()
        );
        let result: Value = response.json().await.unwrap();
        assert_eq!(result["rows"][0][0]["value"], name);
        assert_eq!(result["rows"][0][1]["value"]["text"]["value"], "α\0");
        assert_eq!(
            result["rows"][0][1]["value"]["count"]["value"],
            "9007199254740993"
        );
        assert!(result["rows"][0][2]["value"].is_null());
        let result:Value=client.post(format!("{url}/api/graphs/query")).headers(headers(&f.raw,false))
            .json(&json!({"id":"actual-graph","version":version,"query":
                "MATCH (a)-[r:CONTAINS]->(b) RETURN id(a),id(r),id(b),r.uid,r.weight ORDER BY r.uid"}))
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        let rows = result["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0][4]["value"], "9007199254740993");
        assert_eq!(rows[3][1]["value"]["member"], 1);
        assert_eq!(rows[4][1]["value"]["member"], 2);
        assert_eq!(rows[4][0]["value"]["table"], 1);
        assert_eq!(rows[4][2]["value"]["table"], 0);
        let ids = rows
            .iter()
            .map(|r| r[1].to_string())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), 6);
        let entities:Value=client.post(format!("{url}/api/graphs/query")).headers(headers("test-local-key",true))
            .json(&json!({"id":"actual-graph","version":version,"query":
                "MATCH path=(a:Function)-[r:CONTAINS]->(b:File) RETURN a,r,b,path ORDER BY r.uid LIMIT 1"}))
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        assert_eq!(entities["rows"][0][1]["value"]["id"]["value"]["member"], 2);
        assert_eq!(
            entities["rows"][0][3]["value"]["_NODES"]["value"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    s.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}
#[tokio::test]
async fn query_http_denies_effects_credentials_and_invalid_input_causally() {
    let (f, s) = fixture(true);
    let installed = install_with_members(&f, &s, "query-deny", "safe", true);
    let client = reqwest::Client::new();
    let (url, served) = socket(&s).await;
    for query in [
        "CREATE (:File {path:'bad'})",
        "RETURN 1; RETURN 2",
        "LOAD EXTENSION httpfs",
        "MATCH (n:NotThere) RETURN n",
        "NOT A QUERY",
    ] {
        let response = client
            .post(format!("{url}/api/graphs/query"))
            .headers(headers(&f.raw, false))
            .json(&json!({"id":"actual-graph","version":"query-deny","query":query}))
            .send()
            .await
            .unwrap();
        assert!(!response.status().is_success(), "{query}");
        let cause = response.text().await.unwrap();
        assert!(!cause.is_empty(), "{query}");
    }
    let request = json!({"id":"actual-graph","version":"query-deny","query":"RETURN 1"});
    let unauthorized = client
        .post(format!("{url}/api/graphs/query"))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let denied = client
        .post(format!("{url}/api/graphs/query"))
        .headers(headers("invalid", false))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let mut invalid = request.clone();
    invalid["unknown"] = json!(true);
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/query"))
            .headers(headers(&f.raw, false))
            .json(&invalid)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    // Same server remains usable after each rejected native operation.
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/query"))
            .headers(headers(&f.raw, false))
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    fs::rename(
        installed.join("graph.icebug/File.parquet"),
        installed.join("graph.icebug/removed.parquet"),
    )
    .unwrap();
    assert!(
        !client
            .post(format!("{url}/api/graphs/query"))
            .headers(headers(&f.raw, false))
            .json(&request)
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    s.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    let (denied_f, denied_s) = fixture(false);
    install(&denied_f, &denied_s, "query-deny", "safe");
    let response = denied_s
        .read(
            headers(&denied_f.raw, false),
            Bytes::from(serde_json::to_vec(&request).unwrap()),
            Operation::Query,
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    denied_s.shutdown().await.unwrap();
}
#[tokio::test]
async fn query_http_native_deadline_then_sibling_and_shutdown_join() {
    let (f, mut s) = fixture(true);
    Arc::get_mut(&mut s.0).unwrap().policy.timeout = Duration::from_millis(500);
    install_with_members(&f, &s, "query-timeout", "safe", true);
    let (url, served) = socket(&s).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{url}/api/graphs/query"))
        .headers(headers(&f.raw, false))
        .json(&json!({"id":"actual-graph","version":"query-timeout",
        "query":"UNWIND RANGE(1,10000) AS a UNWIND RANGE(1,10000) AS b RETURN sum(a*b)"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    let response = client
        .post(format!("{url}/api/graphs/query"))
        .headers(headers(&f.raw, false))
        .json(&json!({"id":"actual-graph","version":"query-timeout","query":"RETURN 1"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    s.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    assert!(s.0.jobs.lock().await.is_empty());
}

#[tokio::test]
async fn query_dropped_handler_cancels_only_its_job_and_preserves_shutdown_join() {
    let (f, s) = fixture(true);
    install(&f, &s, "release-1", "sibling");
    let (started, observed) = std::sync::mpsc::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    *s.0.gate.lock().unwrap() = Some((started, waiting));
    let input = Bytes::from(
        serde_json::to_vec(&json!({"id":"actual-graph","version":"release-1","query":"RETURN 1"}))
            .unwrap(),
    );
    let request = s.clone();
    let credential = f.raw.clone();
    let pending_input = input.clone();
    let pending = tokio::spawn(async move {
        request
            .read(headers(&credential, false), pending_input, Operation::Query)
            .await
    });
    let cancellation =
        tokio::task::spawn_blocking(move || observed.recv_timeout(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert!(cancellation.is_cancelled());
    let sibling = s
        .read(headers(&f.raw, false), input, Operation::Query)
        .await;
    assert_eq!(sibling.status(), StatusCode::OK);
    let closing = s.clone();
    let shutdown = tokio::spawn(async move { closing.shutdown().await });
    tokio::task::yield_now().await;
    assert!(!shutdown.is_finished());
    shutdown.abort();
    assert!(shutdown.await.unwrap_err().is_cancelled());
    let closing = s.clone();
    let shutdown = tokio::spawn(async move { closing.shutdown().await });
    tokio::task::yield_now().await;
    assert!(!shutdown.is_finished());
    release.send(()).unwrap();
    shutdown.await.unwrap().unwrap();
    assert!(s.0.jobs.lock().await.is_empty());
}

#[path = "graph_server/entity_source.rs"]
mod entity_source;

#[cfg(target_os = "linux")]
async fn project_fixture(
    read_permission: bool,
) -> (source_fixture::Fixture, GraphServer, std::path::PathBuf) {
    project_fixture_from(fixture(read_permission)).await
}
#[cfg(target_os = "linux")]
pub(crate) async fn web_fixture() -> (source_fixture::Fixture, GraphServer, std::path::PathBuf) {
    project_fixture_from(fixture_with_permissions(
        &[Permission::Read(Module::Ast)],
        &[Permission::ManageAuthProfiles],
    ))
    .await
}
#[cfg(target_os = "linux")]
pub(crate) async fn web_import_fixture() -> (source_fixture::Fixture, GraphServer) {
    let fixture = source_fixture::fixture_with_imported_ast_provider(
        &[Permission::Read(Module::Ast)],
        &[Permission::ManageAuthProfiles],
        Brand::default(),
        NativeBinding::KiroV3,
        &["project"],
        &[],
        Some("broker"),
    );
    let (fixture, server, _) = project_fixture_from(fixture_from(fixture)).await;
    entity_source::publish_entity_payload(
        &fixture,
        &server,
        "web-import",
        Some("first\nfn hello\nbody\nlast\n"),
    )
    .await;
    (fixture, server)
}
#[cfg(target_os = "linux")]
pub(crate) async fn live_project_graph(
    fixture: &source_fixture::Fixture,
    server: &GraphServer,
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> Arc<crate::foreground_ast_sources::progressive::LiveIndex> {
    let (paths, id, version) = &fixture.projects[0];
    let scope = CapturedPublicationScope::project(paths, *id, scopes(), original).unwrap();
    let record = scope.binding().unwrap();
    server
        .0
        .live_ast
        .capture(
            fixture.auth.installation(),
            graphit_auth::ProjectCapture {
                scope: ScopeCapture {
                    live: &scope,
                    record: &record,
                },
                version,
            },
            server.0.policy.index,
            original,
            true,
        )
        .await
        .unwrap()
        .unwrap()
        .0
}

#[cfg(target_os = "linux")]
async fn project_fixture_from(
    (fixture, mut server): (source_fixture::Fixture, GraphServer),
) -> (source_fixture::Fixture, GraphServer, std::path::PathBuf) {
    use graphit_data::graph_writer::{GraphEdge, GraphNode, GraphValue};
    use std::collections::BTreeMap;
    let config = ConfigSnapshot::new(fixture.paths.brand(), []).unwrap();
    server
        .attach_projects(
            config,
            Arc::new(crate::live_project_selections::LiveProjectSelections::new(
                Arc::clone(&fixture.projects),
                0,
            )),
        )
        .unwrap();
    let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    let graph = live_project_graph(&fixture, &server, &original).await;
    let nodes = [
        GraphNode {
            label: "File".into(),
            key: "main.rs".into(),
            properties: BTreeMap::from([
                ("path".into(), GraphValue::String("main.rs".into())),
                ("name".into(), GraphValue::String("native-project".into())),
            ]),
        },
        GraphNode {
            label: "Function".into(),
            key: "f1".into(),
            properties: BTreeMap::from([
                ("path".into(), GraphValue::String("main.rs".into())),
                ("name".into(), GraphValue::String("α".into())),
            ]),
        },
        GraphNode {
            label: "Function".into(),
            key: "f2".into(),
            properties: BTreeMap::from([
                ("path".into(), GraphValue::String("main.rs".into())),
                ("name".into(), GraphValue::String("β".into())),
            ]),
        },
    ];
    let edges = ["f1", "f1", "f2"]
        .into_iter()
        .enumerate()
        .map(|(index, target)| GraphEdge {
            kind: "CONTAINS".into(),
            source_key: "main.rs".into(),
            target_key: target.into(),
            uid: format!("u{index}"),
        })
        .collect::<Vec<_>>();
    graph
        .graph
        .replace_file(
            graphit_ast::live_graph::LiveFileInput {
                path: "main.rs",
                source_sha256: [0; 32],
                raw_version: 0,
                nodes: &nodes,
                edges: &edges,
                definitions: &[],
                references: &[],
                scip_document: None,
                pending_compiler: None,
                scip_compiler: None,
                oracle: &[],
                oracle_file: None,
            },
            &original,
        )
        .unwrap();
    let (paths, id, version) = &fixture.projects[0];
    let scope = CapturedPublicationScope::project(paths, *id, scopes(), &original).unwrap();
    let binding = scope.binding().unwrap();
    let (destination, _) = crate::foreground_ast_sources::progressive::location(
        fixture.auth.installation(),
        graphit_auth::ProjectCapture {
            scope: ScopeCapture {
                live: &scope,
                record: &binding,
            },
            version,
        },
    )
    .unwrap();
    drop(graph);
    (fixture, server, destination)
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn project_http_uses_live_graph_exact_selection_and_rejects_captured_directory_replacement() {
    let (f, s, destination) = project_fixture(true).await;
    let (url, served) = socket(&s).await;
    let client = reqwest::Client::new();
    let selected = json!({"selection":{}});
    let mut generation = None;
    for operation in ["schema", "sample"] {
        let response = client
            .post(format!("{url}/api/graphs/project/{operation}"))
            .headers(headers(&f.raw, false))
            .json(&selected)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{operation}: {}",
            response.text().await.unwrap()
        );
        let value = response.json::<Value>().await.unwrap();
        if let Some(generation) = &generation {
            assert_eq!(
                &value["generation"], generation,
                "unchanged native FILE retains its generation across fresh requests"
            );
        } else {
            generation = Some(value["generation"].clone());
        }
    }
    let query =
        json!({"selection":{},"query":{"query":"MATCH (f:File) RETURN f.name","parameters":{}}});
    let response = client
        .post(format!("{url}/api/graphs/project/query"))
        .headers(headers(&f.raw, false))
        .json(&query)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let input = json!({"selection":{},"neighborhood":{"anchor_label":"File","anchor_key":{"type":"string","value":"main.rs"},"relationship":"CONTAINS","other_label":"Function","direction":"outgoing","limit":1}});
    let response = client
        .post(format!("{url}/api/graphs/project/neighborhood"))
        .headers(headers(&f.raw, false))
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["edges"][0]["uid"],
        "u0"
    );
    let absent = json!({"selection":{"project_version":"absent"}});
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/project/schema"))
            .headers(headers(&f.raw, false))
            .json(&absent)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    // Retain the actual native owner: substituting its captured directory must
    // not make an in-use database silently point at another physical scope.
    let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    let retained = live_project_graph(&f, &s, &original).await;
    fs::rename(&destination, destination.with_extension("old")).unwrap();
    drop(graphit_fs::create_private_directory(&destination).unwrap());
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/project/schema"))
            .headers(headers(&f.raw, false))
            .json(&selected)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    drop(retained);
    s.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn project_http_permission_denial_precedes_absent_publication_and_bad_credentials() {
    let (f, s, destination) = project_fixture(false).await;
    fs::remove_dir_all(destination).unwrap();
    let (url, served) = socket(&s).await;
    let client = reqwest::Client::new();
    let input = json!({"selection":{}});
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/project/schema"))
            .headers(headers(&f.raw, false))
            .json(&input)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{url}/api/graphs/project/schema"))
            .json(&input)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    s.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[cfg(target_os = "linux")]
pub(crate) fn browser_provider(server: &GraphServer, raw: &str, endpoint: &str) -> u64 {
    use graphit_auth::profile::BrokerService;
    use graphit_auth::profile_manager::{NativeProfileManager, ProfileManagerAuthority};
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let manager = NativeProfileManager::new(issuer);
    let snapshot = manager
        .begin(ProfileManagerAuthority::Admin(&parent))
        .unwrap();
    if let Some(provider) = snapshot
        .providers()
        .find(|provider| provider.name() == "broker")
    {
        assert!(
            matches!(&provider.config().kind, ProviderKind::Broker(service) if service.endpoint == endpoint)
        );
        return provider.revision();
    }
    let revision = snapshot.revision();
    let publication = manager
        .add_provider(
            snapshot,
            "broker".into(),
            ProviderConfig {
                kind: ProviderKind::Broker(BrokerService {
                    endpoint: endpoint.into(),
                    mcp_resource: None,
                    allow_anonymous: false,
                }),
                embedding: AiService::Disabled,
                rerank: AiService::Disabled,
                s3: S3Location {
                    bucket: None,
                    region: None,
                    endpoint: None,
                    prefix: None,
                    source: S3Source::BrokerScope,
                },
            },
            revision,
            1,
        )
        .unwrap();
    assert!(publication.failure().is_none() && publication.cleanup_failure().is_none());
    assert!(publication.into_pending_cleanup().is_none());
    let snapshot = manager
        .begin(ProfileManagerAuthority::Admin(&parent))
        .unwrap();
    snapshot
        .providers()
        .find(|p| p.name() == "broker")
        .unwrap()
        .revision()
}

/// Both Browser and Source MCP consumers use this same real SDK graph/Raw fixture.
#[cfg(target_os = "linux")]
pub(crate) async fn web_source_fixture(stored: bool) -> (source_fixture::Fixture, GraphServer) {
    use arrow_array::{ArrayRef, RecordBatch, StringArray};
    use graphit_data::{
        graph_writer::{GraphNode, GraphValue},
        lance_module_store::LanceModuleStore,
    };
    use sha2::{Digest, Sha256};
    let (fixture, server, _) = web_fixture().await;
    let text = "before\nfn hello() {\n  work();\n}\nafter";
    let (paths, id, _) = &fixture.projects[0];
    fs::write(paths.root().join("main.rs"), text).unwrap();
    let raw = fixture
        .auth
        .installation()
        .live
        .owner_root()
        .join("projects")
        .join(id.to_string())
        .join("ast")
        .join("source.lance");
    fs::create_dir_all(raw.parent().unwrap()).unwrap();
    let schema = graphit_ast::indexed_source::source_schema();
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(StringArray::from(vec!["main.rs"])) as ArrayRef,
            Arc::new(StringArray::from(vec![stored.then_some(text)])) as ArrayRef,
        ],
    )
    .unwrap();
    let raw = LanceModuleStore::create(raw.to_str().unwrap(), schema, vec![batch])
        .await
        .unwrap();
    let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    let properties = |name: &str, start: i64, end: i64| {
        BTreeMap::from([
            ("name".into(), GraphValue::String(name.into())),
            ("path".into(), GraphValue::String("main.rs".into())),
            ("start_line".into(), GraphValue::Int64(start)),
            ("end_line".into(), GraphValue::Int64(end)),
            (
                "source_sha256".into(),
                GraphValue::String(
                    Sha256::digest(text.as_bytes())
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect(),
                ),
            ),
            (
                "raw_version".into(),
                GraphValue::String(raw.snapshot().version().to_string()),
            ),
            ("available".into(), GraphValue::Bool(true)),
            ("extent_status".into(), GraphValue::String("Exact".into())),
        ])
    };
    let nodes = [
        GraphNode {
            label: "Function".into(),
            key: "hello".into(),
            properties: properties("hello", 2, 4),
        },
        GraphNode {
            label: "Function".into(),
            key: "duplicate-a".into(),
            properties: properties("duplicate", 2, 4),
        },
        GraphNode {
            label: "Function".into(),
            key: "duplicate-b".into(),
            properties: properties("duplicate", 2, 4),
        },
        GraphNode {
            label: "Function".into(),
            key: "invalid".into(),
            properties: properties("invalid", 4, 2),
        },
        GraphNode {
            label: "Function".into(),
            key: "nonexact".into(),
            properties: {
                let mut values = properties("nonexact", 2, 4);
                values.insert("available".into(), GraphValue::Bool(false));
                values.insert(
                    "extent_status".into(),
                    GraphValue::String("NonExact".into()),
                );
                values
            },
        },
    ];
    let index = live_project_graph(&fixture, &server, &original).await;
    index
        .graph
        .replace_file(
            graphit_ast::live_graph::LiveFileInput {
                path: "main.rs",
                source_sha256: Sha256::digest(text.as_bytes()).into(),
                raw_version: raw.snapshot().version(),
                nodes: &nodes,
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                pending_compiler: None,
                scip_compiler: None,
                oracle: &[],
                oracle_file: None,
            },
            &original,
        )
        .unwrap();
    drop(index);
    (fixture, server)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_import_catalog_http_mcp_and_cli_use_real_installation_and_read_ast() {
    let (f, graph) = fixture(true);
    let raw = graph_payload(&f, "catalog-real", "native", false);
    publish_graph_payload(&f, &graph, "catalog-real", &raw);
    let expected = json!({"imports":[{"id":"actual-graph","version":"catalog-real","context":"actual-graph@catalog-real","backend":"native"}]});
    let response = graph
        .read(
            headers(&f.raw, false),
            Bytes::from_static(b"{}"),
            Operation::Imports,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Value>(
            &axum::body::to_bytes(response.into_body(), 65536)
                .await
                .unwrap()
        )
        .unwrap(),
        expected
    );
    assert_eq!(
        graph
            .read(
                headers(&f.raw, false),
                Bytes::from_static(b"{\"context\":\"actual-graph@catalog-real\"}"),
                Operation::Imports
            )
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let listener = tcp_listener::reserve(
        Options {
            address: "127.0.0.1".parse().unwrap(),
            ports: PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let original =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    graphit_daemon::source_discovery::publish_discovery(
        &f.auth,
        &listener,
        graph.0.policy.index,
        original.snapshot(),
    )
    .unwrap();
    let server = f.server.clone();
    let served = tokio::spawn(async move { server.serve(listener).await });
    let mut output = Vec::new();
    crate::graph_entry::run(
        &f.paths,
        &reqwest::Client::new(),
        crate::graph_entry::Command::Imports
            .prepare(content())
            .unwrap(),
        (graph.0.policy.index, content()),
        &original,
        &mut output,
    )
    .await
    .unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&output).unwrap(), expected);
    f.server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    graph.shutdown().await.unwrap();
    let (f, graph) = fixture(false);
    assert_eq!(
        graph
            .read(
                headers(&f.raw, false),
                Bytes::from_static(b"{}"),
                Operation::Imports
            )
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    f.server.shutdown().await.unwrap();
    graph.shutdown().await.unwrap();
}

#[tokio::test]
async fn native_ast_export_requires_read_ast_and_unconditional_external_before_io() {
    use graphit_auth::ExternalEffect;
    use rmcp::model::CallToolRequestParams;
    for permissions in [
        vec![Permission::Read(Module::Ast)],
        vec![Permission::External(ExternalEffect::PublishedFilesystem)],
    ] {
        let (fixture, server) = fixture_with_permissions(&permissions, &[]);
        let destination = fixture.temp.path().join("denied.zip");
        let result=server.call_mcp(CallToolRequestParams::new("graphit_ast_export").with_arguments(json!({"ai_optimized":false,"output":NativePath::new(destination.clone()).unwrap(),"context":"absent/artifact@version"}).as_object().unwrap().clone()),headers(&fixture.raw,false),CancellationToken::new()).await;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["code"],
            "authentication_or_permission_denied"
        );
        assert!(!destination.exists());
        server.shutdown().await.unwrap();
    }
}
