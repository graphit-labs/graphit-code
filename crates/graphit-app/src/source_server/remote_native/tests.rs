use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{
    LocalGraphPolicy,
    graph_writer::{GraphEdge, GraphNode, GraphValue, IcebugWriter},
    remote_graph::RemoteIcebugGraph,
};
use object_store::{ObjectStore, ObjectStoreExt, memory::InMemory, path::Path};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn remote_engine_queries_pinned_ranges_and_rejects_replacement_and_cancel() {
    exercise_remote_engine(Arc::new(InMemory::new()));
}

fn exercise_remote_engine(store: Arc<dyn ObjectStore>) {
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ));
    let node = |key: &str, name: &str, line: i64| GraphNode {
        label: "Function".into(),
        key: key.into(),
        properties: BTreeMap::from([
            ("uid".into(), GraphValue::String(key.into())),
            ("name".into(), GraphValue::String(name.into())),
            ("path".into(), GraphValue::String("example.any".into())),
            ("start_line".into(), GraphValue::Int64(line)),
            ("available".into(), GraphValue::Bool(true)),
            ("extent_status".into(), GraphValue::String("Exact".into())),
            ("end_line".into(), GraphValue::Int64(line + 2)),
        ]),
    };
    let output = IcebugWriter::new(Arc::clone(&original))
        .unwrap()
        .push_file(
            [node("one", "ação", 2), node("two", "second", 7)],
            [GraphEdge {
                kind: "CALLS".into(),
                source_key: "one".into(),
                target_key: "two".into(),
                uid: "edge".into(),
            }],
        )
        .unwrap()
        .finish()
        .unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let prefix = Path::from("immutable/version-one/graph");
    let mut pending = vec![output.directory().to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(output.directory())
                .unwrap()
                .to_owned();
            let path = Path::parse(format!("{prefix}/{}", relative.to_str().unwrap())).unwrap();
            let bytes = std::fs::read(entry.path()).unwrap();
            runtime.block_on(store.put(&path, bytes.into())).unwrap();
        }
    }
    let manifest = std::fs::read(output.directory().join("icebug.json")).unwrap();
    let config =
        graphit_core::config::ConfigSnapshot::new(&graphit_core::brand::Brand::default(), [])
            .unwrap();
    let policy = LocalGraphPolicy::resolve(&config, &original).unwrap();
    let object_store: Arc<dyn ObjectStore> = store.clone();
    let graph = Arc::new(
        runtime
            .block_on(RemoteIcebugGraph::open(
                object_store,
                prefix,
                Sha256::digest(&manifest).into(),
                policy,
                Arc::clone(&original),
            ))
            .unwrap(),
    );
    let options = graphit_ast::icebug_query::QueryOptions {
        buffer_pool_bytes: 128 * 1024 * 1024,
        query_threads: 2,
        max_rows: 16,
        max_bytes: 64 * 1024,
        max_depth: 16,
    };
    let result = graphit_ast::icebug_query::execute_remote(
        &graph,
        &original,
        runtime.handle(),
        "MATCH (a:Function)-[r:CALLS]->(b:Function) RETURN a.name,r.uid,b.name",
        &BTreeMap::new(),
        options,
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0]["value"], "ação");
    assert_eq!(result.rows[0][1]["value"], "edge");
    assert_eq!(result.rows[0][2]["value"], "second");
    let extent = graphit_ast::imported_graph::resolve_remote_entity_window(
        &graph,
        &original,
        runtime.handle(),
        "example.any",
        "ação",
        Some("Function"),
        options,
    )
    .unwrap();
    assert_eq!(extent, (2, 4));
    assert!(
        graphit_ast::icebug_query::execute_remote(
            &graph,
            &original,
            runtime.handle(),
            "CREATE (:Function {uid:'forbidden'})",
            &BTreeMap::new(),
            options
        )
        .is_err()
    );
    let manifest_value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let function_file = manifest_value["node_tables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|table| table["label"] == "Function")
        .unwrap()["shards"][0]["file"]
        .as_str()
        .unwrap();
    let changed = Path::from("immutable/version-one/graph").join(function_file);
    let bytes = runtime.block_on(store.get(&changed)).unwrap();
    let bytes = runtime.block_on(bytes.bytes()).unwrap();
    let mut changed_bytes = bytes.to_vec();
    changed_bytes.push(0);
    runtime
        .block_on(store.put(&changed, changed_bytes.into()))
        .unwrap();
    assert!(
        graphit_ast::icebug_query::execute_remote(
            &graph,
            &original,
            runtime.handle(),
            "MATCH (a:Function)-[r:CALLS]->(b:Function) RETURN a.name,r.uid,b.name",
            &BTreeMap::new(),
            options
        )
        .is_err()
    );
    original.cancellation().cancel();
    assert!(
        graphit_ast::icebug_query::execute_remote(
            &graph,
            &original,
            runtime.handle(),
            "RETURN 1",
            &BTreeMap::new(),
            options
        )
        .is_err()
    );
}

// Reuses the existing isolated Moto/STS runner; never selects ambient credentials.
#[test]
#[ignore = "requires scripts/test-record-storage-s3.py and its temporary STS fixture"]
fn official_ladybug_progressive_publication_and_remote_queries_s3() {
    use graphit_ast::live_graph::{LiveFileInput, LiveGraph};
    let path = std::env::var_os("GRAPHIT_TEST_S3_FIXTURE").expect("explicit isolated S3 fixture");
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let text = |name: &str| fixture[name].as_str().expect("valid fixture field");
    let endpoint = text("endpoint");
    let parsed = url::Url::parse(endpoint).unwrap();
    assert_eq!(parsed.scheme(), "http");
    assert_eq!(parsed.host_str(), Some("127.0.0.1"));
    let store: Arc<dyn ObjectStore> = Arc::new(
        object_store::aws::AmazonS3Builder::new()
            .with_endpoint(endpoint)
            .with_bucket_name(text("bucket"))
            .with_region(text("region"))
            .with_access_key_id(text("access_key_id"))
            .with_secret_access_key(text("secret_access_key"))
            .with_token(text("session_token"))
            .with_allow_http(true)
            .build()
            .unwrap(),
    );
    exercise_remote_engine(Arc::clone(&store));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ));
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("graph");
    let graph = LiveGraph::open_path_owned(root.clone(), &original, [71; 32], true).unwrap();
    let node = |key: &str, name: &str| GraphNode {
        label: "Function".into(),
        key: key.into(),
        properties: BTreeMap::from([("name".into(), GraphValue::String(name.into()))]),
    };
    let publish_file = |path: &str, revision: u8, nodes: &[GraphNode], edges: &[GraphEdge]| {
        graph
            .replace_file(
                LiveFileInput {
                    path,
                    source_sha256: [revision; 32],
                    raw_version: u64::from(revision),
                    nodes,
                    edges,
                    definitions: &[],
                    references: &[],
                    scip_document: None,
                    oracle: &[],
                    oracle_file: None,
                    pending_compiler: None,
                    scip_compiler: None,
                },
                &original,
            )
            .unwrap();
    };
    let snapshot = |version: &str| {
        let prefix = Path::from(format!("progressive-native/{version}/graph"));
        let mut pending = vec![root.clone()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(entry.path());
                    continue;
                }
                let relative = entry.path().strip_prefix(&root).unwrap().to_owned();
                runtime
                    .block_on(store.put(
                        &Path::parse(format!("{prefix}/{}", relative.to_str().unwrap())).unwrap(),
                        std::fs::read(entry.path()).unwrap().into(),
                    ))
                    .unwrap();
            }
        }
        let manifest = std::fs::read(root.join("icebug.json")).unwrap();
        let config =
            graphit_core::config::ConfigSnapshot::new(&graphit_core::brand::Brand::default(), [])
                .unwrap();
        let policy = LocalGraphPolicy::resolve(&config, &original).unwrap();
        Arc::new(
            runtime
                .block_on(RemoteIcebugGraph::open(
                    Arc::clone(&store),
                    prefix,
                    Sha256::digest(manifest).into(),
                    policy,
                    Arc::clone(&original),
                ))
                .unwrap(),
        )
    };
    let options = graphit_ast::icebug_query::QueryOptions {
        buffer_pool_bytes: 128 * 1024 * 1024,
        query_threads: 2,
        max_rows: 64,
        max_bytes: 65536,
        max_depth: 16,
    };
    let query = |snapshot: &Arc<RemoteIcebugGraph>, cypher: &str| {
        graphit_ast::icebug_query::execute_remote(
            snapshot,
            &original,
            runtime.handle(),
            cypher,
            &BTreeMap::new(),
            options,
        )
        .unwrap()
    };
    publish_file(
        "a.rs",
        1,
        &[node("a", "ação")],
        &[GraphEdge {
            kind: "CALLS".into(),
            source_key: "a".into(),
            target_key: "b".into(),
            uid: "ab".into(),
        }],
    );
    let first = snapshot("one");
    assert_eq!(
        query(&first, "MATCH(n:Function) RETURN count(n)").rows[0][0]["value"],
        "1"
    );
    let self_edge = GraphEdge {
        kind: "CALLS".into(),
        source_key: "b".into(),
        target_key: "b".into(),
        uid: "bb".into(),
    };
    publish_file(
        "b.rs",
        2,
        &[node("b", "before")],
        std::slice::from_ref(&self_edge),
    );
    let full = snapshot("two");
    assert_eq!(
        query(&full, "MATCH(a)-[r:CALLS]->(b) RETURN a.name,b.name").rows[0][1]["value"],
        "before"
    );
    publish_file(
        "b.rs",
        3,
        &[node("b", "changed")],
        std::slice::from_ref(&self_edge),
    );
    let changed = snapshot("three");
    assert_eq!(
        query(
            &changed,
            "MATCH(a:Function {name:'ação'})-[:CALLS*2..2]->(b:Function) RETURN b.name"
        )
        .rows[0][0]["value"],
        "changed"
    );
    assert_eq!(
        query(&changed, "MATCH(a)<-[r:CALLS]-(b) RETURN a.name").rows[0][0]["value"],
        "changed"
    );
    assert_eq!(
        query(&full, "MATCH(a)-[r:CALLS]->(b) RETURN b.name").rows[0][0]["value"],
        "before"
    );
    graph.remove_file("b.rs", &original).unwrap();
    let removed = snapshot("four");
    assert_eq!(
        query(&removed, "MATCH(n:Function) RETURN count(n)").rows[0][0]["value"],
        "1"
    );
    assert_eq!(
        query(&removed, "MATCH()-[r:CALLS]->() RETURN count(r)").rows[0][0]["value"],
        "0"
    );
    drop(graph);
    let reopened = LiveGraph::open_path_owned(root, &original, [71; 32], false).unwrap();
    assert_eq!(
        reopened
            .query(
                "MATCH(n:Function) RETURN count(n)",
                &BTreeMap::new(),
                options,
                &original
            )
            .unwrap()
            .rows[0][0]["value"],
        "1"
    );
}
