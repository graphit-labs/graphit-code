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
        relationship: "CONTAINS",
        other_label: "Function",
        direction: GraphDirection::Outgoing,
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
    r.other_label = "File";
    r.direction = GraphDirection::Incoming;
    r.limit = 100;
    let incoming = graph.neighborhood_graph(r, limits()).unwrap();
    assert_eq!(incoming.edges().len(), 2);
    assert_eq!(incoming.edges()[0].from().row(), 0);
    assert_eq!(incoming.edges()[0].to(), incoming.anchor());
    assert_eq!(incoming.edges()[1].to(), incoming.anchor());
    let mut r = request();
    r.anchor_label = "Function";
    r.anchor_key = GraphAnchorKey::String("f0");
    r.other_label = "Function";
    r.relationship = "CALLS";
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
    r.other_label = "Function";
    r.relationship = "CALLS";
    r.direction = GraphDirection::Incoming;
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
        cursor.generation(),
        cursor.query_fingerprint(),
        "zzzz",
    );
    let mut r = request();
    r.cursor = Some(beyond);
    let page = graph.neighborhood_graph(r, limits()).unwrap();
    assert!(page.edges().is_empty());
    assert_eq!(page.nodes().len(), 1);
}
#[test]
fn pure_input_cursor_and_request_denials_perform_zero_io() {
    let dir = fixture();
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
    let page = graph.neighborhood_graph(request(), limits()).unwrap();
    let cursor = page.next_cursor().unwrap();

    for limit in [0, 101, usize::MAX] {
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
    r.relationship = "Missing";
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::UnknownRelationship)
    ));
    let mut r = request();
    r.other_label = "File";
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::UnavailableRelationshipEndpoint)
    ));
    let mut r = request();
    r.cursor = Some(GraphNeighborhoodCursor::from_parts(
        cursor.generation(),
        [0; 32],
        "edge0",
    ));
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidCursor)
    ));
    let mut r = request();
    r.cursor = Some(GraphNeighborhoodCursor::from_parts(
        cursor.generation(),
        cursor.query_fingerprint(),
        "",
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
    r.relationship = &name;
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidNeighborhoodRequest)
    ));

    let name = "x".repeat(policy().max_name_bytes);
    let mut r = request();
    r.relationship = &name;
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
        relationship: "R",
        other_label: "B",
        direction: GraphDirection::Outgoing,
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
        relationship: "R",
        other_label: "A",
        direction: GraphDirection::Outgoing,
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
        relationship: "R",
        other_label: "A",
        direction: GraphDirection::Outgoing,
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
                relationship: "R",
                other_label: "A",
                direction: GraphDirection::Outgoing,
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
fn foreign_generation_cursor_and_original_cancellation_fail_closed() {
    let dir = fixture();
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
    let page = graph.neighborhood_graph(request(), limits()).unwrap();
    let c = page.next_cursor().unwrap();
    let mut r = request();
    r.cursor = Some(GraphNeighborhoodCursor::from_parts(
        GraphGeneration([1; 32]),
        c.query_fingerprint(),
        c.last_uid(),
    ));

    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::InvalidCursor)
    ));

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
    r.direction = GraphDirection::Incoming;
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
fn shuffled_fixture(count: usize, duplicate: bool) -> tempfile::TempDir {
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
fn duplicate_uid_outside_cursor_and_evicted_heap_still_fails() {
    let dir = shuffled_fixture(300, true);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let mut r = request();
    r.limit = 1;
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::DuplicateEdgeUid)
    ));
    let mut r = request();
    r.cursor = Some(GraphNeighborhoodCursor::from_parts(
        graph.generation(),
        fingerprint(&request()).unwrap(),
        "zzzz",
    ));
    assert!(matches!(
        graph.neighborhood_graph(r, limits()),
        Err(DataReadError::DuplicateEdgeUid)
    ));
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
        if duplicate {
            assert!(matches!(
                graph.neighborhood_graph(request(), limits()),
                Err(DataReadError::DuplicateEdgeUid)
            ));
            continue;
        }
        let page = graph.neighborhood_graph(request(), limits()).unwrap();
        assert_eq!(
            page.edges().iter().map(|e| e.uid()).collect::<Vec<_>>(),
            ["aaa", "edge0"]
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
                    .find(|e| e.uid() == edge.uid())
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
        relationship: "EMPTY",
        other_label: "Directory",
        direction: GraphDirection::Outgoing,
        limit: 1,
        cursor: None,
    };
    let page = graph.neighborhood_graph(r, limits()).unwrap();
    assert!(page.edges().is_empty());
    assert_eq!(
        prop(page.nodes()[0].properties(), "title"),
        GraphPropertyValueRef::String("")
    );
    let r = GraphNeighborhoodRequest {
        anchor_label: "Directory",
        anchor_key: GraphAnchorKey::String(""),
        relationship: "EMPTY",
        other_label: "A",
        direction: GraphDirection::Incoming,
        limit: 1,
        cursor: None,
    };
    let page = graph.neighborhood_graph(r, limits()).unwrap();
    assert!(page.edges().is_empty());
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
fn full_suffix_csr_uid_and_primary_key_validation_is_not_page_truncated() {
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
        cursor.generation(),
        cursor.query_fingerprint(),
        "zzzz",
    );
    let mut r = request();
    r.cursor = Some(beyond);
    let empty = graph.neighborhood_graph(r, exact).unwrap();
    assert!(empty.edges().is_empty());
    assert!(!empty.has_more());
    assert_eq!(empty.nodes().len(), 1);
}
