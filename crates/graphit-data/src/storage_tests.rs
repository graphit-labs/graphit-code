use super::*;
use crate::tests::{capture, context, fixture, policy, save};
use arrow_array_parquet::{ArrayRef, BooleanArray, RecordBatch, StringArray, UInt64Array};
use arrow_schema_parquet::{DataType, Field, Schema};
use graphit_core::file_lock::Cancellation;
use std::{
    fs,
    io::{self, Read},
    time::{Duration, Instant},
};

const FIXTURE_SCHEMA: &str = "CREATE NODE TABLE File(path STRING, name STRING, line_number INT64, ignored BOOL, PRIMARY KEY(path));\nCREATE NODE TABLE Function(uid STRING, name STRING, PRIMARY KEY(uid));\nCREATE REL TABLE CALLS(FROM Function TO Function, uid STRING);\nCREATE REL TABLE CONTAINS(FROM File TO Function, uid STRING);\n";

pub(crate) fn fanout_fixture() -> tempfile::TempDir {
    let dir = fixture();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    let nodes = "shards/ab/cd/nodes";
    let edges = "shards/ef/edges";
    let schema = "shards/01/23/schema";
    for directory in [nodes, edges, schema] {
        fs::create_dir_all(dir.path().join(directory)).unwrap();
    }
    for node in manifest["node_tables"].as_array_mut().unwrap() {
        let old = node["shards"][0]["file"].as_str().unwrap();
        let new = format!("{nodes}/{old}.data");
        fs::rename(dir.path().join(old), dir.path().join(&new)).unwrap();
        node["shards"][0]["file"] = new.into();
    }
    for group in manifest["relationships"].as_array_mut().unwrap() {
        for member in group["members"].as_array_mut().unwrap() {
            for key in ["indices", "indptr"] {
                let old = member[key].as_str().unwrap();
                let new = format!("{edges}/{old}.data");
                fs::rename(dir.path().join(old), dir.path().join(&new)).unwrap();
                member[key] = new.into();
            }
        }
    }
    let old = manifest["schema_file"].as_str().unwrap();
    fs::write(dir.path().join(old), FIXTURE_SCHEMA).unwrap();
    let new = format!("{schema}/schema.cypher");
    fs::rename(dir.path().join(old), dir.path().join(&new)).unwrap();
    manifest["schema_file"] = new.into();
    save(dir.path(), &manifest);
    dir
}

fn grouped_file(path: &Path, schema: Arc<Schema>, groups: Vec<Vec<ArrayRef>>) {
    use ::parquet::{
        arrow::ArrowWriter,
        basic::Compression,
        file::{metadata::KeyValue, properties::WriterProperties},
    };
    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .set_dictionary_enabled(false)
        .set_key_value_metadata(Some(vec![KeyValue::new(
            "icebug_disk_version".into(),
            Some("v1".into()),
        )]))
        .build();
    let mut writer = ArrowWriter::try_new(
        fs::File::create(path).unwrap(),
        schema.clone(),
        Some(properties),
    )
    .unwrap();
    for columns in groups {
        writer
            .write(&RecordBatch::try_new(schema.clone(), columns).unwrap())
            .unwrap();
        writer.flush().unwrap();
    }
    writer.close().unwrap();
}

pub(crate) fn physical_slice_fixture() -> tempfile::TempDir {
    let dir = crate::tests::shard_fixture();
    fs::create_dir_all(dir.path().join("shards/ab/slices")).unwrap();
    let node_file = "shards/ab/slices/Function.parquet";
    grouped_file(
        &dir.path().join(node_file),
        Arc::new(Schema::new(vec![
            Field::new("uid", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("added", DataType::Boolean, true),
        ])),
        vec![
            vec![
                Arc::new(StringArray::from(vec!["f0", "f1", "f2"])),
                Arc::new(StringArray::from(vec!["alpha", "beta", "gamma"])),
                Arc::new(BooleanArray::from(vec![None, None, None])),
            ],
            vec![
                Arc::new(StringArray::from(vec!["f3"])),
                Arc::new(StringArray::from(vec!["delta"])),
                Arc::new(BooleanArray::from(vec![Some(true)])),
            ],
        ],
    );
    let indices_file = "shards/ab/slices/contains.indices";
    grouped_file(
        &dir.path().join(indices_file),
        Arc::new(Schema::new(vec![
            Field::new("target", DataType::UInt64, false),
            Field::new("uid", DataType::Utf8, false),
        ])),
        vec![
            vec![
                Arc::new(UInt64Array::from(vec![0, 0])),
                Arc::new(StringArray::from(vec!["edge0", "edge1"])),
            ],
            vec![
                Arc::new(UInt64Array::from(vec![2, 1])),
                Arc::new(StringArray::from(vec!["edge2", "edge3"])),
            ],
        ],
    );
    let indptr_file = "shards/ab/slices/shared.indptr";
    grouped_file(
        &dir.path().join(indptr_file),
        Arc::new(Schema::new(vec![Field::new(
            "ptr",
            DataType::UInt64,
            false,
        )])),
        vec![
            vec![Arc::new(UInt64Array::from(vec![0, 1, 1, 2]))],
            vec![Arc::new(UInt64Array::from(vec![0, 2, 2]))],
            vec![Arc::new(UInt64Array::from(vec![0, 2, 2]))],
            vec![Arc::new(UInt64Array::from(vec![0, 0, 0]))],
        ],
    );
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    let schema = FIXTURE_SCHEMA.replace(
        "name STRING, PRIMARY KEY(uid)",
        "name STRING, added BOOL, PRIMARY KEY(uid)",
    );
    fs::write(
        dir.path().join(manifest["schema_file"].as_str().unwrap()),
        schema,
    )
    .unwrap();
    let shards = manifest["node_tables"][0]["shards"].as_array_mut().unwrap();
    shards[0]["file"] = node_file.into();
    shards[0]["row_offset"] = 0.into();
    shards[1]["file"] = node_file.into();
    shards[1]["row_offset"] = 3.into();
    shards.push(serde_json::json!({
        "file":node_file, "row_base":400, "row_offset":4, "rows":0
    }));
    manifest["relationships"][0]["members"][0]["indptr"] = indptr_file.into();
    let mut member = manifest["relationships"][1]["members"][0].clone();
    member["indices"] = indices_file.into();
    member["indptr"] = indptr_file.into();
    member["rows"] = 2.into();
    member["indices_row_offset"] = 0.into();
    member["indptr_row_offset"] = 4.into();
    let mut second = member.clone();
    second["table"] = "a-second".into();
    second["indices_row_offset"] = 2.into();
    second["indptr_row_offset"] = 7.into();
    let mut empty = member.clone();
    empty["table"] = "a-empty".into();
    empty["rows"] = 0.into();
    empty["indices_row_offset"] = 4.into();
    empty["indptr_row_offset"] = 10.into();
    manifest["relationships"][1]["members"] = serde_json::json!([member, second, empty]);
    save(dir.path(), &manifest);
    dir
}

#[test]
fn physical_slices_preserve_sparse_ids_nulls_unicode_and_member_local_csr() {
    let dir = physical_slice_fixture();
    let graph = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(policy()).unwrap(),
    )
    .unwrap();
    let output = graph
        .sample_graph(GraphSampleLimits::new(6, 6, 18, 1024, 10000, 10000, 2).unwrap())
        .unwrap();
    let mut rows = output
        .nodes()
        .iter()
        .map(|node| node.id().row())
        .collect::<Vec<_>>();
    rows.sort_unstable();
    assert_eq!(rows, [10, 11, 100, 101, 102, 300]);
    let next = output
        .nodes()
        .iter()
        .find(|node| node.label() == "Function" && node.id().row() == 300)
        .unwrap();
    assert_eq!(next.uid(), Some("f3"));
    assert_eq!(next.name(), Some("delta"));
    assert_eq!(output.edges().len(), 6);
    assert!(
        output
            .nodes()
            .iter()
            .any(|node| node.name() == Some("ação 🚀"))
    );
    let page = graph
        .neighborhood_graph(
            GraphNeighborhoodRequest {
                anchor_label: "File",
                anchor_key: GraphAnchorKey::String("a.rs"),
                relationship: Some("CONTAINS"),
                other_label: Some("Function"),
                direction: Some(GraphDirection::Outgoing),
                limit: 10,
                cursor: None,
            },
            GraphNeighborhoodLimits::new(1024, 16, 10000, 100000, 10000, 2).unwrap(),
        )
        .unwrap();
    assert_eq!(page.anchor().row(), 10);
    assert_eq!(page.edges().len(), 4);
    assert!(page.edges().iter().all(|edge| edge.from().row() == 10));
    for node in page
        .nodes()
        .iter()
        .filter(|node| node.label() == "Function")
    {
        assert!(node.properties().iter().any(|property| {
            property.name() == "added" && property.value() == GraphPropertyValueRef::Null
        }));
    }
}

pub(crate) const INVALID_PHYSICAL_SLICES: [&str; 6] = [
    "node beyond end",
    "empty node beyond end",
    "partial node groups",
    "partial indices groups",
    "partial pointer groups",
    "multiple pointer groups",
];

pub(crate) fn invalid_physical_slice_fixture(case: &str) -> tempfile::TempDir {
    let dir = physical_slice_fixture();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    match case {
        "node beyond end" => {
            manifest["node_tables"][0]["shards"][0]["row_offset"] = 5.into();
        }
        "empty node beyond end" => {
            manifest["node_tables"][0]["shards"][2]["row_offset"] = 5.into();
        }
        "partial node groups" => {
            manifest["node_tables"][0]["shards"]
                .as_array_mut()
                .unwrap()
                .truncate(1);
            manifest["node_tables"][0]["shards"][0]["row_offset"] = 1.into();
            manifest["node_tables"][0]["rows"] = 3.into();
        }
        "partial indices groups" | "partial pointer groups" => {
            manifest["relationships"][1]["members"]
                .as_array_mut()
                .unwrap()
                .truncate(1);
            let field = if case == "partial indices groups" {
                "indices_row_offset"
            } else {
                "indptr_row_offset"
            };
            manifest["relationships"][1]["members"][0][field] = if case == "partial indices groups"
            {
                1
            } else {
                5
            }
            .into();
            manifest["n_edges"] = 4.into();
        }
        "multiple pointer groups" => {
            let path = manifest["relationships"][0]["members"][0]["indptr"]
                .as_str()
                .unwrap();
            grouped_file(
                &dir.path().join(path),
                Arc::new(Schema::new(vec![Field::new(
                    "ptr",
                    DataType::UInt64,
                    false,
                )])),
                vec![
                    vec![Arc::new(UInt64Array::from(vec![0, 1, 1, 2]))],
                    vec![Arc::new(UInt64Array::from(vec![0]))],
                    vec![Arc::new(UInt64Array::from(vec![2, 2]))],
                    vec![Arc::new(UInt64Array::from(vec![0, 2, 2]))],
                    vec![Arc::new(UInt64Array::from(vec![0, 0, 0]))],
                ],
            );
        }
        _ => unreachable!(),
    }
    // These declarations are structurally valid. Only the actual footer can
    // distinguish a complete row-group slice from an arbitrary row range.
    Manifest::parse(&serde_json::to_vec(&manifest).unwrap(), policy()).unwrap();
    save(dir.path(), &manifest);
    dir
}

#[test]
fn physical_slice_footers_reject_bounds_partial_groups_and_multiple_pointer_groups() {
    for case in INVALID_PHYSICAL_SLICES {
        let dir = invalid_physical_slice_fixture(case);
        assert!(
            matches!(
                CapturedLocalGraph::capture_generated_metadata(
                    dir.path(),
                    context(),
                    LocalGraphPolicy::new(policy()).unwrap(),
                ),
                Err(DataReadError::InvalidParquet(_))
            ),
            "accepted or misclassified {case}"
        );
    }
}

#[test]
fn borrowed_storage_names_open_the_actual_independently_named_files() {
    let dir = fanout_fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let schema = graph.schema().unwrap();
    let nodes = schema.node_tables().collect::<Vec<_>>();
    let members = schema.relationships().collect::<Vec<_>>();
    let names = nodes
        .iter()
        .flat_map(|node| node.shards().map(|shard| shard.storage_file()))
        .chain(
            members
                .iter()
                .flat_map(|member| [member.indices_file(), member.indptr_file()]),
        )
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 6);
    for relative in names {
        assert!(relative.starts_with("shards/"));
        let expected = fs::read(dir.path().join(relative)).unwrap();
        let file = graph.open_file(Path::new(relative)).unwrap();
        assert_eq!(file.len(), expected.len() as u64);
        assert_eq!(file.read_range(0, 4).unwrap().as_ref(), b"PAR1");
        let mut stream = file.stream(4).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, expected[4..]);
    }
}

#[test]
fn generated_capture_reads_fanout_declarations_and_excludes_unrelated_files() {
    let dir = fanout_fixture();
    fs::write(
        dir.path().join("shards/unpublished.parquet"),
        b"unpublished",
    )
    .unwrap();
    let graph = CapturedLocalGraph::capture_generated_metadata(
        dir.path(),
        context(),
        LocalGraphPolicy::new(policy()).unwrap(),
    )
    .unwrap();
    let schema = graph.schema().unwrap();
    assert!(schema.schema_file().starts_with("shards/01/23/"));
    assert_eq!(schema.referenced_files().count(), 8);
    let sample = graph.sample_graph(crate::tests::sample()).unwrap();
    assert_eq!(sample.nodes().len(), 4);
    assert_eq!(sample.edges().len(), 3);
    assert!(
        graph
            .open_file(Path::new("shards/unpublished.parquet"))
            .is_err()
    );
}

#[test]
fn manifest_storage_roles_reject_escapes_and_ambiguous_paths() {
    let dir = fixture();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    for role in ["schema", "node", "indices", "indptr"] {
        for path in [
            "",
            "/shards/ab/file",
            "../file",
            "shards/ab/../file",
            "shards/./file",
            "shards//file",
            "shards/ab/file/",
            "shards\\ab\\file",
            "shards/ab/\0file",
            "shards/ab/file:stream",
            "C:/shards/file",
        ] {
            let mut value = manifest.clone();
            match role {
                "schema" => value["schema_file"] = path.into(),
                "node" => value["node_tables"][0]["shards"][0]["file"] = path.into(),
                member => value["relationships"][0]["members"][0][member] = path.into(),
            }
            let bytes = serde_json::to_vec(&value).unwrap();
            for result in [
                Manifest::parse(&bytes, policy()),
                Manifest::parse_generated(&bytes, policy()),
            ] {
                assert!(
                    matches!(result, Err(DataReadError::InvalidManifest(_))),
                    "accepted {role} path {path:?}"
                );
            }
        }
    }
}

fn sliced_manifest_declarations() -> serde_json::Value {
    let dir = crate::tests::shard_fixture();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    let nodes = value["node_tables"][0]["shards"].as_array_mut().unwrap();
    nodes[0]["file"] = "shards/ab/nodes.parquet".into();
    nodes[0]["row_offset"] = 0.into();
    nodes[1]["file"] = "shards/ab/nodes.parquet".into();
    nodes[1]["row_offset"] = 3.into();
    nodes.push(serde_json::json!({
        "file":"shards/ab/nodes.parquet", "row_offset":4, "row_base":400, "rows":0
    }));
    let mut next = value["relationships"][0]["members"][0].clone();
    next["table"] = "next-calls".into();
    next["indices_row_offset"] = 2.into();
    next["indptr_row_offset"] = 4.into();
    value["relationships"][0]["members"]
        .as_array_mut()
        .unwrap()
        .push(next);
    value["relationships"][1]["members"][0]["indptr"] = "z.indptr".into();
    value["relationships"][1]["members"][0]["indptr_row_offset"] = 8.into();
    value["n_edges"] = 8.into();
    value
}

#[test]
fn shared_physical_slices_preserve_logical_addresses_and_member_local_offsets() {
    let value = sliced_manifest_declarations();
    let bytes = serde_json::to_vec(&value).unwrap();
    for manifest in [
        Manifest::parse(&bytes, policy()).unwrap(),
        Manifest::parse_generated(&bytes, policy()).unwrap(),
    ] {
        let nodes = manifest
            .node_tables
            .iter()
            .find(|node| node.label == "Function")
            .unwrap();
        assert_eq!(nodes.rows, 4);
        assert_eq!(nodes.row_extent(), 400);
        assert_eq!(
            nodes
                .shards
                .iter()
                .map(|shard| (shard.row_base, shard.row_offset, shard.rows))
                .collect::<Vec<_>>(),
            [(100, 0, 3), (300, 3, 1), (400, 4, 0)]
        );
        let calls = &manifest.relationships[0].members;
        assert_eq!(calls[0].source_row_base, 100);
        assert_eq!(calls[0].indices_row_offset, 0);
        assert_eq!(calls[1].indices_row_offset, 2);
        assert_eq!(calls[1].indptr_row_offset, 4);
        assert_eq!(manifest.relationships[1].members[0].indptr_row_offset, 8);
    }
}

#[test]
fn shared_storage_rejects_overlaps_overflow_and_conflicting_roles() {
    let manifest = sliced_manifest_declarations();
    for case in [
        "node overlap",
        "node overflow",
        "indices overlap",
        "indices overflow",
        "indptr overlap",
        "indptr overflow",
        "node label",
        "indices label",
        "indices role",
        "indptr role",
        "schema role",
        "manifest role",
    ] {
        let mut value = manifest.clone();
        let expected = match case {
            "node overlap" => {
                value["node_tables"][0]["shards"][1]["row_offset"] = 2.into();
                "overlapping physical slices"
            }
            "node overflow" => {
                value["node_tables"][0]["shards"][0]["row_offset"] = u64::MAX.into();
                "physical row range"
            }
            "indices overlap" => {
                value["relationships"][0]["members"][1]["indices_row_offset"] = 1.into();
                "overlapping physical slices"
            }
            "indices overflow" => {
                value["relationships"][0]["members"][1]["indices_row_offset"] = u64::MAX.into();
                "physical row range"
            }
            "indptr overlap" => {
                value["relationships"][0]["members"][1]["indptr_row_offset"] = 3.into();
                "overlapping physical slices"
            }
            "indptr overflow" => {
                value["relationships"][0]["members"][1]["indptr_row_offset"] = u64::MAX.into();
                "physical row range"
            }
            "node label" => {
                value["node_tables"][1]["shards"][0]["file"] = "shards/ab/nodes.parquet".into();
                value["node_tables"][1]["shards"][0]["row_offset"] = 20.into();
                "conflicting storage roles"
            }
            "indices label" => {
                value["relationships"][1]["members"][0]["indices"] = "z.indices".into();
                value["relationships"][1]["members"][0]["indices_row_offset"] = 20.into();
                "conflicting storage roles"
            }
            "indices role" => {
                value["relationships"][0]["members"][0]["indices"] =
                    "shards/ab/nodes.parquet".into();
                "conflicting storage roles"
            }
            "indptr role" => {
                value["relationships"][0]["members"][0]["indptr"] = "z.indices".into();
                "conflicting storage roles"
            }
            "schema role" => {
                value["schema_file"] = "shards/ab/nodes.parquet".into();
                "conflicting storage roles"
            }
            "manifest role" => {
                value["node_tables"][0]["shards"][0]["file"] = "icebug.json".into();
                "conflicting storage roles"
            }
            _ => unreachable!(),
        };
        let bytes = serde_json::to_vec(&value).unwrap();
        for result in [
            Manifest::parse(&bytes, policy()),
            Manifest::parse_generated(&bytes, policy()),
        ] {
            assert!(
                matches!(result, Err(DataReadError::InvalidManifest(message)) if message == expected),
                "accepted or misclassified {case}"
            );
        }
    }
}

#[test]
fn physical_slice_order_is_independent_of_logical_order_and_empty_slices_do_not_overlap() {
    let mut value = sliced_manifest_declarations();
    value["node_tables"][0]["shards"][0]["row_offset"] = 10.into();
    value["node_tables"][0]["shards"][1]["row_offset"] = 2.into();
    // The empty logical shard occupies no physical rows, even within another slice.
    value["node_tables"][0]["shards"][2]["row_offset"] = 11.into();
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(Manifest::parse(&bytes, policy()).is_ok());
    assert!(Manifest::parse_generated(&bytes, policy()).is_ok());
    // A later declaration can overlap an interval that starts after its own offset.
    value["node_tables"][0]["shards"][1]["row_offset"] = 9.into();
    value["node_tables"][0]["shards"][1]["rows"] = 2.into();
    value["node_tables"][0]["rows"] = 5.into();
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(matches!(
        Manifest::parse(&bytes, policy()),
        Err(DataReadError::InvalidManifest(
            "overlapping physical slices"
        ))
    ));
}

#[cfg(unix)]
#[test]
fn generated_fanout_capture_rejects_linked_files_and_ancestors() {
    for ancestor in [false, true] {
        let dir = fanout_fixture();
        let graph = CapturedLocalGraph::capture_generated_metadata(
            dir.path(),
            context(),
            LocalGraphPolicy::new(policy()).unwrap(),
        )
        .unwrap();
        let name = graph
            .schema()
            .unwrap()
            .node_tables()
            .next()
            .unwrap()
            .shards()
            .next()
            .unwrap()
            .storage_file()
            .to_owned();
        let captured = graph.open_file(Path::new(&name)).unwrap();
        let target = if ancestor {
            dir.path().join("shards/ab")
        } else {
            dir.path().join(&name)
        };
        let outside = tempfile::tempdir().unwrap();
        let moved = outside.path().join("moved");
        fs::rename(&target, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &target).unwrap();
        assert!(captured.read_range(0, 4).is_err());
        assert!(graph.open_file(Path::new(&name)).is_err());
        assert!(matches!(
            CapturedLocalGraph::capture_generated_metadata(
                dir.path(),
                context(),
                LocalGraphPolicy::new(policy()).unwrap(),
            ),
            Err(DataReadError::Source(_))
        ));
    }
}

#[test]
fn invalid_or_uncaptured_paths_do_not_open_another_file() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    for relative in ["../File.parquet", "/File.parquet", "missing.parquet", ""] {
        assert!(graph.open_file(Path::new(relative)).is_err(), "{relative}");
    }
    fs::write(dir.path().join("added.parquet"), b"PAR1").unwrap();
    assert!(graph.open_file(Path::new("added.parquet")).is_err());
}

#[test]
fn owned_file_keeps_original_cancellation_and_deadline_after_graph_drop() {
    for expired in [false, true] {
        let dir = fixture();
        let cancel = Cancellation::default();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(if expired { 1 } else { 60 }),
            cancel.clone(),
        ));
        let graph = capture(dir.path(), Arc::clone(&original), policy()).unwrap();
        let file = graph.open_file(Path::new("File.parquet")).unwrap();
        drop(graph);
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
        assert_eq!(file.read_range(0, 4).err().unwrap().kind(), expected);
        assert_eq!(file.stream(0).err().unwrap().kind(), expected);
    }
}

#[test]
fn changed_file_and_cancelled_open_preserve_causal_errors() {
    let dir = fixture();
    let cancel = Cancellation::default();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        cancel.clone(),
    ));
    let graph = capture(dir.path(), Arc::clone(&original), policy()).unwrap();
    let file = graph.open_file(Path::new("File.parquet")).unwrap();
    fs::write(dir.path().join("File.parquet"), b"changed").unwrap();
    assert!(file.read_range(0, 4).is_err());
    assert!(graph.open_file(Path::new("File.parquet")).is_err());
    cancel.cancel();
    assert!(matches!(graph.open_file(Path::new("Function.parquet")),
        Err(DataReadError::Source(ref error)) if error.kind() == io::ErrorKind::Interrupted));
}
