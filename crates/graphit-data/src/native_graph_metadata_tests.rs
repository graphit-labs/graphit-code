use super::*;
use crate::tests::{Values, capture, context, fixture, manifest, policy, save, writer};
use std::{fs, io::Read};

#[test]
fn publication_references_include_reverse_storage_and_open_through_capture() {
    let dir = fixture();
    fs::write(dir.path().join("schema.cypher"), b"fixture schema").unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    value["generation"] = serde_json::json!("publication-generation-α");
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
    value["add_reverse_edges"] = serde_json::json!(true);
    value["relationships"][1]["reverse_members"] = serde_json::json!([{
        "from": "Function", "to": "File", "table": "reverse",
        "indices": "reverse.indices", "indptr": "reverse.indptr", "rows": 4
    }]);
    save(dir.path(), &value);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let schema = graph.schema().unwrap();
    assert_eq!(schema.manifest_generation(), "publication-generation-α");
    assert_eq!(schema.generation(), graph.generation());
    assert_eq!(schema.schema_file(), "schema.cypher");
    assert_eq!(schema.relationships().len(), 2);
    let files = schema.referenced_files().collect::<Vec<_>>();
    assert_eq!(
        files,
        [
            "icebug.json",
            "schema.cypher",
            "File.parquet",
            "Function.parquet",
            "z.indices",
            "z.indptr",
            "a.indices",
            "a.indptr",
            "reverse.indices",
            "reverse.indptr"
        ]
    );
    for path in files {
        let file = graph.open_file(Path::new(path)).unwrap();
        let mut stream = file.stream(0).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, fs::read(dir.path().join(path)).unwrap());
    }
}

#[test]
fn a_declared_schema_reference_does_not_fabricate_a_captured_file() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let schema = graph.schema().unwrap();
    assert!(
        schema
            .referenced_files()
            .any(|path| path == schema.schema_file())
    );
    assert!(graph.open_file(Path::new(schema.schema_file())).is_err());
}

#[test]
fn existing_manifest_aliases_remain_declared_references() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = manifest();
    value["schema_file"] = serde_json::json!("icebug.json");
    save(dir.path(), &value);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    assert_eq!(
        graph
            .schema()
            .unwrap()
            .referenced_files()
            .collect::<Vec<_>>(),
        ["icebug.json", "icebug.json"]
    );
}
