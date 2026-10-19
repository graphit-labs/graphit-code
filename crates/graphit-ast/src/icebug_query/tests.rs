use super::*;

fn options() -> QueryOptions {
    QueryOptions {
        buffer_pool_bytes: 128 * 1024 * 1024,
        query_threads: 2,
        max_rows: 1024,
        max_bytes: 1024 * 1024,
        max_depth: 32,
    }
}

#[test]
fn query_options_accept_native_auto_and_preserve_explicit_bounds() {
    let mut selected = options();
    for bytes in [0, 4096, 128 * 1024 * 1024] {
        selected.buffer_pool_bytes = bytes;
        selected.check().unwrap();
        assert_eq!(selected.buffer_pool_bytes, bytes);
    }
    for bytes in [1, 4095] {
        selected.buffer_pool_bytes = bytes;
        assert_eq!(
            selected.check().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    selected.buffer_pool_bytes = 0;
    for invalid in [
        QueryOptions {
            query_threads: 0,
            ..selected
        },
        QueryOptions {
            max_rows: 0,
            ..selected
        },
        QueryOptions {
            max_bytes: 0,
            ..selected
        },
        QueryOptions {
            max_depth: 0,
            ..selected
        },
    ] {
        assert_eq!(
            invalid.check().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[test]
fn typed_cells_preserve_precision_null_unicode_and_exact_encoded_bounds() {
    let mut sink = ResultSink::new(options()).unwrap();
    sink.columns(vec!["精度".into(), "nested".into(), "node".into()])
        .unwrap();
    sink.begin_row().unwrap();
    sink.atom(
        "INT128",
        "",
        ffi::AtomKind::Text,
        "170141183460469231731687303715884105727",
    )
    .unwrap();
    sink.begin_cell("STRUCT(text STRING, values INT64[])", "", true)
        .unwrap();
    sink.atom("STRING", "text", ffi::AtomKind::Text, "α\0\"\n")
        .unwrap();
    sink.begin_cell("INT64[]", "values", false).unwrap();
    sink.atom("INT64", "", ffi::AtomKind::Text, "9007199254740993")
        .unwrap();
    sink.atom("INT64", "", ffi::AtomKind::Null, "").unwrap();
    sink.end_cell().unwrap();
    sink.end_cell().unwrap();
    sink.identity("", &[7; 32], 3, 9007199254740993, false)
        .unwrap();
    sink.end_row().unwrap();
    let encoded = serde_json::to_vec(&sink.result).unwrap();
    assert_eq!(sink.encoded_bytes, encoded.len());
    assert_eq!(sink.result.rows[0][2]["value"]["table"], 3);
    assert!(sink.result.rows[0][2]["value"].get("key").is_none());
    assert_eq!(sink.result.rows[0][2]["value"]["row"], "9007199254740993");
    assert_eq!(sink.result.rows[0][1]["value"]["text"]["value"], "α\0\"\n");
    let mut bounded = options();
    bounded.max_bytes = encoded.len() - 1;
    let mut denied = ResultSink::new(bounded).unwrap();
    denied.columns(sink.result.columns.clone()).unwrap();
    denied.begin_row().unwrap();
    assert!(
        denied
            .atom(
                "STRING",
                "",
                ffi::AtomKind::Text,
                &"x".repeat(encoded.len())
            )
            .is_err()
    );
}

#[test]
fn sink_rejects_invalid_native_shapes_without_partial_success() {
    let mut sink = ResultSink::new(options()).unwrap();
    sink.columns(vec!["value".into()]).unwrap();
    sink.begin_row().unwrap();
    assert!(
        sink.atom("BOOL", "", ffi::AtomKind::Boolean, "maybe")
            .is_err()
    );
    assert!(
        sink.atom("DOUBLE", "", ffi::AtomKind::Number, "NaN")
            .is_err()
    );
    sink.begin_cell("STRUCT(a INT64)", "", true).unwrap();
    sink.atom("INT64", "a", ffi::AtomKind::Text, "1").unwrap();
    assert!(sink.atom("INT64", "a", ffi::AtomKind::Text, "2").is_err());
    assert!(sink.end_row().is_err());
    sink.end_cell().unwrap();
    sink.end_row().unwrap();
    assert_eq!(sink.result.rows[0][0]["value"]["a"]["value"], "1");
}

use graphit_core::{
    artifact_store::{
        CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy,
        SourceReadLimits,
    },
    brand::Brand,
    config::ConfigSnapshot,
};
use graphit_data::LocalGraphPolicy;
use std::{
    fs,
    time::{Duration, Instant},
};

fn original(duration: Duration) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + duration,
        Cancellation::default(),
    ))
}
enum Column<'a> {
    Ints(&'a [i64]),
    Strings(&'a [&'a str]),
    OptionalInts(&'a [Option<i64>]),
}
fn parquet(path: &Path, schema: &str, values: &[Column<'_>]) {
    use parquet_graph_fixture::{
        basic::Compression,
        data_type::{ByteArray, ByteArrayType, Int64Type},
        file::{
            metadata::KeyValue,
            properties::{WriterProperties, WriterVersion},
            writer::SerializedFileWriter,
        },
        schema::parser::parse_message_type,
    };
    let properties = Arc::new(
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
    let mut writer = SerializedFileWriter::new(
        fs::File::create(path).unwrap(),
        Arc::new(parse_message_type(schema).unwrap()),
        properties,
    )
    .unwrap();
    if values.iter().all(|column| match column {
        Column::Ints(values) => values.is_empty(),
        Column::Strings(values) => values.is_empty(),
        Column::OptionalInts(values) => values.is_empty(),
    }) {
        writer.close().unwrap();
        return;
    }
    let mut group = writer.next_row_group().unwrap();
    for value in values {
        let mut column = group.next_column().unwrap().unwrap();
        match value {
            Column::Ints(values) => {
                column
                    .typed::<Int64Type>()
                    .write_batch(values, None, None)
                    .unwrap();
            }
            Column::Strings(values) => {
                let bytes = values
                    .iter()
                    .map(|value| ByteArray::from(*value))
                    .collect::<Vec<_>>();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&bytes, None, None)
                    .unwrap();
            }
            Column::OptionalInts(values) => {
                let bytes = values.iter().flatten().copied().collect::<Vec<_>>();
                let definitions = values
                    .iter()
                    .map(|value| i16::from(value.is_some()))
                    .collect::<Vec<_>>();
                column
                    .typed::<Int64Type>()
                    .write_batch(&bytes, Some(&definitions), None)
                    .unwrap();
            }
        }
        column.close().unwrap();
    }
    group.close().unwrap();
    writer.close().unwrap();
}
fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    parquet(
        &temp.path().join("A.parquet"),
        "message graph { required binary uid (UTF8); required binary name (UTF8); }",
        &[
            Column::Strings(&["a0", "a1", "a2"]),
            Column::Strings(&["a0", "a1", "a2"]),
        ],
    );
    parquet(
        &temp.path().join("B.parquet"),
        "message graph { required binary uid (UTF8); }",
        &[Column::Strings(&["b0", "b1"])],
    );
    parquet(
        &temp.path().join("p1.parquet"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); optional int64 weight; }",
        &[
            Column::Ints(&[1, 1, 1]),
            Column::Strings(&["r0", "r1", "r2"]),
            Column::OptionalInts(&[Some(9007199254740993), None, Some(13)]),
        ],
    );
    parquet(
        &temp.path().join("q1.parquet"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Column::Ints(&[0, 2, 3, 3])],
    );
    parquet(
        &temp.path().join("p2.parquet"),
        "message graph { required int64 target (UINT_64); required binary weight (UTF8); required binary uid (UTF8); }",
        &[
            Column::Ints(&[2, 1]),
            Column::Strings(&["twenty-two", "twenty-three"]),
            Column::Strings(&["r3", "r4"]),
        ],
    );
    parquet(
        &temp.path().join("q2.parquet"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Column::Ints(&[0, 1, 1, 2])],
    );
    parquet(
        &temp.path().join("pb.parquet"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); }",
        &[Column::Ints(&[1, 0]), Column::Strings(&["b0", "b1"])],
    );
    parquet(
        &temp.path().join("qb.parquet"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Column::Ints(&[0, 1, 2, 2])],
    );
    fs::write(
        temp.path().join("schema.cypher"),
        "CREATE NODE TABLE Untrusted(key STRING,PRIMARY KEY(key));",
    )
    .unwrap();
    let manifest = json!({"manifest_version":3,"format":"icebug-canonical","storage":"direct-parquet","finished":true,"add_reverse_edges":false,"schema_file":"schema.cypher",
        "node_tables":[{"label":"A","rows":3,"shards":[{"file":"A.parquet","row_base":0,"rows":3}],"primary_key":"uid","columns":[{"name":"uid","type":"STRING"},{"name":"name","type":"STRING"}],"lang_counts":[]},{"label":"B","rows":2,"shards":[{"file":"B.parquet","row_base":0,"rows":2}],"primary_key":"uid","columns":[{"name":"uid","type":"STRING"}],"lang_counts":[]}],
        "relationships":[{"type":"LINK","members":[{"from":"A","to":"A","table":"p1","indices":"p1.parquet","indptr":"q1.parquet","rows":3,"source_row_base":0,"source_rows":3,"target_row_base":0,"target_rows":3},{"from":"A","to":"A","table":"p2","indices":"p2.parquet","indptr":"q2.parquet","rows":2,"source_row_base":0,"source_rows":3,"target_row_base":0,"target_rows":3},{"from":"A","to":"B","table":"pb","indices":"pb.parquet","indptr":"qb.parquet","rows":2,"source_row_base":0,"source_rows":3,"target_row_base":0,"target_rows":2}]}],
        "relation_uids":true,"generation":"real-query","n_edges":7,"repaired_strings":0,"invariants":{"indptr_row_groups":1,"self_loops":"forward-once"}});
    fs::write(
        temp.path().join("icebug.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    temp
}
fn capture(path: &Path, ctx: &Arc<SnapshotContext>) -> Arc<CapturedLocalGraph> {
    let retained = RetainedBytesLimits::new(4096, 4, 4096).unwrap();
    let file = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(16 * 1024 * 1024, 64, 4096, 8).unwrap(),
        RetainedFileSet::new([], retained).unwrap(),
        retained,
    )
    .unwrap();
    let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
    Arc::new(
        CapturedLocalGraph::capture(
            path,
            &file,
            SourceReadLimits::new(1024 * 1024, 4096, 16 * 1024 * 1024).unwrap(),
            Arc::clone(ctx),
            LocalGraphPolicy::resolve(&config, ctx).unwrap(),
        )
        .unwrap(),
    )
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn native_multipart_skips_an_empty_member_without_losing_later_edges() {
    let temp = fixture();
    parquet(
        &temp.path().join("empty.parquet"),
        "message graph { required int64 target (UINT_64); required binary uid (UTF8); optional int64 weight; }",
        &[
            Column::Ints(&[]),
            Column::Strings(&[]),
            Column::OptionalInts(&[]),
        ],
    );
    parquet(
        &temp.path().join("empty_ptr.parquet"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Column::Ints(&[0, 0, 0, 0])],
    );
    let manifest_path = temp.path().join("icebug.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["relationships"][0]["members"].as_array_mut().unwrap().insert(
        1,
        json!({"from":"A","to":"A","table":"empty","indices":"empty.parquet","indptr":"empty_ptr.parquet","rows":0,"source_row_base":0,"source_rows":3,"target_row_base":0,"target_rows":3}),
    );
    fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let ctx = original(Duration::from_secs(30));
    let graph = capture(temp.path(), &ctx);
    let runtime = runtime();
    for query in [
        "MATCH (a)-[r:LINK]->(b) RETURN count(r)",
        "MATCH (a)<-[r:LINK]-(b) RETURN count(r)",
    ] {
        count(&graph, &ctx, runtime.handle(), query, "7");
    }
    let result = read(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A)-[r:LINK]->(b:A) RETURN r.uid,id(r) ORDER BY r.uid",
    );
    assert_eq!(result.rows.len(), 5);
    assert_eq!(result.rows[3][0]["value"], "r3");
    assert_eq!(result.rows[3][1]["value"]["member"], 2);
    assert_eq!(result.rows[3][1]["value"]["row"], "0");
}

#[test]
fn native_multipart_nodes_skip_empty_shards_and_preserve_later_relations() {
    let temp = fixture();
    parquet(
        &temp.path().join("empty_nodes.parquet"),
        "message graph { required binary uid (UTF8); required binary name (UTF8); }",
        &[Column::Strings(&[]), Column::Strings(&[])],
    );
    let path = temp.path().join("icebug.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["node_tables"][0]["shards"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            json!({"file":"empty_nodes.parquet","row_base":0,"rows":0}),
        );
    fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let ctx = original(Duration::from_secs(30));
    let graph = capture(temp.path(), &ctx);
    let runtime = runtime();
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH(a:A) RETURN count(a)",
        "3",
    );
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH(a)-[r:LINK]->(b) RETURN count(r)",
        "7",
    );
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH(a)<-[r:LINK]-(b) RETURN count(r)",
        "7",
    );
}

#[test]
fn native_node_queries_do_not_open_unrelated_shards() {
    let temp = fixture();
    let ctx = original(Duration::from_secs(30));
    let graph = capture(temp.path(), &ctx);
    let runtime = runtime();
    let run = |query: &str| {
        let (mut io, schema) = mount_schema(QueryGraph::Local(Arc::clone(&graph)), None).unwrap();
        // Keep B in the catalog, but make its physical open fail at the captured
        // filesystem boundary. A query must not open unrelated node readers.
        for relative in io.files.values_mut() {
            if relative == "B.parquet" {
                *relative = "not-captured.parquet".into();
            }
        }
        let signal = Box::new(QuerySignal {
            original: Arc::clone(&ctx),
            complete: Cancellation::default(),
            runtime: runtime.handle().clone(),
        });
        let mut result = ResultSink::new(options()).unwrap();
        ffi::run_query(io, signal, &schema, query, "{}", &options(), &mut result)
            .map(|()| result.result)
    };
    let selected = run("MATCH (a:A) RETURN count(a)").unwrap();
    assert_eq!(selected.rows[0][0]["value"], "3");
    assert!(run("MATCH (b:B) RETURN b.uid").is_err());
}

#[test]
fn native_multipart_rejects_an_out_of_range_csr_pointer() {
    let temp = fixture();
    parquet(
        &temp.path().join("q1.parquet"),
        "message graph { required int64 ptr (UINT_64); }",
        &[Column::Ints(&[0, 9, 3, 3])],
    );
    let ctx = original(Duration::from_secs(30));
    let graph = capture(temp.path(), &ctx);
    let runtime = runtime();
    // Unrelated node queries must not decode every relation's CSR. Integrity is
    // still checked when a traversal (or degree-only plan) uses that relation.
    let nodes = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A) RETURN count(a)",
        &BTreeMap::new(),
        options(),
    )
    .unwrap();
    assert_eq!(nodes.rows[0][0]["value"], "3");
    let error = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A)-[r:LINK]->(b:A) RETURN r.uid",
        &BTreeMap::new(),
        options(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("pointer exceeds"), "{error}");
}

#[test]
fn generated_icebug_preserves_tuple_keys_and_exact_raw_version_in_native_query() {
    use graphit_data::graph_writer::{GraphEdge, GraphNode, GraphValue, IcebugWriter};
    let ctx = original(Duration::from_secs(60));
    let make_node = |data_key: &str, name: Option<&str>| {
        let mut properties = BTreeMap::from([
            ("uid".into(), GraphValue::String("same genuine uid".into())),
            ("data_key".into(), GraphValue::String(data_key.into())),
            (
                "raw_version".into(),
                GraphValue::String(u64::MAX.to_string()),
            ),
        ]);
        if let Some(name) = name {
            properties.insert("name".into(), GraphValue::String(name.into()));
        }
        GraphNode {
            label: "Function".into(),
            key: serde_json::to_string(&["same genuine uid", data_key]).unwrap(),
            properties,
        }
    };
    let source = make_node("declaration", Some("ação 🚀"));
    let target = make_node("body", None);
    let source_key = source.key.clone();
    let target_key = target.key.clone();
    let edge = GraphEdge {
        kind: "CALLS".into(),
        source_key: source_key.clone(),
        target_key: target_key.clone(),
        uid: "call-1".into(),
    };
    let output = IcebugWriter::new(ctx.clone())
        .unwrap()
        .push_file([source, target], [edge])
        .unwrap()
        .finish()
        .unwrap();
    let graph = capture(&output.directory(), &ctx);
    let runtime = runtime();
    let result = read(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:Function)-[r:CALLS]->(b:Function) RETURN a.entity_key,b.entity_key,a.uid,a.raw_version,a.name,b.name,r.uid",
    );
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0]["value"], source_key);
    assert_eq!(result.rows[0][1]["value"], target_key);
    assert_eq!(result.rows[0][2]["value"], "same genuine uid");
    assert_eq!(result.rows[0][3]["value"], u64::MAX.to_string());
    assert_eq!(result.rows[0][4]["value"], "ação 🚀");
    assert!(result.rows[0][5]["value"].is_null());
    assert_eq!(result.rows[0][6]["value"], "call-1");
}
#[test]
fn native_external_catalogue_keeps_primary_key_lookup_without_empty_internal_indexes() {
    use graphit_data::graph_writer::{GraphNode, GraphValue, IcebugWriter};
    let ctx = original(Duration::from_secs(60));
    let nodes = (0..16).map(|index| {
        let mut properties: BTreeMap<_, _> = (0..40)
            .map(|column| {
                (
                    format!("field_{column}"),
                    GraphValue::String("value".into()),
                )
            })
            .collect();
        properties.insert("uid".into(), GraphValue::String(format!("uid-{index}")));
        GraphNode {
            label: format!("Wide{index}"),
            key: format!("key-{index}"),
            properties,
        }
    });
    let output = IcebugWriter::new(ctx.clone())
        .unwrap()
        .push_file(nodes, [])
        .unwrap()
        .finish()
        .unwrap();
    let graph = capture(&output.directory(), &ctx);
    let runtime = runtime();
    let mut bounded = options();
    bounded.buffer_pool_bytes = 8 * 1024 * 1024;
    let result = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (n:Wide15) WHERE n.entity_key = 'key-15' RETURN n.uid",
        &BTreeMap::new(),
        bounded,
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0]["value"], "uid-15");
    let result = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (n) RETURN count(n)",
        &BTreeMap::new(),
        bounded,
    )
    .unwrap();
    assert_eq!(result.rows[0][0]["value"], "16");
}

#[test]
fn native_scan_preserves_wide_properties_and_small_result_projection() {
    use graphit_data::graph_writer::{GraphNode, GraphValue, IcebugWriter};
    let ctx = original(Duration::from_secs(60));
    let payload = "x".repeat(512 * 1024);
    let nodes = (0..32).map(|index| GraphNode {
        label: "AstFileObservation".into(),
        key: format!("file-{index}"),
        properties: BTreeMap::from([
            (
                "path".into(),
                GraphValue::String(format!("src/file-{index}.rs")),
            ),
            ("selected".into(), GraphValue::Bool(index == 17)),
            (
                "metadata_json".into(),
                GraphValue::String(format!("{payload}{index}")),
            ),
        ]),
    });
    let output = IcebugWriter::new(ctx.clone())
        .unwrap()
        .push_file(nodes, [])
        .unwrap()
        .finish()
        .unwrap();
    let graph = capture(&output.directory(), &ctx);
    let runtime = runtime();
    // Upstream owns buffer allocation and projection strategy. Test the product
    // result, not the retired patched scanner's ability to fit an 8MiB pool.
    let bounded = options();
    let result = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (f:AstFileObservation) WHERE f.selected = true RETURN f.path LIMIT 1",
        &BTreeMap::new(),
        bounded,
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0]["value"], "src/file-17.rs");
    let result = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (f:AstFileObservation) WHERE f.selected = true RETURN id(f), f.path UNION ALL MATCH (f:AstFileObservation) WHERE f.selected = true RETURN id(f), f.path",
        &BTreeMap::new(),
        bounded,
    )
    .unwrap();
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0], result.rows[1]);
    assert_eq!(result.rows[0][1]["value"], "src/file-17.rs");
    let result = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (f:AstFileObservation) WHERE f.path = 'src/file-0.rs' RETURN id(f)",
        &BTreeMap::new(),
        bounded,
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0]["value"]["table"], 0);
    assert_eq!(result.rows[0][0]["value"]["row"], "0");
    let wide = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (f:AstFileObservation) WHERE f.selected = true RETURN f.metadata_json LIMIT 1",
        &BTreeMap::new(),
        bounded,
    )
    .unwrap();
    assert_eq!(wide.rows.len(), 1);
    assert_eq!(wide.rows[0][0]["value"], format!("{payload}17"));
}

#[test]
fn native_observation_reads_column_chunks_beyond_retained_ranges() {
    use graphit_data::graph_writer::{GraphNode, GraphValue, IcebugWriter};
    use parquet_graph_fixture::file::reader::{FileReader, SerializedFileReader};
    use sha2::{Digest, Sha256};

    let ctx = original(Duration::from_secs(60));
    let observations = (0..96)
        .map(|index| {
            let text = (0..(32 * 1024 / 64))
                .map(|part| {
                    Sha256::digest(format!("{index}:{part}"))
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                })
                .collect::<String>();
            format!("[{{\"target\":\"{text}\"}}]")
        })
        .collect::<Vec<_>>();
    let nodes = observations
        .iter()
        .enumerate()
        .map(|(index, outcomes)| GraphNode {
            label: "AstFileObservation".into(),
            key: format!("file-{index}"),
            properties: BTreeMap::from([
                (
                    "path".into(),
                    GraphValue::String(format!("src/file-{index}.rs")),
                ),
                (
                    "resolution_outcomes_json".into(),
                    GraphValue::String(outcomes.clone()),
                ),
            ]),
        });
    let output = IcebugWriter::new(Arc::clone(&ctx))
        .unwrap()
        .push_file(nodes, [])
        .unwrap()
        .finish()
        .unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(output.directory().join("icebug.json")).unwrap()).unwrap();
    let relative = manifest["node_tables"][0]["shards"][0]["file"]
        .as_str()
        .unwrap();
    let parquet =
        SerializedFileReader::new(fs::File::open(output.directory().join(relative)).unwrap())
            .unwrap();
    let column = parquet
        .metadata()
        .row_group(0)
        .columns()
        .iter()
        .find(|column| column.column_descr().name() == "resolution_outcomes_json")
        .unwrap();
    let (offset, length) = column.byte_range();
    assert!(length > 1024 * 1024);
    let graph = capture(&output.directory(), &ctx);
    let file = graph.open_file(Path::new(relative)).unwrap();
    assert_eq!(
        file.read_range(offset, usize::try_from(length).unwrap())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge,
    );
    let runtime = runtime();
    let parameters = BTreeMap::from([("path".into(), json!("src/file-1.rs"))]);
    let sized = execute(
        &graph, &ctx, runtime.handle(),
        "MATCH (f:AstFileObservation) WHERE f.path = $path RETURN size(f.resolution_outcomes_json) LIMIT 2",
        &parameters, options(),
    ).unwrap();
    assert_eq!(sized.rows[0][0]["value"], observations[1].len().to_string());
    let selected = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (f:AstFileObservation) WHERE f.path = $path RETURN f LIMIT 2",
        &parameters,
        options(),
    )
    .unwrap();
    assert_eq!(selected.rows.len(), 1);
    assert_eq!(
        selected.rows[0][0]["value"]["properties"]["value"]["resolution_outcomes_json"]["value"],
        observations[1]
    );

    let file = QueryFile::Local(file);
    assert_eq!(
        file.read_at(file.len() - 1, &mut [0; 2])
            .unwrap_err()
            .kind(),
        io::ErrorKind::UnexpectedEof
    );
    ctx.cancellation().cancel();
    assert_eq!(
        file.read_at(0, &mut [0; 1]).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn native_observation_filters_wide_rows_across_byte_sized_groups() {
    use graphit_data::graph_writer::{GraphEdge, GraphNode, GraphValue, IcebugWriter};
    use parquet_graph_fixture::file::reader::{FileReader, SerializedFileReader};

    let ctx = original(Duration::from_secs(60));
    let selected = "[{\"target\":\"ação 🚀\"}]";
    let output = IcebugWriter::new(Arc::clone(&ctx))
        .unwrap()
        .push_file(
            (0..160).map(|index| GraphNode {
                label: "AstFileObservation".into(),
                key: format!("file-{index}"),
                properties: BTreeMap::from([
                    (
                        "path".into(),
                        GraphValue::String(format!("src/file-{index}.rs")),
                    ),
                    (
                        "resolution_outcomes_json".into(),
                        GraphValue::String(if index == 17 {
                            selected.into()
                        } else {
                            format!("[{{\"target\":\"{}\"}}]", "x".repeat(1024 * 1024))
                        }),
                    ),
                ]),
            }),
            [(0, 17), (17, 159)].map(|(source, target)| GraphEdge {
                kind: "LINKS".into(),
                source_key: format!("file-{source}"),
                target_key: format!("file-{target}"),
                uid: format!("edge-{source}-{target}"),
            }),
        )
        .unwrap()
        .finish()
        .unwrap();
    let manifest: Value =
        serde_json::from_reader(fs::File::open(output.directory().join("icebug.json")).unwrap())
            .unwrap();
    let table = manifest["node_tables"][0]["shards"][0]["file"]
        .as_str()
        .unwrap();
    let parquet =
        SerializedFileReader::new(fs::File::open(output.directory().join(table)).unwrap()).unwrap();
    assert!(parquet.num_row_groups() > 1);
    let total_uncompressed = parquet
        .metadata()
        .row_groups()
        .iter()
        .map(|group| group.total_byte_size())
        .sum::<i64>();
    assert!(total_uncompressed > options().buffer_pool_bytes as i64);

    let graph = capture(&output.directory(), &ctx);
    let runtime = runtime();
    let parameters = BTreeMap::from([("path".into(), json!("src/file-17.rs"))]);
    let result = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (f:AstFileObservation) WHERE f.path = $path RETURN f LIMIT 2",
        &parameters,
        options(),
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0][0]["value"]["properties"]["value"]["resolution_outcomes_json"]["value"],
        selected
    );
    let identity = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (f:AstFileObservation) WHERE f.path = $path RETURN id(f)",
        &parameters,
        options(),
    )
    .unwrap();
    assert_eq!(identity.rows[0][0]["value"]["row"], "17");
    let edges = read(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (s:AstFileObservation)-[:LINKS]->(t:AstFileObservation) RETURN s.path, t.path ORDER BY s.path",
    );
    assert_eq!(edges.rows.len(), 2);
    assert_eq!(edges.rows[0][0]["value"], "src/file-0.rs");
    assert_eq!(edges.rows[0][1]["value"], "src/file-17.rs");
    assert_eq!(edges.rows[1][0]["value"], "src/file-17.rs");
    assert_eq!(edges.rows[1][1]["value"], "src/file-159.rs");
}

fn read(
    graph: &Arc<CapturedLocalGraph>,
    ctx: &Arc<SnapshotContext>,
    runtime: &Handle,
    query: &str,
) -> QueryResult {
    execute(graph, ctx, runtime, query, &BTreeMap::new(), options()).unwrap()
}
fn count(
    graph: &Arc<CapturedLocalGraph>,
    ctx: &Arc<SnapshotContext>,
    runtime: &Handle,
    query: &str,
    expected: &str,
) {
    let result = read(graph, ctx, runtime, query);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0]["value"], expected);
}
#[test]
fn native_queries_keep_every_member_graph_identity_and_parameters() {
    let temp = fixture();
    let ctx = original(Duration::from_secs(30));
    let graph = capture(temp.path(), &ctx);
    let runtime = runtime();
    let automatic = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a)-[r:LINK]->(b) RETURN count(r)",
        &BTreeMap::new(),
        QueryOptions {
            buffer_pool_bytes: 0,
            ..options()
        },
    )
    .unwrap();
    assert_eq!(automatic.rows.len(), 1);
    assert_eq!(automatic.rows[0][0]["value"], "7");
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A)<-[r:LINK]-(b:A) RETURN count(r)",
        "5",
    );
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A)-[:LINK]->(b:A) RETURN count(DISTINCT a)",
        "3",
    );
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (dst:A)<-[r:LINK]-(src:A) WHERE dst.uid='a1' RETURN count(r)",
        "4",
    );
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A)-[r:LINK]->(a) RETURN count(r)",
        "1",
    );
    let result = read(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A)-[r:LINK]->(b) RETURN id(a),id(r),id(b),r.uid,r.weight ORDER BY r.uid",
    );
    assert_eq!(result.rows.len(), 7);
    let identities = result
        .rows
        .iter()
        .map(|row| serde_json::to_string(&row[1]).unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(identities.len(), 7);
    let r0 = result
        .rows
        .iter()
        .find(|row| row[3]["value"] == "r0")
        .unwrap();
    assert_eq!(r0[1]["value"]["member"], 0);
    assert_eq!(r0[1]["value"]["row"], "0");
    assert!(r0[4].to_string().contains("9007199254740993"));
    let r1 = result
        .rows
        .iter()
        .find(|row| row[3]["value"] == "r1")
        .unwrap();
    assert!(r1[4]["value"].is_null());
    let r3 = result
        .rows
        .iter()
        .find(|row| row[3]["value"] == "r3")
        .unwrap();
    assert_eq!(r3[1]["value"]["member"], 1);
    assert!(r3[4].to_string().contains("twenty-two"));
    let entity_result = read(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH path=(a:A)-[r:LINK]->(b:B) RETURN a,r,b,path ORDER BY r.uid LIMIT 1",
    );
    let entities = &entity_result.rows[0];
    assert_eq!(entities[1]["value"]["id"]["value"]["member"], 2);
    assert_eq!(entities[1]["value"]["from"]["value"]["table"], 0);
    assert_eq!(entities[1]["value"]["to"]["value"]["table"], 1);
    assert_eq!(
        entities[1]["value"]["properties"]["value"]["uid"]["value"],
        "b0"
    );
    assert_eq!(
        entities[3]["value"]["_NODES"]["value"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        entities[3]["value"]["_RELS"]["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let params = BTreeMap::from([
        ("uids".into(), json!(["a0", "a2"])),
        ("record".into(), json!({"x":"α\0"})),
    ]);
    let result = execute(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A) WHERE a.uid IN $uids RETURN a,$record AS record ORDER BY a.uid",
        &params,
        options(),
    )
    .unwrap();
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0][1]["value"]["x"]["value"], "α\0");
    let entity = &result.rows[0][0]["value"];
    assert_eq!(entity["id"]["value"]["table"], 0);
    assert_eq!(entity["properties"]["value"]["uid"]["value"], "a0");
}
#[test]
fn native_values_preserve_exact_numeric_temporal_nested_and_nonfinite_data() {
    let temp = fixture();
    let ctx = original(Duration::from_secs(30));
    let graph = capture(temp.path(), &ctx);
    let runtime = runtime();
    let params = BTreeMap::from([("text".into(), json!("α\0\"\n"))]);
    let result=execute(&graph,&ctx,runtime.handle(),"RETURN CAST('18446744073709551615' AS UINT64),CAST('170141183460469231731687303715884105727' AS INT128),CAST('2024-01-02' AS DATE),TIMESTAMP('2024-01-02 03:04:05'),[1,NULL,3],{text:$text,flag:true},CAST('NaN' AS DOUBLE)",&params,options()).unwrap();
    let cells = &result.rows[0];
    assert_eq!(cells[0]["value"], "18446744073709551615");
    assert_eq!(cells[1]["value"], "170141183460469231731687303715884105727");
    assert_eq!(cells[2]["type"], "DATE");
    assert!(cells[2]["value"].is_string());
    assert_eq!(cells[3]["type"], "TIMESTAMP");
    assert!(cells[3]["value"].is_string());
    assert!(cells[4]["value"][1]["value"].is_null());
    assert_eq!(cells[5]["value"]["text"]["value"], "α\0\"\n");
    assert_eq!(cells[5]["value"]["flag"]["value"], true);
    assert_eq!(cells[6]["value"], "NaN");
}
#[test]
fn native_query_effects_and_changed_captures_fail_causally() {
    let temp = fixture();
    let ctx = original(Duration::from_secs(30));
    let graph = capture(temp.path(), &ctx);
    let runtime = runtime();
    for query in [
        "CREATE (:A {uid:'foreign'})",
        "MATCH (a:A) DELETE a",
        "CREATE NODE TABLE Foreign(uid STRING,PRIMARY KEY(uid))",
        "RETURN 1; RETURN 2",
        "LOAD EXTENSION httpfs",
    ] {
        assert!(
            execute(
                &graph,
                &ctx,
                runtime.handle(),
                query,
                &BTreeMap::new(),
                options()
            )
            .is_err(),
            "{query}"
        );
    }
    assert!(
        execute(
            &graph,
            &ctx,
            runtime.handle(),
            "MATCH (a:Untrusted) RETURN a",
            &BTreeMap::new(),
            options()
        )
        .is_err()
    );
    let external = format!(
        "LOAD FROM '{}' RETURN uid",
        temp.path().join("A.parquet").display()
    );
    let denied = execute(
        &graph,
        &ctx,
        runtime.handle(),
        &external,
        &BTreeMap::new(),
        options(),
    )
    .unwrap_err();
    assert_eq!(denied.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(
        denied.to_string(),
        "external data and procedure calls are not permitted"
    );
    let destination = temp.path().join("effect.csv");
    let query = format!("COPY (RETURN 1) TO '{}'", destination.display());
    assert!(
        execute(
            &graph,
            &ctx,
            runtime.handle(),
            &query,
            &BTreeMap::new(),
            options()
        )
        .is_err()
    );
    assert!(!destination.exists());
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A) RETURN count(a)",
        "3",
    );
    fs::write(temp.path().join("A.parquet"), b"replaced physical file").unwrap();
    assert!(
        execute(
            &graph,
            &ctx,
            runtime.handle(),
            "MATCH (a:A) RETURN a",
            &BTreeMap::new(),
            options()
        )
        .is_err()
    );
}
#[test]
fn original_interrupt_deadline_and_join_preserve_an_independent_query() {
    let temp = fixture();
    let ctx = original(Duration::from_secs(30));
    let graph = capture(temp.path(), &ctx);
    let runtime = runtime();
    let cancellation = ctx.cancellation().clone();
    let next = original(Duration::from_secs(30));
    let sibling = capture(temp.path(), &next);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            execute(
                &graph,
                &ctx,
                runtime.handle(),
                "UNWIND range(1,10000) AS x UNWIND range(1,10000) AS y RETURN sum(size(reverse(CAST(x+y AS STRING))))",
                &BTreeMap::new(),
                options(),
            )
        });
        std::thread::sleep(Duration::from_millis(25));
        count(
            &sibling,
            &next,
            runtime.handle(),
            "MATCH (a:A) RETURN count(a)",
            "3",
        );
        cancellation.cancel();
        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted, "{error:?}");
    });
    count(
        &sibling,
        &next,
        runtime.handle(),
        "MATCH (a:A) RETURN count(a)",
        "3",
    );
    let finite = original(Duration::from_millis(200));
    let graph = capture(temp.path(), &finite);
    let native_error = execute(
            &graph,
            &finite,
            runtime.handle(),
            "UNWIND range(1,10000) AS x UNWIND range(1,10000) AS y RETURN sum(size(reverse(CAST(x+y AS STRING))))",
            &BTreeMap::new(),
            options()
        ).unwrap_err();
    assert_eq!(
        native_error.kind(),
        io::ErrorKind::TimedOut,
        "{native_error:?}"
    );
    let cause = finite.check().unwrap_err();
    assert_eq!(cause.kind(), io::ErrorKind::TimedOut);
    count(
        &sibling,
        &next,
        runtime.handle(),
        "MATCH (a:A) RETURN count(a)",
        "3",
    );
}
#[test]
fn service_original_queries_cancel_and_join_without_a_deadline() {
    let temp = fixture();
    let ctx = Arc::new(SnapshotContext::service(Cancellation::default()));
    let graph = capture(temp.path(), &ctx);
    let runtime = runtime();
    assert!(ctx.deadline().is_none());
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A) RETURN count(a)",
        "3",
    );
    let sibling_ctx = Arc::new(SnapshotContext::service(Cancellation::default()));
    let sibling = capture(temp.path(), &sibling_ctx);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            execute(
                &graph,
                &ctx,
                runtime.handle(),
                "UNWIND range(1,10000) AS x UNWIND range(1,10000) AS y RETURN sum(size(reverse(CAST(x+y AS STRING))))",
                &BTreeMap::new(),
                options(),
            )
        });
        std::thread::sleep(Duration::from_millis(25));
        ctx.cancellation().cancel();
        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted, "{error:?}");
    });
    count(
        &sibling,
        &sibling_ctx,
        runtime.handle(),
        "MATCH (a:A) RETURN count(a)",
        "3",
    );
}

#[test]
fn optional_query_signal_distinguishes_native_timeout_and_finishing_controls() {
    let runtime = runtime();
    let ctx = Arc::new(SnapshotContext::service(Cancellation::default()));
    let signal = QuerySignal {
        original: Arc::clone(&ctx),
        complete: Cancellation::default(),
        runtime: runtime.handle().clone(),
    };
    assert_eq!(signal.remaining_ms().unwrap(), 0);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| signal.wait_interrupt());
        signal.finish();
        assert!(!worker.join().unwrap());
    });
    ctx.check().unwrap();

    let cancelled = QuerySignal {
        original: Arc::new(SnapshotContext::service(Cancellation::default())),
        complete: Cancellation::default(),
        runtime: runtime.handle().clone(),
    };
    cancelled.original.cancellation().cancel();
    assert!(cancelled.wait_interrupt());
    assert_eq!(
        cancelled.remaining_ms().unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );

    let finite = QuerySignal {
        original: original(Duration::from_secs(30)),
        complete: Cancellation::default(),
        runtime: runtime.handle().clone(),
    };
    assert!(finite.remaining_ms().unwrap() > 0);
    let expired = QuerySignal {
        original: original(Duration::ZERO),
        complete: Cancellation::default(),
        runtime: runtime.handle().clone(),
    };
    assert_eq!(
        expired.remaining_ms().unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
}

#[cfg(unix)]
#[test]
fn captured_paths_reject_non_unicode_and_preserve_unicode_parents() {
    use std::os::unix::ffi::OsStringExt;
    let temp = fixture();
    let parent = tempfile::tempdir().unwrap();
    let path = parent
        .path()
        .join(std::ffi::OsString::from_vec(b"native-\xff".to_vec()));
    fs::rename(temp.path(), &path).unwrap();
    let ctx = original(Duration::from_secs(30));
    let retained = RetainedBytesLimits::new(4096, 4, 4096).unwrap();
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(16 * 1024 * 1024, 64, 4096, 8).unwrap(),
        RetainedFileSet::new([], retained).unwrap(),
        retained,
    )
    .unwrap();
    let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
    let rejected = CapturedLocalGraph::capture(
        &path,
        &policy,
        SourceReadLimits::new(1024 * 1024, 4096, 16 * 1024 * 1024).unwrap(),
        Arc::clone(&ctx),
        LocalGraphPolicy::resolve(&config, &ctx).unwrap(),
    )
    .err()
    .expect("non-Unicode paths must fail explicitly");
    let cause = std::error::Error::source(&rejected)
        .unwrap()
        .downcast_ref::<io::Error>()
        .expect("preserve the actual path validation cause");
    assert_eq!(cause.kind(), io::ErrorKind::InvalidInput);
    assert!(cause.to_string().contains("path is not valid Unicode"));

    let unicode = parent.path().join("capturado café 日本 \\");
    fs::rename(&path, &unicode).unwrap();
    let graph = capture(&unicode, &ctx);
    let runtime = runtime();
    count(
        &graph,
        &ctx,
        runtime.handle(),
        "MATCH (a:A)-[r:LINK]->(b) RETURN count(r)",
        "7",
    );
}

#[test]
fn installed_reader_checks_actual_runtime_auth_registry_and_original_before_delivery() {
    use crate::imported_graph::{
        ImportedGraphError, ImportedGraphInput, open_imported_graph_runtime,
    };
    use graphit_auth::{
        AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, RoleIssuer,
        ScopeCapture, ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy,
        TtlPolicy, profile::ProfileLimits, store::AuthStore,
    };
    use graphit_core::{
        artifact_store::{CapturedPublicationScope, ScopeLimits},
        auth_document::{AuthDocumentLimits, AuthDocumentStore},
        installation_bootstrap::{self, BootstrapLimits},
        installation_identity::InstallationIdentityStore,
        module_settings::Module,
        native_document_file::NativeDocumentFileLimits,
        native_path::NativePath,
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::{InstallationPaths, ProjectPaths},
        runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    };
    use graphit_hub::{
        ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
        native::ContentLimits,
        policy::{HubResourcePolicy, RequestContext},
        registry::{
            HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
        },
    };
    let temp = tempfile::tempdir().unwrap();
    let data = fixture();
    let installed = temp.path().join("installed");
    fs::create_dir(&installed).unwrap();
    fs::rename(data.path(), installed.join("graph.icebug")).unwrap();
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 1048576,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 65536,
            max_collection_entries: 1024,
        },
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let ctx = request.snapshot();
    let paths = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let private =
        installation_bootstrap::prepare(&paths, BootstrapLimits::new(65536, 256).unwrap(), ctx)
            .into_installation()
            .unwrap();
    let identity = InstallationIdentityStore::new(&paths)
        .unwrap()
        .ensure(Duration::from_secs(5), &cancel)
        .unwrap();
    let runtime_key = Arc::new(
        RuntimeKeyOwner::start(
            private,
            RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
            ctx,
        )
        .into_owner()
        .unwrap(),
    );
    let limits = ScopeLimits::new(16, 4096, 128).unwrap();
    let installation = CapturedPublicationScope::installation(
        &paths,
        identity.unit_id().clone(),
        [installed.clone()],
        limits,
        ctx,
    )
    .unwrap();
    let record = installation.binding().unwrap();
    let authscope = CapturedPublicationScope::installation(
        &paths,
        identity.unit_id().clone(),
        [paths.root().to_path_buf()],
        limits,
        ctx,
    )
    .unwrap();
    let store = Arc::new(
        AuthStore::new(
            AuthDocumentStore::capture(
                authscope,
                AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
                ctx,
            )
            .unwrap(),
            ProfileLimits {
                bytes: 65536,
                nodes: 4096,
                scalar_bytes: 4096,
                collection_entries: 64,
                providers: 16,
                profiles: 16,
            },
            &request,
        )
        .unwrap(),
    );
    let allowed = [Permission::Read(Module::Ast)];
    let policy = || SourceAuthorityPolicy {
        ordinary: &allowed,
        administrative: &allowed,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    let issuer = RoleIssuer::new(
        ScopeCapture {
            live: &installation,
            record: &record,
        },
        Arc::clone(&runtime_key),
        store,
        IssuerAuthorityPolicy::new(
            policy(),
            policy(),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 4096,
            },
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 8192,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap(),
        TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
        &request,
    )
    .unwrap();
    let raw = String::from_utf8(
        runtime_key
            .check(ctx)
            .unwrap()
            .secret(ctx)
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
    let registry = HubRegistry::global(
        &paths,
        identity.unit_id(),
        HubResourcePolicy::resolve(&config).unwrap().hub(),
    )
    .unwrap();
    let projection = registry.limits().projection();
    let project = ProjectPaths::at(&installed, &paths).unwrap();
    let project_id = ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .ensure(&ProjectMetadata::new("query").unwrap())
        .unwrap()
        .id();
    let id = ArtifactId::new("captured-query", projection).unwrap();
    let version = ConcreteVersion::new("release", projection).unwrap();
    let source = SourceRef::local_link(
        NativePath::new(installed.clone()).unwrap(),
        project_id,
        ArtifactType::Ast,
        id.clone(),
        version.clone(),
        projection,
    )
    .unwrap();
    registry
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: ArtifactType::Ast,
            id: id.clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: source,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })
        .unwrap();
    let retained = RetainedBytesLimits::new(4096, 4, 4096).unwrap();
    let file = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(16 * 1024 * 1024, 64, 4096, 8).unwrap(),
        RetainedFileSet::new([], retained).unwrap(),
        retained,
    )
    .unwrap();
    let input = || ImportedGraphInput {
        registry: &registry,
        id: &id,
        version: &version,
        installation: ScopeCapture {
            live: &installation,
            record: &record,
        },
        file_policy: &file,
        read_limits: SourceReadLimits::new(1024 * 1024, 4096, 16 * 1024 * 1024).unwrap(),
        graph_policy: LocalGraphPolicy::resolve(&config, ctx).unwrap(),
        directory_limits: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
        request: &request,
    };
    assert!(matches!(
        open_imported_graph_runtime(&issuer, "not-a-runtime-key", input()),
        Err(ImportedGraphError::Authentication(_))
    ));
    let graph = open_imported_graph_runtime(&issuer, &raw, input()).unwrap();
    let runtime = runtime();
    let result = graph
        .query(
            runtime.handle(),
            "MATCH (a)-[r:LINK]->(b) RETURN count(r)",
            &BTreeMap::new(),
            options(),
        )
        .unwrap();
    assert_eq!(result.rows[0][0]["value"], "7");
    fs::write(
        installed.join("graph.icebug/schema.cypher"),
        "changed physical schema",
    )
    .unwrap();
    assert!(
        graph
            .query(runtime.handle(), "RETURN 1", &BTreeMap::new(), options())
            .is_err()
    );
}
