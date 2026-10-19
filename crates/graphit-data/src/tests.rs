use super::*;
use ::parquet::{
    basic::Compression,
    data_type::{BoolType, ByteArray, ByteArrayType, Int64Type},
    file::{
        metadata::KeyValue,
        properties::{WriterProperties, WriterVersion},
        writer::SerializedFileWriter,
    },
    schema::parser::parse_message_type,
};
use graphit_core::{
    artifact_store::{CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet},
    file_lock::Cancellation,
};
use serde_json::{Value, json};
use std::{
    fs,
    time::{Duration, Instant},
};

pub(crate) fn policy() -> LocalGraphLimits {
    LocalGraphLimits {
        manifest_bytes: 65536,
        max_tables: 8,
        max_members: 16,
        max_columns: 16,
        max_name_bytes: 256,
        query_decoded_rows: 50000,
        max_row_groups: 8,
        max_footer_bytes: 65536,
        max_protocol_bytes: 1024 * 1024,
        max_list_entries: 1000,
        max_protocol_entries: 10000,
        max_header_bytes: 128,
        max_metadata_entries: 16,
        max_metadata_bytes: 65536,
        max_chunk_bytes: 1024 * 1024,
        max_pages: 1000,
        max_page_compressed_bytes: 65536,
        max_page_uncompressed_bytes: 65536,
    }
}
pub(crate) fn sample() -> GraphSampleLimits {
    GraphSampleLimits::new(2, 3, 8, 1024, 1000, 1000, 2).unwrap()
}
pub(crate) fn context() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ))
}
fn snapshot_policy() -> SnapshotPolicy {
    let retention = RetainedBytesLimits::new(1024, 1, 1024).unwrap();
    SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(16 * 1024 * 1024, 64, 4096, 8).unwrap(),
        RetainedFileSet::new([], retention).unwrap(),
        retention,
    )
    .unwrap()
}
pub(crate) fn capture(
    path: &Path,
    ctx: Arc<SnapshotContext>,
    limits: LocalGraphLimits,
) -> Result<CapturedLocalGraph> {
    CapturedLocalGraph::capture(
        path,
        &snapshot_policy(),
        SourceReadLimits::new(1024 * 1024, 4096, 1024 * 1024).unwrap(),
        ctx,
        LocalGraphPolicy::new(limits).unwrap(),
    )
}
pub(crate) enum Values<'a> {
    Int(&'a [i64]),
    String(&'a [&'a str]),
    OptionalString(&'a [Option<&'a str>]),
    Bool(&'a [bool]),
    OptionalBool(&'a [Option<bool>]),
}
pub(crate) fn writer(path: &Path, schema: &str, columns: &[Values<'_>]) {
    let schema = Arc::new(parse_message_type(schema).unwrap());
    let props = Arc::new(
        WriterProperties::builder()
            .set_writer_version(WriterVersion::PARQUET_1_0)
            .set_dictionary_enabled(false)
            .set_compression(Compression::ZSTD(Default::default()))
            .set_key_value_metadata(Some(vec![KeyValue::new(
                "icebug_disk_version".into(),
                Some("v1".into()),
            )]))
            .build(),
    );
    let mut file =
        SerializedFileWriter::new(fs::File::create(path).unwrap(), schema, props).unwrap();
    let mut group = file.next_row_group().unwrap();
    for values in columns {
        let mut column = group.next_column().unwrap().unwrap();
        match values {
            Values::Int(values) => {
                column
                    .typed::<Int64Type>()
                    .write_batch(values, None, None)
                    .unwrap();
            }
            Values::String(values) => {
                let bytes: Vec<_> = values.iter().map(|value| ByteArray::from(*value)).collect();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&bytes, None, None)
                    .unwrap();
            }
            Values::OptionalString(values) => {
                let bytes: Vec<_> = values
                    .iter()
                    .flatten()
                    .map(|value| ByteArray::from(*value))
                    .collect();
                let levels: Vec<_> = values
                    .iter()
                    .map(|value| i16::from(value.is_some()))
                    .collect();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&bytes, Some(&levels), None)
                    .unwrap();
            }
            Values::Bool(values) => {
                column
                    .typed::<BoolType>()
                    .write_batch(values, None, None)
                    .unwrap();
            }
            Values::OptionalBool(values) => {
                let levels: Vec<i16> = values
                    .iter()
                    .map(|value| i16::from(value.is_some()))
                    .collect();
                let values: Vec<bool> = values.iter().filter_map(|value| *value).collect();
                column
                    .typed::<BoolType>()
                    .write_batch(&values, Some(&levels), None)
                    .unwrap();
            }
        }
        column.close().unwrap();
    }
    group.close().unwrap();
    file.close().unwrap();
}
pub(crate) fn manifest() -> Value {
    json!({"manifest_version":3,"format":"icebug-canonical","storage":"direct-parquet",
    "finished":true,"add_reverse_edges":false,"schema_file":"schema.cypher","node_tables":[],"relationships":[],
    "relation_uids":true,"generation":"fixture-generation","n_edges":0,"repaired_strings":0,
    "invariants":{"indptr_row_groups":1,"self_loops":"forward-once"}})
}
pub(crate) fn save(path: &Path, manifest: &Value) {
    let mut manifest = manifest.clone();
    let nodes = manifest["node_tables"].as_array().unwrap().clone();
    for group in manifest["relationships"].as_array_mut().unwrap() {
        for field in ["members", "reverse_members"] {
            if let Some(members) = group.get_mut(field).and_then(Value::as_array_mut) {
                for member in members {
                    for (label, base, count) in [
                        ("from", "source_row_base", "source_rows"),
                        ("to", "target_row_base", "target_rows"),
                    ] {
                        if member.get(base).is_none() {
                            let node = nodes.iter().find(|node| node["label"] == member[label]);
                            member[base] = json!(0);
                            member[count] =
                                node.map(|node| node["rows"].clone()).unwrap_or(json!(0));
                        }
                    }
                }
            }
        }
    }
    fs::write(
        path.join("icebug.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}
pub(crate) fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    writer(
        &root.join("File.parquet"),
        "message graph { required binary path (UTF8); optional binary name (UTF8); required int64 line_number; required boolean ignored; }",
        &[
            Values::String(&["a.rs", "isolated.rs"]),
            Values::OptionalString(&[Some("ação 🚀"), None]),
            Values::Int(&[0, 7]),
            Values::Bool(&[true, false]),
        ],
    );
    writer(
        &root.join("Function.parquet"),
        "message graph { required binary uid (UTF8); required binary name (UTF8); }",
        &[
            Values::String(&["f0", "f1", "f2"]),
            Values::String(&["alpha", "beta", "gamma"]),
        ],
    );
    writer(
        &root.join("a.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[
            Values::Int(&[0, 0, 2, 1]),
            Values::String(&["edge0", "edge1", "edge2", "edge3"]),
        ],
    );
    writer(
        &root.join("a.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 4, 4])],
    );
    writer(
        &root.join("z.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[Values::Int(&[0, 1]), Values::String(&["loop", "other"])],
    );
    writer(
        &root.join("z.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&[0, 1, 1, 2])],
    );
    let mut value = manifest();
    value["n_edges"] = json!(6);
    value["node_tables"] = json!([
        {"label":"Function","rows":3,"shards":[{"file":"Function.parquet","row_base":0,"rows":3}],"primary_key":"uid","columns":[{"name":"uid","type":"STRING"},{"name":"name","type":"STRING"}],"lang_counts":[]},
        {"label":"File","rows":2,"shards":[{"file":"File.parquet","row_base":0,"rows":2}],"primary_key":"path","columns":[{"name":"path","type":"STRING"},{"name":"name","type":"STRING"},{"name":"line_number","type":"INT64"},{"name":"ignored","type":"BOOL"}],"lang_counts":[]}
    ]);
    value["relationships"] = json!([
        {"type":"CALLS","members":[{"from":"Function","to":"Function","table":"z","indices":"z.indices","indptr":"z.indptr","rows":2}]},
        {"type":"CONTAINS","members":[{"from":"File","to":"Function","table":"a","indices":"a.indices","indptr":"a.indptr","rows":4}]}
    ]);
    save(root, &value);
    dir
}
fn error<T>(result: Result<T>) -> DataReadError {
    result.err().expect("must fail closed")
}
#[test]
fn genuine_global_sample_and_summary_contract() {
    let dir = fixture();
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
    let result = graph.sample_graph(sample()).unwrap();
    assert_eq!(result.edges.len(), 3);
    assert_eq!(result.nodes.len(), 4);
    assert_eq!(result.nodes[0].name(), Some("ação 🚀"));
    assert_eq!(result.nodes[0].line(), None);
    assert_eq!(result.nodes[1].name(), None);
    assert_eq!(result.nodes[1].line(), Some(7));
    assert_eq!(result.edges[0].from(), result.edges[1].from());
    assert_eq!(result.edges[0].to(), result.edges[1].to());
    assert_ne!(result.edges[0].uid(), result.edges[1].uid());
    assert_eq!(result.edges[2].to().row(), 2);

    let again = graph.sample_graph(sample()).unwrap();
    assert_eq!(again.generation(), result.generation());
    assert!(
        again
            .nodes
            .iter()
            .map(|node| node.id())
            .eq(result.nodes.iter().map(|node| node.id()))
    );
}
#[test]
fn global_cap_spans_members_and_keeps_self_loop_once() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let result = graph
        .sample_graph(GraphSampleLimits::new(2, 5, 12, 1024, 1000, 1000, 2).unwrap())
        .unwrap();
    assert_eq!(result.edges.len(), 5);
    let edge = &result.edges[4];
    assert_eq!(edge.uid(), "loop");
    assert_eq!(edge.from(), edge.to());
}
#[test]
fn empty_catalog_is_successful_empty_input() {
    let dir = tempfile::tempdir().unwrap();
    save(dir.path(), &manifest());
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let output = graph.sample_graph(sample()).unwrap();
    assert!(output.nodes().is_empty());
    assert!(output.edges().is_empty());
}
#[test]
fn actual_empty_zero_column_sdk_footer() {
    let dir = tempfile::tempdir().unwrap();
    writer(&dir.path().join("empty.parquet"), "message graph { }", &[]);
    let mut value = manifest();
    value["node_tables"] = json!([{"label":"File","rows":0,"shards":[{"file":"empty.parquet","row_base":0,"rows":0}],
        "primary_key":"","columns":[],"lang_counts":[]}]);
    save(dir.path(), &value);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    assert!(graph.sample_graph(sample()).unwrap().nodes().is_empty());
}
#[test]
fn unsigned_csr_genuine_footer_is_admitted() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    assert_eq!(graph.members[0].indices.fields[0].1, Scalar::UInt64);
    assert_eq!(graph.members[0].indptr.fields[0].1, Scalar::UInt64);
}
#[test]
fn cancellation_preserves_original_request() {
    let dir = fixture();
    let ctx = context();
    let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
    ctx.cancellation().cancel();
    assert!(matches!(
        error(graph.sample_graph(sample())),
        DataReadError::Source(_)
    ));
}
#[test]
fn reader_composition_preserves_original_request() {
    let dir = fixture();
    let ctx = context();
    let make_reader = || {
        CapturedSourceReader::capture(
            dir.path(),
            NodeKind::Directory,
            &snapshot_policy(),
            SourceReadLimits::new(1024 * 1024, 4096, 1024 * 1024).unwrap(),
            Arc::clone(&ctx),
        )
        .unwrap()
    };
    let rejected = CapturedLocalGraph::from_reader(
        make_reader(),
        context(),
        LocalGraphPolicy::new(policy()).unwrap(),
    );
    assert!(matches!(
        error(rejected),
        DataReadError::Source(error) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
    let graph = CapturedLocalGraph::from_reader(
        make_reader(),
        Arc::clone(&ctx),
        LocalGraphPolicy::new(policy()).unwrap(),
    )
    .unwrap();
    assert_eq!(graph.sample_graph(sample()).unwrap().edges().len(), 3);
    ctx.cancellation().cancel();
    assert!(matches!(error(graph.schema()), DataReadError::Source(_)));
}
#[test]
fn original_capture_never_adopts_changed_source() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    fs::write(dir.path().join("Function.parquet"), b"changed").unwrap();
    assert!(matches!(
        error(graph.sample_graph(sample())),
        DataReadError::Source(_)
    ));
}
#[test]
fn pk_duplicate_and_null_are_rejected() {
    for null in [false, true] {
        let dir = fixture();
        writer(
            &dir.path().join("Function.parquet"),
            "message graph { optional binary uid (UTF8); required binary name (UTF8); }",
            &[
                Values::OptionalString(if null {
                    &[Some("f0"), None, Some("f2")]
                } else {
                    &[Some("f0"), Some("f0"), Some("f2")]
                }),
                Values::String(&["a", "b", "c"]),
            ],
        );
        let graph = capture(dir.path(), context(), policy()).unwrap();
        let failure = error(graph.sample_graph(sample()));
        assert!(if null {
            matches!(
                failure,
                DataReadError::InvalidParquet("nonnull primary key")
            )
        } else {
            matches!(failure, DataReadError::DuplicatePrimaryKey)
        });
    }
}
#[test]
fn csr_bad_target_pointer_and_output_uid_fail_closed() {
    for variant in 0..4 {
        let dir = fixture();
        if variant == 0 {
            writer(
                &dir.path().join("a.indices"),
                "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
                &[
                    Values::Int(&[9, 0, 2, 1]),
                    Values::String(&["edge0", "edge1", "edge2", "edge3"]),
                ],
            );
        }
        if variant == 1 {
            writer(
                &dir.path().join("a.indptr"),
                "message graph { required int64 ptr (UINT_64); }",
                &[Values::Int(&[0, 4, 3])],
            );
        }
        if variant == 2 {
            writer(
                &dir.path().join("a.indptr"),
                "message graph { required int64 ptr (UINT_64); }",
                &[Values::Int(&[0, 3, 3])],
            );
        }
        if variant == 3 {
            writer(
                &dir.path().join("a.indices"),
                "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
                &[
                    Values::Int(&[0, 0, 2, 1]),
                    Values::String(&["same", "same", "edge2", "edge3"]),
                ],
            );
        }
        let graph = capture(dir.path(), context(), policy()).unwrap();
        let failure = error(graph.sample_graph(sample()));
        assert!(matches!(
            failure,
            DataReadError::InvalidCsr(_) | DataReadError::DuplicateEdgeUid
        ));
    }
}
#[test]
fn duplicate_manifest_fields_are_rejected() {
    let value = serde_json::to_string(&manifest()).unwrap();
    let bytes = value.replacen("{", "{\"n_edges\":0,", 1);
    assert!(matches!(
        error(Manifest::parse(bytes.as_bytes(), policy())),
        DataReadError::InvalidManifest(_)
    ));
}
#[test]
fn manifest_identity_and_path_boundaries() {
    for variant in 0..5 {
        let dir = fixture();
        let mut value: Value =
            serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
        match variant {
            0 => value["node_tables"][0]["label"] = json!("File"),
            1 => value["node_tables"][0]["shards"][0]["file"] = json!("../escape"),
            2 => value["node_tables"][0]["shards"][0]["file"] = json!("/absolute"),
            3 => value["node_tables"][0]["shards"][0]["file"] = json!("File.parquet"),
            _ => value["n_edges"] = json!(5),
        }
        save(dir.path(), &value);
        assert!(matches!(
            error(capture(dir.path(), context(), policy())),
            DataReadError::InvalidManifest(_)
        ));
    }
}
#[test]
fn seed_endpoint_constructor_bounds_and_batch_admission() {
    assert!(GraphSampleLimits::new(300, 1000, 2299, 1, 1, 1, 1).is_err());
    assert!(GraphSampleLimits::new(300, 1000, 2300, 1, 1, 1, 1).is_ok());
    assert!(GraphSampleLimits::new(1, usize::MAX, usize::MAX, 1, 1, 1, 1).is_err());
    assert!(GraphSampleLimits::new(1, 1, 3, 1, 1, 1, 2).is_err());
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    assert!(matches!(
        error(graph.sample_graph(GraphSampleLimits::new(2, 3, 8, 1024, 3, 1000, 2).unwrap())),
        DataReadError::LimitExceeded("decoded rows")
    ));
}
#[test]
fn string_limits_are_closed() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    assert!(matches!(
        error(graph.sample_graph(GraphSampleLimits::new(2, 3, 8, 1, 1000, 1000, 2).unwrap())),
        DataReadError::LimitExceeded(_)
    ));
}
#[test]
fn logical_policy_positive_fields() {
    let mut limits = policy();
    limits.max_protocol_entries = 0;
    assert!(LocalGraphPolicy::new(limits).is_err());
}
#[test]
fn footer_manifest_row_differential_is_rejected() {
    let dir = fixture();
    let mut value: Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    value["node_tables"][0]["rows"] = json!(4);
    value["node_tables"][0]["shards"][0]["rows"] = json!(4);
    for group in value["relationships"].as_array_mut().unwrap() {
        for member in group["members"].as_array_mut().unwrap() {
            if member["from"] == "Function" {
                member["source_rows"] = json!(4);
            }
            if member["to"] == "Function" {
                member["target_rows"] = json!(4);
            }
        }
    }
    save(dir.path(), &value);
    assert!(matches!(
        error(capture(dir.path(), context(), policy())),
        DataReadError::InvalidParquet("footer identity")
    ));
}
#[test]
fn retained_source_range_bytes_clone_and_slice_preserve_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source");
    fs::write(&path, [1, 2, 3, 4]).unwrap();
    let reader = CapturedSourceReader::capture(
        &path,
        NodeKind::File,
        &snapshot_policy(),
        SourceReadLimits::new(4, 4, 4).unwrap(),
        context(),
    )
    .unwrap();
    let file = reader.open_file(Path::new("")).unwrap();
    let original = bytes::Bytes::from_owner(file.read_range(0, 4).unwrap());
    let clone = original.clone();
    let slice = clone.slice(1..2);
    let empty = clone.slice(0..0);
    drop(original);
    drop(clone);
    assert_eq!(file.read_range(0, 1).unwrap().as_ref(), [1]);
    assert_eq!(slice.as_ref(), [2]);
    drop(slice);
    assert_eq!(file.read_range(0, 4).unwrap().as_ref(), [1, 2, 3, 4]);
    drop(empty);
}
#[cfg(unix)]
#[test]
fn native_non_utf8_root_is_rejected_without_replacement() {
    use std::os::unix::ffi::OsStringExt;
    let parent = tempfile::tempdir().unwrap();
    let path = parent
        .path()
        .join(std::ffi::OsString::from_vec(vec![b'x', 0xff]));
    fs::create_dir(&path).unwrap();
    save(&path, &manifest());
    assert!(matches!(
        capture(&path, context(), policy()),
        Err(DataReadError::Source(_))
    ));
}
#[cfg(unix)]
#[test]
fn inode_replacement_equal_bytes_and_mode_change_are_conflicts() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [false, true] {
        let dir = fixture();
        let graph = capture(dir.path(), context(), policy()).unwrap();
        let path = dir.path().join("Function.parquet");
        if mode {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        } else {
            fs::rename(&path, dir.path().join("previous")).unwrap();
            fs::copy(dir.path().join("previous"), &path).unwrap();
        }
        assert!(matches!(
            error(graph.sample_graph(sample())),
            DataReadError::Source(_)
        ));
    }
}

#[test]
fn all_local_policy_positive_fields_are_enforced() {
    let mut limits = policy();
    limits.manifest_bytes = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "manifest_bytes");
    let mut limits = policy();
    limits.max_tables = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_tables");
    let mut limits = policy();
    limits.max_members = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_members");
    let mut limits = policy();
    limits.max_columns = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_columns");
    let mut limits = policy();
    limits.max_name_bytes = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_name_bytes");
    let mut limits = policy();
    limits.query_decoded_rows = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "query_decoded_rows");
    let mut limits = policy();
    limits.max_row_groups = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_row_groups");
    let mut limits = policy();
    limits.max_footer_bytes = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_footer_bytes");
    let mut limits = policy();
    limits.max_protocol_bytes = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_protocol_bytes");
    let mut limits = policy();
    limits.max_list_entries = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_list_entries");
    let mut limits = policy();
    limits.max_protocol_entries = 0;
    assert!(
        LocalGraphPolicy::new(limits).is_err(),
        "max_protocol_entries"
    );
    let mut limits = policy();
    limits.max_header_bytes = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_header_bytes");
    let mut limits = policy();
    limits.max_metadata_entries = 0;
    assert!(
        LocalGraphPolicy::new(limits).is_err(),
        "max_metadata_entries"
    );
    let mut limits = policy();
    limits.max_metadata_bytes = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_metadata_bytes");
    let mut limits = policy();
    limits.max_chunk_bytes = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_chunk_bytes");
    let mut limits = policy();
    limits.max_pages = 0;
    assert!(LocalGraphPolicy::new(limits).is_err(), "max_pages");
    let mut limits = policy();
    limits.max_page_compressed_bytes = 0;
    assert!(
        LocalGraphPolicy::new(limits).is_err(),
        "max_page_compressed_bytes"
    );
    let mut limits = policy();
    limits.max_page_uncompressed_bytes = 0;
    assert!(
        LocalGraphPolicy::new(limits).is_err(),
        "max_page_uncompressed_bytes"
    );
}
#[test]
fn manifest_and_catalog_exact_plus_one_boundaries() {
    let dir = fixture();
    let manifest_len = fs::metadata(dir.path().join("icebug.json")).unwrap().len() as usize;
    for variant in 0..4 {
        let mut exact = policy();
        match variant {
            0 => exact.manifest_bytes = manifest_len,
            1 => exact.max_tables = 2,
            2 => exact.max_members = 2,
            _ => exact.max_columns = 4,
        }
        assert!(
            capture(dir.path(), context(), exact).is_ok(),
            "exact {variant}"
        );
        match variant {
            0 => exact.manifest_bytes -= 1,
            1 => exact.max_tables -= 1,
            2 => exact.max_members -= 1,
            _ => exact.max_columns -= 1,
        }
        assert!(
            matches!(
                error(capture(dir.path(), context(), exact)),
                DataReadError::LimitExceeded(_)
            ),
            "plus-one {variant}"
        );
    }
}

#[test]
fn generated_graph_accepts_auxiliary_lance_files_without_a_corpus_file_quota() {
    let dir = fixture();
    let lance = dir.path().join("source.lance");
    fs::create_dir(&lance).unwrap();
    for index in 0..64 {
        fs::write(lance.join(format!("fragment-{index}.lance")), b"auxiliary").unwrap();
    }
    let original = context();
    let reader = CapturedSourceReader::capture_generated(
        dir.path(),
        NodeKind::Directory,
        SourceReadLimits::generated(1024 * 1024, 4096).unwrap(),
        Arc::clone(&original),
    )
    .unwrap();
    let graph =
        CapturedLocalGraph::from_reader(reader, original, LocalGraphPolicy::new(policy()).unwrap())
            .unwrap();
    assert_eq!(graph.schema().unwrap().node_tables().len(), 2);
    assert_eq!(graph.sample_graph(sample()).unwrap().nodes().len(), 4);
}

#[test]
fn manifest_row_counts_do_not_impose_corpus_admission_quotas() {
    let dir = fixture();
    let mut value: Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    let rows = 100_000_001u64;
    value["node_tables"][0]["rows"] = json!(rows);
    value["node_tables"][0]["shards"][0]["rows"] = json!(rows);
    for group in value["relationships"].as_array_mut().unwrap() {
        for member in group["members"].as_array_mut().unwrap() {
            if member["from"] == "Function" {
                member["source_rows"] = json!(rows);
            }
            if member["to"] == "Function" {
                member["target_rows"] = json!(rows);
            }
        }
    }
    value["relationships"][0]["members"][0]["rows"] = json!(rows);
    value["n_edges"] = json!(rows + 4);
    let bytes = serde_json::to_vec(&value).unwrap();
    let parsed = Manifest::parse(&bytes, policy()).unwrap();
    assert!(parsed.node_tables.iter().any(|table| table.rows == rows));
    // Admission is independent of corpus size; physical row-count integrity is not.
    save(dir.path(), &value);
    assert!(matches!(
        error(capture(dir.path(), context(), policy())),
        DataReadError::InvalidParquet(_)
    ));
}

#[test]
fn query_decode_limit_does_not_reject_the_captured_corpus() {
    let dir = fixture();
    let mut limits = policy();
    limits.query_decoded_rows = 1;
    let graph = capture(dir.path(), context(), limits).unwrap();
    assert_eq!(graph.schema().unwrap().node_tables().len(), 2);
    let mut request = sample();
    request.max_decoded_rows = limits.query_decoded_rows;
    request.row_batch_size = 1;
    assert!(matches!(
        error(graph.sample_graph(request)),
        DataReadError::LimitExceeded("decoded rows")
    ));
}
#[test]
fn original_cancellation_and_deadline_prevent_capture() {
    let dir = fixture();
    let ctx = context();
    ctx.cancellation().cancel();
    assert!(matches!(
        error(capture(dir.path(), Arc::clone(&ctx), policy())),
        DataReadError::Source(_)
    ));
    let ctx = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    assert!(matches!(
        error(capture(dir.path(), ctx, policy())),
        DataReadError::Source(_)
    ));
}

#[cfg(unix)]
#[test]
fn source_symlink_rejection_and_original_hardlink_alias_conflict() {
    use std::os::unix::fs::symlink;
    let dir = fixture();
    let target = dir.path().join("Function.parquet");
    let other = dir.path().join("other");
    fs::rename(&target, &other).unwrap();
    symlink(&other, &target).unwrap();
    assert!(matches!(
        error(capture(dir.path(), context(), policy())),
        DataReadError::Source(_)
    ));
    let dir = fixture();
    let target = dir.path().join("Function.parquet");
    let other = dir.path().join("other");
    fs::hard_link(&target, &other).unwrap();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    assert!(graph.sample_graph(sample()).is_ok());
    let mut bytes = fs::read(&other).unwrap();
    bytes[4] ^= 1;
    fs::write(&other, bytes).unwrap();
    assert!(matches!(
        error(graph.sample_graph(sample())),
        DataReadError::Source(_)
    ));
}

#[test]
fn genuine_300_seed_1000_edges_reaches_2300_nodes_without_endpoint_loss() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let seeds: Vec<_> = (0..300).map(|i| format!("seed{i}")).collect();
    let seed_refs: Vec<_> = seeds.iter().map(String::as_str).collect();
    let endpoints: Vec<_> = (0..2000).map(|i| format!("node{i}")).collect();
    let endpoint_refs: Vec<_> = endpoints.iter().map(String::as_str).collect();
    writer(
        &root.join("A.parquet"),
        "message graph { required binary uid (UTF8); }",
        &[Values::String(&seed_refs)],
    );
    writer(
        &root.join("B.parquet"),
        "message graph { required binary uid (UTF8); }",
        &[Values::String(&endpoint_refs)],
    );
    let targets: Vec<_> = (1000..2000).collect();
    let uids: Vec<_> = (0..1000).map(|i| format!("edge{i}")).collect();
    let uid_refs: Vec<_> = uids.iter().map(String::as_str).collect();
    writer(
        &root.join("r.indices"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[Values::Int(&targets), Values::String(&uid_refs)],
    );
    let ptrs: Vec<_> = (0..=2000).map(|i| i.min(1000)).collect();
    writer(
        &root.join("r.indptr"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Values::Int(&ptrs)],
    );
    let mut value = manifest();
    value["n_edges"] = json!(1000);
    value["node_tables"] = json!([
        {"label":"A","rows":300,"shards":[{"file":"A.parquet","row_base":0,"rows":300}],"primary_key":"uid","columns":[{"name":"uid","type":"STRING"}],"lang_counts":[]},
        {"label":"B","rows":2000,"shards":[{"file":"B.parquet","row_base":0,"rows":2000}],"primary_key":"uid","columns":[{"name":"uid","type":"STRING"}],"lang_counts":[]}]);
    value["relationships"] = json!([{"type":"PAIR","members":[{"from":"B","to":"B","table":"r","indices":"r.indices","indptr":"r.indptr","rows":1000}]}]);
    save(root, &value);
    let policy = policy();
    let graph = capture(root, context(), policy).unwrap();
    let result = graph
        .sample_graph(GraphSampleLimits::new(300, 1000, 2300, 256, 10000, 1000, 64).unwrap())
        .unwrap();
    assert_eq!(result.nodes().len(), 2300);
    assert_eq!(result.edges().len(), 1000);
    assert_eq!(result.nodes()[299].label(), "A");
    assert_eq!(result.nodes()[2299].id().row(), 1999);
    for (index, edge) in result.edges().iter().enumerate() {
        assert_eq!(edge.from().row(), index as u64);
        assert_eq!(edge.to().row(), index as u64 + 1000);
    }
}

#[test]
fn full_decoded_rows_and_string_exact_plus_one_limits() {
    let dir = fixture();
    let resources = policy();
    let graph = capture(dir.path(), context(), resources).unwrap();
    assert!(
        graph
            .sample_graph(GraphSampleLimits::new(2, 3, 8, 11, 18, 1000, 2).unwrap())
            .is_ok()
    );
    assert!(matches!(
        error(graph.sample_graph(GraphSampleLimits::new(2, 3, 8, 11, 17, 1000, 2).unwrap())),
        DataReadError::LimitExceeded("decoded rows")
    ));
    assert!(matches!(
        error(graph.sample_graph(GraphSampleLimits::new(2, 3, 8, 10, 18, 1000, 2).unwrap())),
        DataReadError::LimitExceeded(_)
    ));
}
#[test]
fn page_work_failure_preserves_the_captured_graph() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    assert!(matches!(
        graph.sample_graph(GraphSampleLimits::new(2, 3, 8, 1024, 1000, 1, 2).unwrap()),
        Err(DataReadError::LimitExceeded("page work"))
    ));
    assert!(graph.sample_graph(sample()).is_ok());
}
#[cfg(test)]
mod configured_local_graph {
    use super::*;
    use graphit_core::{
        brand::Brand,
        config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
    };

    fn document(source: ConfigSource, value: Value) -> ConfigDocument {
        ConfigDocument::new(
            &Brand::default(),
            source,
            [(ConfigKey::new(LocalGraphPolicy::CONFIG_KEY).unwrap(), value)],
        )
        .unwrap()
    }
    fn resolve(config: &ConfigSnapshot) -> LocalGraphPolicy {
        LocalGraphPolicy::resolve(config, context().as_ref()).unwrap()
    }

    #[test]
    fn defaults_open_canonical_graph_and_sample() {
        let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        let policy = resolve(&config);
        let dir = fixture();
        let original = context();
        let graph = CapturedLocalGraph::capture(
            dir.path(),
            &snapshot_policy(),
            SourceReadLimits::new(1024 * 1024, 4096, 1024 * 1024).unwrap(),
            original,
            policy,
        )
        .unwrap();
        assert!(!graph.sample_graph(sample()).unwrap().nodes().is_empty());
    }

    #[test]
    fn highest_layer_selects_partial_object_without_merging_lower_limits() {
        let global = document(ConfigSource::Global, json!({"manifest_bytes":1}));
        let config = ConfigSnapshot::new(&Brand::default(), [global.clone()]).unwrap();
        let dir = fixture();
        assert!(matches!(
            capture(dir.path(), context(), resolve(&config).limits()),
            Err(DataReadError::LimitExceeded("manifest bytes"))
        ));
        let command = document(ConfigSource::Command, json!({"max_tables":2}));
        let config = ConfigSnapshot::new(&Brand::default(), [global, command]).unwrap();
        assert_eq!(
            config.source(&ConfigKey::new(LocalGraphPolicy::CONFIG_KEY).unwrap()),
            Some(ConfigSource::Command)
        );
        let graph = capture(dir.path(), context(), resolve(&config).limits()).unwrap();
        assert!(!graph.sample_graph(sample()).unwrap().nodes().is_empty());
    }

    #[test]
    fn environment_json_uses_explicit_captured_variable() {
        let brand = Brand::default();
        let key = ConfigKey::new(LocalGraphPolicy::CONFIG_KEY).unwrap();
        let environment = ConfigSnapshot::capture_environment(
            &brand,
            std::slice::from_ref(&key),
            [(
                key.environment_variable(&brand).into(),
                r#"{"max_tables":1}"#.into(),
            )],
        )
        .unwrap();
        let config = ConfigSnapshot::new(&brand, [environment]).unwrap();
        assert_eq!(config.source(&key), Some(ConfigSource::Environment));
        let dir = fixture();
        assert!(matches!(
            capture(dir.path(), context(), resolve(&config).limits()),
            Err(DataReadError::LimitExceeded("tables"))
        ));
    }

    #[test]
    fn invalid_configuration_preserves_errors_before_dataset_access() {
        for value in [
            json!({"max_tables":0}),
            json!({"manifest_bytes":-1}),
            json!({"max_columns":1.5}),
            json!({"max_footer_bytes":"many"}),
            json!({"unknown":1}),
            json!([]),
            json!("not an object"),
        ] {
            let config =
                ConfigSnapshot::new(&Brand::default(), [document(ConfigSource::Global, value)])
                    .unwrap();
            assert_eq!(
                LocalGraphPolicy::resolve(&config, context().as_ref())
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::InvalidData
            );
        }
        #[cfg(target_pointer_width = "64")]
        {
            let config = ConfigSnapshot::new(
                &Brand::default(),
                [document(
                    ConfigSource::Command,
                    json!({"max_tables":u64::from(u32::MAX)+1}),
                )],
            )
            .unwrap();
            let error = LocalGraphPolicy::resolve(&config, context().as_ref()).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
            assert!(matches!(
                error.get_ref().unwrap().downcast_ref::<DataReadError>(),
                Some(DataReadError::InvalidPolicy("identity count"))
            ));
        }
    }

    #[test]
    fn original_cancel_and_deadline_are_not_refreshed() {
        let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
        let cancel = Cancellation::default();
        cancel.cancel();
        let original = SnapshotContext::new(Instant::now() + Duration::from_secs(60), cancel);
        assert_eq!(
            LocalGraphPolicy::resolve(&config, &original)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Interrupted
        );
        let original = SnapshotContext::new(
            Instant::now() - Duration::from_secs(1),
            Cancellation::default(),
        );
        assert_eq!(
            LocalGraphPolicy::resolve(&config, &original)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::TimedOut
        );
    }
}

#[test]
fn sample_marked_metadata_does_not_exhaust_seeds_or_hide_custom_labels() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut names: Vec<_> = (0..303).map(|i| format!("node{i}")).collect();
    let refs: Vec<_> = names.iter().map(String::as_str).collect();
    let mut markers = vec![Some(true); 303];
    markers[301] = Some(false);
    markers[302] = None;
    writer(
        &root.join("observations.parquet"),
        "message graph { required binary uid (UTF8); optional boolean is_file_observation; }",
        &[Values::String(&refs), Values::OptionalBool(&markers)],
    );
    writer(
        &root.join("Function.parquet"),
        "message graph { required binary uid (UTF8); }",
        &[Values::String(&["function0", "function1"])],
    );
    let mut value = manifest();
    value["n_edges"] = json!(0);
    value["node_tables"] = json!([
        {"label":"AstFileObservation","rows":303,"shards":[{"file":"observations.parquet","row_base":0,"rows":303}],"primary_key":"uid","columns":[{"name":"uid","type":"STRING"},{"name":"is_file_observation","type":"BOOL"}],"lang_counts":[]},
        {"label":"Function","rows":2,"shards":[{"file":"Function.parquet","row_base":0,"rows":2}],"primary_key":"uid","columns":[{"name":"uid","type":"STRING"}],"lang_counts":[]}]);
    value["relationships"] = json!([]);
    save(root, &value);
    let graph = capture(root, context(), policy()).unwrap();
    let limits = GraphSampleLimits::new(300, 1000, 2300, 256, 10000, 1000, 64).unwrap();
    let generic = graph.sample_graph(limits).unwrap();
    assert_eq!(generic.nodes().len(), 300);
    assert!(
        generic
            .nodes()
            .iter()
            .all(|n| n.label() == "AstFileObservation")
    );
    let filtered = graph
        .sample_graph_excluding_marked(limits, "is_file_observation")
        .unwrap();
    assert_eq!(filtered.nodes().len(), 4);
    assert_eq!(filtered.nodes()[0].label(), "AstFileObservation");
    assert_eq!(filtered.nodes()[0].id().row(), 301);
    assert_eq!(filtered.nodes()[1].id().row(), 302);
    assert_eq!(filtered.nodes()[2].label(), "Function");
    assert!(filtered.edges().is_empty());
    // A duplicate at the tail still fails; omitted observations are not skipped integrity work.
    names[302] = names[0].clone();
    let refs: Vec<_> = names.iter().map(String::as_str).collect();
    drop(graph);
    writer(
        &root.join("observations.parquet"),
        "message graph { required binary uid (UTF8); optional boolean is_file_observation; }",
        &[Values::String(&refs), Values::OptionalBool(&markers)],
    );
    let graph = capture(root, context(), policy()).unwrap();
    assert!(matches!(
        graph.sample_graph_excluding_marked(limits, "is_file_observation"),
        Err(DataReadError::DuplicatePrimaryKey)
    ));
}

#[test]
fn sample_primary_key_preserves_observed_string_and_int64() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    writer(
        &root.join("strings.parquet"),
        "message graph { required binary entity_key (UTF8); required binary uid (UTF8); }",
        &[
            Values::String(&["[exact,key]", "different"]),
            Values::String(&["uid0", "uid1"]),
        ],
    );
    writer(
        &root.join("ints.parquet"),
        "message graph { required int64 number_key; }",
        &[Values::Int(&[-7, i64::MAX])],
    );
    let mut value = manifest();
    value["n_edges"] = json!(0);
    value["node_tables"] = json!([
        {"label":"Function","rows":2,"shards":[{"file":"strings.parquet","row_base":0,"rows":2}],"primary_key":"entity_key","columns":[{"name":"entity_key","type":"STRING"},{"name":"uid","type":"STRING"}],"lang_counts":[]},
        {"label":"Numeric","rows":2,"shards":[{"file":"ints.parquet","row_base":0,"rows":2}],"primary_key":"number_key","columns":[{"name":"number_key","type":"INT64"}],"lang_counts":[]}
    ]);
    value["relationships"] = json!([]);
    save(root, &value);
    let graph = capture(root, context(), policy()).unwrap();
    let limits = GraphSampleLimits::new(4, 1000, 2300, 256, 10000, 1000, 64).unwrap();
    let result = graph.sample_graph(limits).unwrap();
    let keys: Vec<_> = result
        .nodes()
        .iter()
        .map(|node| {
            let key = node.primary_key().unwrap();
            (key.name(), key.value())
        })
        .collect();
    assert_eq!(
        keys,
        vec![
            ("entity_key", GraphPropertyValueRef::String("[exact,key]")),
            ("entity_key", GraphPropertyValueRef::String("different")),
            ("number_key", GraphPropertyValueRef::Int64(-7)),
            ("number_key", GraphPropertyValueRef::Int64(i64::MAX))
        ]
    );
    assert_eq!(result.nodes()[0].uid(), Some("uid0"));
}

pub(crate) fn shard_fixture() -> tempfile::TempDir {
    let dir = fixture();
    writer(
        &dir.path().join("Function-next.parquet"),
        "message graph { required binary uid (UTF8); required binary name (UTF8); required boolean added; }",
        &[
            Values::String(&["f3"]),
            Values::String(&["delta"]),
            Values::Bool(&[true]),
        ],
    );
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    for node in manifest["node_tables"].as_array_mut().unwrap() {
        let base = if node["label"] == "File" { 10 } else { 100 };
        node["shards"][0]["row_base"] = json!(base);
        if node["label"] == "Function" {
            node["rows"] = json!(4);
            node["shards"]
                .as_array_mut()
                .unwrap()
                .push(json!({"file":"Function-next.parquet","row_base":300,"rows":1}));
            node["columns"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name":"added","type":"BOOL"}));
        }
    }
    for group in manifest["relationships"].as_array_mut().unwrap() {
        for member in group["members"].as_array_mut().unwrap() {
            member["source_row_base"] = json!(if member["from"] == "File" { 10 } else { 100 });
            member["target_row_base"] = json!(if member["to"] == "File" { 10 } else { 100 });
        }
    }
    save(dir.path(), &manifest);
    dir
}

#[test]
fn file_shards_preserve_sparse_node_ids_and_csr_endpoints_without_rewriting_tables() {
    let dir = shard_fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let sampled = graph
        .sample_graph(GraphSampleLimits::new(6, 6, 18, 1024, 10000, 10000, 2).unwrap())
        .unwrap();
    let mut rows = sampled
        .nodes()
        .iter()
        .map(|node| node.id().row())
        .collect::<Vec<_>>();
    rows.sort_unstable();
    assert_eq!(rows, [10, 11, 100, 101, 102, 300]);
    assert_eq!(sampled.edges().len(), 6);
    assert!(
        sampled
            .edges()
            .iter()
            .all(|edge| edge.from().row() >= 10 && edge.to().row() >= 100)
    );
    let request = GraphNeighborhoodRequest {
        anchor_label: "File",
        anchor_key: GraphAnchorKey::String("a.rs"),
        relationship: "CONTAINS",
        other_label: "Function",
        direction: GraphDirection::Outgoing,
        limit: 10,
        cursor: None,
    };
    let page = graph
        .neighborhood_graph(
            request,
            GraphNeighborhoodLimits::new(1024, 16, 10000, 100000, 10000, 2).unwrap(),
        )
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
                    && property.value() == GraphPropertyValueRef::Null)
        );
    }
}

#[test]
fn generated_local_manifest_has_no_imported_corpus_quota_and_keeps_structural_validation() {
    let dir = fixture();
    fs::write(dir.path().join("schema.cypher"), b"producer schema").unwrap();
    let mut input = policy();
    input.manifest_bytes = 1;
    input.max_tables = 1;
    input.max_members = 1;
    let bytes = fs::read(dir.path().join("icebug.json")).unwrap();
    assert!(matches!(
        Manifest::parse(&bytes, input),
        Err(DataReadError::LimitExceeded(_))
    ));
    assert_eq!(
        Manifest::parse_generated(&bytes, input)
            .unwrap()
            .node_tables
            .len(),
        2
    );
    let graph = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(input).unwrap(),
    )
    .unwrap();
    assert_eq!(graph.schema().unwrap().node_tables().len(), 2);
    assert_eq!(graph.sample_graph(sample()).unwrap().edges().len(), 3);
    let mut malformed: Value = serde_json::from_slice(&bytes).unwrap();
    malformed["node_tables"][0]["shards"][0]["row_base"] = json!(100);
    assert!(matches!(
        Manifest::parse_generated(&serde_json::to_vec(&malformed).unwrap(), input),
        Err(DataReadError::InvalidManifest("CSR shard bounds"))
    ));
}
