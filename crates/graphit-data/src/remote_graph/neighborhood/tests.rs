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
        relationship: Some("CONTAINS"),
        other_label: Some("Function"),
        direction: Some(GraphDirection::Outgoing),
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
async fn all_filters_both_directions_preserve_parallel_occurrences_and_self_loop_once() {
    let dir = crate::neighborhood::parallel_fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let mut pages: Vec<GraphNeighborhoodPage> = Vec::new();
    loop {
        let page = graph
            .neighborhood_graph(
                GraphNeighborhoodRequest {
                    anchor_label: "Function",
                    anchor_key: GraphAnchorKey::String("f0"),
                    relationship: None,
                    other_label: None,
                    direction: None,
                    limit: 1,
                    cursor: pages.last().and_then(GraphNeighborhoodPage::next_cursor),
                },
                limits(),
                context(),
            )
            .await
            .unwrap();
        let more = page.has_more();
        pages.push(page);
        if !more {
            break;
        }
    }
    let edges = pages
        .iter()
        .flat_map(|page| page.edges())
        .collect::<Vec<_>>();
    assert_eq!(
        edges.iter().map(|edge| edge.uid()).collect::<Vec<_>>(),
        ["edge0", "edge0", "edge1", "loop"]
    );
    assert_eq!(edges[0].position().occurrence, 0);
    assert_eq!(edges[1].position().occurrence, 1);
    assert_ne!(edges[0].id(), edges[1].id());
    assert_eq!(edges[0].position().from_key, GraphAnchorKey::String("a.rs"));
    assert_eq!(edges[0].position().to_key, GraphAnchorKey::String("f0"));
    let sample = graph
        .sample_graph(
            crate::GraphSampleLimits::new(5, 10, 25, 1024, 100000, 10000, 2).unwrap(),
            context(),
        )
        .await
        .unwrap();
    assert_eq!(
        sample
            .edges()
            .iter()
            .filter(|edge| edge.uid() == "edge0")
            .count(),
        2
    );
    assert_eq!(
        sample
            .edges()
            .iter()
            .filter(|edge| edge.uid() == "loop")
            .count(),
        1
    );
}

#[tokio::test]
async fn one_thousand_remote_results_keep_complete_continuation() {
    let dir = crate::neighborhood::shuffled_fixture(1001, false);
    let (_, graph) = open_fixture(dir.path()).await;
    let input = GraphNeighborhoodLimits::new(1024, 16, 10000, 100000, 10000, 64).unwrap();
    let first = graph
        .neighborhood_graph(outgoing(1000), input, context())
        .await
        .unwrap();
    assert_eq!(first.edges().len(), 1000);
    assert!(first.has_more());
    let mut next = outgoing(1000);
    next.cursor = first.next_cursor();
    let last = graph
        .neighborhood_graph(next, input, context())
        .await
        .unwrap();
    assert_eq!(last.edges().len(), 1);
    assert_eq!(last.edges()[0].uid(), "uid01000");
    assert!(!last.has_more());
}

#[tokio::test]
async fn repeated_reverse_occurrences_share_forward_ranges_with_the_same_work_guard() {
    let dir = crate::neighborhood::repeated_reverse_fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let input = GraphNeighborhoodLimits::new(1024, 16, 1000, 256, 1000, 32).unwrap();
    let page = graph
        .neighborhood_graph(
            GraphNeighborhoodRequest {
                anchor_label: "Function",
                anchor_key: GraphAnchorKey::String("f0"),
                relationship: Some("CONTAINS"),
                other_label: Some("File"),
                direction: Some(GraphDirection::Incoming),
                limit: 32,
                cursor: None,
            },
            input,
            context(),
        )
        .await
        .unwrap();
    assert_eq!(page.edges().len(), 32);
    assert_eq!(
        page.edges()
            .iter()
            .map(|edge| edge.position().occurrence)
            .collect::<Vec<_>>(),
        (0..32).collect::<Vec<_>>()
    );
    assert_eq!(
        page.edges()
            .iter()
            .map(|edge| edge.id())
            .collect::<HashSet<_>>()
            .len(),
        32
    );
    assert!(!page.has_more());
}

#[tokio::test]
async fn resolved_anchor_and_reverse_rows_keep_the_same_remote_pin() {
    let dir = crate::neighborhood::reverse_fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let (_, foreign) = open_fixture(dir.path()).await;
    let anchor = graph
        .resolve_node(
            "Function",
            GraphAnchorKey::String("f0"),
            limits(),
            context(),
        )
        .await
        .unwrap();
    let request = || GraphNeighborhoodRequest {
        anchor_label: "Function",
        anchor_key: GraphAnchorKey::String("f0"),
        relationship: Some("CONTAINS"),
        other_label: Some("File"),
        direction: Some(GraphDirection::Incoming),
        limit: 1,
        cursor: None,
    };
    assert!(matches!(
        failure(
            foreign
                .neighborhood_graph_at(&anchor, request(), limits(), context())
                .await
        ),
        DataReadError::InvalidAnchorKey
    ));
    let mut wrong_type = request();
    wrong_type.anchor_key = GraphAnchorKey::Int64(0);
    assert!(matches!(
        failure(
            graph
                .neighborhood_graph_at(&anchor, wrong_type, limits(), context())
                .await
        ),
        DataReadError::InvalidAnchorKey
    ));
    let first = graph
        .neighborhood_graph_at(&anchor, request(), limits(), context())
        .await
        .unwrap();
    assert_eq!(first.edges()[0].uid(), "edge0");
    assert_eq!(first.edges()[0].id().row(), 0);
    assert_eq!(first.edges()[0].to(), anchor.id());
    let mut next = request();
    next.cursor = first.next_cursor();
    let last = graph
        .neighborhood_graph_at(&anchor, next, limits(), context())
        .await
        .unwrap();
    assert_eq!(last.edges()[0].uid(), "edge1");
    assert_eq!(last.edges()[0].id().row(), 1);
    assert!(!last.has_more());
}

#[tokio::test]
async fn exact_remote_key_lookup_prunes_shared_groups_and_keeps_logical_row() {
    let dir = crate::storage_tests::physical_slice_fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let one_row = GraphNeighborhoodLimits::new(1024, 16, 1000, 1, 1000, 1).unwrap();
    let anchor = graph
        .resolve_node("Function", GraphAnchorKey::String("f3"), one_row, context())
        .await
        .unwrap();
    assert_eq!(anchor.id().row(), 300);
    assert!(matches!(
        failure(
            graph
                .resolve_node("Function", GraphAnchorKey::String("f0"), one_row, context())
                .await
        ),
        DataReadError::LimitExceeded(_)
    ));
}

#[tokio::test]
async fn long_remote_key_lookup_prunes_with_real_truncated_statistics() {
    let (dir, first, last) = crate::neighborhood::truncated_key_fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let one_row = GraphNeighborhoodLimits::new(1024, 16, 1000, 1, 1000, 1).unwrap();
    assert_eq!(
        graph
            .resolve_node(
                "Function",
                GraphAnchorKey::String(&last),
                one_row,
                context()
            )
            .await
            .unwrap()
            .id()
            .row(),
        300
    );
    assert!(matches!(
        failure(
            graph
                .resolve_node(
                    "Function",
                    GraphAnchorKey::String(&first),
                    one_row,
                    context()
                )
                .await
        ),
        DataReadError::LimitExceeded(_)
    ));
}

#[tokio::test]
async fn query_remote_key_lookup_uses_captured_policy_and_explicit_sdk_batch() {
    let (dir, first, last) = crate::neighborhood::truncated_key_fixture();
    let store = Arc::new(InMemory::new());
    let prefix = Path::from("release/query-policy");
    let digest = upload(store.as_ref(), &prefix, dir.path()).await;
    let mut input = policy();
    input.max_name_bytes = 64;
    input.query_decoded_rows = 1;
    let graph = RemoteIcebugGraph::open(
        store,
        prefix,
        digest,
        crate::LocalGraphPolicy::new(input).unwrap(),
        context(),
    )
    .await
    .unwrap();
    assert!(last.len() > graph.input_limits().max_name_bytes);
    assert_eq!(
        graph
            .resolve_query_node(
                "Function",
                GraphAnchorKey::String(&last),
                usize::MAX,
                context()
            )
            .await
            .unwrap()
            .unwrap()
            .id()
            .row(),
        300
    );
    assert!(matches!(
        failure(
            graph
                .resolve_query_node(
                    "Function",
                    GraphAnchorKey::String(&first),
                    usize::MAX,
                    context()
                )
                .await
        ),
        DataReadError::LimitExceeded("decoded rows")
    ));
    assert!(matches!(
        failure(
            graph
                .resolve_query_node("Function", GraphAnchorKey::String(&last), 0, context())
                .await
        ),
        DataReadError::InvalidPolicy("row_batch_size")
    ));
    assert!(
        graph
            .resolve_query_node(
                "Function",
                GraphAnchorKey::String("missing"),
                usize::MAX,
                context()
            )
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        failure(
            graph
                .resolve_query_node("Function", GraphAnchorKey::Int64(0), usize::MAX, context())
                .await
        ),
        DataReadError::InvalidAnchorKey
    ));
    let anchor = graph
        .resolve_query_node(
            "Function",
            GraphAnchorKey::String(&last),
            usize::MAX,
            context(),
        )
        .await
        .unwrap()
        .unwrap();
    let node = graph
        .read_query_node(&anchor, &["name"], usize::MAX, context())
        .await
        .unwrap();
    assert_eq!(
        node.properties()
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>(),
        ["name", "uid"]
    );
    assert_eq!(
        node.primary_key().value(),
        GraphPropertyValueRef::String(&last)
    );
    assert_eq!(
        node.properties()
            .iter()
            .find(|p| p.name() == "name")
            .unwrap()
            .value(),
        GraphPropertyValueRef::String("long key")
    );
    assert!(matches!(
        failure(
            graph
                .read_query_node(&anchor, &["missing"], 1, context())
                .await
        ),
        DataReadError::InvalidParquet("projection")
    ));
    let (_, other) = open_fixture(dir.path()).await;
    assert!(matches!(
        failure(
            other
                .read_query_node(&anchor, &["name"], 1, context())
                .await
        ),
        DataReadError::InvalidAnchorKey
    ));
}

#[tokio::test]
async fn query_remote_node_projection_skips_unrequested_corruption_and_fails_when_selected() {
    let (dir, key) = crate::neighborhood::corrupt_unselected_property_fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let anchor = graph
        .resolve_query_node("Function", GraphAnchorKey::String(&key), 1, context())
        .await
        .unwrap()
        .unwrap();
    let node = graph
        .read_query_node(&anchor, &[], 1, context())
        .await
        .unwrap();
    assert_eq!(node.properties().len(), 1);
    assert_eq!(node.primary_key().name(), "uid");
    assert_eq!(
        node.primary_key().value(),
        GraphPropertyValueRef::String(&key)
    );
    assert!(matches!(
        graph
            .read_query_node(&anchor, &["name"], 1, context())
            .await,
        Err(super::super::RemoteGraphError::Data(
            DataReadError::InvalidParquet(_)
                | DataReadError::UnsupportedFormat(_)
                | DataReadError::LimitExceeded(_)
        )) | Err(super::super::RemoteGraphError::Parquet(_))
    ));
}

#[tokio::test]
async fn exact_remote_lookup_leaves_unselected_corruption_unread_and_fails_when_selected() {
    let dir = crate::neighborhood::pruned_corrupt_group_fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    assert_eq!(
        graph
            .resolve_node(
                "Function",
                GraphAnchorKey::String("f3"),
                limits(),
                context()
            )
            .await
            .unwrap()
            .id()
            .row(),
        300
    );
    assert!(
        graph
            .resolve_node(
                "Function",
                GraphAnchorKey::String("f0"),
                limits(),
                context()
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn remote_reverse_shared_slices_keep_forward_offsets_and_sparse_endpoints() {
    let dir = crate::neighborhood::shared_reverse_fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let page = graph
        .neighborhood_graph(
            GraphNeighborhoodRequest {
                anchor_label: "Function",
                anchor_key: GraphAnchorKey::String("f1"),
                relationship: Some("CONTAINS"),
                other_label: Some("File"),
                direction: Some(GraphDirection::Incoming),
                limit: 10,
                cursor: None,
            },
            limits(),
            context(),
        )
        .await
        .unwrap();
    assert_eq!(page.anchor().row(), 101);
    assert_eq!(page.edges().len(), 1);
    assert_eq!(page.edges()[0].uid(), "edge3");
    assert_eq!(page.edges()[0].id().row(), 1);
    let schema = graph.schema();
    let member = schema
        .relationships()
        .nth(page.edges()[0].id().member() as usize)
        .unwrap();
    assert_eq!(member.indices_row_offset(), 2);
    assert_eq!(page.edges()[0].from().row(), 10);
    assert_eq!(page.edges()[0].to(), page.anchor());
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
            relationship: Some("CONTAINS"),
            other_label: Some("File"),
            direction: Some(GraphDirection::Incoming),
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
        relationship: Some("CONTAINS"),
        other_label: Some("File"),
        direction: Some(GraphDirection::Incoming),
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
        relationship: Some("CONTAINS"),
        other_label: Some("File"),
        direction: Some(GraphDirection::Incoming),
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
            relationship: Some("CALLS"),
            other_label: Some("Function"),
            direction: Some(direction),
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
        relationship: Some("CALLS"),
        other_label: Some("Function"),
        direction: Some(GraphDirection::Outgoing),
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
    let pk = page.node(page.anchor()).unwrap().primary_key();
    assert_eq!(pk.name(), "key");
    assert_eq!(pk.value(), GraphPropertyValueRef::Int64(-7));
    assert_eq!(
        page.edges()[0].position().from_key,
        GraphAnchorKey::Int64(-7)
    );
    assert_eq!(page.edges()[0].position().to_key, GraphAnchorKey::Int64(-7));
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
async fn cursor_rejects_foreign_query_empty_and_oversized_uid_without_authority() {
    let dir = fixture();
    let (_, graph) = open_fixture(dir.path()).await;
    let first = graph
        .neighborhood_graph(outgoing(1), limits(), context())
        .await
        .unwrap();
    let cursor = first.next_cursor().unwrap();
    let oversized = "x".repeat(1025);
    for variant in 0..3 {
        let mut request = outgoing(1);
        request.cursor = Some(match variant {
            0 => GraphNeighborhoodCursor::from_parts([0; 32], cursor.position()),
            1 => GraphNeighborhoodCursor::from_parts(
                cursor.query_fingerprint(),
                crate::GraphNeighborhoodPosition {
                    uid: "",
                    ..cursor.position()
                },
            ),
            _ => GraphNeighborhoodCursor::from_parts(
                cursor.query_fingerprint(),
                crate::GraphNeighborhoodPosition {
                    uid: &oversized,
                    ..cursor.position()
                },
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
    request.relationship = Some("MISSING");
    assert!(matches!(
        failure(graph.neighborhood_graph(request, limits(), context()).await),
        DataReadError::UnknownRelationship
    ));
    request = outgoing(1);
    request.relationship = Some("CALLS");
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
async fn duplicate_anchor_and_used_csr_fail_while_repeated_uid_occurrences_survive() {
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
        if variant == 1 {
            let page = graph
                .neighborhood_graph(outgoing(2), limits(), context())
                .await
                .unwrap();
            assert_eq!(
                page.edges()
                    .iter()
                    .map(|edge| edge.uid())
                    .collect::<Vec<_>>(),
                ["a", "a"]
            );
            assert_eq!(page.edges()[0].position().occurrence, 0);
            assert_eq!(page.edges()[1].position().occurrence, 1);
            assert_ne!(page.edges()[0].id(), page.edges()[1].id());
            continue;
        }
        let error = failure(
            graph
                .neighborhood_graph(outgoing(2), limits(), context())
                .await,
        );
        assert!(matches!(
            error,
            DataReadError::DuplicatePrimaryKey | DataReadError::InvalidCsr(_)
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
