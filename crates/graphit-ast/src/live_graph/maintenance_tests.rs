use super::*;
use graphit_data::graph_writer::IcebugWriter;
use protobuf::Message;

fn publish(graph: &LiveGraph, path: &str, key: &str, target: &str, score: Option<i64>) {
    let mut node = node(key, &format!("ação {key}"));
    node.properties.extend([
        ("uid".into(), GraphValue::String(format!("uid-{key}"))),
        ("data_key".into(), GraphValue::String(format!("data-{key}"))),
        ("path".into(), GraphValue::String(path.into())),
        ("lang".into(), GraphValue::String("rust".into())),
        ("start_byte".into(), GraphValue::Int64(7)),
        ("end_byte".into(), GraphValue::Int64(23)),
    ]);
    if let Some(score) = score {
        node.properties
            .insert("score".into(), GraphValue::Int64(score));
    }
    let edges = if target.is_empty() {
        vec![]
    } else {
        ["CALLS", "REFERENCES"]
            .map(|kind| GraphEdge {
                source_key: key.into(),
                target_key: target.into(),
                kind: kind.into(),
                uid: format!("{kind}:{key}->{target}"),
            })
            .to_vec()
    };
    let document = ::scip::types::Document {
        relative_path: path.into(),
        ..Default::default()
    };
    let bytes = document.write_to_bytes().unwrap();
    graph
        .replace_file(
            LiveFileInput {
                path,
                source_sha256: [7; 32],
                raw_version: 100,
                nodes: &[node],
                edges: &edges,
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
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
fn documents(graph: &LiveGraph) -> Vec<String> {
    let mut paths = Vec::new();
    graph
        .visit_scip_documents(&original(), |document| {
            paths.push(document.relative_path);
            Ok(())
        })
        .unwrap();
    paths
}

#[test]
fn consolidation_preserves_identities_null_ranges_and_file_updates() {
    let runtime = runtime();
    let _entered = runtime.enter();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("graph");
    let graph = LiveGraph::open_path_owned(path.clone(), &original(), [30; 32], true).unwrap();
    publish(&graph, "a.rs", "a", "b", Some(42));
    publish(&graph, "b.rs", "b", "a", None);
    let observations = graph.files(&original()).unwrap();
    let old = graph.state.lock().unwrap().catalog.clone();
    let query =
        "MATCH (n:Function) RETURN n.uid,n.name,n.score,n.start_byte,n.end_byte ORDER BY n.uid";
    let before = graph
        .query(query, &BTreeMap::new(), options(), &original())
        .unwrap();
    assert_eq!(before.rows[0][2]["value"], "42");
    assert!(before.rows[1][2]["value"].is_null());
    let report = graph.maintain_shards(&original()).unwrap();
    assert_eq!(report.consolidated_files, 2);
    assert!(!report.more);
    assert!(report.reclaimed_files > 0);
    assert!(report.reclaimed_bytes > 0);
    assert_eq!(graph.files(&original()).unwrap(), observations);
    assert_eq!(
        graph
            .query(query, &BTreeMap::new(), options(), &original())
            .unwrap()
            .rows,
        before.rows
    );
    assert_eq!(count(&graph, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "2");
    assert_eq!(
        count(
            &graph,
            "MATCH (a:Function)-[:CALLS]->(b:Function)-[:CALLS]->(a) RETURN count(a)"
        ),
        "2"
    );
    assert_eq!(
        count(&graph, "MATCH ()-[r:REFERENCES]->() RETURN count(r)"),
        "2"
    );
    assert_eq!(documents(&graph), vec!["a.rs", "b.rs"]);
    let shared = graph.state.lock().unwrap().catalog.clone();
    assert_eq!(shared.files["a.rs"].facts, shared.files["b.rs"].facts);
    assert_ne!(
        shared.files["a.rs"].facts_row,
        shared.files["b.rs"].facts_row
    );
    assert_eq!(
        shared.files["a.rs"].nodes[0].shard.file,
        shared.files["b.rs"].nodes[0].shard.file
    );
    for name in ["a.rs", "b.rs"] {
        assert_eq!(
            shared.files[name].nodes[0].row_base,
            old.files[name].nodes[0].row_base
        );
    }
    let stable = disk_image(&path);
    let cleanup = graph.maintain_shards(&original()).unwrap();
    assert_eq!(cleanup.consolidated_files, 0);
    assert_eq!(cleanup.reclaimed_files, 0);
    assert_eq!(
        stable,
        disk_image(&path),
        "already consolidated storage is unchanged"
    );
    let writer = graph
        .write_snapshot(IcebugWriter::new(original()).unwrap(), &original())
        .unwrap();
    drop(graph);
    let graph = LiveGraph::open_path_owned(path.clone(), &original(), [30; 32], false).unwrap();
    publish(&graph, "a.rs", "a", "b", Some(99));
    assert!(
        path.join(&shared.files["b.rs"].nodes[0].shard.file)
            .is_file()
    );
    assert_eq!(documents(&graph), vec!["a.rs", "b.rs"]);
    graph.remove_file("a.rs", &original()).unwrap();
    assert_eq!(count(&graph, "MATCH(n:Function) RETURN count(n)"), "1");
    assert_eq!(count(&graph, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "0");
    assert_eq!(
        graph
            .maintain_shards(&original())
            .unwrap()
            .consolidated_files,
        1
    );
    assert_eq!(documents(&graph), vec!["b.rs"]);
    assert_eq!(
        graph
            .file_observation("b.rs", &original())
            .unwrap()
            .unwrap(),
        observations[1]
    );
    let exported = writer.finish().unwrap();
    let exported = graphit_data::CapturedLocalGraph::capture_generated_metadata(
        &exported.directory(),
        original(),
        graphit_data::LocalGraphPolicy::resolve(
            &graphit_core::config::ConfigSnapshot::new(&graphit_core::brand::Brand::default(), [])
                .unwrap(),
            &original(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        exported
            .schema()
            .unwrap()
            .node_tables()
            .map(|table| table.rows())
            .sum::<u64>(),
        2
    );
    drop(graph);
    let graph = LiveGraph::open_path_owned(path, &original(), [30; 32], false).unwrap();
    assert_eq!(count(&graph, "MATCH(n:Function) RETURN count(n)"), "1");
}

#[test]
fn consolidation_rejects_a_candidate_from_another_file_slice() {
    let temp = tempfile::tempdir().unwrap();
    let graph =
        LiveGraph::open_path_owned(temp.path().join("graph"), &original(), [31; 32], true).unwrap();
    publish(&graph, "a.rs", "a", "", None);
    publish(&graph, "b.rs", "b", "", None);
    graph.maintain_shards(&original()).unwrap();
    let observed = graph
        .file_observation("a.rs", &original())
        .unwrap()
        .unwrap();
    let mut candidate = crate::entity_search::Entity {
        entity_key: "a".into(),
        uid: "uid-a".into(),
        data_key: "data-a".into(),
        name: "ação a".into(),
        kind: "Function".into(),
        path: "a.rs".into(),
        language: "rust".into(),
        grammar: "rust".into(),
        source_sha256: "07".repeat(32),
        extent_status: "observed".into(),
        raw_version: 100,
        start_byte: Some(7),
        end_byte: Some(23),
        start_line: None,
        end_line: None,
    };
    graph
        .validate_entities("a.rs", &observed, &[&candidate], &original())
        .unwrap();
    candidate.entity_key = "b".into();
    candidate.uid = "uid-b".into();
    candidate.data_key = "data-b".into();
    assert_eq!(
        graph
            .validate_entities("a.rs", &observed, &[&candidate], &original())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn interrupted_consolidation_never_publishes_partial_and_tracks_confirmed_effect() {
    let runtime = runtime();
    let _entered = runtime.enter();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("graph");
    let graph = LiveGraph::open_path_owned(path.clone(), &original(), [32; 32], true).unwrap();
    publish(&graph, "a.rs", "a", "b", None);
    publish(&graph, "b.rs", "b", "a", None);
    let old = std::fs::read(path.join("icebug.json")).unwrap();
    let observations = graph.files(&original()).unwrap();
    let revision = graph.revision().unwrap();
    let stop = Cancellation::default();
    let context = Arc::new(SnapshotContext::service(stop.clone()));
    portable::BEFORE_MANIFEST_REPLACE
        .with(|slot| *slot.borrow_mut() = Some(Box::new(move || stop.cancel())));
    assert_eq!(
        graph.maintain_shards(&context).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(std::fs::read(path.join("icebug.json")).unwrap(), old);
    assert_eq!(graph.revision().unwrap(), revision);
    assert_eq!(count(&graph, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "2");
    let stop = Cancellation::default();
    let context = Arc::new(SnapshotContext::service(stop.clone()));
    portable::AFTER_MANIFEST_REPLACE
        .with(|slot| *slot.borrow_mut() = Some(Box::new(move || stop.cancel())));
    let error = graph.maintain_shards(&context).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(graph.revision().unwrap(), revision + 1);
    assert_ne!(std::fs::read(path.join("icebug.json")).unwrap(), old);
    assert_eq!(graph.files(&original()).unwrap(), observations);
    drop(graph);
    let graph = LiveGraph::open_path_owned(path, &original(), [32; 32], false).unwrap();
    assert_eq!(graph.revision().unwrap(), revision + 1);
    assert_eq!(count(&graph, "MATCH ()-[r:CALLS]->() RETURN count(r)"), "2");
    assert_eq!(documents(&graph), vec!["a.rs", "b.rs"]);
    let cleanup = graph.maintain_shards(&original()).unwrap();
    assert_eq!(cleanup.consolidated_files, 0);
    assert!(
        cleanup.reclaimed_files > 0,
        "interrupted staging must be reclaimed"
    );
}

#[test]
fn active_graph_operation_blocks_consolidation_then_new_read_uses_publication() {
    use std::{sync::mpsc, time::Duration};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("graph");
    let graph =
        Arc::new(LiveGraph::open_path_owned(path.clone(), &original(), [33; 32], true).unwrap());
    publish(&graph, "a.rs", "a", "b", None);
    publish(&graph, "b.rs", "b", "a", None);
    let retained = graph.state.lock().unwrap().catalog.files["a.rs"].nodes[0]
        .shard
        .file
        .clone();
    let (reading, started) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let reader = Arc::clone(&graph);
    let read = std::thread::spawn(move || {
        reader
            .with_graph(&original(), |_| {
                reading.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(path.join(retained).is_file());
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
        done.send(writer.maintain_shards(&original()).unwrap())
            .unwrap();
    });
    observed.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(finished.try_recv().is_err());
    release.send(()).unwrap();
    read.join().unwrap();
    assert_eq!(
        finished
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .consolidated_files,
        2
    );
    write.join().unwrap();
    assert_eq!(documents(&graph), vec!["a.rs", "b.rs"]);
}

#[test]
fn maintenance_releases_one_bounded_batch_and_derives_remaining_work() {
    let temp = tempfile::tempdir().unwrap();
    let graph =
        LiveGraph::open_path_owned(temp.path().join("graph"), &original(), [34; 32], true).unwrap();
    for index in 0..130 {
        let key = format!("f{index:03}");
        publish(&graph, &format!("{key}.rs"), &key, "", None);
    }
    let observations = graph.files(&original()).unwrap();
    let orphan = graph.directory.join("shards/interrupted-stage");
    std::fs::create_dir(&orphan).unwrap();
    std::fs::write(orphan.join("partial.parquet"), b"unpublished staging").unwrap();
    let first = graph.maintain_shards(&original()).unwrap();
    assert_eq!(first.consolidated_files, 128);
    assert!(first.more);
    assert_eq!(first.reclaimed_files, 0);
    assert!(orphan.is_dir(), "intermediate batches defer collection");
    let second = graph.maintain_shards(&original()).unwrap();
    assert_eq!(second.consolidated_files, 2);
    assert!(!second.more);
    assert!(second.reclaimed_files > 0);
    assert!(!orphan.exists(), "final batch collects interrupted staging");
    assert_eq!(graph.files(&original()).unwrap(), observations);
    assert_eq!(
        graph
            .maintain_shards(&original())
            .unwrap()
            .consolidated_files,
        0
    );
}

#[test]
fn lexical_insert_and_delete_preserve_unaffected_physical_bundles() {
    fn refs(file: &portable::FileRecord) -> Value {
        json!({"facts":file.facts,"facts_row":file.facts_row,
            "nodes":file.nodes,"relationships":file.relationships})
    }
    let runtime = runtime();
    let _entered = runtime.enter();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("graph");
    let graph = LiveGraph::open_path_owned(path.clone(), &original(), [35; 32], true).unwrap();
    for index in 0..256 {
        let key = format!("f{index:03}");
        publish(&graph, &format!("{key}.rs"), &key, "", None);
    }
    let first = graph.maintain_shards(&original()).unwrap();
    assert_eq!(first.consolidated_files, 128);
    assert!(first.more);
    let second = graph.maintain_shards(&original()).unwrap();
    assert_eq!(second.consolidated_files, 128);
    assert!(!second.more);
    let before = graph
        .state
        .lock()
        .unwrap()
        .catalog
        .files
        .iter()
        .map(|(path, file)| (path.clone(), refs(file)))
        .collect::<BTreeMap<_, _>>();
    publish(&graph, "000.rs", "new-first", "", None);
    let inserted = graph.maintain_shards(&original()).unwrap();
    assert_eq!(
        inserted.consolidated_files, 0,
        "new path cannot split full bundles"
    );
    assert!(!inserted.more);
    {
        let state = graph.state.lock().unwrap();
        for (path, expected) in &before {
            assert_eq!(refs(&state.catalog.files[path]), *expected, "{path}");
        }
    }
    graph.remove_file("f000.rs", &original()).unwrap();
    let deleted = graph.maintain_shards(&original()).unwrap();
    assert_eq!(
        deleted.consolidated_files, 128,
        "only the bundle with a hole and the fresh FILE are repacked"
    );
    assert!(!deleted.more);
    {
        let state = graph.state.lock().unwrap();
        for index in 128..256 {
            let path = format!("f{index:03}.rs");
            assert_eq!(
                refs(&state.catalog.files[&path]),
                before[&path],
                "unaffected {path}"
            );
        }
    }
    let observations = graph.files(&original()).unwrap();
    assert_eq!(observations.len(), 256);
    drop(graph);
    let reopened = LiveGraph::open_path_owned(path, &original(), [35; 32], false).unwrap();
    assert_eq!(reopened.files(&original()).unwrap(), observations);
    assert_eq!(count(&reopened, "MATCH(n:Function) RETURN count(n)"), "256");
    assert_eq!(
        count(
            &reopened,
            "MATCH(n:Function {uid:'uid-new-first'}) RETURN count(n)"
        ),
        "1"
    );
    assert_eq!(
        reopened
            .maintain_shards(&original())
            .unwrap()
            .consolidated_files,
        0
    );
}
