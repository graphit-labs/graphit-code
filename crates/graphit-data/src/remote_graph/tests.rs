use super::*;
use crate::tests::{Values, context, fixture, manifest, policy, sample, save, writer};
use bytes::Bytes;
use graphit_core::file_lock::Cancellation;
use object_store::{
    ObjectStoreExt,
    memory::InMemory,
    throttle::{ThrottleConfig, ThrottledStore},
};
use std::{
    fs,
    time::{Duration, Instant},
};

pub(super) async fn upload(
    store: &dyn ObjectStore,
    prefix: &Path,
    root: &std::path::Path,
) -> [u8; 32] {
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                directories.push(entry.path());
                continue;
            }
            let name = entry
                .path()
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|part| part.as_os_str().to_str().unwrap().to_owned())
                .collect::<Vec<_>>()
                .join("/");
            store
                .put(
                    &object_path(prefix, &name),
                    Bytes::from(fs::read(entry.path()).unwrap()).into(),
                )
                .await
                .unwrap();
        }
    }
    Sha256::digest(fs::read(root.join("icebug.json")).unwrap()).into()
}
pub(super) async fn open_fixture(root: &std::path::Path) -> (Arc<InMemory>, RemoteIcebugGraph) {
    let store = Arc::new(InMemory::new());
    let prefix = Path::from("release/one");
    let digest = upload(store.as_ref(), &prefix, root).await;
    let graph = RemoteIcebugGraph::open(
        store.clone(),
        prefix,
        digest,
        LocalGraphPolicy::new(policy()).unwrap(),
        context(),
    )
    .await
    .unwrap();
    (store, graph)
}
fn all() -> GraphSampleLimits {
    GraphSampleLimits::new(2, 6, 14, 1024, 1000, 1000, 2).unwrap()
}
fn failure<T>(result: Result<T>) -> RemoteGraphError {
    result.err().expect("expected causal failure")
}

#[tokio::test]
async fn sdk_sample_preserves_global_edges_physical_rows_and_nullable_summaries() {
    let dir = fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let output = graph.sample_graph(sample(), context()).await.unwrap();
    assert_eq!(output.edges().len(), 3);
    assert_eq!(output.nodes().len(), 4);
    assert_eq!(output.nodes()[0].name(), Some("ação 🚀"));
    assert_eq!(output.nodes()[0].line(), None);
    assert_eq!(output.nodes()[1].name(), None);
    assert_eq!(output.nodes()[1].line(), Some(7));
    assert_eq!(output.edges()[0].from(), output.edges()[1].from());
    assert_eq!(output.edges()[0].to(), output.edges()[1].to());
    assert_ne!(output.edges()[0].uid(), output.edges()[1].uid());
    assert_eq!(output.edges()[2].to().row(), 2);
    let full = graph.sample_graph(all(), context()).await.unwrap();
    assert_eq!(full.edges().len(), 6);
    assert_eq!(full.nodes().len(), 5);
    assert_eq!(full.edges()[4].uid(), "loop");
    assert_eq!(full.edges()[4].from(), full.edges()[4].to());
    assert_eq!(output.generation(), graph.generation());
}

#[tokio::test]
async fn empty_manifest_is_available_empty_data() {
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), &manifest());
    let (_, graph) = open_fixture(dir.path()).await;
    let output = graph.sample_graph(sample(), context()).await.unwrap();
    assert!(output.nodes().is_empty());
    assert!(output.edges().is_empty());
}

#[tokio::test]
async fn originals_are_operation_scoped_and_store_drops_with_last_graph_owner() {
    for service in [false, true] {
        let dir = fixture();
        let store = Arc::new(InMemory::new());
        let weak_store = Arc::downgrade(&store);
        let prefix = Path::from("release");
        let digest = upload(store.as_ref(), &prefix, dir.path()).await;
        let original = if service {
            Arc::new(SnapshotContext::service(Cancellation::default()))
        } else {
            context()
        };
        let weak_original = Arc::downgrade(&original);
        let graph = RemoteIcebugGraph::open(
            store.clone(),
            prefix,
            digest,
            LocalGraphPolicy::new(policy()).unwrap(),
            original,
        )
        .await
        .unwrap();
        assert!(weak_original.upgrade().is_none());
        let query = if service {
            Arc::new(SnapshotContext::service(Cancellation::default()))
        } else {
            context()
        };
        let weak_query = Arc::downgrade(&query);
        graph.sample_graph(sample(), query).await.unwrap();
        assert!(weak_query.upgrade().is_none());
        drop(store);
        assert!(weak_store.upgrade().is_some());
        drop(graph);
        assert!(weak_store.upgrade().is_none());
    }
}

#[tokio::test]
async fn immutable_prefixes_keep_separate_generations() {
    let dir = fixture();
    let store = Arc::new(InMemory::new());
    let first = Path::from("releases/one");
    let first_digest = upload(store.as_ref(), &first, dir.path()).await;
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    value["generation"] = serde_json::json!("second-generation");
    save(dir.path(), &value);
    let second = Path::from("releases/two");
    let second_digest = upload(store.as_ref(), &second, dir.path()).await;
    let one = RemoteIcebugGraph::open(
        store.clone(),
        first,
        first_digest,
        LocalGraphPolicy::new(policy()).unwrap(),
        context(),
    )
    .await
    .unwrap();
    let two = RemoteIcebugGraph::open(
        store,
        second,
        second_digest,
        LocalGraphPolicy::new(policy()).unwrap(),
        context(),
    )
    .await
    .unwrap();
    assert_ne!(one.generation(), two.generation());
    assert_ne!(
        one.sample_graph(sample(), context()).await.unwrap().nodes()[0].id(),
        two.sample_graph(sample(), context()).await.unwrap().nodes()[0].id()
    );
}

#[tokio::test]
async fn digest_missing_object_and_invalid_magic_preserve_causes() {
    let dir = fixture();
    let store = Arc::new(InMemory::new());
    let prefix = Path::from("release");
    let digest = upload(store.as_ref(), &prefix, dir.path()).await;
    assert!(matches!(
        failure(
            RemoteIcebugGraph::open(
                store.clone(),
                prefix.clone(),
                [0; 32],
                LocalGraphPolicy::new(policy()).unwrap(),
                context()
            )
            .await
        ),
        RemoteGraphError::ManifestDigest
    ));
    store
        .delete(&prefix.clone().join("File.parquet"))
        .await
        .unwrap();
    let error = failure(
        RemoteIcebugGraph::open(
            store.clone(),
            prefix.clone(),
            digest,
            LocalGraphPolicy::new(policy()).unwrap(),
            context(),
        )
        .await,
    );
    assert!(matches!(
        error,
        RemoteGraphError::ObjectStore(object_store::Error::NotFound { .. })
    ));
    assert!(error.source().is_some());
    store
        .put(
            &prefix.clone().join("File.parquet"),
            Bytes::from_static(b"broken footer").into(),
        )
        .await
        .unwrap();
    assert!(matches!(
        failure(
            RemoteIcebugGraph::open(
                store,
                prefix,
                digest,
                LocalGraphPolicy::new(policy()).unwrap(),
                context()
            )
            .await
        ),
        RemoteGraphError::Data(DataReadError::InvalidParquet("magic"))
    ));
}

#[tokio::test]
async fn changed_object_after_open_is_not_silently_adopted() {
    let dir = fixture();
    let (store, graph) = open_fixture(dir.path()).await;
    store
        .put(
            &Path::from("release/one/a.indices"),
            Bytes::from(fs::read(dir.path().join("a.indices")).unwrap()).into(),
        )
        .await
        .unwrap();
    let error = failure(graph.sample_graph(sample(), context()).await);
    assert!(matches!(error, RemoteGraphError::Parquet(_)));
    assert!(error.source().is_some());
}

#[tokio::test]
async fn observed_target_uid_pointer_and_primary_key_integrity_are_checked() {
    for variant in 0..6 {
        let dir = fixture();
        match variant {
            0 | 1 => writer(
                &dir.path().join("a.indices"),
                "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
                &[
                    Values::Int(if variant == 0 {
                        &[9, 0, 2, 1]
                    } else {
                        &[0, 0, 2, 1]
                    }),
                    Values::String(if variant == 1 {
                        &["same", "same", "edge2", "edge3"]
                    } else {
                        &["edge0", "edge1", "edge2", "edge3"]
                    }),
                ],
            ),
            2 | 3 => writer(
                &dir.path().join("a.indptr"),
                "message graph { required int64 ptr (UINT_64); }",
                &[Values::Int(if variant == 2 {
                    &[0, 4, 3]
                } else {
                    &[0, 3, 3]
                })],
            ),
            _ => writer(
                &dir.path().join("Function.parquet"),
                "message graph { optional binary uid (UTF8); required binary name (UTF8); }",
                &[
                    Values::OptionalString(if variant == 4 {
                        &[Some("f0"), Some("f0"), Some("f2")]
                    } else {
                        &[Some("f0"), None, Some("f2")]
                    }),
                    Values::String(&["a", "b", "c"]),
                ],
            ),
        }
        let (_, graph) = open_fixture(dir.path()).await;
        assert!(matches!(
            failure(graph.sample_graph(all(), context()).await),
            RemoteGraphError::Data(
                DataReadError::InvalidCsr(_)
                    | DataReadError::DuplicateEdgeUid
                    | DataReadError::DuplicatePrimaryKey
                    | DataReadError::InvalidParquet("nonnull primary key")
            )
        ));
    }
}

#[tokio::test]
async fn input_and_actual_row_page_string_guards_remain_functional() {
    let dir = fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    for limits in [
        GraphSampleLimits::new(2, 3, 8, 1, 1000, 1000, 2).unwrap(),
        GraphSampleLimits::new(2, 3, 8, 1024, 3, 1000, 2).unwrap(),
        GraphSampleLimits::new(2, 3, 8, 1024, 1000, 1, 1).unwrap(),
    ] {
        assert!(graph.sample_graph(limits, context()).await.is_err());
    }
    let mut restricted = policy();
    restricted.max_footer_bytes = 8;
    let store = Arc::new(InMemory::new());
    let prefix = Path::from("limited");
    let digest = upload(store.as_ref(), &prefix, dir.path()).await;
    assert!(matches!(
        failure(
            RemoteIcebugGraph::open(
                store,
                prefix,
                digest,
                LocalGraphPolicy::new(restricted).unwrap(),
                context()
            )
            .await
        ),
        RemoteGraphError::Data(DataReadError::LimitExceeded("footer bytes"))
    ));
}

#[tokio::test]
async fn malformed_loaded_page_is_rejected_before_sdk_decode() {
    let dir = fixture();
    let path = dir.path().join("a.indices");
    let mut bytes = fs::read(&path).unwrap();
    bytes[4] = 0;
    fs::write(path, bytes).unwrap();
    let (_, graph) = open_fixture(dir.path()).await;
    assert!(matches!(
        failure(graph.sample_graph(sample(), context()).await),
        RemoteGraphError::Parquet(_)
    ));
}

#[tokio::test]
async fn sdk_io_cancel_and_deadline_do_not_cancel_sibling_queries() {
    let dir = fixture();
    let store = Arc::new(ThrottledStore::new(
        InMemory::new(),
        ThrottleConfig::default(),
    ));
    let prefix = Path::from("release");
    let digest = upload(store.as_ref(), &prefix, dir.path()).await;
    let graph = RemoteIcebugGraph::open(
        store.clone(),
        prefix,
        digest,
        LocalGraphPolicy::new(policy()).unwrap(),
        context(),
    )
    .await
    .unwrap();
    store.config_mut(|config| config.wait_get_per_call = Duration::from_millis(250));
    for service in [false, true] {
        let original = if service {
            Arc::new(SnapshotContext::service(Cancellation::default()))
        } else {
            context()
        };
        let weak = Arc::downgrade(&original);
        let cancellation = original.cancellation().clone();
        let mut pending = Box::pin(graph.sample_graph(sample(), original));
        tokio::select! {
            result = &mut pending => panic!("SDK query ended before cancellation: {}", result.is_ok()),
            _ = tokio::time::sleep(Duration::from_millis(20)) => {}
        }
        cancellation.cancel();
        let result = pending.await;
        assert!(
            matches!(failure(result), RemoteGraphError::Original(error) if error.kind() == io::ErrorKind::Interrupted)
        );
        assert!(weak.upgrade().is_none());
    }
    let deadline = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    ));
    assert!(
        matches!(failure(graph.sample_graph(sample(), deadline).await), RemoteGraphError::Original(error) if error.kind() == io::ErrorKind::TimedOut)
    );
    store.config_mut(|config| config.wait_get_per_call = Duration::ZERO);
    assert_eq!(
        graph
            .sample_graph(sample(), context())
            .await
            .unwrap()
            .edges()
            .len(),
        3
    );
}

#[tokio::test]
async fn nested_paths_and_declared_reverse_metadata_use_canonical_release() {
    let dir = fixture();
    fs::create_dir(dir.path().join("nodes")).unwrap();
    fs::rename(
        dir.path().join("File.parquet"),
        dir.path().join("nodes/File.parquet"),
    )
    .unwrap();
    fs::copy(dir.path().join("z.indices"), dir.path().join("rz.indices")).unwrap();
    fs::copy(dir.path().join("z.indptr"), dir.path().join("rz.indptr")).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    value["node_tables"][1]["shards"][0]["file"] = serde_json::json!("nodes/File.parquet");
    value["add_reverse_edges"] = serde_json::json!(true);
    value["relationships"][0]["reverse_members"] = serde_json::json!([
        {"from":"Function","to":"Function","table":"rz","indices":"rz.indices","indptr":"rz.indptr","rows":2}
    ]);
    save(dir.path(), &value);
    let (_, graph) = open_fixture(dir.path()).await;
    assert_eq!(
        graph
            .sample_graph(all(), context())
            .await
            .unwrap()
            .edges()
            .len(),
        6
    );
    fs::write(dir.path().join("rz.indptr"), b"bad reverse metadata").unwrap();
    let store = Arc::new(InMemory::new());
    let prefix = Path::from("malformed");
    let digest = upload(store.as_ref(), &prefix, dir.path()).await;
    assert!(matches!(
        failure(
            RemoteIcebugGraph::open(
                store,
                prefix,
                digest,
                LocalGraphPolicy::new(policy()).unwrap(),
                context()
            )
            .await
        ),
        RemoteGraphError::Data(DataReadError::InvalidParquet("magic"))
    ));
}

#[tokio::test]
async fn file_shards_use_the_same_sparse_ids_and_csr_ranges_over_object_storage() {
    let dir = crate::tests::shard_fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let sampled = graph
        .sample_graph(
            GraphSampleLimits::new(6, 6, 18, 1024, 10000, 10000, 2).unwrap(),
            context(),
        )
        .await
        .unwrap();
    let mut rows = sampled
        .nodes()
        .iter()
        .map(|node| node.id().row())
        .collect::<Vec<_>>();
    rows.sort_unstable();
    assert_eq!(rows, [10, 11, 100, 101, 102, 300]);
    assert_eq!(sampled.edges().len(), 6);
    let request = crate::GraphNeighborhoodRequest {
        anchor_label: "File",
        anchor_key: crate::GraphAnchorKey::String("a.rs"),
        relationship: "CONTAINS",
        other_label: "Function",
        direction: crate::GraphDirection::Outgoing,
        limit: 10,
        cursor: None,
    };
    let page = graph
        .neighborhood_graph(
            request,
            crate::GraphNeighborhoodLimits::new(1024, 16, 10000, 100000, 10000, 2).unwrap(),
            context(),
        )
        .await
        .unwrap();
    assert_eq!(page.anchor().row(), 10);
    assert_eq!(page.edges().len(), 4);
    assert!(
        page.edges()
            .iter()
            .all(|edge| edge.from().row() == 10 && (100..103).contains(&edge.to().row()))
    );
    for node in page
        .nodes()
        .iter()
        .filter(|node| node.label() == "Function")
    {
        assert!(
            node.properties()
                .iter()
                .any(|property| property.name() == "added"
                    && property.value() == crate::GraphPropertyValueRef::Null)
        );
    }
}
