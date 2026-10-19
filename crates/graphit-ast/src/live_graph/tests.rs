use super::*;
use graphit_core::file_lock::Cancellation;
fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::service(Cancellation::default()))
}

fn options() -> QueryOptions {
    QueryOptions {
        buffer_pool_bytes: 67108864,
        query_threads: 2,
        max_rows: 100,
        max_bytes: 65536,
        max_depth: 32,
    }
}
fn node(key: &str, name: &str) -> GraphNode {
    GraphNode {
        label: "Function".into(),
        key: key.into(),
        properties: BTreeMap::from([("name".into(), GraphValue::String(name.into()))]),
    }
}
fn edge() -> GraphEdge {
    GraphEdge {
        kind: "CALLS".into(),
        source_key: "a".into(),
        target_key: "b".into(),
        uid: "ab".into(),
    }
}
fn count(g: &LiveGraph, q: &str) -> String {
    g.query(q, &BTreeMap::new(), options(), &original())
        .unwrap()
        .rows[0][0]["value"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn disk_image(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, output: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), output);
            } else {
                output.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    std::fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

#[test]
fn existing_graph_reader_and_writer_reopens_leave_storage_unchanged() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("graph");
    let image = || disk_image(&path);
    let g = LiveGraph::open_path_owned(path.clone(), &original(), [15; 32], true).unwrap();
    assert_eq!(g.revision().unwrap(), 0);
    drop(g);
    let before = image();
    let g = LiveGraph::open_path_owned(path.clone(), &original(), [15; 32], false).unwrap();
    assert_eq!(
        g.revision().unwrap(),
        0,
        "a fresh graph may have no state row yet"
    );
    assert!(g.files(&original()).unwrap().is_empty());
    assert_eq!(
        before,
        image(),
        "reading an empty graph must not initialize it again"
    );
    g.replace_file(
        LiveFileInput {
            path: "a.rs",
            source_sha256: [1; 32],
            raw_version: 1,
            nodes: &[node("a", "preserved")],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    drop(g);
    for create in [false, true] {
        let before = image();
        let g = LiveGraph::open_path_owned(path.clone(), &original(), [15; 32], create).unwrap();
        assert_eq!(g.revision().unwrap(), 1);
        assert!(g.state.lock().unwrap().index.is_none());
        assert_eq!(count(&g, "MATCH (n:Function) RETURN count(n)"), "1");
        assert!(
            g.state.lock().unwrap().index.is_none(),
            "readonly queries never build the mutation index"
        );
        // Reopen and readonly queries preserve the committed Parquet generation.
        assert_eq!(
            before,
            image(),
            "an existing graph reopen must not issue DDL"
        );
        drop(g);
    }
    let rejected = LiveGraph::open_path_owned(path, &original(), [16; 32], true);
    assert!(rejected.is_err(), "namespace mismatch still fails closed");
}

#[test]
fn readers_reject_missing_or_incomplete_graphs_without_initializing() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("graph");
    let error = LiveGraph::open_path_owned(path.clone(), &original(), [17; 32], false)
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(!path.exists());
    std::fs::write(&path, []).unwrap();
    assert!(LiveGraph::open_path_owned(path.clone(), &original(), [17; 32], false).is_err());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    std::fs::remove_file(&path).unwrap();
    let g = LiveGraph::open_path_owned(path.clone(), &original(), [17; 32], true).unwrap();
    drop(g);
    let manifest_path = path.join("icebug.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["live"]
        .as_object_mut()
        .unwrap()
        .remove("externals");
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let before = disk_image(&path);
    for create in [false, true] {
        assert!(LiveGraph::open_path_owned(path.clone(), &original(), [17; 32], create).is_err());
        assert_eq!(
            before,
            disk_image(&path),
            "a reader never repairs a malformed catalogue"
        );
    }
}

#[test]
fn per_file_update_cross_reference_and_reopen() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("graph");
    let g = LiveGraph::open_path_owned(path.clone(), &original(), [9; 32], true).unwrap();
    g.replace_file(
        LiveFileInput {
            path: "a.rs",
            source_sha256: [1; 32],
            raw_version: 1,
            nodes: &[node("a", "before")],
            edges: &[edge()],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(count(&g, "MATCH (n:Function) RETURN count(n)"), "1");
    assert_eq!(count(&g, "MATCH (n) RETURN count(n)"), "1");
    g.replace_file(
        LiveFileInput {
            path: "b.rs",
            source_sha256: [2; 32],
            raw_version: 1,
            nodes: &[node("b", "target")],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(count(&g, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "1");
    g.replace_file(
        LiveFileInput {
            path: "b.rs",
            source_sha256: [3; 32],
            raw_version: 2,
            nodes: &[node("b", "changed")],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(count(&g, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "1");
    assert_eq!(
        count(
            &g,
            "MATCH (n:Function) WHERE n.name='before' RETURN count(n)"
        ),
        "1"
    );
    g.remove_file("b.rs", &original()).unwrap();
    assert_eq!(count(&g, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "0");
    g.replace_file(
        LiveFileInput {
            path: "b.rs",
            source_sha256: [4; 32],
            raw_version: 3,
            nodes: &[node("b", "again")],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(count(&g, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "1");
    g.remove_file("a.rs", &original()).unwrap();
    assert_eq!(count(&g, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "0");
    assert_eq!(g.revision().unwrap(), 6);
    drop(g);
    let g = LiveGraph::open_path_owned(path, &original(), [9; 32], true).unwrap();
    assert_eq!(g.revision().unwrap(), 6);
    assert_eq!(count(&g, "MATCH (n:Function) RETURN count(n)"), "1");
}
#[test]
fn orphan_cleanup_preserves_cross_file_fanout_and_removes_only_unreferenced_keys() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let g =
        LiveGraph::open_path_owned(tmp.path().join("graph"), &original(), [23; 32], true).unwrap();
    // Both sides exceed the SDK vector capacity. Counting joined adjacency
    // would combine more than four million rows just for the hub key.
    const FANOUT: usize = 2049;
    let incoming = (0..FANOUT)
        .map(|i| GraphEdge {
            kind: "CALLS".into(),
            source_key: "a".into(),
            target_key: "hub".into(),
            uid: format!("incoming-{i}"),
        })
        .collect::<Vec<_>>();
    let outgoing = (0..FANOUT)
        .map(|i| GraphEdge {
            kind: "CALLS".into(),
            source_key: "hub".into(),
            target_key: "c".into(),
            uid: format!("outgoing-{i}"),
        })
        .collect::<Vec<_>>();
    let replace = |path, sha, nodes: &[GraphNode], edges: &[GraphEdge]| {
        g.replace_file(
            LiveFileInput {
                path,
                source_sha256: [sha; 32],
                raw_version: u64::from(sha),
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
            &original(),
        )
        .unwrap();
    };
    let internal_count = |query: &str| {
        g.state
            .lock()
            .unwrap()
            .index
            .as_ref()
            .unwrap()
            .database
            .query_row(query, [], |row| row.get::<_, i64>(0))
            .map(|count| u64::try_from(count).unwrap())
            .unwrap()
    };
    replace("b.rs", 1, &[node("hub", "target")], &[]);
    // An owned node without intentions is retained.
    assert_eq!(internal_count("SELECT COUNT(*) FROM entities"), 1);
    replace("a.rs", 2, &[node("a", "caller")], &incoming);
    replace("c.rs", 3, &[node("c", "callee")], &outgoing);
    assert_eq!(
        count(&g, "MATCH ()-[r:CALLS]->() RETURN count(r)"),
        (2 * FANOUT).to_string()
    );
    g.remove_file("b.rs", &original()).unwrap();
    assert_eq!(
        internal_count("SELECT COUNT(DISTINCT key) FROM edge_ref WHERE key='hub'"),
        1
    );
    assert_eq!(
        internal_count("SELECT COUNT(*) / 2 FROM edge_ref"),
        (2 * FANOUT) as u64
    );
    assert_eq!(count(&g, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "0");
    // Recreating the target restores both intact FILEs' intentions.
    replace("b.rs", 4, &[node("hub", "replaced")], &[]);
    assert_eq!(
        count(&g, "MATCH ()-[r:CALLS]->() RETURN count(r)"),
        (2 * FANOUT).to_string()
    );
    g.remove_file("c.rs", &original()).unwrap();
    g.remove_file("b.rs", &original()).unwrap();
    // Incoming intentions alone keep an unowned locator alive.
    assert_eq!(
        internal_count("SELECT COUNT(DISTINCT key) FROM edge_ref WHERE key='hub'"),
        1
    );
    replace("c.rs", 5, &[node("c", "returned")], &outgoing);
    g.remove_file("a.rs", &original()).unwrap();
    // Outgoing intentions alone also keep the unowned locator alive.
    assert_eq!(
        internal_count("SELECT COUNT(DISTINCT key) FROM edge_ref WHERE key='hub'"),
        1
    );
    g.remove_file("c.rs", &original()).unwrap();
    assert_eq!(internal_count("SELECT COUNT(*) / 2 FROM edge_ref"), 0);
    assert_eq!(internal_count("SELECT COUNT(*) FROM entities"), 0);
}

#[test]
fn file_delta_reuses_unaffected_parquet_and_preserves_sparse_row_addresses() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("graph");
    let graph = LiveGraph::open_path_owned(path.clone(), &original(), [24; 32], true).unwrap();
    let replace = |name, sha, nodes: &[GraphNode], edges: &[GraphEdge]| {
        graph
            .replace_file(
                LiveFileInput {
                    path: name,
                    source_sha256: [sha; 32],
                    raw_version: u64::from(sha),
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
                &original(),
            )
            .unwrap()
    };
    replace("a.rs", 1, &[node("a", "caller")], &[edge()]);
    replace("b.rs", 2, &[node("b", "target")], &[]);
    let unrelated = GraphEdge {
        kind: "REFERENCES".into(),
        source_key: "c".into(),
        target_key: "d".into(),
        uid: "cd".into(),
    };
    replace("c.rs", 3, &[node("c", "unrelated")], &[unrelated]);
    replace("d.rs", 3, &[node("d", "unrelated target")], &[]);
    let retained = graph.state.lock().unwrap().catalog.files["c.rs"].clone();
    let bytes = std::fs::read(path.join(&retained.nodes[0].shard.file)).unwrap();
    let address = retained.nodes[0].row_base;
    let csr = retained
        .relationships
        .iter()
        .flat_map(|member| {
            ["indices", "indptr"].map(|field| {
                let file = member[field].as_str().unwrap().to_owned();
                let bytes = std::fs::read(path.join(&file)).unwrap();
                (file, bytes)
            })
        })
        .collect::<BTreeMap<_, _>>();
    assert!(!csr.is_empty());
    replace("b.rs", 4, &[node("b", "replacement")], &[]);
    let current = graph.state.lock().unwrap().catalog.files["c.rs"].clone();
    assert_eq!(current.nodes[0].shard.file, retained.nodes[0].shard.file);
    assert_eq!(current.nodes[0].row_base, address);
    assert_eq!(current.relationships, retained.relationships);
    for (file, bytes) in csr {
        assert_eq!(std::fs::read(path.join(file)).unwrap(), bytes);
    }
    assert_eq!(
        std::fs::read(path.join(&current.nodes[0].shard.file)).unwrap(),
        bytes
    );
    assert_eq!(count(&graph, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "1");
    let after = disk_image(&path);
    assert!(
        after
            .keys()
            .all(|file| file.extension().and_then(|value| value.to_str()) != Some("lbug"))
    );
    assert!(after.keys().all(|file| !matches!(
        file.extension().and_then(|value| value.to_str()),
        Some("sqlite" | "wal" | "shadow")
    )));
    drop(graph);
    let reopened = LiveGraph::open_path_owned(path.clone(), &original(), [24; 32], false).unwrap();
    assert_eq!(
        count(&reopened, "MATCH ()-[r:CALLS]->() RETURN count(r)"),
        "1"
    );
    assert_eq!(
        reopened.state.lock().unwrap().catalog.files["c.rs"].nodes[0].row_base,
        address
    );
    assert_eq!(after, disk_image(&path));
}

#[test]
fn rollback_readonly_and_cancellation() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let g =
        LiveGraph::open_path_owned(tmp.path().join("graph"), &original(), [9; 32], true).unwrap();
    g.replace_file(
        LiveFileInput {
            path: "a.rs",
            source_sha256: [1; 32],
            raw_version: 1,
            nodes: &[node("a", "one")],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert!(
        g.replace_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [2; 32],
                raw_version: 2,
                nodes: &[node("a", "bad"), node("a", "dup")],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            &original()
        )
        .is_err()
    );
    assert_eq!(
        count(&g, "MATCH (n:Function) WHERE n.name='one' RETURN count(n)"),
        "1"
    );
    assert!(
        g.query(
            "MATCH (n:Function) DELETE n",
            &BTreeMap::new(),
            options(),
            &original()
        )
        .is_err()
    );
    assert!(
        g.query(
            "MATCH (n:_GraphitFile) RETURN n",
            &BTreeMap::new(),
            options(),
            &original()
        )
        .is_err()
    );
    assert!(
        g.query(
            "LOAD FROM '/etc/passwd' RETURN *",
            &BTreeMap::new(),
            options(),
            &original()
        )
        .is_err()
    );
    assert!(
        g.query(
            "CALL show_tables() RETURN *",
            &BTreeMap::new(),
            options(),
            &original()
        )
        .is_err()
    );
    assert!(
        g.query(
            "RETURN 1; RETURN 2",
            &BTreeMap::new(),
            options(),
            &original()
        )
        .is_err()
    );
    let cancel = Cancellation::default();
    cancel.cancel();
    let o = Arc::new(SnapshotContext::service(cancel));
    assert_eq!(
        g.remove_file("a.rs", &o).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(count(&g, "MATCH (n:Function) RETURN count(n)"), "1");
}

#[test]
fn bulk_scalar_values_wire_generation_and_native_deadline() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("graph");
    let g = LiveGraph::open_path_owned(path.clone(), &original(), [7; 32], true).unwrap();
    // Cross one native input vector without changing the FILE transaction.
    const NODE_COUNT: i64 = 2_051;
    let nodes = (0..NODE_COUNT)
        .map(|i| {
            let mut n = node(&format!("e{i}"), &format!("ação_{i}"));
            if i % 2 == 0 {
                n.properties.insert("ordinal".into(), GraphValue::Int64(i));
            }
            if i % 3 == 0 {
                n.properties
                    .insert("enabled".into(), GraphValue::Bool(true));
            }
            n
        })
        .collect::<Vec<_>>();
    let relation = GraphEdge {
        kind: "CALLS".into(),
        source_key: "e0".into(),
        target_key: "e1".into(),
        uid: "e0-e1".into(),
    };
    g.replace_file(
        LiveFileInput {
            path: "bulk.rs",
            source_sha256: [2; 32],
            raw_version: 4,
            nodes: &nodes,
            edges: &[relation],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(
        count(&g, "MATCH(n) RETURN count(n)"),
        NODE_COUNT.to_string()
    );
    assert_eq!(
        g.file_observation("bulk.rs", &original()).unwrap().unwrap(),
        LiveFileObservation {
            path: "bulk.rs".into(),
            source_sha256: [2; 32],
            file_revision: 1,
            raw_version: 4,
            scip_compiler: None,
        }
    );
    assert_eq!(g.files(&original()).unwrap().len(), 1);
    let neighborhood = g
        .neighborhood(
            "Function",
            "e0",
            LiveGraphDirection::Outgoing,
            10,
            options(),
            &original(),
        )
        .unwrap();
    assert_eq!(neighborhood.rows.len(), 1);
    assert_eq!(neighborhood.rows[0][1]["type"], "REL");
    assert_eq!(
        neighborhood.rows[0][0]["value"]["id"]["value"]["generation"],
        json!(g.generation().unwrap())
    );
    assert_eq!(
        neighborhood.rows[0][1]["value"]["id"]["value"]["generation"],
        json!(g.generation().unwrap())
    );
    let scalar = g
        .query(
            "MATCH(n:Function) WHERE n.ordinal=2 RETURN n.name,n.ordinal,n.enabled",
            &BTreeMap::new(),
            options(),
            &original(),
        )
        .unwrap();
    assert_eq!(scalar.rows[0][0]["value"], "ação_2");
    assert_eq!(scalar.rows[0][1]["value"], "2");
    assert!(scalar.rows[0][2]["value"].is_null());
    assert_eq!(g.sample(3, options(), &original()).unwrap().rows.len(), 3);
    let deadline = Arc::new(SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_millis(20),
        Cancellation::default(),
    ));
    let error = g
        .query(
            "UNWIND range(1,10000) AS a UNWIND range(1,10000) AS b RETURN sum(a*b)",
            &BTreeMap::new(),
            options(),
            &deadline,
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_eq!(
        count(&g, "MATCH(n) RETURN count(n)"),
        NODE_COUNT.to_string()
    );
    g.remove_file("bulk.rs", &original()).unwrap();
    assert!(g.files(&original()).unwrap().is_empty());
    drop(g);
    assert!(LiveGraph::open_path_owned(path, &original(), [8; 32], true).is_err());
}

#[test]
fn scip_targets_arrive_change_and_disappear_across_file_commits() {
    use crate::semantic_graph::scip_document::{
        AstScipDocumentDefinition as Definition, AstScipDocumentReference as Reference,
        scip_edge_uid,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("graph");
    let open = || LiveGraph::open_path_owned(path.clone(), &original(), [6; 32], true).unwrap();
    let mut g = open();
    let association = |path: &str, sha: u8| {
        serde_json::from_value(json!({
        "source_observation":{"IndexedStored":{"empty":false}},"path":path,"relative_components":[path],
        "raw_version":7,"source_sha256":([sha;32].to_vec()),"physical_branch":null,"inherited_target":null
    })).unwrap()
    };
    let mut reference = Reference {
        association: association("a.rs", 1),
        path: "a.rs".into(),
        symbol: "scip-rust cargo demo 1 function().".into(),
        start_byte: 5,
        end_byte: 6,
        symbol_roles: 0,
        kind: Some("CALLS".into()),
        source_key: Some("a".into()),
        relation_index: Some(0),
        source_index: Some(0),
        capture_index: Some(0),
        observation_json: Some(json!({"proof":"observed-parser-capture"}).to_string()),
        target_labels: vec!["Function".into()],
        graph_edge_uids: vec!["superseded-parser".into()],
        parser_target: None,
    };
    let definition = Definition {
        association: association("b.rs", 2),
        path: "b.rs".into(),
        symbol: reference.symbol.clone(),
        start_byte: 3,
        end_byte: 4,
        symbol_roles: 1,
        entity_key: "b".into(),
        label: "Function".into(),
    };
    let file = |path: &str, sha: u8, reference: &Reference| {
        GraphNode{label:"File".into(),key:format!("observation:{path}"),properties:BTreeMap::from([
        ("is_file_observation".into(),GraphValue::Bool(true)),("path".into(),GraphValue::String(path.into())),("raw_version".into(),GraphValue::String("7".into())),("source_sha256".into(),GraphValue::String(format!("{sha:02x}").repeat(32))),
        ("diagnostics_json".into(),GraphValue::String("[{\"diagnostic\":\"original\"}]".into())),
        ("capabilities_json".into(),GraphValue::String("[{\"policy\":\"original\"}]".into())),
        ("lang".into(),GraphValue::String("rust".into())),
        ("grammar".into(),GraphValue::String("tree-sitter-rust".into())),
        ("extra_metadata_json".into(),GraphValue::String("{\"extra\":\"ação\"}".into())),
        ("resolution_outcomes_json".into(),GraphValue::String(json!([{"relation_index":0,"source_index":0,"capture_index":0,"target":{"state":"NotFound"},"observation":serde_json::from_str::<Value>(reference.observation_json.as_ref().unwrap()).unwrap()}]).to_string()))
    ])}
    };
    g.replace_file(
        LiveFileInput {
            path: "a.rs",
            source_sha256: [1; 32],
            raw_version: 7,
            nodes: &[node("a", "caller"), file("a.rs", 1, &reference)],
            edges: &[],
            definitions: &[],
            references: &[reference.clone()],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(count(&g, "MATCH(n:Function) RETURN count(n)"), "1");
    let assert_state = |g: &LiveGraph, state: &str| {
        let q = g
            .query(
                "MATCH(n:File {path:'a.rs'}) RETURN n.resolution_outcomes_json,n.diagnostics_json,n.capabilities_json,n.lang,n.grammar,n.extra_metadata_json",
                &BTreeMap::new(),
                options(),
                &original(),
            )
            .unwrap();
        let value: Value = serde_json::from_str(q.rows[0][0]["value"].as_str().unwrap()).unwrap();
        assert_eq!(value[0]["target"]["state"], state);
        assert_eq!(q.rows[0][1]["value"], "[{\"diagnostic\":\"original\"}]");
        assert_eq!(q.rows[0][2]["value"], "[{\"policy\":\"original\"}]");
        assert_eq!(q.rows[0][3]["value"], "rust");
        assert_eq!(q.rows[0][4]["value"], "tree-sitter-rust");
        assert_eq!(q.rows[0][5]["value"], "{\"extra\":\"ação\"}");
    };
    assert_state(&g, "NotFound");
    g.replace_file(
        LiveFileInput {
            path: "b.rs",
            source_sha256: [2; 32],
            raw_version: 7,
            nodes: &[node("b", "target")],
            edges: &[],
            definitions: std::slice::from_ref(&definition),
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_state(&g, "Resolved");
    assert_eq!(count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"), "1");
    let q = g
        .query(
            "MATCH()-[r:CALLS]->() RETURN r.uid",
            &BTreeMap::new(),
            options(),
            &original(),
        )
        .unwrap();
    assert_eq!(
        q.rows[0][0]["value"],
        scip_edge_uid(&reference, &definition).unwrap()
    );
    // Same Source revision can still represent changed SCIP/config facts.
    g.replace_file(
        LiveFileInput {
            path: "b.rs",
            source_sha256: [2; 32],
            raw_version: 7,
            nodes: &[node("b", "updated-target")],
            edges: &[],
            definitions: std::slice::from_ref(&definition),
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_state(&g, "Resolved");
    assert_eq!(
        g.file_observation("b.rs", &original())
            .unwrap()
            .unwrap()
            .file_revision,
        3
    );
    let mut competitor = definition.clone();
    competitor.path = "c.rs".into();
    competitor.association = association("c.rs", 3);
    competitor.entity_key = "c".into();
    g.replace_file(
        LiveFileInput {
            path: "c.rs",
            source_sha256: [3; 32],
            raw_version: 7,
            nodes: &[node("c", "competitor")],
            edges: &[],
            definitions: &[competitor],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_state(&g, "Ambiguous");
    assert_eq!(count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"), "0");
    drop(g);
    g = open();
    g.remove_file("c.rs", &original()).unwrap();
    assert_state(&g, "Resolved");
    assert_eq!(count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"), "1");
    g.remove_file("b.rs", &original()).unwrap();
    assert_state(&g, "Unresolved");
    assert_eq!(count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"), "0");
    // Local symbol spelling cannot join definitions from another FILE.
    reference.symbol = "local 0".into();
    let mut local = definition.clone();
    local.symbol = reference.symbol.clone();
    g.replace_file(
        LiveFileInput {
            path: "a.rs",
            source_sha256: [1; 32],
            raw_version: 7,
            nodes: &[node("a", "caller"), file("a.rs", 1, &reference)],
            edges: &[],
            definitions: &[],
            references: &[reference],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    g.replace_file(
        LiveFileInput {
            path: "b.rs",
            source_sha256: [2; 32],
            raw_version: 7,
            nodes: &[node("b", "local")],
            edges: &[],
            definitions: &[local],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_state(&g, "NotFound");
    assert_eq!(count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"), "0");
}

#[test]
fn export_current_snapshot_and_directory_metadata_after_updates() {
    use graphit_data::graph_writer::IcebugWriter;
    use parquet_graph_fixture::{
        file::reader::{FileReader, SerializedFileReader},
        record::{Field, RowAccessor},
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let g =
        LiveGraph::open_path_owned(tmp.path().join("graph"), &original(), [5; 32], true).unwrap();
    let nodes = (0..257)
        .rev()
        .map(|i| {
            let mut node = node(&format!("e{i:04}"), &format!("ação_{i}"));
            if i % 2 == 0 {
                node.properties.insert(
                    "payload".into(),
                    GraphValue::String(format!("{i}:{}", "ação".repeat(2048))),
                );
            }
            node
        })
        .collect::<Vec<_>>();
    g.replace_file(
        LiveFileInput {
            path: "src/a.rs",
            source_sha256: [1; 32],
            raw_version: 7,
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
        &original(),
    )
    .unwrap();
    let relation = GraphEdge {
        kind: "CALLS".into(),
        source_key: "e0000".into(),
        target_key: "b".into(),
        uid: "current".into(),
    };
    g.replace_file(
        LiveFileInput {
            path: "src/nested/b.rs",
            source_sha256: [2; 32],
            raw_version: 8,
            nodes: &[node("b", "before")],
            edges: &[relation],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    g.replace_file(
        LiveFileInput {
            path: "src-other/removed.rs",
            source_sha256: [3; 32],
            raw_version: 8,
            nodes: &[node("removed", "gone")],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(g.files_under("src", &original()).unwrap().len(), 2);
    assert_eq!(g.files_under("src/a.rs", &original()).unwrap().len(), 1);
    assert!(g.files_under("sr", &original()).unwrap().is_empty());
    assert_eq!(g.files_under("", &original()).unwrap().len(), 3);
    g.remove_file("src-other/removed.rs", &original()).unwrap();
    let writer = g
        .write_snapshot(IcebugWriter::new(original()).unwrap(), &original())
        .unwrap();
    let database = writer.staging_database().unwrap();
    assert_eq!(
        database
            .query_row("SELECT count(*) FROM entities", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        258
    );
    assert_eq!(
        database
            .query_row(
                "SELECT count(*) FROM entities WHERE key='e0256'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        database
            .query_row(
                "SELECT count(*) FROM entities WHERE key='removed'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        database
            .query_row(
                "SELECT count(*) FROM edges WHERE uid='current'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let output = writer.finish().unwrap();
    let manifest: Value = serde_json::from_reader(
        std::fs::File::open(output.directory().join("icebug.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["n_edges"], 1);
    assert_eq!(manifest["node_tables"][0]["rows"], 258);
    let table = manifest["node_tables"][0]["shards"][0]["file"]
        .as_str()
        .unwrap();
    let reader =
        SerializedFileReader::new(std::fs::File::open(output.directory().join(table)).unwrap())
            .unwrap();
    let mut exported = BTreeMap::new();
    for row in reader.get_row_iter(None).unwrap() {
        let row = row.unwrap();
        let payload = match row.get_column_iter().nth(2).unwrap().1 {
            Field::Null => None,
            Field::Str(value) => Some(value.clone()),
            other => panic!("unexpected payload {other:?}"),
        };
        assert!(
            exported
                .insert(
                    row.get_string(0).unwrap().clone(),
                    (row.get_string(1).unwrap().clone(), payload)
                )
                .is_none()
        );
    }
    assert_eq!(exported.remove("b"), Some(("before".into(), None)));
    for i in 0..257 {
        assert_eq!(
            exported.remove(&format!("e{i:04}")),
            Some((
                format!("ação_{i}"),
                (i % 2 == 0).then(|| format!("{i}:{}", "ação".repeat(2048)))
            ))
        );
    }
    assert!(exported.is_empty());
    assert_eq!(count(&g, "WITH 1 AS value RETURN value"), "1");
    assert_eq!(
        g.query(
            "RETURN 1 AS value UNION RETURN 2 AS value",
            &BTreeMap::new(),
            options(),
            &original()
        )
        .unwrap()
        .rows
        .len(),
        2
    );
    assert_eq!(
        g.query(
            "RETURN EXISTS { MATCH(n:Function) WHERE n.entity_key='e0256' }",
            &BTreeMap::new(),
            options(),
            &original()
        )
        .unwrap()
        .rows[0][0]["value"],
        true
    );
}

#[test]
fn batch_scip_references_keep_outcomes_and_edge_identities_associated() {
    use crate::semantic_graph::scip_document::{
        AstScipDocumentDefinition as Definition, AstScipDocumentReference as Reference,
        scip_edge_uid,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let g =
        LiveGraph::open_path_owned(tmp.path().join("graph"), &original(), [4; 32], true).unwrap();
    let association = |path: &str, sha: u8| {
        serde_json::from_value(json!({"source_observation":{"IndexedStored":{"empty":false}},"path":path,"relative_components":[path],"raw_version":7,"source_sha256":([sha;32].to_vec()),"physical_branch":null,"inherited_target":null})).unwrap()
    };
    // Definitions, references and derived links all cross the SDK vector boundary.
    const REFERENCE_COUNT: usize = 2_051;
    let references = (0..REFERENCE_COUNT)
        .map(|i| Reference {
            association: association("a.rs", 1),
            path: "a.rs".into(),
            symbol: if i == 0 {
                "scip-rust cargo demo 1 target().".into()
            } else {
                format!("scip-rust cargo demo 1 extra_{i}().")
            },
            start_byte: i as u64 + 1,
            end_byte: i as u64 + 2,
            symbol_roles: 0,
            kind: Some("CALLS".into()),
            source_key: Some("a".into()),
            relation_index: Some(i),
            source_index: Some(0),
            capture_index: Some(i),
            observation_json: Some(json!({"capture":i}).to_string()),
            target_labels: vec!["Function".into()],
            graph_edge_uids: vec![format!("parser-{i}")],
            parser_target: None,
        })
        .collect::<Vec<_>>();
    let mut outcomes=references.iter().map(|reference|json!({"relation_index":reference.relation_index,"source_index":reference.source_index,"capture_index":reference.capture_index,"target":{"state":"NotFound"},"observation":serde_json::from_str::<Value>(reference.observation_json.as_ref().unwrap()).unwrap()})).collect::<Vec<_>>();
    let file = GraphNode {
        label: "File".into(),
        key: "observation:a.rs".into(),
        properties: BTreeMap::from([
            ("is_file_observation".into(), GraphValue::Bool(true)),
            ("path".into(), GraphValue::String("a.rs".into())),
            ("raw_version".into(), GraphValue::String("7".into())),
            ("source_sha256".into(), GraphValue::String("01".repeat(32))),
            (
                "resolution_outcomes_json".into(),
                GraphValue::String(serde_json::to_string(&outcomes).unwrap()),
            ),
        ]),
    };
    let definition = Definition {
        association: association("b.rs", 2),
        path: "b.rs".into(),
        symbol: references[0].symbol.clone(),
        start_byte: 3,
        end_byte: 4,
        symbol_roles: 1,
        entity_key: "b".into(),
        label: "Function".into(),
    };
    let mut target_nodes = vec![node("b", "target")];
    let mut target_definitions = vec![definition.clone()];
    for index in 1..REFERENCE_COUNT {
        let key = format!("extra-{index}");
        target_nodes.push(node(&key, &format!("extra_{index}")));
        let mut extra = definition.clone();
        extra.entity_key = key;
        extra.symbol = format!("scip-rust cargo demo 1 extra_{index}().");
        target_definitions.push(extra);
    }
    let mut caller_nodes = [node("a", "caller"), file];
    g.replace_file(
        LiveFileInput {
            path: "b.rs",
            source_sha256: [2; 32],
            raw_version: 7,
            nodes: &target_nodes,
            edges: &[],
            definitions: &target_definitions,
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    g.replace_file(
        LiveFileInput {
            path: "a.rs",
            source_sha256: [1; 32],
            raw_version: 7,
            nodes: &caller_nodes,
            edges: &[],
            definitions: &[],
            references: &references,
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    // Verify the official symbol and FILE association across the former native vector boundary.
    {
        let state = g.state.lock().unwrap();
        let definitions = portable::read_facts(
            &g.directory,
            &state.catalog.files["b.rs"].facts,
            &original(),
        )
        .unwrap()
        .definitions;
        assert_eq!(definitions.len(), REFERENCE_COUNT);
        assert_eq!(
            serde_json::to_value(&definitions).unwrap(),
            serde_json::to_value(&target_definitions).unwrap()
        );
        let stored = portable::read_facts(
            &g.directory,
            &state.catalog.files["a.rs"].facts,
            &original(),
        )
        .unwrap()
        .references;
        assert_eq!(stored.len(), REFERENCE_COUNT);
        assert_eq!(
            serde_json::to_value(&stored).unwrap(),
            serde_json::to_value(&references).unwrap()
        );
    }
    assert_eq!(
        count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"),
        REFERENCE_COUNT.to_string()
    );
    let actual = g
        .query(
            "MATCH(s)-[r:CALLS]->(t) RETURN s.entity_key,t.entity_key,r.uid",
            &BTreeMap::new(),
            QueryOptions {
                max_rows: REFERENCE_COUNT,
                max_bytes: 4 * 1024 * 1024,
                ..options()
            },
            &original(),
        )
        .unwrap();
    let expected = references
        .iter()
        .zip(&target_definitions)
        .map(|(reference, definition)| {
            (
                scip_edge_uid(reference, definition).unwrap(),
                definition.entity_key.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut observed = BTreeMap::new();
    for row in actual.rows {
        assert_eq!(row[0]["value"], "a");
        assert!(
            observed
                .insert(
                    row[2]["value"].as_str().unwrap().to_owned(),
                    row[1]["value"].as_str().unwrap().to_owned()
                )
                .is_none()
        );
    }
    assert_eq!(observed, expected);
    g.remove_file("b.rs", &original()).unwrap();
    assert_eq!(count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"), "0");
    g.replace_file(
        LiveFileInput {
            path: "b.rs",
            source_sha256: [2; 32],
            raw_version: 7,
            nodes: &target_nodes,
            edges: &[],
            definitions: &target_definitions,
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(
        count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"),
        REFERENCE_COUNT.to_string()
    );
    // Retain genuine parser intentions at two distant outcome slots. Their
    // proofs must suppress only the corresponding duplicate derived edges.
    let mut parser_references = references.clone();
    let mut parser_edges = Vec::new();
    for index in [0, REFERENCE_COUNT - 1] {
        let target = &target_definitions[index].entity_key;
        parser_references[index].graph_edge_uids.clear();
        parser_references[index].parser_target = Some(target.clone());
        outcomes[index]["target"] = json!({"state":"Resolved","key":target});
        parser_edges.push(GraphEdge {
            kind: "CALLS".into(),
            source_key: "a".into(),
            target_key: target.clone(),
            uid: format!("original-parser-{index}"),
        });
    }
    caller_nodes[1].properties.insert(
        "resolution_outcomes_json".into(),
        GraphValue::String(serde_json::to_string(&outcomes).unwrap()),
    );
    g.replace_file(
        LiveFileInput {
            path: "a.rs",
            source_sha256: [1; 32],
            raw_version: 7,
            nodes: &caller_nodes,
            edges: &parser_edges,
            definitions: &[],
            references: &parser_references,
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(
        count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"),
        REFERENCE_COUNT.to_string()
    );
    assert_eq!(
        count(
            &g,
            &format!(
                "MATCH()-[r:CALLS]->() WHERE r.uid IN ['original-parser-0','original-parser-{}'] RETURN count(r)",
                REFERENCE_COUNT - 1
            )
        ),
        "2"
    );
}

#[test]
fn compiler_enrichment_preserves_source_tag_and_rejects_superseded_file() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let g =
        LiveGraph::open_path_owned(tmp.path().join("graph"), &original(), [3; 32], true).unwrap();
    let nodes = [node("a", "parser")];
    assert_eq!(
        g.replace_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 7,
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
            &original()
        )
        .unwrap(),
        1
    );
    let before = g.generation().unwrap();
    let enriched = [node("a", "compiler")];
    assert_eq!(
        g.enrich_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 7,
                nodes: &enriched,
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            1,
            &original()
        )
        .unwrap(),
        2
    );
    assert_eq!(
        g.file_observation("a.rs", &original())
            .unwrap()
            .unwrap()
            .file_revision,
        1
    );
    assert_ne!(g.generation().unwrap(), before);
    assert_eq!(
        g.enrich_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [2; 32],
                raw_version: 7,
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
            1,
            &original()
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        g.enrich_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 8,
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
            1,
            &original()
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        count(
            &g,
            "MATCH(n:Function) WHERE n.name='compiler' RETURN count(n)"
        ),
        "1"
    );
    assert_eq!(
        g.replace_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 7,
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
            &original()
        )
        .unwrap(),
        3
    );
    assert_eq!(
        g.enrich_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 7,
                nodes: &enriched,
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            1,
            &original()
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        g.file_observation("a.rs", &original())
            .unwrap()
            .unwrap()
            .file_revision,
        3
    );
}

#[test]
fn current_scip_documents_and_family_externals_are_transactional_and_streamed() {
    use ::scip::types::{Document, SymbolInformation};
    use protobuf::Message;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let g =
        LiveGraph::open_path_owned(tmp.path().join("graph"), &original(), [2; 32], true).unwrap();
    let document = Document {
        relative_path: "a.rs".into(),
        language: "rust".into(),
        text: "fn ação() {}\n".repeat(8192),
        ..Default::default()
    };
    let mut bytes = document.write_to_bytes().unwrap();
    // Preserve an original unknown protobuf field as well as the large text.
    bytes.extend_from_slice(&[0xf8, 0x07, 0x2a]);
    let document = Document::parse_from_bytes(&bytes).unwrap();
    let source_sha256 = Sha256::digest(document.text.as_bytes()).into();
    g.replace_file(
        LiveFileInput {
            path: "a.rs",
            source_sha256,
            raw_version: 7,
            nodes: &[node("a", "source")],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: Some(&bytes),
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    let mut later = document.clone();
    later.relative_path = "z.rs".into();
    let later_bytes = later.write_to_bytes().unwrap();
    g.replace_file(
        LiveFileInput {
            path: "z.rs",
            source_sha256,
            raw_version: 7,
            nodes: &[],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: Some(&later_bytes),
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    // A complete path page with no document must not be mistaken for EOF.
    let holes = (0..256)
        .map(|i| format!("0-hole-{i:04}.rs"))
        .chain(["m-hole.rs".into(), "zz-hole.rs".into()])
        .collect::<Vec<_>>();
    for hole in &holes {
        g.replace_file(
            LiveFileInput {
                path: hole,
                source_sha256: [1; 32],
                raw_version: 7,
                nodes: &[],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            &original(),
        )
        .unwrap();
    }
    let mut documents = Vec::new();
    g.visit_scip_documents(&original(), |document| {
        documents.push(document);
        Ok(())
    })
    .unwrap();
    assert_eq!(documents, vec![document, later]);
    assert_eq!(documents[0].write_to_bytes().unwrap(), bytes);
    assert_eq!(documents[1].write_to_bytes().unwrap(), later_bytes);
    let stop = Cancellation::default();
    let cancelled = Arc::new(SnapshotContext::service(stop.clone()));
    let mut visits = 0;
    let error = g
        .visit_scip_documents(&cancelled, |_| {
            visits += 1;
            stop.cancel();
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(visits, 1);
    let wrong = Document {
        relative_path: "other.rs".into(),
        ..Default::default()
    }
    .write_to_bytes()
    .unwrap();
    assert!(
        g.replace_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 7,
                nodes: &[node("a", "bad")],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: Some(&wrong),
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            &original()
        )
        .is_err()
    );
    assert_eq!(
        count(
            &g,
            "MATCH(n:Function) WHERE n.name='source' RETURN count(n)"
        ),
        "1"
    );
    let symbol = |name: String| SymbolInformation {
        documentation: vec![format!("{name}:{}", "documentação".repeat(1024))],
        symbol: name,
        ..Default::default()
    };
    g.replace_scip_externals(
        "rust",
        (0..130).map(|i| Ok(symbol(format!("rust-{i:04}")))),
        &original(),
    )
    .unwrap();
    g.replace_scip_externals(
        "python",
        std::iter::once(Ok(symbol("python".into()))),
        &original(),
    )
    .unwrap();
    let mut names = Vec::new();
    g.visit_scip_externals(&original(), |actual| {
        assert_eq!(actual, symbol(actual.symbol.clone()));
        names.push(actual.symbol);
        Ok(())
    })
    .unwrap();
    let expected = std::iter::once("python".to_owned())
        .chain((0..130).map(|i| format!("rust-{i:04}")))
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
    let stop = Cancellation::default();
    let cancelled = Arc::new(SnapshotContext::service(stop.clone()));
    let mut visits = 0;
    let error = g
        .visit_scip_externals(&cancelled, |_| {
            visits += 1;
            stop.cancel();
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(visits, 1);
    let broken = (0..128)
        .map(|i| Ok(symbol(format!("broken-{i}"))))
        .chain(std::iter::once(Err(io::Error::other(
            "incomplete compiler result",
        ))));
    assert!(
        g.replace_scip_externals("rust", broken, &original())
            .is_err()
    );
    let mut names = Vec::new();
    g.visit_scip_externals(&original(), |symbol| {
        names.push(symbol.symbol);
        Ok(())
    })
    .unwrap();
    assert_eq!(names, expected);
    assert!(!names.iter().any(|name| name.starts_with("broken-")));
    g.replace_scip_externals("rust", std::iter::empty(), &original())
        .unwrap();
    let mut names = Vec::new();
    g.visit_scip_externals(&original(), |symbol| {
        names.push(symbol.symbol);
        Ok(())
    })
    .unwrap();
    assert_eq!(names, vec!["python"]);
    assert!(
        g.visit_scip_documents(&original(), |_| Err(io::Error::other("consumer stopped")))
            .is_err()
    );
    g.remove_file("a.rs", &original()).unwrap();
    g.remove_file("z.rs", &original()).unwrap();
    let mut count = 0;
    g.visit_scip_documents(&original(), |_| {
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn multiple_profile_observations_resolve_only_their_own_outcomes() {
    use crate::semantic_graph::scip_document::{
        AstScipDocumentDefinition as Definition, AstScipDocumentReference as Reference,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let g =
        LiveGraph::open_path_owned(tmp.path().join("graph"), &original(), [1; 32], true).unwrap();
    let association = |path: &str, sha: u8| {
        serde_json::from_value(json!({"source_observation":{"IndexedStored":{"empty":false}},"path":path,"relative_components":[path],"raw_version":7,"source_sha256":([sha;32].to_vec()),"physical_branch":null,"inherited_target":null})).unwrap()
    };
    let reference = |language: &str| Reference {
        association: association("a.sql", 1),
        path: "a.sql".into(),
        symbol: format!("{language}-symbol"),
        start_byte: 5,
        end_byte: 6,
        symbol_roles: 0,
        kind: Some("CALLS".into()),
        source_key: Some(format!("{language}-caller")),
        relation_index: Some(0),
        source_index: Some(0),
        capture_index: Some(0),
        observation_json: Some(json!({"capture":0}).to_string()),
        target_labels: vec!["Function".into()],
        graph_edge_uids: vec![format!("{language}-parser")],
        parser_target: None,
    };
    let mut nodes = Vec::new();
    for (language, grammar) in [("sql", "tree-sitter-sql"), ("db2", "antlr-db2")] {
        let mut source = node(&format!("{language}-caller"), language);
        source
            .properties
            .insert("lang".into(), GraphValue::String(language.into()));
        source
            .properties
            .insert("grammar".into(), GraphValue::String(grammar.into()));
        nodes.push(source);
        nodes.push(GraphNode{label:"AstFileObservation".into(),key:json!(["file_observation","a.sql",[language,grammar]]).to_string(),properties:BTreeMap::from([("is_file_observation".into(),GraphValue::Bool(true)),("path".into(),GraphValue::String("a.sql".into())),("raw_version".into(),GraphValue::String("7".into())),("source_sha256".into(),GraphValue::String("01".repeat(32))),("lang".into(),GraphValue::String(language.into())),("grammar".into(),GraphValue::String(grammar.into())),("resolution_outcomes_json".into(),GraphValue::String(json!([{"relation_index":0,"source_index":0,"capture_index":0,"target":{"state":"NotFound"},"observation":{"capture":0}}]).to_string()))])});
    }
    g.replace_file(
        LiveFileInput {
            path: "a.sql",
            source_sha256: [1; 32],
            raw_version: 7,
            nodes: &nodes,
            edges: &[],
            definitions: &[],
            references: &[reference("sql"), reference("db2")],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    let states = |graph: &LiveGraph| {
        graph
            .query(
                "MATCH(n:AstFileObservation) RETURN n.lang,n.resolution_outcomes_json",
                &BTreeMap::new(),
                options(),
                &original(),
            )
            .unwrap()
            .rows
            .into_iter()
            .map(|row| {
                let outcome: Value =
                    serde_json::from_str(row[1]["value"].as_str().unwrap()).unwrap();
                (
                    row[0]["value"].as_str().unwrap().to_owned(),
                    outcome[0]["target"]["state"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let target = |path: &str, sha: u8, language: &str| Definition {
        association: association(path, sha),
        path: path.into(),
        symbol: format!("{language}-symbol"),
        start_byte: 1,
        end_byte: 2,
        symbol_roles: 1,
        entity_key: format!("{language}-target"),
        label: "Function".into(),
    };
    g.replace_file(
        LiveFileInput {
            path: "b.sql",
            source_sha256: [2; 32],
            raw_version: 7,
            nodes: &[node("sql-target", "target")],
            edges: &[],
            definitions: &[target("b.sql", 2, "sql")],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(
        states(&g),
        BTreeMap::from([
            ("sql".into(), "Resolved".into()),
            ("db2".into(), "NotFound".into())
        ])
    );
    g.replace_file(
        LiveFileInput {
            path: "c.db2",
            source_sha256: [3; 32],
            raw_version: 7,
            nodes: &[node("db2-target", "target")],
            edges: &[],
            definitions: &[target("c.db2", 3, "db2")],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &original(),
    )
    .unwrap();
    assert_eq!(
        states(&g)
            .values()
            .filter(|state| state.as_str() == "Resolved")
            .count(),
        2
    );
    g.remove_file("b.sql", &original()).unwrap();
    assert_eq!(
        states(&g),
        BTreeMap::from([
            ("sql".into(), "Unresolved".into()),
            ("db2".into(), "Resolved".into())
        ])
    );
    assert_eq!(count(&g, "MATCH()-[r:CALLS]->() RETURN count(r)"), "1");
}

#[test]
fn cancellation_after_real_manifest_commit_keeps_cache_and_generation_coherent() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("graph");
    let graph = LiveGraph::open_path_owned(path.clone(), &original(), [11; 32], true).unwrap();
    let stop = Cancellation::default();
    let context = Arc::new(SnapshotContext::service(stop.clone()));
    portable::AFTER_MANIFEST_REPLACE
        .with(|slot| *slot.borrow_mut() = Some(Box::new(move || stop.cancel())));
    let error = graph
        .replace_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 7,
                nodes: &[node("a", "committed")],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            &context,
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(graph.revision().unwrap(), 1);
    assert_eq!(count(&graph, "MATCH(n:Function) RETURN count(n)"), "1");
    drop(graph);
    let reopened = LiveGraph::open_path_owned(path, &original(), [11; 32], false).unwrap();
    assert_eq!(reopened.revision().unwrap(), 1);
    assert_eq!(count(&reopened, "MATCH(n:Function) RETURN count(n)"), "1");
}

#[test]
fn candidate_validation_checks_native_identity_and_file_supersession() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let context = original();
    let g = LiveGraph::open_path_owned(tmp.path().join("graph"), &context, [12; 32], true).unwrap();
    let mut n = node("a", "candidate");
    n.properties.extend([
        ("uid".into(), GraphValue::String("uid-a".into())),
        ("data_key".into(), GraphValue::String("data-a".into())),
    ]);
    let nodes = [n];
    let publish = || {
        g.replace_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 7,
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
            &context,
        )
        .unwrap()
    };
    publish();
    let observed = g.file_observation("a.rs", &context).unwrap().unwrap();
    assert_eq!(observed.raw_version, 7);
    let entity = crate::entity_search::Entity {
        entity_key: "a".into(),
        uid: "uid-a".into(),
        data_key: "data-a".into(),
        name: "candidate".into(),
        kind: "Function".into(),
        path: "a.rs".into(),
        language: "rust".into(),
        grammar: "rust".into(),
        source_sha256: "01".repeat(32),
        extent_status: "observed".into(),
        raw_version: 7,
        start_byte: None,
        end_byte: None,
        start_line: None,
        end_line: None,
    };
    g.validate_entities("a.rs", &observed, &[&entity], &context)
        .unwrap();
    let mut wrong = entity.clone();
    wrong.uid = "wrong-uid".into();
    assert_eq!(
        g.validate_entities("a.rs", &observed, &[&wrong], &context)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    wrong = entity.clone();
    wrong.entity_key = "missing".into();
    assert_eq!(
        g.validate_entities("a.rs", &observed, &[&wrong], &context)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    // Source bytes and Raw version can stay unchanged when the parsing profile changes.
    publish();
    assert_eq!(
        g.validate_entities("a.rs", &observed, &[&entity], &context)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let current = g.file_observation("a.rs", &context).unwrap().unwrap();
    g.validate_entities("a.rs", &current, &[&entity], &context)
        .unwrap();
    g.remove_file("a.rs", &context).unwrap();
    assert_eq!(
        g.validate_entities("a.rs", &current, &[&entity], &context)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn all_null_columns_keep_their_native_scalar_types_across_files() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let context = original();
    let g = LiveGraph::open_path_owned(tmp.path().join("graph"), &context, [13; 32], true).unwrap();
    let mut first = node("a", "typed");
    first.properties.extend([
        ("optional_text".into(), GraphValue::String("value".into())),
        ("optional_number".into(), GraphValue::Int64(42)),
        ("optional_flag".into(), GraphValue::Bool(true)),
    ]);
    g.replace_file(
        LiveFileInput {
            path: "a.rs",
            source_sha256: [1; 32],
            raw_version: 1,
            nodes: &[first],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &context,
    )
    .unwrap();
    g.replace_file(
        LiveFileInput {
            path: "b.rs",
            source_sha256: [2; 32],
            raw_version: 2,
            nodes: &[node("b", "missing"), node("c", "also missing")],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &context,
    )
    .unwrap();
    let result = g
        .query(
            "MATCH(n:Function) WHERE n.entity_key='b' OR n.entity_key='c' RETURN n.optional_text,n.optional_number,n.optional_flag",
            &BTreeMap::new(),
            options(),
            &context,
        )
        .unwrap();
    assert_eq!(result.rows.len(), 2);
    for row in result.rows {
        for (cell, kind) in row.iter().zip(["STRING", "INT64", "BOOL"]) {
            assert_eq!(cell["type"], kind);
            assert!(cell["value"].is_null());
        }
    }
    assert_eq!(
        count(
            &g,
            "MATCH(n:Function) WHERE n.optional_number=42 RETURN count(n)"
        ),
        "1"
    );
}

#[test]
fn pending_compiler_work_survives_reopen_and_clears_only_after_current_enrichment() {
    use graphit_data::graph_writer::IcebugWriter;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("graph");
    let context = original();
    let g = LiveGraph::open_path_owned(path.clone(), &context, [14; 32], true).unwrap();
    // Storage is intentionally opaque: the App owns the genuine batch/Oracle serde types.
    let pending_a = format!(
        r#"[[{{"batch":"a'ação","payload":"{}"}},{{"oracle":"a"}}]]"#,
        "conteúdo".repeat(8192)
    );
    let pending_b = format!(
        r#"[[{{"batch":"b","payload":"{}"}},null]]"#,
        "parser".repeat(8192)
    );
    for (file, sha, key, payload) in [
        ("b.rs", 2, "b", pending_b.as_str()),
        ("a.rs", 1, "a", pending_a.as_str()),
    ] {
        let mut n = node(key, key);
        n.properties.insert("typed".into(), GraphValue::Int64(42));
        g.replace_file(
            LiveFileInput {
                path: file,
                source_sha256: [sha; 32],
                raw_version: u64::from(sha),
                nodes: &[n],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: Some(payload),
                scip_compiler: None,
            },
            &context,
        )
        .unwrap();
    }
    g.replace_file(
        LiveFileInput {
            path: "0-complete.rs",
            source_sha256: [3; 32],
            raw_version: 3,
            nodes: &[],
            edges: &[],
            definitions: &[],
            references: &[],
            scip_document: None,
            oracle: &[],
            oracle_file: None,
            pending_compiler: None,
            scip_compiler: None,
        },
        &context,
    )
    .unwrap();
    assert!(
        g.pending_compiler_file("0-complete.rs", &context)
            .unwrap()
            .is_none()
    );
    assert_eq!(g.files(&context).unwrap().len(), 3);
    assert_eq!(count(&g, "MATCH(n) RETURN count(n)"), "2");
    assert!(
        !g.schema()
            .unwrap()
            .nodes
            .contains_key("_GraphitPendingCompiler")
    );
    assert!(
        g.query(
            "MATCH(p:_GraphitPendingCompiler) RETURN p.path",
            &BTreeMap::new(),
            options(),
            &context
        )
        .is_err()
    );
    let exported = g
        .write_snapshot(IcebugWriter::new(context.clone()).unwrap(), &context)
        .unwrap();
    assert_eq!(
        exported
            .staging_database()
            .unwrap()
            .query_row("SELECT count(*) FROM entities", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(exported);
    let a = g.pending_compiler_file("a.rs", &context).unwrap().unwrap();
    let b = g.pending_compiler_file("b.rs", &context).unwrap().unwrap();
    assert_eq!(a.1, pending_a);
    assert_eq!(b.1, pending_b);
    assert_eq!(a.0.raw_version, 1);
    assert_eq!(a.0.source_sha256, [1; 32]);
    drop(g);
    let g = LiveGraph::open_path_owned(path.clone(), &context, [14; 32], true).unwrap();
    assert_eq!(
        g.pending_compiler_file("a.rs", &context).unwrap(),
        Some(a.clone())
    );
    assert_eq!(
        g.pending_compiler_files(&context).unwrap(),
        [a.0.clone(), b.0.clone()]
    );
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = Arc::new(SnapshotContext::service(cancellation));
    assert_eq!(
        g.pending_compiler_files(&cancelled).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        g.pending_compiler_file("a.rs", &cancelled)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let mut invalid = node("b", "bad");
    invalid
        .properties
        .insert("typed".into(), GraphValue::String("wrong type".into()));
    assert!(
        g.replace_file(
            LiveFileInput {
                path: "b.rs",
                source_sha256: [3; 32],
                raw_version: 3,
                nodes: &[invalid],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: Some("must roll back"),
                scip_compiler: None,
            },
            &context,
        )
        .is_err()
    );
    assert_eq!(
        g.pending_compiler_file("b.rs", &context).unwrap(),
        Some(b.clone())
    );
    let mut valid = node("b", "late failure");
    valid
        .properties
        .insert("typed".into(), GraphValue::Int64(42));
    assert!(
        g.replace_file(
            LiveFileInput {
                path: "b.rs",
                source_sha256: [3; 32],
                raw_version: 3,
                nodes: &[valid],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: Some(&[0xff]),
                oracle: &[],
                oracle_file: None,
                pending_compiler: Some("inserted before invalid SCIP"),
                scip_compiler: None,
            },
            &context,
        )
        .is_err()
    );
    assert_eq!(
        g.pending_compiler_file("b.rs", &context).unwrap(),
        Some(b.clone())
    );
    let mut enriched = node("a", "compiler complete");
    enriched
        .properties
        .insert("typed".into(), GraphValue::Int64(42));
    let input = || LiveFileInput {
        path: "a.rs",
        source_sha256: [1; 32],
        raw_version: 1,
        nodes: std::slice::from_ref(&enriched),
        edges: &[],
        definitions: &[],
        references: &[],
        scip_document: None,
        oracle: &[],
        oracle_file: None,
        pending_compiler: None,
        scip_compiler: None,
    };
    assert_eq!(
        g.enrich_file(input(), a.0.file_revision + 1, &context)
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        g.pending_compiler_file("a.rs", &context).unwrap(),
        Some(a.clone())
    );
    g.enrich_file(input(), a.0.file_revision, &context).unwrap();
    assert!(g.pending_compiler_file("a.rs", &context).unwrap().is_none());
    assert_eq!(g.file_observation("a.rs", &context).unwrap(), Some(a.0));
    assert_eq!(
        g.pending_compiler_files(&context).unwrap(),
        std::slice::from_ref(&b.0)
    );
    assert_eq!(g.pending_compiler_file("b.rs", &context).unwrap(), Some(b));
    g.remove_file("b.rs", &context).unwrap();
    assert!(g.pending_compiler_file("b.rs", &context).unwrap().is_none());
    assert!(g.pending_compiler_files(&context).unwrap().is_empty());
    let manifest_path = path.join("icebug.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["live"]["files"]["a.rs"]["pending_compiler"] = json!("invalid");
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    drop(g);
    assert_eq!(
        LiveGraph::open_path_owned(path, &context, [14; 32], true)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[cfg(unix)]
#[test]
fn cleanup_failure_after_manifest_effect_reopens_the_committed_file() {
    use std::os::unix::fs::symlink;
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("graph");
    let graph = LiveGraph::open_path_owned(path.clone(), &original(), [25; 32], true).unwrap();
    symlink(
        temporary.path().join("absent"),
        path.join("shards/unowned-link"),
    )
    .unwrap();
    let error = graph
        .replace_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 1,
                nodes: &[node("a", "committed")],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            &original(),
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("FILE committed at graph revision 1"),
        "{error}"
    );
    assert_eq!(graph.revision().unwrap(), 1);
    drop(graph);
    let reopened = LiveGraph::open_path_owned(path.clone(), &original(), [25; 32], false).unwrap();
    assert_eq!(
        reopened
            .file_observation("a.rs", &original())
            .unwrap()
            .unwrap()
            .raw_version,
        1
    );
    std::fs::remove_file(path.join("shards/unowned-link")).unwrap();
    reopened.remove_file("a.rs", &original()).unwrap();
    assert_eq!(reopened.revision().unwrap(), 2);
}

#[test]
fn retained_scip_reader_blocks_file_replacement_and_shard_collection() {
    use protobuf::Message;
    use std::sync::mpsc;
    use std::time::Duration;
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("graph");
    let graph =
        Arc::new(LiveGraph::open_path_owned(path.clone(), &original(), [26; 32], true).unwrap());
    let document = ::scip::types::Document {
        relative_path: "a.rs".into(),
        ..Default::default()
    };
    let bytes = document.write_to_bytes().unwrap();
    graph
        .replace_file(
            LiveFileInput {
                path: "a.rs",
                source_sha256: [1; 32],
                raw_version: 1,
                nodes: &[node("a", "retained")],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: Some(&bytes),
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: None,
            },
            &original(),
        )
        .unwrap();
    let retained = graph.state.lock().unwrap().catalog.files["a.rs"].clone();
    let (reading, started) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let reader = Arc::clone(&graph);
    let read = std::thread::spawn(move || {
        reader
            .visit_scip_documents(&original(), |document| {
                assert_eq!(document.relative_path, "a.rs");
                reading.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(10)).unwrap();
                Ok(())
            })
            .unwrap()
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    let (blocked, observed) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let writer = Arc::clone(&graph);
    let write = std::thread::spawn(move || {
        assert!(matches!(
            writer.state.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        blocked.send(()).unwrap();
        writer.remove_file("a.rs", &original()).unwrap();
        done.send(()).unwrap();
    });
    observed.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(finished.try_recv().is_err());
    assert!(path.join(&retained.nodes[0].shard.file).is_file());
    release.send(()).unwrap();
    read.join().unwrap();
    finished.recv_timeout(Duration::from_secs(10)).unwrap();
    write.join().unwrap();
    assert!(!path.join(&retained.nodes[0].shard.file).exists());
    assert!(
        graph
            .file_observation("a.rs", &original())
            .unwrap()
            .is_none()
    );
}

#[test]
fn exact_compiler_identity_survives_reopen_and_retires_only_unreferenced_externals() {
    use ::scip::types::SymbolInformation;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("graph");
    let g = LiveGraph::open_path_owned(path.clone(), &original(), [29; 32], true).unwrap();
    let publish = |path: &str, producer: &str| {
        g.replace_file(
            LiveFileInput {
                path,
                source_sha256: [1; 32],
                raw_version: 1,
                nodes: &[node(path, path)],
                edges: &[],
                definitions: &[],
                references: &[],
                scip_document: None,
                oracle: &[],
                oracle_file: None,
                pending_compiler: None,
                scip_compiler: Some(producer),
            },
            &original(),
        )
        .unwrap();
    };
    publish("a.rs", "registry/parser:one");
    publish("b.rs", "registry/parser:one");
    g.replace_scip_externals(
        "registry/parser:one",
        std::iter::once(Ok(SymbolInformation {
            symbol: "external-one".into(),
            ..Default::default()
        })),
        &original(),
    )
    .unwrap();
    publish("a.rs", "registry/parser:two");
    assert!(
        g.state
            .lock()
            .unwrap()
            .catalog
            .externals
            .contains_key("registry/parser:one"),
        "another FILE still uses the original image"
    );
    publish("b.rs", "registry/parser:two");
    assert!(
        !g.state
            .lock()
            .unwrap()
            .catalog
            .externals
            .contains_key("registry/parser:one")
    );
    assert_eq!(
        g.file_observation("a.rs", &original())
            .unwrap()
            .unwrap()
            .scip_compiler
            .as_deref(),
        Some("registry/parser:two")
    );
    drop(g);
    let reopened = LiveGraph::open_path_owned(path, &original(), [29; 32], false).unwrap();
    assert_eq!(
        reopened
            .file_observation("a.rs", &original())
            .unwrap()
            .unwrap()
            .scip_compiler
            .as_deref(),
        Some("registry/parser:two")
    );
}
