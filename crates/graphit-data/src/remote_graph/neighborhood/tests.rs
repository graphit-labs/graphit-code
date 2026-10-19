use super::*;
use crate::{
    GraphNeighborhoodCursor, GraphPropertyValueRef,
    remote_graph::tests::{open_fixture, upload},
    tests::{Values, context, fixture, policy, save, writer},
};
use graphit_core::file_lock::Cancellation;
use object_store::{
    memory::InMemory,
    path::Path,
    throttle::{ThrottleConfig, ThrottledStore},
};
use std::{
    fs, io,
    time::{Duration, Instant},
};

fn limits() -> GraphNeighborhoodLimits {
    GraphNeighborhoodLimits::new(1024, 16, 1000, 1000, 1000, 2).unwrap()
}
fn outgoing(limit: usize) -> GraphNeighborhoodRequest<'static> {
    GraphNeighborhoodRequest {
        anchor_label: "File",
        anchor_key: GraphAnchorKey::String("a.rs"),
        relationship: "CONTAINS",
        other_label: "Function",
        direction: GraphDirection::Outgoing,
        limit,
        cursor: None,
    }
}
fn failure<T>(result: Result<T>) -> DataReadError {
    fn data(error: super::super::RemoteGraphError) -> DataReadError {
        match error {
            super::super::RemoteGraphError::Data(error) => error,
            super::super::RemoteGraphError::Parquet(parquet::errors::ParquetError::External(
                cause,
            )) => data(
                *cause
                    .downcast::<super::super::RemoteGraphError>()
                    .expect("typed adapter cause"),
            ),
            other => panic!("unexpected cause: {other}"),
        }
    }
    data(result.err().expect("expected typed failure"))
}

fn property<'a>(node: &'a GraphPropertyNode, name: &str) -> GraphPropertyValueRef<'a> {
    node.properties()
        .iter()
        .find(|value| value.name() == name)
        .unwrap()
        .value()
}

#[tokio::test]
async fn incoming_pointer_gaps_resolve_physical_sources_and_malformed_prefixes_fail() {
    for variant in 0..3 {
        let dir = fixture();
        writer(
            &dir.path().join("a.indptr"),
            "message graph { required int64 ptr (UINT_64); }",
            &[Values::Int(match variant {
                0 => &[0, 0, 4],
                1 => &[1, 4, 4],
                _ => &[0, 0, 0],
            })],
        );
        let (_, graph) = open_fixture(dir.path()).await;
        let request = GraphNeighborhoodRequest {
            anchor_label: "Function",
            anchor_key: GraphAnchorKey::String("f0"),
            relationship: "CONTAINS",
            other_label: "File",
            direction: GraphDirection::Incoming,
            limit: 2,
            cursor: None,
        };
        let result = graph.neighborhood_graph(request, limits(), context()).await;
        if variant == 0 {
            let page = result.unwrap();
            assert_eq!(page.edges().len(), 2);
            assert_eq!(page.edges()[0].from().row(), 1);
            let source = page.node(page.edges()[0].from()).unwrap();
            assert_eq!(
                property(source, "path"),
                GraphPropertyValueRef::String("isolated.rs")
            );
            assert_eq!(property(source, "name"), GraphPropertyValueRef::Null);
        } else {
            assert!(matches!(failure(result), DataReadError::InvalidCsr(_)));
        }
    }
}

#[tokio::test]
async fn outgoing_properties_parallel_edges_and_complete_keyset_pages() {
    let dir = fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let first = graph
        .neighborhood_graph(outgoing(2), limits(), context())
        .await
        .unwrap();
    assert_eq!(
        first
            .edges()
            .iter()
            .map(|edge| edge.uid())
            .collect::<Vec<_>>(),
        ["edge0", "edge1"]
    );
    assert_eq!(first.nodes().len(), 2);
    assert_eq!(first.edges()[0].to(), first.edges()[1].to());
    assert_ne!(first.edges()[0].id(), first.edges()[1].id());
    let anchor = first.node(first.anchor()).unwrap();
    assert_eq!(
        property(anchor, "name"),
        GraphPropertyValueRef::String("ação 🚀")
    );
    assert_eq!(
        property(anchor, "ignored"),
        GraphPropertyValueRef::Bool(true)
    );
    assert_eq!(
        property(anchor, "line_number"),
        GraphPropertyValueRef::Int64(0)
    );
    assert_eq!(
        property(anchor, "path"),
        GraphPropertyValueRef::String("a.rs")
    );
    assert!(
        first.edges()[0]
            .properties()
            .iter()
            .all(|property| property.name() != "target")
    );
    assert!(first.has_more());
    let mut next = outgoing(2);
    next.cursor = first.next_cursor();
    let second = graph
        .neighborhood_graph(next, limits(), context())
        .await
        .unwrap();
    assert_eq!(
        second
            .edges()
            .iter()
            .map(|edge| edge.uid())
            .collect::<Vec<_>>(),
        ["edge2", "edge3"]
    );
    assert!(!second.has_more());
    assert!(second.next_cursor().is_none());
    assert_eq!(first.anchor(), second.anchor());
    assert_eq!(first.generation(), graph.generation());
    assert_eq!(first.edges()[0].from(), first.anchor());
    assert_eq!(second.edges()[0].to().row(), 2);
}

#[tokio::test]
async fn incoming_resolves_forward_sources_and_preserves_parallel_edges() {
    let dir = fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let request = GraphNeighborhoodRequest {
        anchor_label: "Function",
        anchor_key: GraphAnchorKey::String("f0"),
        relationship: "CONTAINS",
        other_label: "File",
        direction: GraphDirection::Incoming,
        limit: 1,
        cursor: None,
    };
    let first = graph
        .neighborhood_graph(request, limits(), context())
        .await
        .unwrap();
    assert_eq!(first.edges()[0].uid(), "edge0");
    assert_eq!(first.edges()[0].to(), first.anchor());
    assert_eq!(first.edges()[0].from().row(), 0);
    let request = GraphNeighborhoodRequest {
        anchor_label: "Function",
        anchor_key: GraphAnchorKey::String("f0"),
        relationship: "CONTAINS",
        other_label: "File",
        direction: GraphDirection::Incoming,
        limit: 1,
        cursor: first.next_cursor(),
    };
    let second = graph
        .neighborhood_graph(request, limits(), context())
        .await
        .unwrap();
    assert_eq!(second.edges()[0].uid(), "edge1");
    assert_eq!(first.edges()[0].from(), second.edges()[0].from());
    assert!(!second.has_more());
}

#[tokio::test]
async fn self_loop_once_in_each_direction_and_isolated_anchor_properties() {
    let dir = fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    for direction in [GraphDirection::Outgoing, GraphDirection::Incoming] {
        let request = GraphNeighborhoodRequest {
            anchor_label: "Function",
            anchor_key: GraphAnchorKey::String("f0"),
            relationship: "CALLS",
            other_label: "Function",
            direction,
            limit: 10,
            cursor: None,
        };
        let page = graph
            .neighborhood_graph(request, limits(), context())
            .await
            .unwrap();
        assert_eq!(page.edges().len(), 1);
        assert_eq!(page.edges()[0].uid(), "loop");
        assert_eq!(page.edges()[0].from(), page.edges()[0].to());
        assert_eq!(page.nodes().len(), 1);
    }
    let mut request = outgoing(10);
    request.anchor_key = GraphAnchorKey::String("isolated.rs");
    let page = graph
        .neighborhood_graph(request, limits(), context())
        .await
        .unwrap();
    assert!(page.edges().is_empty());
    assert_eq!(page.nodes().len(), 1);
    let anchor = page.node(page.anchor()).unwrap();
    assert_eq!(property(anchor, "name"), GraphPropertyValueRef::Null);
    assert_eq!(
        property(anchor, "ignored"),
        GraphPropertyValueRef::Bool(false)
    );
    assert_eq!(
        property(anchor, "line_number"),
        GraphPropertyValueRef::Int64(7)
    );
}

#[tokio::test]
async fn integer_anchor_and_nullable_edge_properties_remain_typed() {
    let dir = fixture();
    writer(
        &dir.path().join("Function.parquet"),
        "message graph { required int64 key; required binary name (UTF8); }",
        &[
            Values::Int(&[-7, 0, i64::MAX]),
            Values::String(&["alpha", "beta", "gamma"]),
        ],
    );
    writer(
        &dir.path().join("z.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); optional binary detail (UTF8); required boolean enabled; required int64 weight; }",
        &[
            Values::Int(&[0, 1]),
            Values::String(&["loop", "other"]),
            Values::OptionalString(&[None, Some("é")]),
            Values::Bool(&[true, false]),
            Values::Int(&[-3, 0]),
        ],
    );
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    value["node_tables"][0]["primary_key"] = serde_json::json!("key");
    value["node_tables"][0]["columns"][0]["name"] = serde_json::json!("key");
    value["node_tables"][0]["columns"][0]["type"] = serde_json::json!("INT64");
    save(dir.path(), &value);
    let (_, graph) = open_fixture(dir.path()).await;
    let request = GraphNeighborhoodRequest {
        anchor_label: "Function",
        anchor_key: GraphAnchorKey::Int64(-7),
        relationship: "CALLS",
        other_label: "Function",
        direction: GraphDirection::Outgoing,
        limit: 10,
        cursor: None,
    };
    let page = graph
        .neighborhood_graph(request, limits(), context())
        .await
        .unwrap();
    assert_eq!(
        property(page.node(page.anchor()).unwrap(), "key"),
        GraphPropertyValueRef::Int64(-7)
    );
    let props = page.edges()[0].properties();
    assert_eq!(
        props
            .iter()
            .map(|property| property.name())
            .collect::<Vec<_>>(),
        ["detail", "enabled", "uid", "weight"]
    );
    assert_eq!(props[0].value(), GraphPropertyValueRef::Null);
    assert_eq!(props[1].value(), GraphPropertyValueRef::Bool(true));
    assert_eq!(props[3].value(), GraphPropertyValueRef::Int64(-3));
}

#[tokio::test]
async fn uid_order_is_independent_of_physical_row_order_and_cursor_limit() {
    let dir = fixture();
    writer(
        &dir.path().join("a.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[
            Values::Int(&[0, 0, 2, 1]),
            Values::String(&["z", "a", "m", "b"]),
        ],
    );
    let (_, graph) = open_fixture(dir.path()).await;
    let first = graph
        .neighborhood_graph(outgoing(1), limits(), context())
        .await
        .unwrap();
    assert_eq!(first.edges()[0].uid(), "a");
    assert_eq!(first.edges()[0].id().row(), 1);
    let mut request = outgoing(2);
    request.cursor = first.next_cursor();
    let second = graph
        .neighborhood_graph(request, limits(), context())
        .await
        .unwrap();
    assert_eq!(
        second
            .edges()
            .iter()
            .map(|edge| edge.uid())
            .collect::<Vec<_>>(),
        ["b", "m"]
    );
    assert_eq!(second.edges()[0].id().row(), 3);
    let mut request = outgoing(2);
    request.cursor = second.next_cursor();
    let last = graph
        .neighborhood_graph(request, limits(), context())
        .await
        .unwrap();
    assert_eq!(last.edges()[0].uid(), "z");
    assert!(!last.has_more());
}

#[tokio::test]
async fn cursor_rejects_generation_query_and_empty_uid_without_restoring_authority() {
    let dir = fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let first = graph
        .neighborhood_graph(outgoing(1), limits(), context())
        .await
        .unwrap();
    let cursor = first.next_cursor().unwrap();
    for variant in 0..3 {
        let mut request = outgoing(1);
        request.cursor = Some(match variant {
            0 => GraphNeighborhoodCursor::from_parts(
                crate::GraphGeneration([0; 32]),
                cursor.query_fingerprint(),
                cursor.last_uid(),
            ),
            1 => {
                GraphNeighborhoodCursor::from_parts(cursor.generation(), [0; 32], cursor.last_uid())
            }
            _ => GraphNeighborhoodCursor::from_parts(
                cursor.generation(),
                cursor.query_fingerprint(),
                "",
            ),
        });
        assert!(matches!(
            failure(graph.neighborhood_graph(request, limits(), context()).await),
            DataReadError::InvalidCursor
        ));
    }
    let mut changed = outgoing(1);
    changed.anchor_key = GraphAnchorKey::String("isolated.rs");
    changed.cursor = Some(cursor);
    assert!(matches!(
        failure(graph.neighborhood_graph(changed, limits(), context()).await),
        DataReadError::InvalidCursor
    ));
}

#[tokio::test]
async fn invalid_names_types_endpoints_missing_anchor_and_actual_guards_are_errors() {
    let dir = fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let mut request = outgoing(0);
    assert!(matches!(
        failure(graph.neighborhood_graph(request, limits(), context()).await),
        DataReadError::InvalidNeighborhoodRequest
    ));
    request = outgoing(1);
    request.anchor_label = "Unknown";
    assert!(matches!(
        failure(graph.neighborhood_graph(request, limits(), context()).await),
        DataReadError::UnknownNodeLabel
    ));
    request = outgoing(1);
    request.relationship = "MISSING";
    assert!(matches!(
        failure(graph.neighborhood_graph(request, limits(), context()).await),
        DataReadError::UnknownRelationship
    ));
    request = outgoing(1);
    request.relationship = "CALLS";
    assert!(matches!(
        failure(graph.neighborhood_graph(request, limits(), context()).await),
        DataReadError::UnavailableRelationshipEndpoint
    ));
    request = outgoing(1);
    request.anchor_key = GraphAnchorKey::Int64(0);
    assert!(matches!(
        failure(graph.neighborhood_graph(request, limits(), context()).await),
        DataReadError::InvalidAnchorKey
    ));
    request = outgoing(1);
    request.anchor_key = GraphAnchorKey::String("not-found");
    assert!(matches!(
        failure(graph.neighborhood_graph(request, limits(), context()).await),
        DataReadError::AnchorNotFound
    ));
    for guarded in [
        GraphNeighborhoodLimits::new(1024, 1, 1000, 1000, 1000, 2).unwrap(),
        GraphNeighborhoodLimits::new(1024, 16, 2, 1000, 1000, 2).unwrap(),
        GraphNeighborhoodLimits::new(1024, 16, 1000, 1, 1000, 1).unwrap(),
        GraphNeighborhoodLimits::new(1024, 16, 1000, 1000, 1, 1).unwrap(),
    ] {
        assert!(matches!(
            failure(
                graph
                    .neighborhood_graph(outgoing(1), guarded, context())
                    .await
            ),
            DataReadError::LimitExceeded(_)
        ));
    }
}

#[tokio::test]
async fn duplicate_anchor_uid_and_observed_csr_are_rejected() {
    for variant in 0..4 {
        let dir = fixture();
        match variant {
            0 => writer(
                &dir.path().join("File.parquet"),
                "message graph { required binary path (UTF8); optional binary name (UTF8); required int64 line_number; required boolean ignored; }",
                &[
                    Values::String(&["a.rs", "a.rs"]),
                    Values::OptionalString(&[Some("n"), None]),
                    Values::Int(&[0, 7]),
                    Values::Bool(&[true, false]),
                ],
            ),
            1 => writer(
                &dir.path().join("a.indices"),
                "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
                &[
                    Values::Int(&[0, 0, 2, 1]),
                    Values::String(&["a", "a", "b", "c"]),
                ],
            ),
            _ => writer(
                &dir.path().join("a.indptr"),
                "message graph { required int64 ptr (UINT_64); }",
                &[Values::Int(if variant == 2 {
                    &[1, 4, 4]
                } else {
                    &[0, 5, 5]
                })],
            ),
        }
        let (_, graph) = open_fixture(dir.path()).await;
        let error = failure(
            graph
                .neighborhood_graph(outgoing(2), limits(), context())
                .await,
        );
        assert!(matches!(
            error,
            DataReadError::DuplicatePrimaryKey
                | DataReadError::DuplicateEdgeUid
                | DataReadError::InvalidCsr(_)
        ));
    }
}

#[tokio::test]
async fn operation_original_is_released_and_cancel_deadline_leave_sibling_usable() {
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
        crate::LocalGraphPolicy::new(policy()).unwrap(),
        context(),
    )
    .await
    .unwrap();
    let original = context();
    let weak = Arc::downgrade(&original);
    graph
        .neighborhood_graph(outgoing(1), limits(), original)
        .await
        .unwrap();
    assert!(weak.upgrade().is_none());
    store.config_mut(|config| config.wait_get_per_call = Duration::from_millis(250));
    let original = context();
    let weak = Arc::downgrade(&original);
    let cancellation = original.cancellation().clone();
    let (result, ()) = tokio::join!(
        graph.neighborhood_graph(outgoing(1), limits(), original),
        async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancellation.cancel();
        }
    );
    assert!(
        matches!(result,Err(super::super::RemoteGraphError::Original(error)) if error.kind()==io::ErrorKind::Interrupted)
    );
    assert!(weak.upgrade().is_none());
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    ));
    assert!(
        matches!(graph.neighborhood_graph(outgoing(1),limits(),original).await,Err(super::super::RemoteGraphError::Original(error)) if error.kind()==io::ErrorKind::TimedOut)
    );
    store.config_mut(|config| config.wait_get_per_call = Duration::ZERO);
    assert_eq!(
        graph
            .neighborhood_graph(outgoing(1), limits(), context())
            .await
            .unwrap()
            .edges()
            .len(),
        1
    );
}
