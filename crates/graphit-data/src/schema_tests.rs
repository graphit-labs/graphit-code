use super::*;
use crate::tests::{Values, capture, context, fixture, manifest, policy, save, writer};
use graphit_core::file_lock::Cancellation;
use std::{
    fs, io,
    time::{Duration, Instant},
};

#[test]
fn complete_schema_borrows_declared_tables_and_validated_forward_members() {
    let dir = fixture();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    let function = manifest["node_tables"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|table| table["label"] == "Function")
        .unwrap();
    function["lang_counts"] = serde_json::json!([{"lang": "rust", "rows": 3}]);
    writer(
        &dir.path().join("reverse.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[
            Values::Int(&[0, 0, 0, 0]),
            Values::String(&["edge0", "edge1", "edge2", "edge3"]),
        ],
    );
    writer(
        &dir.path().join("reverse.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 2, 2, 4])],
    );
    manifest["add_reverse_edges"] = serde_json::json!(true);
    manifest["relationships"][1]["reverse_members"] = serde_json::json!([{
        "from":"Function", "to":"File", "table":"reverse",
        "indices":"reverse.indices", "indptr":"reverse.indptr", "rows":4
    }]);
    save(dir.path(), &manifest);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let schema = graph.schema().unwrap();
    let nodes = schema.node_tables().collect::<Vec<_>>();
    assert_eq!(nodes.len(), 2);
    let file = nodes.iter().find(|node| node.label() == "File").unwrap();
    assert_eq!(file.primary_key(), "path");
    assert_eq!(file.rows(), 2);
    assert_eq!(
        file.columns().collect::<Vec<_>>(),
        [
            ("path", "STRING"),
            ("name", "STRING"),
            ("line_number", "INT64"),
            ("ignored", "BOOL")
        ]
    );
    let function = nodes
        .iter()
        .find(|node| node.label() == "Function")
        .unwrap();
    assert_eq!(function.primary_key(), "uid");
    assert_eq!(function.rows(), 3);
    assert_eq!(
        function.language_counts().collect::<Vec<_>>(),
        [("rust", 3)]
    );
    let relationships = schema.relationships().collect::<Vec<_>>();
    assert_eq!(relationships.len(), 2);
    let contains = relationships
        .iter()
        .find(|member| member.kind() == "CONTAINS")
        .unwrap();
    assert_eq!(
        (contains.from_label(), contains.to_label(), contains.rows()),
        ("File", "Function", 4)
    );
    assert_eq!(
        contains.properties().collect::<Vec<_>>(),
        [("uid", "STRING")]
    );
    let calls = relationships
        .iter()
        .find(|member| member.kind() == "CALLS")
        .unwrap();
    assert_eq!(
        (calls.from_label(), calls.to_label(), calls.rows()),
        ("Function", "Function", 2)
    );
    assert!(calls.properties().all(|(name, _)| name != "target"));
}

#[test]
fn empty_validated_catalogue_exposes_empty_schema() {
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), &manifest());
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let schema = graph.schema().unwrap();
    assert_eq!(schema.node_tables().len(), 0);
    assert_eq!(schema.relationships().len(), 0);
}

#[test]
fn schema_preserves_original_cancellation_and_deadline_errors() {
    for expired in [false, true] {
        let dir = fixture();
        let cancel = Cancellation::default();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(if expired { 1 } else { 60 }),
            cancel.clone(),
        ));
        let graph = capture(dir.path(), Arc::clone(&original), policy()).unwrap();
        if expired {
            std::thread::sleep(
                original
                    .deadline()
                    .unwrap()
                    .saturating_duration_since(Instant::now())
                    + Duration::from_millis(1),
            );
        } else {
            cancel.cancel();
        }
        let expected = if expired {
            io::ErrorKind::TimedOut
        } else {
            io::ErrorKind::Interrupted
        };
        assert!(
            matches!(graph.schema(), Err(DataReadError::Source(ref e)) if e.kind() == expected)
        );
    }
}

#[test]
fn schema_rejects_changed_captured_content() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    fs::write(dir.path().join("icebug.json"), b"changed").unwrap();
    assert!(graph.schema().is_err());
}
