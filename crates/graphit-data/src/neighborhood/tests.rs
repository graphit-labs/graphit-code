use super::*;
use crate::tests::{Values, capture, context, fixture, manifest, policy, sample, save, writer};
use serde_json::json;
use std::fs;

fn limits() -> GraphNeighborhoodLimits {
    GraphNeighborhoodLimits::new(1024, 16, 10000, 100000, 10000, 2).unwrap()
}
fn request() -> GraphNeighborhoodRequest<'static> {
    GraphNeighborhoodRequest {
        anchor_label: "File",
        anchor_key: GraphAnchorKey::String("a.rs"),
        relationship: Some("CONTAINS"),
        other_label: Some("Function"),
        direction: Some(GraphDirection::Outgoing),
        limit: 2,
        cursor: None,
    }
}
fn prop<'a>(properties: &'a [GraphProperty], name: &str) -> GraphPropertyValueRef<'a> {
    properties
        .iter()
        .find(|p| p.name() == name)
        .unwrap()
        .value()
}
#[test]
fn resolved_anchor_is_bound_to_the_reader_and_exact_key() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let other_capture = capture(dir.path(), context(), policy()).unwrap();
    let anchor = graph
        .resolve_node("File", GraphAnchorKey::String("a.rs"), limits())
        .unwrap();
    let other = other_capture
        .resolve_node("File", GraphAnchorKey::String("a.rs"), limits())
        .unwrap();
    assert_eq!(
        (anchor.id().table(), anchor.id().row()),
        (other.id().table(), other.id().row())
    );
    assert!(matches!(
        other_capture.neighborhood_graph_at(&anchor, request(), limits()),
        Err(DataReadError::InvalidAnchorKey)
    ));
    let mut wrong = request();
    wrong.anchor_key = GraphAnchorKey::String("isolated.rs");
    assert!(matches!(
        graph.neighborhood_graph_at(&anchor, wrong, limits()),
        Err(DataReadError::InvalidAnchorKey)
    ));
    let page = graph
        .neighborhood_graph_at(&anchor, request(), limits())
        .unwrap();
    assert_eq!(page.anchor(), anchor.id());
    assert_eq!(
        page.edges()
            .iter()
            .map(|edge| edge.uid())
            .collect::<Vec<_>>(),
        ["edge0", "edge1"]
    );
    assert!(matches!(
        graph.resolve_node("File", GraphAnchorKey::Int64(0), limits()),
        Err(DataReadError::InvalidAnchorKey)
    ));
    assert!(matches!(
        graph.resolve_node("File", GraphAnchorKey::String("missing"), limits()),
        Err(DataReadError::AnchorNotFound)
    ));
}

#[test]
fn exact_key_lookup_prunes_shared_physical_groups_and_keeps_logical_row() {
    let dir = crate::storage_tests::physical_slice_fixture();
    let graph = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(policy()).unwrap(),
    )
    .unwrap();
    let one_row = GraphNeighborhoodLimits::new(1024, 16, 1000, 1, 1000, 1).unwrap();
    let anchor = graph
        .resolve_node("Function", GraphAnchorKey::String("f3"), one_row)
        .unwrap();
    assert_eq!(anchor.id().row(), 300);
    assert!(matches!(
        graph.resolve_node("Function", GraphAnchorKey::String("f0"), one_row),
        Err(DataReadError::LimitExceeded(_))
    ));
}

pub(crate) fn truncated_key_fixture() -> (tempfile::TempDir, String, String) {
    use ::parquet::{
        arrow::ArrowWriter,
        basic::Compression,
        file::{
            metadata::KeyValue,
            properties::WriterProperties,
            reader::{FileReader, SerializedFileReader},
        },
    };
    use arrow_array_parquet::{BooleanArray, RecordBatch, StringArray};
    use arrow_schema_parquet::{DataType, Field, Schema};
    let dir = crate::storage_tests::physical_slice_fixture();
    let path = dir.path().join("shards/ab/slices/Function.parquet");
    let schema = Arc::new(Schema::new(vec![
        Field::new("uid", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("added", DataType::Boolean, true),
    ]));
    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .set_dictionary_enabled(false)
        .set_statistics_truncate_length(Some(8))
        .set_key_value_metadata(Some(vec![KeyValue::new(
            "icebug_disk_version".into(),
            Some("v1".into()),
        )]))
        .build();
    let first = "a".repeat(67);
    let last = "z".repeat(67);
    let mut writer = ArrowWriter::try_new(
        fs::File::create(&path).unwrap(),
        schema.clone(),
        Some(properties),
    )
    .unwrap();
    for keys in [
        vec![first.clone(), "b".repeat(67), "c".repeat(67)],
        vec![last.clone()],
    ] {
        let count = keys.len();
        writer
            .write(
                &RecordBatch::try_new(
                    schema.clone(),
                    vec![
                        Arc::new(StringArray::from(keys)),
                        Arc::new(StringArray::from(vec!["long key"; count])),
                        Arc::new(BooleanArray::from(vec![None; count])),
                    ],
                )
                .unwrap(),
            )
            .unwrap();
        writer.flush().unwrap();
    }
    writer.close().unwrap();
    let reader = SerializedFileReader::new(fs::File::open(&path).unwrap()).unwrap();
    for group in reader.metadata().row_groups() {
        let stats = group.column(0).statistics().unwrap();
        assert!(!stats.is_min_max_deprecated());
        assert!(!stats.min_is_exact());
        assert!(!stats.max_is_exact());
    }
    (dir, first, last)
}

#[test]
fn long_key_lookup_prunes_with_real_truncated_statistics() {
    let (dir, first, last) = truncated_key_fixture();
    let graph = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(policy()).unwrap(),
    )
    .unwrap();
    let one_row = GraphNeighborhoodLimits::new(1024, 16, 1000, 1, 1000, 1).unwrap();
    assert_eq!(
        graph
            .resolve_node("Function", GraphAnchorKey::String(&last), one_row)
            .unwrap()
            .id()
            .row(),
        300
    );
    assert!(matches!(
        graph.resolve_node("Function", GraphAnchorKey::String(&first), one_row),
        Err(DataReadError::LimitExceeded(_))
    ));
}

#[test]
fn query_key_lookup_uses_captured_policy_and_explicit_sdk_batch() {
    let (dir, first, last) = truncated_key_fixture();
    let mut input = policy();
    input.max_name_bytes = 64;
    input.query_decoded_rows = 1;
    let graph = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(input).unwrap(),
    )
    .unwrap();
    assert!(last.len() > graph.input_limits().max_name_bytes);
    assert_eq!(
        graph
            .resolve_query_node("Function", GraphAnchorKey::String(&last), usize::MAX)
            .unwrap()
            .unwrap()
            .id()
            .row(),
        300
    );
    assert!(matches!(
        graph.resolve_query_node("Function", GraphAnchorKey::String(&first), usize::MAX),
        Err(DataReadError::LimitExceeded("decoded rows"))
    ));
    assert!(matches!(
        graph.resolve_query_node("Function", GraphAnchorKey::String(&last), 0),
        Err(DataReadError::InvalidPolicy("row_batch_size"))
    ));
    assert!(
        graph
            .resolve_query_node("Function", GraphAnchorKey::String("missing"), usize::MAX)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        graph.resolve_query_node("Function", GraphAnchorKey::Int64(0), usize::MAX),
        Err(DataReadError::InvalidAnchorKey)
    ));
    let anchor = graph
        .resolve_query_node("Function", GraphAnchorKey::String(&last), usize::MAX)
        .unwrap()
        .unwrap();
    let node = graph
        .read_query_node(&anchor, &["name"], usize::MAX)
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
        prop(node.properties(), "name"),
        GraphPropertyValueRef::String("long key")
    );
    assert!(matches!(
        graph.read_query_node(&anchor, &["missing"], 1),
        Err(DataReadError::InvalidParquet("projection"))
    ));
    let other = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(input).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        other.read_query_node(&anchor, &["name"], 1),
        Err(DataReadError::InvalidAnchorKey)
    ));
}

pub(crate) fn corrupt_unselected_property_fixture() -> (tempfile::TempDir, String) {
    use ::parquet::file::reader::{FileReader, SerializedFileReader};
    let (dir, _, key) = truncated_key_fixture();
    let path = dir.path().join("shards/ab/slices/Function.parquet");
    let reader = SerializedFileReader::new(fs::File::open(&path).unwrap()).unwrap();
    let start = reader.metadata().row_group(1).column(1).data_page_offset() as usize;
    drop(reader);
    let mut bytes = fs::read(&path).unwrap();
    bytes[start..start + 8].fill(0xff);
    fs::write(&path, bytes).unwrap();
    (dir, key)
}

#[test]
fn query_node_projection_skips_unrequested_corruption_and_fails_when_selected() {
    let (dir, key) = corrupt_unselected_property_fixture();
    let graph = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(policy()).unwrap(),
    )
    .unwrap();
    let anchor = graph
        .resolve_query_node("Function", GraphAnchorKey::String(&key), 1)
        .unwrap()
        .unwrap();
    let node = graph.read_query_node(&anchor, &[], 1).unwrap();
    assert_eq!(node.properties().len(), 1);
    assert_eq!(node.primary_key().name(), "uid");
    assert_eq!(
        node.primary_key().value(),
        GraphPropertyValueRef::String(&key)
    );
    assert!(matches!(
        graph.read_query_node(&anchor, &["name"], 1),
        Err(DataReadError::InvalidParquet(_)
            | DataReadError::UnsupportedFormat(_)
            | DataReadError::LimitExceeded(_))
    ));
}

pub(crate) fn pruned_corrupt_group_fixture() -> tempfile::TempDir {
    use ::parquet::file::reader::{FileReader, SerializedFileReader};
    let dir = crate::storage_tests::physical_slice_fixture();
    let path = dir.path().join("shards/ab/slices/Function.parquet");
    let reader = SerializedFileReader::new(fs::File::open(&path).unwrap()).unwrap();
    let column = reader.metadata().row_group(0).column(0);
    let start = column.data_page_offset() as usize;
    drop(reader);
    let mut bytes = fs::read(&path).unwrap();
    bytes[start..start + 8].fill(0xff);
    fs::write(&path, bytes).unwrap();
    dir
}

#[test]
fn exact_lookup_does_not_certify_unloaded_bodies_and_rejects_corruption_when_used() {
    let dir = pruned_corrupt_group_fixture();
    let graph = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(policy()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        graph
            .resolve_node("Function", GraphAnchorKey::String("f3"), limits())
            .unwrap()
            .id()
            .row(),
        300
    );
    assert!(matches!(
        graph.resolve_node("Function", GraphAnchorKey::String("f0"), limits()),
        Err(DataReadError::InvalidParquet(_)
            | DataReadError::UnsupportedFormat(_)
            | DataReadError::LimitExceeded(_))
    ));
}

pub(crate) fn reverse_fixture() -> tempfile::TempDir {
    let dir = fixture();
    writer(
        &dir.path().join("reverse.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[
            Values::Int(&[0, 0, 0, 0]),
            Values::String(&["edge1", "edge0", "edge3", "edge2"]),
        ],
    );
    writer(
        &dir.path().join("reverse.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 2, 3, 4])],
    );
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    value["add_reverse_edges"] = json!(true);
    value["relationships"][1]["reverse_members"] = json!([{"from":"Function", "to":"File", "table":"reverse", "indices":"reverse.indices", "indptr":"reverse.indptr", "rows":4}]);
    save(dir.path(), &value);
    dir
}

pub(crate) fn parallel_fixture() -> tempfile::TempDir {
    let dir = reverse_fixture();
    writer(
        &dir.path().join("parallel.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[Values::Int(&[0]), Values::String(&["edge0"])],
    );
    writer(
        &dir.path().join("parallel.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 1, 1])],
    );
    writer(
        &dir.path().join("parallel-reverse.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[Values::Int(&[0]), Values::String(&["edge0"])],
    );
    writer(
        &dir.path().join("parallel-reverse.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 1, 1, 1])],
    );
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    value["relationships"][1]["members"].as_array_mut().unwrap().push(json!({
        "from":"File", "to":"Function", "table":"parallel", "indices":"parallel.indices", "indptr":"parallel.indptr", "rows":1
    }));
    value["relationships"][1]["reverse_members"].as_array_mut().unwrap().push(json!({
        "from":"Function", "to":"File", "table":"parallel-reverse", "indices":"parallel-reverse.indices", "indptr":"parallel-reverse.indptr", "rows":1
    }));
    value["n_edges"] = json!(7);
    save(dir.path(), &value);
    dir
}

pub(crate) fn repeated_reverse_fixture() -> tempfile::TempDir {
    let dir = reverse_fixture();
    let targets = vec![0; 32];
    let uids = vec!["parallel"; 32];
    for name in ["a.indices", "reverse.indices"] {
        writer(
            &dir.path().join(name),
            "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
            &[Values::Int(&targets), Values::String(&uids)],
        );
    }
    writer(
        &dir.path().join("a.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 32, 32])],
    );
    writer(
        &dir.path().join("reverse.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 32, 32, 32])],
    );
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    value["relationships"][1]["members"][0]["rows"] = json!(32);
    value["relationships"][1]["reverse_members"][0]["rows"] = json!(32);
    value["n_edges"] = json!(34);
    save(dir.path(), &value);
    dir
}

#[test]
fn repeated_reverse_occurrences_share_source_reads_with_the_same_work_guard() {
    let dir = repeated_reverse_fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
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
        )
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

#[test]
fn all_filters_both_directions_keep_parallel_occurrences_and_self_loop_once() {
    let dir = parallel_fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
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
            )
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
        .sample_graph(GraphSampleLimits::new(5, 10, 25, 1024, 100000, 10000, 2).unwrap())
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

#[test]
fn one_thousand_results_preserve_complete_continuation() {
    let dir = shuffled_fixture(1001, false);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let mut r = request();
    r.limit = 1000;
    let first = graph.neighborhood_graph(r, limits()).unwrap();
    assert_eq!(first.edges().len(), 1000);
    assert!(first.has_more());
    let mut next = request();
    next.limit = 1000;
    next.cursor = first.next_cursor();
    let last = graph.neighborhood_graph(next, limits()).unwrap();
    assert_eq!(last.edges().len(), 1);
    assert_eq!(last.edges()[0].uid(), "uid01000");
    assert!(!last.has_more());
}

pub(crate) fn shared_reverse_fixture() -> tempfile::TempDir {
    use crate::graph_writer::{concat_parquet_rows, write_csr_shard};
    let dir = crate::storage_tests::physical_slice_fixture();
    let ctx = context();
    let parts = [
        vec![(0, 0, "edge0"), (0, 0, "edge1")],
        vec![(1, 0, "edge3"), (2, 0, "edge2")],
        vec![],
    ];
    let mut directories = Vec::new();
    for (index, edges) in parts.iter().enumerate() {
        let path = dir.path().join(format!("reverse-part-{index}"));
        fs::create_dir(&path).unwrap();
        write_csr_shard(
            &path,
            3,
            2,
            edges
                .iter()
                .map(|(source, target, uid)| Ok((*source, *target, (*uid).to_owned()))),
            &ctx,
        )
        .unwrap();
        directories.push(path);
    }
    let indices = "reverse-shared.indices";
    let pointers = "reverse-shared.indptr";
    let slices = concat_parquet_rows(
        &dir.path().join(indices),
        directories
            .iter()
            .zip(&parts)
            .map(|(path, edges)| Ok((path.join("edges.indices"), 0, edges.len() as u64))),
        &ctx,
    )
    .unwrap();
    let ptr_slices = concat_parquet_rows(
        &dir.path().join(pointers),
        directories
            .iter()
            .map(|path| Ok((path.join("edges.indptr"), 0, 4))),
        &ctx,
    )
    .unwrap();
    assert_eq!(slices, [(0, 2), (2, 2), (4, 0)]);
    assert_eq!(ptr_slices, [(0, 4), (4, 4), (8, 4)]);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    value["add_reverse_edges"] = json!(true);
    value["relationships"][1]["reverse_members"] = json!(slices.iter().zip(&ptr_slices).enumerate().map(|(index, ((offset, rows), (ptr_offset, _)))| {
        json!({"from":"Function", "to":"File", "table":format!("reverse-{index}"),
            "indices":indices, "indptr":pointers, "indices_row_offset":offset, "indptr_row_offset":ptr_offset,
            "source_row_base":100, "source_rows":3, "target_row_base":10, "target_rows":2, "rows":rows})
    }).collect::<Vec<_>>());
    save(dir.path(), &value);
    dir
}

#[test]
fn reverse_shared_slices_resolve_sparse_endpoints_and_forward_member_offsets() {
    let dir = shared_reverse_fixture();
    let graph = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(policy()).unwrap(),
    )
    .unwrap();
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
        )
        .unwrap();
    assert_eq!(page.anchor().row(), 101);
    assert_eq!(page.edges().len(), 1);
    assert_eq!(page.edges()[0].uid(), "edge3");
    assert_eq!(page.edges()[0].id().row(), 1);
    let schema = graph.schema().unwrap();
    let member = schema
        .relationships()
        .nth(page.edges()[0].id().member() as usize)
        .unwrap();
    assert_eq!(member.indices_row_offset(), 2);
    assert_eq!(page.edges()[0].from().row(), 10);
    assert_eq!(page.edges()[0].to(), page.anchor());
}

#[test]
fn reverse_adjacency_resolves_forward_rows_and_preserves_complete_pages() {
    let dir = reverse_fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let mut incoming = request();
    incoming.anchor_label = "Function";
    incoming.anchor_key = GraphAnchorKey::String("f0");
    incoming.other_label = Some("File");
    incoming.direction = Some(GraphDirection::Incoming);
    incoming.limit = 1;
    let first = graph.neighborhood_graph(incoming, limits()).unwrap();
    assert_eq!(first.edges()[0].uid(), "edge0");
    assert_eq!(first.edges()[0].id().row(), 0);
    assert_eq!(first.edges()[0].from().row(), 0);
    assert_eq!(first.edges()[0].to(), first.anchor());
    assert!(first.has_more());
    let mut next = request();
    next.anchor_label = "Function";
    next.anchor_key = GraphAnchorKey::String("f0");
    next.other_label = Some("File");
    next.direction = Some(GraphDirection::Incoming);
    next.cursor = first.next_cursor();
    let last = graph.neighborhood_graph(next, limits()).unwrap();
    assert_eq!(last.edges()[0].uid(), "edge1");
    assert_eq!(last.edges()[0].id().row(), 1);
    assert!(!last.has_more());
    assert_eq!(
        prop(last.nodes()[0].properties(), "name"),
        GraphPropertyValueRef::String("ação 🚀")
    );
}
#[test]
fn full_properties_global_pages_and_sample_identities() {
    let dir = fixture();
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
    let sample = graph.sample_graph(sample()).unwrap();
    let first = graph.neighborhood_graph(request(), limits()).unwrap();
    assert_eq!(
        first.edges().iter().map(|e| e.uid()).collect::<Vec<_>>(),
        ["edge0", "edge1"]
    );
    assert!(first.has_more());
    assert_eq!(first.nodes().len(), 2);
    let node = first.node(first.anchor()).unwrap();
    assert_eq!(
        prop(node.properties(), "line_number"),
        GraphPropertyValueRef::Int64(0)
    );
    assert_eq!(
        prop(node.properties(), "ignored"),
        GraphPropertyValueRef::Bool(true)
    );
    assert_eq!(
        prop(node.properties(), "name"),
        GraphPropertyValueRef::String("ação 🚀")
    );
    assert_eq!(first.edges()[0].id(), sample.edges()[0].id());
    assert_eq!(first.edges()[1].from(), sample.edges()[1].from());
    assert_eq!(first.edges()[1].to(), sample.edges()[1].to());
    assert!(
        !first.edges()[0]
            .properties()
            .iter()
            .any(|p| p.name() == "target")
    );
    let mut next = request();
    next.limit = 100;
    next.cursor = first.next_cursor();
    drop(sample);
    let last = graph.neighborhood_graph(next, limits()).unwrap();
    assert_eq!(
        last.edges().iter().map(|e| e.uid()).collect::<Vec<_>>(),
        ["edge2", "edge3"]
    );
    assert!(!last.has_more());
    assert!(last.next_cursor().is_none());
}
#[test]
fn incoming_mapping_self_loop_and_pathless_primary_keys() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let mut r = request();
    r.anchor_label = "Function";
    r.anchor_key = GraphAnchorKey::String("f0");
    r.other_label = Some("File");
    r.direction = Some(GraphDirection::Incoming);
    r.limit = 100;
    let incoming = graph.neighborhood_graph(r, limits()).unwrap();
    assert_eq!(incoming.edges().len(), 2);
    assert_eq!(incoming.edges()[0].from().row(), 0);
    assert_eq!(incoming.edges()[0].to(), incoming.anchor());
    assert_eq!(incoming.edges()[1].to(), incoming.anchor());
    let mut r = request();
    r.anchor_label = "Function";
    r.anchor_key = GraphAnchorKey::String("f0");
    r.other_label = Some("Function");
    r.relationship = Some("CALLS");
    let loops = graph.neighborhood_graph(r, limits()).unwrap();
    assert_eq!(loops.nodes().len(), 1);
    assert_eq!(loops.edges().len(), 1);
    assert_eq!(loops.edges()[0].from(), loops.edges()[0].to());
    assert_eq!(
        prop(loops.nodes()[0].properties(), "uid"),
        GraphPropertyValueRef::String("f0")
    );
    drop(incoming);
    drop(loops);
    let mut r = request();
    r.anchor_label = "Function";
    r.anchor_key = GraphAnchorKey::String("f1");
    r.other_label = Some("Function");
    r.relationship = Some("CALLS");
    r.direction = Some(GraphDirection::Incoming);
    let page = graph.neighborhood_graph(r, limits()).unwrap();
    assert_eq!(page.edges()[0].from().row(), 2);
    assert_eq!(page.edges()[0].to().row(), 1);
}
#[test]
fn present_empty_anchor_null_properties_and_missing_anchor() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let mut r = request();
    r.anchor_key = GraphAnchorKey::String("isolated.rs");
    let page = graph.neighborhood_graph(r, limits()).unwrap();
    assert!(page.edges().is_empty());
    assert_eq!(page.nodes()[0].primary_key().name(), "path");
    assert_eq!(
        page.nodes()[0].primary_key().value(),
        GraphPropertyValueRef::String("isolated.rs")
    );
    assert_eq!(page.nodes().len(), 1);
    assert_eq!(
        prop(page.nodes()[0].properties(), "name"),
        GraphPropertyValueRef::Null
    );
    assert_eq!(
        prop(page.nodes()[0].properties(), "ignored"),
        GraphPropertyValueRef::Bool(false)
    );
    drop(page);
    let mut r = request();
    r.anchor_key = GraphAnchorKey::String("missing");
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::AnchorNotFound)
    ));
    let first = graph.neighborhood_graph(request(), limits()).unwrap();
    let cursor = first.next_cursor().unwrap();
    let beyond = GraphNeighborhoodCursor::from_parts(
        cursor.query_fingerprint(),
        GraphNeighborhoodPosition {
            uid: "zzzz",
            ..cursor.position()
        },
    );
    let mut r = request();
    r.cursor = Some(beyond);
    let page = graph.neighborhood_graph(r, limits()).unwrap();
    assert!(page.edges().is_empty());
    assert_eq!(page.nodes()[0].primary_key().name(), "path");
    assert_eq!(
        page.nodes()[0].primary_key().value(),
        GraphPropertyValueRef::String("a.rs")
    );
    assert_eq!(page.nodes().len(), 1);
}
#[test]
fn pure_input_cursor_and_request_denials_perform_zero_io() {
    let dir = fixture();
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
    let page = graph.neighborhood_graph(request(), limits()).unwrap();
    let cursor = page.next_cursor().unwrap();

    for limit in [0, 1001, usize::MAX] {
        let mut r = request();
        r.limit = limit;
        assert!(matches!(
            graph.neighborhood_graph(r, limits()),
            Err(DataReadError::InvalidNeighborhoodRequest)
        ));
    }
    let mut r = request();
    r.anchor_key = GraphAnchorKey::Int64(1);
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidAnchorKey)
    ));
    let mut r = request();
    r.anchor_label = "Missing";
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::UnknownNodeLabel)
    ));
    let mut r = request();
    r.relationship = Some("Missing");
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::UnknownRelationship)
    ));
    let mut r = request();
    r.other_label = Some("File");
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::UnavailableRelationshipEndpoint)
    ));
    let mut r = request();
    r.cursor = Some(GraphNeighborhoodCursor::from_parts(
        [0; 32],
        cursor.position(),
    ));
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidCursor)
    ));
    let mut r = request();
    r.cursor = Some(GraphNeighborhoodCursor::from_parts(
        cursor.query_fingerprint(),
        GraphNeighborhoodPosition {
            uid: "",
            ..cursor.position()
        },
    ));
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidCursor)
    ));
    let mut r = request();
    r.anchor_key = GraphAnchorKey::String("isolated.rs");
    r.cursor = Some(cursor);
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidCursor)
    ));
    let mut r = request();
    r.anchor_label = "";
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidNeighborhoodRequest)
    ));
    let key = "x".repeat(1025);
    let mut r = request();
    r.anchor_key = GraphAnchorKey::String(&key);
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::LimitExceeded("anchor key"))
    ));
}
#[test]
fn request_key_and_name_exact_bounds_are_checked_before_io() {
    let dir = fixture();
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();

    let mut p = limits();
    p.max_string_bytes = 3;
    assert!(matches!(
        graph.neighborhood_graph(request(), p),
        Err(DataReadError::LimitExceeded("anchor key"))
    ));

    p.max_string_bytes = 4;
    assert!(matches!(
        graph.neighborhood_graph(request(), p),
        Err(DataReadError::LimitExceeded("primary key"))
    ));

    let name = "x".repeat(policy().max_name_bytes + 1);
    let mut r = request();
    r.relationship = Some(&name);
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidNeighborhoodRequest)
    ));

    let name = "x".repeat(policy().max_name_bytes);
    let mut r = request();
    r.relationship = Some(&name);
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::UnknownRelationship)
    ));
}
#[test]
fn empty_catalog_and_boolean_primary_key_fail_at_the_typed_boundary() {
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), &manifest());
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();

    assert!(matches!(
        graph.neighborhood_graph(request(), limits()),
        Err(DataReadError::UnknownNodeLabel)
    ));

    let dir = fixture();
    let mut m: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    m["node_tables"][1]["primary_key"] = json!("ignored");
    save(dir.path(), &m);
    assert!(matches!(
        capture(dir.path(), context(), policy()),
        Err(DataReadError::InvalidManifest("primary key"))
    ));
}
fn assert_empty_zero_column_anchor(key: GraphAnchorKey<'_>) {
    let dir = tempfile::tempdir().unwrap();
    empty_writer(&dir.path().join("Empty.parquet"), "message graph { }");
    writer(
        &dir.path().join("B.parquet"),
        "message graph { required binary uid (UTF8); }",
        &[Values::String(&["b"])],
    );
    empty_writer(
        &dir.path().join("r.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
    );
    writer(
        &dir.path().join("r.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0])],
    );
    let mut m = manifest();
    m["node_tables"] = json!([
        {"label":"Empty","rows":0,"shards":[{"file":"Empty.parquet","row_base":0,"rows":0}],"primary_key":"","columns":[],"lang_counts":[]},
        {"label":"B","rows":1,"shards":[{"file":"B.parquet","row_base":0,"rows":1}],"primary_key":"uid","columns":[{"name":"uid","type":"STRING"}],"lang_counts":[]}]);
    m["relationships"] = json!([{"type":"R","members":[{"from":"Empty","to":"B","table":"r","indices":"r.indices","indptr":"r.indptr","rows":0}]}]);
    save(dir.path(), &m);
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
    let r = GraphNeighborhoodRequest {
        anchor_label: "Empty",
        anchor_key: key,
        relationship: Some("R"),
        other_label: Some("B"),
        direction: Some(GraphDirection::Outgoing),
        limit: 1,
        cursor: None,
    };

    let result = graph.neighborhood_graph(r, limits());
    assert!(
        matches!(result, Err(DataReadError::AnchorNotFound)),
        "empty original table must report AnchorNotFound, got {:?}",
        result.err()
    );
}
#[test]
fn empty_zero_column_anchor_string_is_not_found_after_final_fence() {
    assert_empty_zero_column_anchor(GraphAnchorKey::String("missing"));
}
#[test]
fn empty_zero_column_anchor_int64_is_not_found_after_final_fence() {
    assert_empty_zero_column_anchor(GraphAnchorKey::Int64(0));
}
#[test]
fn empty_known_primary_key_schema_preserves_wrong_type_denial_without_io() {
    let dir = tempfile::tempdir().unwrap();
    empty_writer(
        &dir.path().join("A.parquet"),
        "message graph { required binary uid (UTF8); }",
    );
    empty_writer(
        &dir.path().join("r.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
    );
    writer(
        &dir.path().join("r.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0])],
    );
    let mut m = manifest();
    m["node_tables"] = json!([{ "label":"A","rows":0,"shards":[{"file":"A.parquet","row_base":0,"rows":0}],"primary_key":"uid","columns":[{"name":"uid","type":"STRING"}],"lang_counts":[]}]);
    m["relationships"] = json!([{"type":"R","members":[{"from":"A","to":"A","table":"r","indices":"r.indices","indptr":"r.indptr","rows":0}]}]);
    save(dir.path(), &m);
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();

    let r = GraphNeighborhoodRequest {
        anchor_label: "A",
        anchor_key: GraphAnchorKey::Int64(0),
        relationship: Some("R"),
        other_label: Some("A"),
        direction: Some(GraphDirection::Outgoing),
        limit: 1,
        cursor: None,
    };
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidAnchorKey)
    ));

    let r = GraphNeighborhoodRequest {
        anchor_label: "A",
        anchor_key: GraphAnchorKey::String("missing"),
        relationship: Some("R"),
        other_label: Some("A"),
        direction: Some(GraphDirection::Outgoing),
        limit: 1,
        cursor: None,
    };
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::AnchorNotFound)
    ));
}
#[test]
fn empty_present_schema_rejects_unsupported_or_missing_primary_key_without_io() {
    for primary_key in ["missing", "key"] {
        let dir = tempfile::tempdir().unwrap();
        empty_writer(
            &dir.path().join("A.parquet"),
            "message graph { required boolean key; }",
        );
        empty_writer(
            &dir.path().join("r.indices"),
            "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        );
        writer(
            &dir.path().join("r.indptr"),
            "message graph { required int64 ptr (UINT_64); }",
            &[Values::Int(&[0])],
        );
        let mut m = manifest();
        m["node_tables"] = json!([{ "label":"A","rows":0,"shards":[{"file":"A.parquet","row_base":0,"rows":0}],"primary_key":primary_key,"columns":[{"name":"key","type":"BOOL"}],"lang_counts":[]}]);
        m["relationships"] = json!([{"type":"R","members":[{"from":"A","to":"A","table":"r","indices":"r.indices","indptr":"r.indptr","rows":0}]}]);
        save(dir.path(), &m);
        let ctx = context();
        let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();

        for anchor_key in [GraphAnchorKey::String("missing"), GraphAnchorKey::Int64(0)] {
            let r = GraphNeighborhoodRequest {
                anchor_label: "A",
                anchor_key,
                relationship: Some("R"),
                other_label: Some("A"),
                direction: Some(GraphDirection::Outgoing),
                limit: 1,
                cursor: None,
            };
            let result = graph.neighborhood_graph(r, limits());
            assert!(
                matches!(result, Err(DataReadError::InvalidAnchorKey)),
                "present schema PK {primary_key}: {:?}",
                result.err()
            );
        }
        if primary_key == "missing" {
            continue;
        }
        drop(graph);
        empty_writer(
            &dir.path().join("A.parquet"),
            "message graph { required float key; }",
        );
        let result = capture(dir.path(), context(), policy());
        assert!(
            matches!(
                result,
                Err(DataReadError::InvalidParquet("footer protocol"))
            ),
            "FLOAT capture: {:?}",
            result.err()
        );
        m["node_tables"][0]["columns"][0]["type"] = json!("FLOAT");
        save(dir.path(), &m);
        assert!(matches!(
            capture(dir.path(), context(), policy()),
            Err(DataReadError::UnsupportedFormat("column type"))
        ));
    }
}
#[cfg(unix)]
#[test]
fn neighborhood_rejects_native_non_utf8_capture_root() {
    use std::os::unix::ffi::OsStringExt;
    let fixture = fixture();
    let parent = tempfile::tempdir().unwrap();
    let path = parent
        .path()
        .join(std::ffi::OsString::from_vec(vec![b'n', 0xff]));
    // Linux supports the physical entry. APFS does not; Data must still
    // reject the same logical path at capture before filesystem lookup.
    #[cfg(target_os = "linux")]
    fs::rename(fixture.path(), &path).unwrap();
    #[cfg(not(target_os = "linux"))]
    assert!(fixture.path().is_dir());
    assert!(matches!(
        capture(&path, context(), policy()),
        Err(DataReadError::Source(error)) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
}
#[test]
fn sibling_update_preserves_logical_keyset_and_original_guards() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let page = graph.neighborhood_graph(request(), limits()).unwrap();
    let cursor = page.next_cursor().unwrap();
    drop(graph);
    writer(
        &dir.path().join("File.parquet"),
        "message graph { required binary path (UTF8); optional binary name (UTF8); required int64 line_number; required boolean ignored; }",
        &[
            Values::String(&["a.rs", "isolated.rs"]),
            Values::OptionalString(&[Some("ação 🚀"), Some("changed sibling")]),
            Values::Int(&[0, 7]),
            Values::Bool(&[true, false]),
        ],
    );
    let next_capture = capture(dir.path(), context(), policy()).unwrap();
    let mut next = request();
    next.cursor = Some(cursor);
    let continued = next_capture.neighborhood_graph(next, limits()).unwrap();
    assert_eq!(
        continued
            .edges()
            .iter()
            .map(|edge| edge.uid())
            .collect::<Vec<_>>(),
        ["edge2", "edge3"]
    );

    let dir = fixture();
    let path = dir.path().join("File.parquet");
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let bytes = fs::read(&path).unwrap();
    let alternate = dir.path().join("replacement");
    fs::write(&alternate, bytes).unwrap();
    fs::rename(alternate, path).unwrap();
    assert!(matches!(
        graph.neighborhood_graph(request(), limits()),
        Err(DataReadError::Source(_))
    ));
    let dir = fixture();
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
    ctx.cancellation().cancel();

    assert!(matches!(
        graph.neighborhood_graph(request(), limits()),
        Err(DataReadError::Source(_))
    ));

    let dir = fixture();
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
    let cancelled = Arc::clone(&ctx);
    STEP.with(|h| {
        *h.borrow_mut() = Some(Box::new(move |at| {
            if at == "selection" {
                cancelled.cancellation().cancel();
            }
        }))
    });
    let result = graph.neighborhood_graph(request(), limits());
    STEP.with(|h| *h.borrow_mut() = None);
    assert!(matches!(result, Err(DataReadError::Source(_))));
}
#[test]
fn canonical_fingerprints_frame_types_lengths_unicode_case_and_direction() {
    let base = fingerprint(&request()).unwrap();
    let mut variants = Vec::new();
    for key in [
        "a.rs", "A.rs", "a|rs", "a\0rs", "ação", "ação", "", "1", "-1",
    ] {
        let mut r = request();
        r.anchor_key = GraphAnchorKey::String(key);
        variants.push(fingerprint(&r).unwrap());
    }
    let mut r = request();
    r.anchor_key = GraphAnchorKey::Int64(1);
    variants.push(fingerprint(&r).unwrap());
    let mut r = request();
    r.anchor_key = GraphAnchorKey::Int64(-1);
    variants.push(fingerprint(&r).unwrap());
    let mut r = request();
    r.direction = Some(GraphDirection::Incoming);
    variants.push(fingerprint(&r).unwrap());
    assert_eq!(
        variants.iter().collect::<HashSet<_>>().len(),
        variants.len()
    );
    let mut r = request();
    r.limit = 100;
    assert_eq!(fingerprint(&r).unwrap(), base);
    let mut a = request();
    a.anchor_label = "a";
    a.anchor_key = GraphAnchorKey::String("b|c");
    let mut b = request();
    b.anchor_label = "a|b";
    b.anchor_key = GraphAnchorKey::String("c");
    assert_ne!(fingerprint(&a).unwrap(), fingerprint(&b).unwrap());
}
#[test]
fn final_source_conflict_overrides_semantic_not_found_and_mixed_properties() {
    for phase in ["anchor", "selection", "properties", "before_final"] {
        let dir = fixture();
        let graph = capture(dir.path(), context(), policy()).unwrap();
        let path = dir.path().join("File.parquet");
        let phase_owned = phase.to_owned();
        STEP.with(|h| {
            *h.borrow_mut() = Some(Box::new(move |at| {
                if at == phase_owned {
                    let mut bytes = fs::read(&path).unwrap();
                    bytes[10] ^= 1;
                    fs::write(&path, bytes).unwrap();
                }
            }))
        });
        let result = graph.neighborhood_graph(request(), limits());
        STEP.with(|h| *h.borrow_mut() = None);
        assert!(matches!(result, Err(DataReadError::Source(_))), "{phase}");
    }
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let path = dir.path().join("File.parquet");
    STEP.with(|h| {
        *h.borrow_mut() = Some(Box::new(move |at| {
            if at == "anchor" {
                fs::write(&path, b"changed").unwrap();
            }
        }))
    });
    let mut r = request();
    r.anchor_key = GraphAnchorKey::String("missing");
    let result = graph.neighborhood_graph(r, limits());
    STEP.with(|h| *h.borrow_mut() = None);
    assert!(matches!(result, Err(DataReadError::Source(_))));
}
#[test]
fn positive_limits_and_overflow_denials_are_closed() {
    for index in 0..6 {
        let mut values = [1; 6];
        values[index] = 0;
        assert!(
            GraphNeighborhoodLimits::new(
                values[0], values[1], values[2], values[3], values[4], values[5]
            )
            .is_err()
        );
    }
    assert!(GraphNeighborhoodLimits::new(1, 1, 1, 1, 1, 2).is_err());
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let mut p = limits();
    p.max_matching_edges = 3;
    assert!(matches!(
        graph.neighborhood_graph(request(), p),
        Err(DataReadError::LimitExceeded("matching edges"))
    ));
    let mut p = limits();
    p.max_properties_per_record = 3;
    assert!(matches!(
        graph.neighborhood_graph(request(), p),
        Err(DataReadError::LimitExceeded("record properties"))
    ));
    assert!(graph.neighborhood_graph(request(), limits()).is_ok());
}
pub(crate) fn shuffled_fixture(count: usize, duplicate: bool) -> tempfile::TempDir {
    let dir = fixture();
    let targets = vec![0; count];
    let owned: Vec<_> = (0..count).rev().map(|n| format!("uid{n:05}")).collect();
    let mut uids: Vec<_> = owned.iter().map(String::as_str).collect();
    if duplicate && count > 1 {
        uids[count - 1] = uids[0];
    }
    writer(
        &dir.path().join("a.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); optional binary note (UTF8); required int64 weight; required boolean active; }",
        &[
            Values::Int(&targets),
            Values::String(&uids),
            Values::OptionalString(&vec![None; count]),
            Values::Int(&vec![0; count]),
            Values::Bool(&vec![false; count]),
        ],
    );
    writer(
        &dir.path().join("a.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, count as i64, count as i64])],
    );
    let mut m: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    m["relationships"][1]["members"][0]["rows"] = json!(count);
    m["n_edges"] = json!(count + 2);
    save(dir.path(), &m);
    dir
}
#[test]
fn large_unsorted_degree_retains_only_limit_plus_one_and_full_edge_scalars() {
    let dir = shuffled_fixture(1000, false);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let mut r = request();
    r.limit = 100;
    let page = graph.neighborhood_graph(r, limits()).unwrap();
    assert_eq!(page.edges().len(), 100);
    assert!(page.has_more());
    assert_eq!(page.edges()[0].uid(), "uid00000");
    assert_eq!(page.edges()[99].uid(), "uid00099");
    assert_eq!(page.edges()[0].id().row(), 999);
    assert_eq!(
        prop(page.edges()[0].properties(), "note"),
        GraphPropertyValueRef::Null
    );
    assert_eq!(
        prop(page.edges()[0].properties(), "weight"),
        GraphPropertyValueRef::Int64(0)
    );
    assert_eq!(
        prop(page.edges()[0].properties(), "active"),
        GraphPropertyValueRef::Bool(false)
    );
    assert_eq!(METRICS.with(|m| m.get().heap), 101);
    let mut r = request();
    r.limit = 1;
    r.cursor = page.next_cursor();
    let next = graph.neighborhood_graph(r, limits()).unwrap();
    assert_eq!(next.edges()[0].uid(), "uid00100");
    assert_eq!(METRICS.with(|m| m.get().heap), 2);
}
#[test]
fn repeated_uid_occurrences_survive_cursor_and_page_boundaries() {
    let dir = shuffled_fixture(300, true);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let mut r = request();
    r.limit = 1;
    let first = graph.neighborhood_graph(r, limits()).unwrap();
    assert_eq!(first.edges()[0].uid(), "uid00001");
    let mut r = request();
    r.limit = 1;
    r.cursor = Some(GraphNeighborhoodCursor::from_parts(
        fingerprint(&r).unwrap(),
        GraphNeighborhoodPosition {
            uid: "uid00299",
            occurrence: 0,
            ..first.edges()[0].position()
        },
    ));
    let last = graph.neighborhood_graph(r, limits()).unwrap();
    assert_eq!(last.edges()[0].uid(), "uid00299");
    assert_eq!(last.edges()[0].position().occurrence, 1);
    assert!(!last.has_more());
    let mut r = request();
    r.cursor = Some(GraphNeighborhoodCursor::from_parts(
        fingerprint(&r).unwrap(),
        GraphNeighborhoodPosition {
            uid: "zzzz",
            ..first.edges()[0].position()
        },
    ));
    assert!(
        graph
            .neighborhood_graph(r, limits())
            .unwrap()
            .edges()
            .is_empty()
    );
}

#[test]
fn global_order_across_forward_members_and_duplicate_member_uids() {
    for duplicate in [false, true] {
        let dir = fixture();
        writer(
            &dir.path().join("b.indices"),
            "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
            &[
                Values::Int(&[1, 0]),
                Values::String(if duplicate {
                    &["edge0", "zzz"]
                } else {
                    &["aaa", "zzz"]
                }),
            ],
        );
        writer(
            &dir.path().join("b.indptr"),
            "message graph { required int64 ptr (UINT_64); }",
            &[Values::Int(&[0, 2, 2])],
        );
        let mut m: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
        m["relationships"][1]["members"].as_array_mut().unwrap().push(json!({"from":"File","to":"Function","table":"b","indices":"b.indices","indptr":"b.indptr","rows":2}));
        m["n_edges"] = json!(8);
        save(dir.path(), &m);
        let graph = capture(dir.path(), context(), policy()).unwrap();
        let page = graph.neighborhood_graph(request(), limits()).unwrap();
        assert_eq!(
            page.edges().iter().map(|e| e.uid()).collect::<Vec<_>>(),
            if duplicate {
                ["edge0", "edge0"]
            } else {
                ["aaa", "edge0"]
            }
        );
        let sample = graph
            .sample_graph(GraphSampleLimits::new(1, 8, 17, 1024, 1000, 1000, 2).unwrap())
            .unwrap();
        for edge in page.edges() {
            assert_eq!(
                edge.id(),
                sample
                    .edges()
                    .iter()
                    .find(|e| e.uid() == edge.uid() && e.to() == edge.to())
                    .unwrap()
                    .id()
            );
        }
    }
}
#[test]
fn int64_primary_key_empty_string_and_empty_member_are_real_data() {
    let dir = tempfile::tempdir().unwrap();
    writer(
        &dir.path().join("A.parquet"),
        "message graph { required int64 key; optional binary title (UTF8); }",
        &[
            Values::Int(&[-1, 0]),
            Values::OptionalString(&[Some(""), None]),
        ],
    );
    writer(
        &dir.path().join("B.parquet"),
        "message graph { required binary path (UTF8); }",
        &[Values::String(&[""])],
    );
    empty_writer(
        &dir.path().join("r.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
    );
    writer(
        &dir.path().join("r.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 0, 0])],
    );
    let mut m = manifest();
    m["node_tables"] = json!([
        {"label":"A","rows":2,"shards":[{"file":"A.parquet","row_base":0,"rows":2}],"primary_key":"key","columns":[{"name":"key","type":"INT64"},{"name":"title","type":"STRING"}],"lang_counts":[]},
        {"label":"Directory","rows":1,"shards":[{"file":"B.parquet","row_base":0,"rows":1}],"primary_key":"path","columns":[{"name":"path","type":"STRING"}],"lang_counts":[]}]);
    m["relationships"] = json!([{"type":"EMPTY","members":[{"from":"A","to":"Directory","table":"r","indices":"r.indices","indptr":"r.indptr","rows":0}]}]);
    save(dir.path(), &m);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let r = GraphNeighborhoodRequest {
        anchor_label: "A",
        anchor_key: GraphAnchorKey::Int64(-1),
        relationship: Some("EMPTY"),
        other_label: Some("Directory"),
        direction: Some(GraphDirection::Outgoing),
        limit: 1,
        cursor: None,
    };
    let page = graph.neighborhood_graph(r, limits()).unwrap();
    assert!(page.edges().is_empty());
    assert_eq!(page.nodes()[0].primary_key().name(), "key");
    assert_eq!(
        page.nodes()[0].primary_key().value(),
        GraphPropertyValueRef::Int64(-1)
    );
    assert_eq!(
        prop(page.nodes()[0].properties(), "title"),
        GraphPropertyValueRef::String("")
    );
    let r = GraphNeighborhoodRequest {
        anchor_label: "Directory",
        anchor_key: GraphAnchorKey::String(""),
        relationship: Some("EMPTY"),
        other_label: Some("A"),
        direction: Some(GraphDirection::Incoming),
        limit: 1,
        cursor: None,
    };
    let page = graph.neighborhood_graph(r, limits()).unwrap();
    assert!(page.edges().is_empty());
    assert_eq!(page.nodes()[0].primary_key().name(), "path");
    assert_eq!(
        page.nodes()[0].primary_key().value(),
        GraphPropertyValueRef::String("")
    );
    assert_eq!(
        prop(page.nodes()[0].properties(), "path"),
        GraphPropertyValueRef::String("")
    );
}
fn empty_writer(path: &Path, schema: &str) {
    use ::parquet::{
        basic::Compression,
        file::{
            metadata::KeyValue,
            properties::{WriterProperties, WriterVersion},
            writer::SerializedFileWriter,
        },
        schema::parser::parse_message_type,
    };
    let props = WriterProperties::builder()
        .set_writer_version(WriterVersion::PARQUET_1_0)
        .set_dictionary_enabled(false)
        .set_compression(Compression::ZSTD(Default::default()))
        .set_key_value_metadata(Some(vec![KeyValue::new(
            "icebug_disk_version".into(),
            Some("v1".into()),
        )]))
        .build();
    SerializedFileWriter::new(
        fs::File::create(path).unwrap(),
        Arc::new(parse_message_type(schema).unwrap()),
        Arc::new(props),
    )
    .unwrap()
    .close()
    .unwrap();
}
#[test]
fn official_empty_row_group_zero_offsets_remain_fail_closed() {
    use ::parquet::file::{reader::FileReader, serialized_reader::SerializedFileReader};
    let dir = shuffled_fixture(0, false);
    let raw =
        SerializedFileReader::new(fs::File::open(dir.path().join("a.indices")).unwrap()).unwrap();
    let column = raw.metadata().row_group(0).column(0);
    eprintln!(
        "EMPTY_ROW_GROUP_RECEIPT {}",
        json!({"rows":raw.metadata().row_group(0).num_rows(),"data_page_offset":column.data_page_offset(),"compressed_size":column.compressed_size(),"uncompressed_size":column.uncompressed_size()})
    );
    assert_eq!(column.data_page_offset(), 0);
    assert_eq!(column.compressed_size(), 0);
    assert!(matches!(
        capture(dir.path(), context(), policy()),
        Err(DataReadError::InvalidParquet("chunk span"))
    ));
}
#[test]
fn used_csr_uid_and_selected_primary_key_errors_are_not_page_truncated() {
    for case in 0..6 {
        let dir = fixture();
        match case {
            0 => writer(
                &dir.path().join("a.indptr"),
                "message graph { required int64 ptr (UINT_64); }",
                &[Values::Int(&[0, 4, 3])],
            ),
            1 => writer(
                &dir.path().join("a.indices"),
                "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
                &[
                    Values::Int(&[0, 0, 2, 99]),
                    Values::String(&["edge0", "edge1", "edge2", "edge3"]),
                ],
            ),
            2 => writer(
                &dir.path().join("a.indices"),
                "message graph { required int64 target (UINT_64); optional binary uid (UTF8); }",
                &[
                    Values::Int(&[0, 0, 2, 1]),
                    Values::OptionalString(&[Some("edge0"), Some("edge1"), Some("edge2"), None]),
                ],
            ),
            3 => writer(
                &dir.path().join("a.indices"),
                "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
                &[
                    Values::Int(&[0, 0, 2, 1]),
                    Values::String(&["edge0", "edge1", "edge2", ""]),
                ],
            ),
            4 => writer(
                &dir.path().join("Function.parquet"),
                "message graph { required binary uid (UTF8); required binary name (UTF8); }",
                &[
                    Values::String(&["f0", "f1", "f0"]),
                    Values::String(&["alpha", "beta", "gamma"]),
                ],
            ),
            _ => writer(
                &dir.path().join("Function.parquet"),
                "message graph { optional binary uid (UTF8); required binary name (UTF8); }",
                &[
                    Values::OptionalString(&[Some("f0"), Some("f1"), None]),
                    Values::String(&["alpha", "beta", "gamma"]),
                ],
            ),
        }
        let graph = capture(dir.path(), context(), policy()).unwrap();
        let mut r = request();
        r.limit = 1;
        if case == 0 {
            // Exercise the malformed interval itself, without certifying an
            // unrelated source's pointers as a side effect of another page.
            r.anchor_key = GraphAnchorKey::String("isolated.rs");
        }
        let error = graph.neighborhood_graph(r, limits()).err().unwrap();
        match case {
            4 => assert!(matches!(error, DataReadError::DuplicatePrimaryKey)),
            5 => assert!(matches!(error, DataReadError::InvalidParquet(_))),
            _ => assert!(matches!(error, DataReadError::InvalidCsr(_))),
        }
    }
}
#[test]
fn rows_and_pages_limits_preserve_results_after_failure() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let mut p = limits();
    p.decode.max_decoded_rows = 17;
    assert!(matches!(
        graph.neighborhood_graph(request(), p),
        Err(DataReadError::LimitExceeded("decoded rows"))
    ));
    p = limits();
    p.decode.max_page_work = 1;
    assert!(matches!(
        graph.neighborhood_graph(request(), p),
        Err(DataReadError::LimitExceeded("page work"))
    ));
    let page = graph.neighborhood_graph(request(), limits()).unwrap();
    assert_eq!(page.edges().len(), 2);
}
#[test]
fn variable_length_uids_preserve_pagination_and_cursor_exclusions() {
    let dir = fixture();
    writer(
        &dir.path().join("a.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[Values::Int(&[0, 0, 0]), Values::String(&["a", "bb", "ccc"])],
    );
    writer(
        &dir.path().join("a.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 3, 3])],
    );
    let mut m: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    let mut relation = m["relationships"][1].clone();
    relation["members"][0]["rows"] = json!(3);
    m["relationships"] = json!([relation]);
    m["n_edges"] = json!(3);
    save(dir.path(), &m);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let exact = limits();
    let mut r = request();
    r.limit = 1;
    let page = graph.neighborhood_graph(r, exact).unwrap();
    assert_eq!(
        page.edges().iter().map(|e| e.uid()).collect::<Vec<_>>(),
        ["a"]
    );
    assert!(page.has_more());
    let cursor = page.next_cursor().unwrap();
    let beyond = GraphNeighborhoodCursor::from_parts(
        cursor.query_fingerprint(),
        GraphNeighborhoodPosition {
            uid: "zzzz",
            ..cursor.position()
        },
    );
    let mut r = request();
    r.cursor = Some(beyond);
    let empty = graph.neighborhood_graph(r, exact).unwrap();
    assert!(empty.edges().is_empty());
    assert!(!empty.has_more());
    assert_eq!(empty.nodes().len(), 1);
}
