#[path = "support/corpus_rows.rs"]
mod corpus_rows;
use std::{
    error::Error,
    time::{Duration, Instant},
};

use graphit_ast::{
    native_nn::NATIVE_ENTITY_PROJECTION_RECIPE,
    tree_sitter_model::{TreeSitterSemanticDocument, TsProjection},
};
#[path = "support/rust_semantic.rs"]
#[allow(
    dead_code,
    reason = "shared fixture also encodes rows for the index locator tests"
)]
mod rust_semantic;
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use rust_semantic::rust_document;

fn original() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}

fn digest(text: &str) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(text.as_bytes()).into()
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

mod native_nn_codec {
    use super::*;
    use arrow_array::{
        Array, ArrayRef, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, ListArray,
        RecordBatch, StringArray, StructArray, UInt64Array,
        builder::{FixedSizeListBuilder, Float32Builder},
    };
    use arrow_schema::{Field, Schema};
    use futures::TryStreamExt;
    use graphit_ast::{
        indexed_source::{
            IndexedSourceSnapshot, read_branch_source, read_indexed_source, source_schema,
        },
        native_nn::{
            NativeNnError, NativeNnModel, NativeNnSource, NativeNnTsInput, encode_native_nn_ts,
            native_nn_schema, validate_native_nn,
        },
        source_inspection::IndexedSource,
    };
    use graphit_data::{
        lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore,
    };
    use std::{io, sync::Arc};
    use tempfile::TempDir;

    type TestResult = Result<(), Box<dyn Error>>;

    fn fixture_model() -> NativeNnModel<'static> {
        NativeNnModel {
            vector_id: [42; 32],
            dimensions: 3,
            model_id: "fixture/code",
            revision: "r1",
        }
    }

    fn raw_batch(path: &str, text: Option<&str>) -> RecordBatch {
        RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(vec![path])),
                Arc::new(StringArray::from(vec![text])),
            ],
        )
        .unwrap()
    }

    async fn codec_raw(
        temp: &TempDir,
        name: &str,
        path: &str,
        text: Option<&str>,
        original: &Arc<SnapshotContext>,
    ) -> IndexedSourceSnapshot {
        let uri = temp.path().join(name);
        let store = LanceModuleStore::create(
            uri.to_str().unwrap(),
            source_schema(),
            vec![raw_batch(path, text)],
        )
        .await
        .unwrap();
        read_indexed_source(&store.snapshot(), path, original)
            .await
            .unwrap()
    }

    fn raw_text<'a>(raw: &'a IndexedSourceSnapshot, original: &Arc<SnapshotContext>) -> &'a str {
        let IndexedSource::Available(text) = raw.source(original).unwrap() else {
            panic!("fixture requires actual raw text")
        };
        text
    }

    fn col<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> &'a T {
        batch
            .column_by_name(name)
            .unwrap()
            .as_any()
            .downcast_ref()
            .unwrap()
    }

    fn replace(batch: &RecordBatch, name: &str, value: ArrayRef) -> RecordBatch {
        let mut columns = batch.columns().to_vec();
        columns[batch.schema().index_of(name).unwrap()] = value;
        RecordBatch::try_new(batch.schema(), columns).unwrap()
    }

    fn nested_replace(array: &StructArray, name: &str, value: ArrayRef) -> StructArray {
        let mut columns = array.columns().to_vec();
        let index = array
            .fields()
            .iter()
            .position(|f| f.name() == name)
            .unwrap();
        columns[index] = value;
        StructArray::try_new(array.fields().clone(), columns, None).unwrap()
    }

    fn assert_range(array: &StructArray, row: usize, expected: [u64; 6]) {
        for (name, expected) in [
            "start",
            "end",
            "start_row",
            "start_column",
            "end_row",
            "end_column",
        ]
        .into_iter()
        .zip(expected)
        {
            assert_eq!(
                array
                    .column_by_name(name)
                    .unwrap()
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .unwrap()
                    .value(row),
                expected,
                "{name}"
            );
        }
    }

    fn assert_extent(array: &StructArray, row: usize, range: [u64; 6], lines: [u64; 2]) {
        assert_range(
            array
                .column_by_name("range")
                .unwrap()
                .as_any()
                .downcast_ref()
                .unwrap(),
            row,
            range,
        );
        for (name, expected) in ["start_line", "end_line"].into_iter().zip(lines) {
            assert_eq!(
                array
                    .column_by_name(name)
                    .unwrap()
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .unwrap()
                    .value(row),
                expected,
                "{name}"
            );
        }
    }

    fn list_values(list: &ListArray, values: ArrayRef) -> ArrayRef {
        let arrow_schema::DataType::List(field) = list.data_type() else {
            panic!("fixture list required")
        };
        Arc::new(ListArray::try_new(field.clone(), list.offsets().clone(), values, None).unwrap())
    }

    fn set_u64(array: &StructArray, name: &str, row: usize, value: u64) -> StructArray {
        let existing = array
            .column_by_name(name)
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        let values: Vec<u64> = (0..existing.len())
            .map(|i| if i == row { value } else { existing.value(i) })
            .collect();
        nested_replace(array, name, Arc::new(UInt64Array::from(values)))
    }

    fn reverse_frames(array: &StructArray) -> StructArray {
        let columns = array
            .columns()
            .iter()
            .map(|value| -> ArrayRef {
                if let Some(value) = value.as_any().downcast_ref::<UInt64Array>() {
                    Arc::new(UInt64Array::from(
                        (0..value.len())
                            .rev()
                            .map(|i| value.value(i))
                            .collect::<Vec<_>>(),
                    ))
                } else if let Some(value) = value.as_any().downcast_ref::<StringArray>() {
                    Arc::new(StringArray::from(
                        (0..value.len())
                            .rev()
                            .map(|i| value.value(i))
                            .collect::<Vec<_>>(),
                    ))
                } else {
                    Arc::new(reverse_frames(
                        value.as_any().downcast_ref::<StructArray>().unwrap(),
                    ))
                }
            })
            .collect();
        StructArray::try_new(array.fields().clone(), columns, None).unwrap()
    }

    fn vector(values: Vec<Option<f32>>, width: i32) -> ArrayRef {
        let mut builder = FixedSizeListBuilder::new(Float32Builder::new(), width);
        builder.values().append_option(values[0]);
        for value in &values[1..] {
            builder.values().append_option(*value);
        }
        builder.append(true);
        Arc::new(builder.finish())
    }

    #[tokio::test]
    async fn raw_function_names_and_qualified_impl_owners_preserve_native_syntax() -> TestResult {
        let temp = TempDir::new()?;
        let original = Arc::new(original());
        let raw = codec_raw(
            &temp,
            "raw.lance",
            "types.rs",
            Some("mod r#type { fn r#match() {} } impl crate::S<u8> { fn r#yield() {} }"),
            &original,
        )
        .await;
        let document_source = raw_text(&raw, &original);
        rust_document!(document, &["types.rs"], document_source, &original);
        let document_indices = rust_semantic::indices(&document)?;
        let vectors = vec![vec![1., 2., 3.]; document_indices.len()];
        let batch = encode_native_nn_ts(
            &fixture_model(),
            &[NativeNnTsInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &document_indices,
                vectors: &vectors,
                model_generation: Some(7),
            }],
            &original,
        )?;
        assert_eq!(
            col::<StringArray>(&batch, "name")
                .iter()
                .flatten()
                .collect::<Vec<_>>(),
            ["match", "yield"]
        );
        for (row, expected) in ["type", "crate::S<u8>"].into_iter().enumerate() {
            let owners = col::<ListArray>(&batch, "namespace").value(row);
            let owners = owners.as_any().downcast_ref::<StructArray>().unwrap();
            assert_eq!(
                owners
                    .column_by_name("name")
                    .unwrap()
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
                    .value(0),
                expected
            );
        }
        validate_native_nn(&batch, &fixture_model(), &original)?;
        Ok(())
    }

    #[tokio::test]
    async fn real_entities_roundtrip_module_and_empty_new_identity() -> TestResult {
        let temp = TempDir::new()?;
        let original = Arc::new(original());
        let text = "mod outer {\r\n  mod inner {\r\n    pub fn ação() { /* 🦀\0 */ }\r\n  }\r\n}";
        let raw = codec_raw(&temp, "raw.lance", "src/ação.rs", Some(text), &original).await;
        let path = ["src", "ação.rs"];
        let document_source = raw_text(&raw, &original);
        rust_document!(document, &path, document_source, &original);
        let document_indices = rust_semantic::indices(&document)?;
        let vectors = vec![vec![1.0, -2.0, 0.0]];
        let model = fixture_model();
        let batch = encode_native_nn_ts(
            &model,
            &[NativeNnTsInput {
                document: &document,
                entity_indices: &document_indices,
                source: NativeNnSource::Indexed(&raw),
                vectors: &vectors,
                model_generation: Some(7),
            }],
            &original,
        )?;
        validate_native_nn(&batch, &model, &original)?;
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.schema(), native_nn_schema(&model)?);
        assert_eq!(batch.schema().metadata().len(), 8);
        assert_eq!(
            batch.schema().metadata()["graphit.ast.nn.projection_recipe"],
            NATIVE_ENTITY_PROJECTION_RECIPE
        );
        assert_eq!(
            batch.schema().metadata()["graphit.ast.nn.vector_id"],
            "2a".repeat(32)
        );
        assert_eq!(
            batch.schema().metadata()["graphit.ast.nn.role"],
            "CodeDocument"
        );
        assert_eq!(
            col::<StringArray>(&batch, "uid").value(0),
            document.entities()?[0].identity().uid().as_str()
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                document.entities()?[0].identity().uid().as_str()
            )?,
            serde_json::json!([
                "ast-syntax-root-1",
                ["src", "ação.rs"],
                "rust",
                "tree-sitter-rust",
                "Function",
                [39, 45],
                [["Namespace", 4, 9], ["Namespace", 19, 24]]
            ])
        );
        assert_eq!(col::<StringArray>(&batch, "name").value(0), "ação");
        assert_eq!(col::<StringArray>(&batch, "data_key").value(0), "functions");
        assert_eq!(
            col::<StringArray>(&batch, "projection").value(0),
            "pub fn ação() { /* 🦀\0 */ }"
        );
        assert_eq!(
            col::<UInt64Array>(&batch, "raw_version").value(0),
            raw.version()
        );
        assert_eq!(col::<UInt64Array>(&batch, "model_generation").value(0), 7);
        assert_eq!(col::<StringArray>(&batch, "raw_origin").value(0), "module");
        assert!(col::<StringArray>(&batch, "raw_physical_branch").is_null(0));
        assert_eq!(
            col::<FixedSizeBinaryArray>(&batch, "full_source_sha256").value(0),
            digest(document_source)
        );
        assert_eq!(
            col::<FixedSizeBinaryArray>(&batch, "projection_sha256").value(0),
            {
                let projections = document.code_projections()?;
                let TsProjection::Available(projection) = &projections[0] else {
                    panic!("configured projection must be available")
                };
                digest(projection.text)
            }
        );
        let extent = col::<StructArray>(&batch, "extent");
        assert_range(
            col::<StructArray>(&batch, "name_anchor"),
            0,
            [39, 45, 2, 11, 2, 17],
        );
        assert_extent(extent, 0, [32, 63, 2, 4, 2, 35], [3, 3]);
        let range = extent
            .column_by_name("range")
            .unwrap()
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        assert_eq!(
            range
                .column_by_name("start")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap()
                .value(0),
            32
        );
        assert_eq!(
            extent
                .column_by_name("start_line")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap()
                .value(0),
            3
        );
        let owners = col::<ListArray>(&batch, "namespace").value(0);
        let owners = owners.as_any().downcast_ref::<StructArray>().unwrap();
        assert_eq!(owners.len(), 2);
        let owner_names = owners
            .column_by_name("name")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(owner_names.len(), 2);
        assert_eq!(owner_names.value(0), "outer");
        assert_eq!(owner_names.value(1), "inner");
        let owner_anchors = owners
            .column_by_name("name_anchor")
            .unwrap()
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        assert_range(owner_anchors, 0, [4, 9, 0, 4, 0, 9]);
        assert_range(owner_anchors, 1, [19, 24, 1, 6, 1, 11]);
        let owner_extents = owners
            .column_by_name("extent")
            .unwrap()
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        assert_extent(owner_extents, 0, [0, 71, 0, 0, 4, 1], [1, 5]);
        assert_extent(owner_extents, 1, [15, 68, 1, 2, 3, 3], [2, 4]);
        let values = col::<FixedSizeListArray>(&batch, "vector").value(0);
        assert_eq!(
            values
                .as_any()
                .downcast_ref::<Float32Array>()
                .unwrap()
                .values()
                .as_ref(),
            &[1.0, -2.0, 0.0]
        );

        let uri = temp.path().join("nn.lance");
        let store =
            LanceModuleStore::create(uri.to_str().unwrap(), batch.schema(), vec![batch.clone()])
                .await?;
        let retained = store.snapshot();
        let reopened = LanceModuleStore::open(uri.to_str().unwrap(), batch.schema()).await?;
        let scanned: Vec<RecordBatch> = reopened
            .snapshot()
            .scan()
            .try_into_stream()
            .await?
            .try_collect()
            .await?;
        assert_eq!(scanned.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        for observed in &scanned {
            validate_native_nn(observed, &model, &original)?;
        }
        let changed = NativeNnModel {
            vector_id: [43; 32],
            dimensions: 4,
            model_id: "fixture/code",
            revision: "r2",
        };
        let empty = encode_native_nn_ts(&changed, &[], &original)?;
        assert_eq!(empty.num_rows(), 0);
        let next = reopened
            .snapshot()
            .prepare_overwrite(empty.schema(), vec![empty])
            .await?
            .commit()
            .await?;
        assert_eq!(
            next.snapshot().schema(),
            native_nn_schema(&changed)?.as_ref()
        );
        assert_eq!(retained.schema(), native_nn_schema(&model)?.as_ref());
        let old: Vec<RecordBatch> = retained
            .scan()
            .try_into_stream()
            .await?
            .try_collect()
            .await?;
        for observed in old {
            validate_native_nn(&observed, &model, &original)?;
            assert!(matches!(
                validate_native_nn(&observed, &changed, &original),
                Err(NativeNnError::SchemaMismatch)
            ));
        }
        let new: Vec<RecordBatch> = next
            .snapshot()
            .scan()
            .try_into_stream()
            .await?
            .try_collect()
            .await?;
        assert_eq!(new.iter().map(RecordBatch::num_rows).sum::<usize>(), 0);
        for observed in new {
            validate_native_nn(&observed, &changed, &original)?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn empty_documents_and_same_width_model_mismatches() -> TestResult {
        let temp = TempDir::new()?;
        let original = Arc::new(original());
        let model = fixture_model();
        for (index, source) in ["", "mod empty {}"].into_iter().enumerate() {
            let raw = codec_raw(
                &temp,
                &format!("empty-{index}.lance"),
                "empty.rs",
                Some(source),
                &original,
            )
            .await;
            let path = ["empty.rs"];
            let document_source = raw_text(&raw, &original);
            rust_document!(document, &path, document_source, &original);
            let document_indices = rust_semantic::indices(&document)?;
            let batch = encode_native_nn_ts(
                &model,
                &[NativeNnTsInput {
                    document: &document,
                    entity_indices: &document_indices,
                    source: NativeNnSource::Indexed(&raw),
                    vectors: &[],
                    model_generation: None,
                }],
                &original,
            )?;
            assert_eq!(batch.num_rows(), 0);
            assert_eq!(batch.schema(), native_nn_schema(&model)?);
            validate_native_nn(&batch, &model, &original)?;
        }
        let batch = encode_native_nn_ts(&model, &[], &original)?;
        for changed in [
            NativeNnModel {
                vector_id: [1; 32],
                ..fixture_model()
            },
            NativeNnModel {
                model_id: "other",
                ..fixture_model()
            },
            NativeNnModel {
                revision: "r2",
                ..fixture_model()
            },
        ] {
            assert!(matches!(
                validate_native_nn(&batch, &changed, &original),
                Err(NativeNnError::SchemaMismatch)
            ));
        }
        for key in [
            "graphit.ast.nn.projection_recipe",
            "graphit.ast.nn.format",
            "graphit.ast.nn.role",
            "graphit.ast.nn.dimensions",
        ] {
            let mut metadata = batch.schema().metadata().clone();
            metadata.insert(key.into(), "changed".into());
            let invalid = RecordBatch::new_empty(Arc::new(Schema::new_with_metadata(
                batch.schema().fields().clone(),
                metadata,
            )));
            assert!(matches!(
                validate_native_nn(&invalid, &model, &original),
                Err(NativeNnError::SchemaMismatch)
            ));
        }
        for invalid in [
            NativeNnModel {
                dimensions: 0,
                ..fixture_model()
            },
            NativeNnModel {
                model_id: "",
                ..fixture_model()
            },
            NativeNnModel {
                revision: "bad\0",
                ..fixture_model()
            },
        ] {
            assert!(matches!(
                native_nn_schema(&invalid),
                Err(NativeNnError::InvalidModel)
            ));
        }
        assert!(matches!(
            native_nn_schema(&NativeNnModel {
                dimensions: u64::MAX,
                ..fixture_model()
            }),
            Err(NativeNnError::Representation)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn corrupt_rows_vectors_ranges_and_observation_are_errors() -> TestResult {
        let temp = TempDir::new()?;
        let original = Arc::new(original());
        let raw = codec_raw(
            &temp,
            "raw.lance",
            "src/a.rs",
            Some("mod outer {\n  mod inner {\n    fn f() {}\n  }\n}"),
            &original,
        )
        .await;
        let path = ["src", "a.rs"];
        let document_source = raw_text(&raw, &original);
        rust_document!(document, &path, document_source, &original);
        let document_indices = rust_semantic::indices(&document)?;
        let model = fixture_model();
        let vectors = vec![vec![1.0, 2.0, 3.0]];
        let input = || NativeNnTsInput {
            document: &document,
            entity_indices: &document_indices,
            source: NativeNnSource::Indexed(&raw),
            vectors: &vectors,
            model_generation: Some(1),
        };
        let batch = encode_native_nn_ts(&model, &[input()], &original)?;
        for (name, value) in [
            ("name", Arc::new(StringArray::from(vec!["z"])) as ArrayRef),
            ("projection", Arc::new(StringArray::from(vec!["fn x() {}"]))),
            ("path", Arc::new(StringArray::from(vec!["src/b.rs"]))),
            ("raw_origin", Arc::new(StringArray::from(vec!["default"]))),
            (
                "raw_physical_branch",
                Arc::new(StringArray::from(vec![Some("feature")])),
            ),
        ] {
            assert!(matches!(
                validate_native_nn(&replace(&batch, name, value), &model, &original),
                Err(NativeNnError::InvalidRow)
            ));
        }
        for value in [
            vector(vec![Some(f32::NAN), Some(0.0), Some(1.0)], 3),
            vector(vec![Some(f32::INFINITY), Some(0.0), Some(1.0)], 3),
            vector(vec![None, Some(0.0), Some(1.0)], 3),
        ] {
            assert!(matches!(
                validate_native_nn(&replace(&batch, "vector", value), &model, &original),
                Err(NativeNnError::InvalidVector)
            ));
        }
        let mut fields: Vec<Arc<Field>> = batch.schema().fields().iter().cloned().collect();
        let index = batch.schema().index_of("name").unwrap();
        fields[index] = Arc::new(fields[index].as_ref().clone().with_nullable(true));
        let mut columns = batch.columns().to_vec();
        columns[index] = Arc::new(StringArray::from(vec![None::<&str>]));
        let invalid = RecordBatch::try_new(
            Arc::new(Schema::new_with_metadata(
                fields,
                batch.schema().metadata().clone(),
            )),
            columns,
        )?;
        assert!(matches!(
            validate_native_nn(&invalid, &model, &original),
            Err(NativeNnError::SchemaMismatch)
        ));
        let extent = col::<StructArray>(&batch, "extent");
        let bad = nested_replace(extent, "end_line", Arc::new(UInt64Array::from(vec![999])));
        assert!(matches!(
            validate_native_nn(&replace(&batch, "extent", Arc::new(bad)), &model, &original),
            Err(NativeNnError::InvalidRow)
        ));
        let name = col::<StructArray>(&batch, "name_anchor");
        let bad = nested_replace(name, "end", Arc::new(UInt64Array::from(vec![u64::MAX])));
        assert!(matches!(
            validate_native_nn(
                &replace(&batch, "name_anchor", Arc::new(bad)),
                &model,
                &original
            ),
            Err(NativeNnError::InvalidRow)
        ));
        let parts = col::<ListArray>(&batch, "path_components");
        let null_parts = Arc::new(StringArray::from(vec![None::<&str>; parts.values().len()]));
        assert!(matches!(
            validate_native_nn(
                &replace(&batch, "path_components", list_values(parts, null_parts)),
                &model,
                &original
            ),
            Err(NativeNnError::InvalidRow)
        ));
        let namespace = col::<ListArray>(&batch, "namespace");
        let null_owners =
            arrow_array::new_null_array(namespace.values().data_type(), namespace.values().len());
        assert!(matches!(
            validate_native_nn(
                &replace(&batch, "namespace", list_values(namespace, null_owners)),
                &model,
                &original
            ),
            Err(NativeNnError::InvalidRow)
        ));
        let frames = namespace
            .values()
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        assert_eq!(frames.len(), 2);
        assert!(matches!(
            validate_native_nn(
                &replace(
                    &batch,
                    "namespace",
                    list_values(namespace, Arc::new(reverse_frames(frames)))
                ),
                &model,
                &original
            ),
            Err(NativeNnError::InvalidRow)
        ));

        // Keep byte containment and each owner's own points/lines coherent, but
        // move the inner owner's points after the function's points.
        let extents = frames
            .column_by_name("extent")
            .unwrap()
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        let range = extents
            .column_by_name("range")
            .unwrap()
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        let number = |name: &str| {
            range
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap()
                .value(1)
        };
        let start = number("start");
        let size = number("end") - start;
        let mut moved = range.clone();
        for (name, value) in [
            ("start_row", 3),
            ("end_row", 3),
            ("start_column", 1),
            ("end_column", 1 + size),
        ] {
            moved = set_u64(&moved, name, 1, value);
        }
        let mut extents = nested_replace(extents, "range", Arc::new(moved));
        extents = set_u64(&extents, "start_line", 1, 4);
        extents = set_u64(&extents, "end_line", 1, 4);
        let anchors = frames
            .column_by_name("name_anchor")
            .unwrap()
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        let number = |name: &str| {
            anchors
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap()
                .value(1)
        };
        let mut anchors = anchors.clone();
        for (name, value) in [
            ("start_row", 3),
            ("end_row", 3),
            ("start_column", 1 + number("start") - start),
            ("end_column", 1 + number("end") - start),
        ] {
            anchors = set_u64(&anchors, name, 1, value);
        }
        let moved = nested_replace(
            &nested_replace(frames, "extent", Arc::new(extents)),
            "name_anchor",
            Arc::new(anchors),
        );
        assert!(matches!(
            validate_native_nn(
                &replace(&batch, "namespace", list_values(namespace, Arc::new(moved))),
                &model,
                &original
            ),
            Err(NativeNnError::InvalidRow)
        ));
        assert!(matches!(
            encode_native_nn_ts(&model, &[input(), input()], &original),
            Err(NativeNnError::DuplicateIdentity)
        ));
        for values in [vec![], vec![vec![1.0]], vec![vec![f32::NAN, 0.0, 1.0]]] {
            assert!(
                encode_native_nn_ts(
                    &model,
                    &[NativeNnTsInput {
                        vectors: &values,
                        ..input()
                    }],
                    &original
                )
                .is_err()
            );
        }
        assert!(matches!(
            encode_native_nn_ts(
                &model,
                &[NativeNnTsInput {
                    model_generation: None,
                    ..input()
                }],
                &original
            ),
            Ok(ref batch) if batch.num_rows() > 0 && batch.column_by_name("model_generation").unwrap().null_count() == batch.num_rows()
        ));
        Ok(())
    }

    #[tokio::test]
    async fn original_source_owner_and_control_failures_preserve_causes() -> TestResult {
        let temp = TempDir::new()?;
        let original = Arc::new(original());
        let raw = codec_raw(&temp, "raw.lance", "a.rs", Some("fn f() {}"), &original).await;
        let path = ["a.rs"];
        let document_source = raw_text(&raw, &original);
        rust_document!(document, &path, document_source, &original);
        let document_indices = rust_semantic::indices(&document)?;
        let model = fixture_model();
        let vectors = vec![vec![0.0; 3]];
        let input = || NativeNnTsInput {
            document: &document,
            entity_indices: &document_indices,
            source: NativeNnSource::Indexed(&raw),
            vectors: &vectors,
            model_generation: Some(1),
        };
        let foreign = Arc::new(super::original());
        assert!(matches!(
            encode_native_nn_ts(&model, &[input()], &foreign),
            Err(NativeNnError::RequestMismatch)
        ));
        let reconstructed = document_source.to_owned();
        let equal_bytes_source = &reconstructed;
        rust_document!(equal_bytes, &path, equal_bytes_source, &original);
        let equal_bytes_indices = rust_semantic::indices(&equal_bytes)?;
        assert!(matches!(
            encode_native_nn_ts(
                &model,
                &[NativeNnTsInput {
                    document: &equal_bytes,
                    entity_indices: &equal_bytes_indices,
                    ..input()
                }],
                &original
            ),
            Err(NativeNnError::InvalidInput)
        ));
        let missing = codec_raw(&temp, "missing.lance", "a.rs", None, &original).await;
        assert!(matches!(
            encode_native_nn_ts(
                &model,
                &[NativeNnTsInput {
                    source: NativeNnSource::Indexed(&missing),
                    ..input()
                }],
                &original
            ),
            Err(NativeNnError::InvalidInput)
        ));
        let foreign_raw =
            codec_raw(&temp, "foreign.lance", "a.rs", Some("fn f() {}"), &foreign).await;
        let error = encode_native_nn_ts(
            &model,
            &[NativeNnTsInput {
                source: NativeNnSource::Indexed(&foreign_raw),
                ..input()
            }],
            &original,
        )
        .err()
        .unwrap();
        assert!(matches!(error, NativeNnError::Source(_)));
        assert!(error.source().is_some());
        let expired = Arc::new(SnapshotContext::new(
            Instant::now(),
            Cancellation::default(),
        ));
        let error = encode_native_nn_ts(&model, &[], &expired).err().unwrap();
        match error {
            NativeNnError::Operation(ref e) => assert_eq!(e.kind(), io::ErrorKind::TimedOut),
            _ => panic!("actual Original IO expected"),
        }
        assert!(error.source().is_some());
        original.cancellation().cancel();
        let error = encode_native_nn_ts(&model, &[input()], &original)
            .err()
            .unwrap();
        match error {
            NativeNnError::Operation(ref e) => assert_eq!(e.kind(), io::ErrorKind::Interrupted),
            _ => panic!("actual Original IO expected"),
        }
        assert!(error.source().is_some());
        Ok(())
    }

    #[tokio::test]
    async fn raw_module_default_named_and_inherited_remain_distinct() -> TestResult {
        let temp = TempDir::new()?;
        let original = Arc::new(original());
        let uri = temp.path().join("raw.lance");
        let module = LanceModuleStore::create(
            uri.to_str().unwrap(),
            source_schema(),
            vec![raw_batch("a.rs", Some("fn f() {}"))],
        )
        .await?;
        let branches = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None).await?;
        let default = branches.capture_target(None).await?;
        let inherited = branches.capture_target(Some("feature")).await?;
        let named = inherited.clone().materialize().await?;
        let raw_module = read_indexed_source(&module.snapshot(), "a.rs", &original).await?;
        let raw_default = read_branch_source(&default, "a.rs", &original).await?;
        let raw_inherited = read_branch_source(&inherited, "a.rs", &original).await?;
        let raw_named = read_branch_source(&named, "a.rs", &original).await?;
        for (raw, origin, physical, target) in [
            (&raw_module, "module", None, None),
            (&raw_default, "branch", None, None),
            (&raw_inherited, "branch", None, Some("feature")),
            (&raw_named, "branch", Some("feature"), None),
        ] {
            let path = ["a.rs"];
            let document_source = raw_text(raw, &original);
            rust_document!(document, &path, document_source, &original);
            let document_indices = rust_semantic::indices(&document)?;
            let vectors = vec![vec![0.0; 3]];
            let batch = encode_native_nn_ts(
                &fixture_model(),
                &[NativeNnTsInput {
                    document: &document,
                    entity_indices: &document_indices,
                    source: NativeNnSource::Indexed(raw),
                    vectors: &vectors,
                    model_generation: Some(2),
                }],
                &original,
            )?;
            assert_eq!(col::<StringArray>(&batch, "raw_origin").value(0), origin);
            for (column, expected) in [
                ("raw_physical_branch", physical),
                ("raw_inherited_target", target),
            ] {
                let observed = col::<StringArray>(&batch, column);
                assert_eq!(
                    if observed.is_null(0) {
                        None
                    } else {
                        Some(observed.value(0))
                    },
                    expected
                );
            }
        }
        Ok(())
    }
}

mod native_nn_physical_source {
    use super::*;
    use graphit_ast::native_nn_physical_source::{
        NativeNnPhysicalSource, NativeNnPhysicalSourceError, capture_native_nn_physical_source,
    };
    use graphit_auth::{ProjectCapture, ScopeCapture};
    use graphit_core::{
        artifact_store::{
            CapturedPublicationScope, CopyLimits, RetainedBytesLimits, ScopeBindingRecord,
            ScopeLimits, SourceReadLimits,
        },
        brand::Brand,
        installation_identity::InstallationIdentityStore,
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::{InstallationPaths, ProjectPaths},
        version_context::{Checkout, VersionContext},
    };
    use std::{fs, io, path::PathBuf, sync::Arc};
    use tempfile::TempDir;

    const BODY: &str =
        "mod outer {\n    mod inner {\n        pub fn answer(x: i32) -> i32 { x + 1 }\n    }\n}\n";
    type TestResult = Result<(), Box<dyn Error>>;
    struct Project {
        root: PathBuf,
        scope: CapturedPublicationScope,
        record: ScopeBindingRecord,
        version: VersionContext,
    }
    struct Fixture {
        _temp: TempDir,
        projects: Vec<Project>,
        installation: CapturedPublicationScope,
        installation_record: ScopeBindingRecord,
    }
    impl Fixture {
        fn new(context: &Arc<SnapshotContext>) -> Result<Self, Box<dyn Error>> {
            let temp = TempDir::new()?;
            let paths = InstallationPaths::resolve(
                Brand::default(),
                temp.path(),
                None,
                Some(&temp.path().join("installation")),
            )?;
            let timeout = Duration::from_secs(5);
            let identity =
                InstallationIdentityStore::new(&paths)?.ensure(timeout, context.cancellation())?;
            fs::create_dir_all(paths.global_path(&["runtime"])?)?;
            let limits = ScopeLimits::new(16, 4096, 256)?;
            let mut projects = Vec::new();
            for name in ["first", "second"] {
                let root = temp.path().join(name);
                fs::create_dir_all(root.join("src"))?;
                fs::write(root.join("src/answer.rs"), BODY)?;
                fs::write(root.join("empty.rs"), [])?;
                fs::write(root.join("invalid.rs"), [0xff])?;
                #[cfg(unix)]
                fs::write(root.join("literal\\name.rs"), "pub fn café() {}\r\n")?;
                let project = ProjectPaths::at(&root, &paths)?;
                let stored = ProjectIdentityStore::at(&project)
                    .writer(timeout, context.cancellation())?
                    .ensure(&ProjectMetadata::new(name)?)?;
                fs::create_dir_all(project.project_runtime(stored.id(), &[])?)?;
                let scope =
                    CapturedPublicationScope::project(&project, stored.id(), limits, context)?;
                let record = scope.binding()?;
                let version =
                    VersionContext::select(&project, Checkout::NonGit, "feature", None, "")?;
                projects.push(Project {
                    root,
                    scope,
                    record,
                    version,
                });
            }
            let installation = CapturedPublicationScope::installation(
                &paths,
                identity.unit_id().clone(),
                projects.iter().map(|p| p.root.clone()),
                limits,
                context,
            )?;
            let installation_record = installation.binding()?;
            Ok(Self {
                _temp: temp,
                projects,
                installation,
                installation_record,
            })
        }
        fn project(&self, index: usize) -> ProjectCapture<'_> {
            let project = &self.projects[index];
            ProjectCapture {
                scope: ScopeCapture {
                    live: &project.scope,
                    record: &project.record,
                },
                version: &project.version,
            }
        }
    }
    fn limits() -> (CopyLimits, RetainedBytesLimits, SourceReadLimits) {
        (
            CopyLimits::new(65536, 16, 4096, 8).unwrap(),
            RetainedBytesLimits::new(1, 1, 256).unwrap(),
            SourceReadLimits::new(65536, 1024, 65536).unwrap(),
        )
    }
    fn capture<'a>(
        project: ProjectCapture<'a>,
        path: &[&str],
        context: &Arc<SnapshotContext>,
    ) -> Result<NativeNnPhysicalSource<'a>, NativeNnPhysicalSourceError> {
        let (copy, retention, read) = limits();
        capture_native_nn_physical_source(project, path, copy, retention, read, context)
    }
    fn assert_io<T>(result: Result<T, NativeNnPhysicalSourceError>, kind: io::ErrorKind) {
        let error = result.err().expect("physical Source must fail");
        let NativeNnPhysicalSourceError::Operation(cause) = &error else {
            panic!("actual IO cause required: {error}")
        };
        assert_eq!(cause.kind(), kind);
        assert!(error.source().unwrap().is::<io::Error>());
    }

    #[test]
    fn actual_project_full_source_is_the_model_allocation_and_content_sha() -> TestResult {
        let context = Arc::new(original());
        let fixture = Fixture::new(&context)?;
        for index in [0, 1] {
            let owner = capture(fixture.project(index), &["src", "answer.rs"], &context)?;
            let source = owner.source(&context)?;
            assert_eq!(source, BODY);
            assert_eq!(owner.relative_path(), "src/answer.rs");
            assert_eq!(owner.relative_components(), &["src", "answer.rs"]);
            assert_eq!(owner.logical_version(), "feature");
            assert_eq!(
                owner.lance_branch(),
                fixture.projects[index].version.lance_branch()
            );
            assert_eq!(
                hex(owner.content_sha256()),
                "97edb42a4b0a3ced3afc5808991f515f1d054f638b4cfd7442a9dacfb67e046b"
            );
            let document_source = source;
            rust_document!(document, &["src", "answer.rs"], document_source, &context);
            let document_indices = rust_semantic::indices(&document)?;
            assert_eq!(
                document
                    .source()?
                    .range(0, document_source.len())?
                    .text()
                    .as_ptr(),
                source.as_ptr()
            );
            assert_eq!(digest(document_source), owner.content_sha256());
            let projections = document.code_projections()?;
            let TsProjection::Available(projection) = &projections[0] else {
                panic!("physical Source fixture projection must be available")
            };
            let entity = projection.entity;
            assert_eq!(document_indices, vec![projection.entity_index]);
            assert_eq!(projection.text, "pub fn answer(x: i32) -> i32 { x + 1 }");
            assert_eq!(
                (entity.name_raw().start(), entity.name_raw().end()),
                (43, 49)
            );
            assert_eq!(
                (
                    projection.extent.range().start(),
                    projection.extent.range().end()
                ),
                (36, 74)
            );
            assert_eq!(projection.text.as_ptr(), source[36..74].as_ptr());
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(entity.identity().uid().as_str())?,
                serde_json::json!([
                    "ast-syntax-root-1",
                    ["src", "answer.rs"],
                    "rust",
                    "tree-sitter-rust",
                    "Function",
                    [43, 49],
                    [["Namespace", 4, 9], ["Namespace", 20, 25]]
                ])
            );
            owner.revalidate(&context)?;
        }
        let empty = capture(fixture.project(0), &["empty.rs"], &context)?;
        let source = empty.source(&context)?;
        assert_eq!(source, "");
        assert_eq!(
            hex(empty.content_sha256()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let document_source = source;
        rust_document!(document, &["empty.rs"], document_source, &context);
        let document_indices = rust_semantic::indices(&document)?;
        assert!(document_indices.is_empty());
        #[cfg(unix)]
        {
            let owner = capture(fixture.project(0), &["literal\\name.rs"], &context)?;
            assert_eq!(owner.relative_path(), "literal\\name.rs");
            let document_source = owner.source(&context)?;
            rust_document!(document, &["literal\\name.rs"], document_source, &context);
            let document_indices = rust_semantic::indices(&document)?;
            assert_eq!(document.entities()?[document_indices[0]].name(), "café");
            assert_eq!(
                {
                    let projections = document.code_projections()?;
                    let TsProjection::Available(projection) = &projections[0] else {
                        panic!("configured projection must be available")
                    };
                    projection.text
                },
                "pub fn café() {}"
            );
        }
        Ok(())
    }

    #[test]
    fn malformed_associations_utf8_limits_and_original_causes_are_preserved() -> TestResult {
        let context = Arc::new(original());
        let fixture = Fixture::new(&context)?;
        let owner = capture(fixture.project(0), &["src", "answer.rs"], &context)?;
        assert!(matches!(
            owner.source(&Arc::new(original())),
            Err(NativeNnPhysicalSourceError::RequestMismatch)
        ));
        let mut wrong = fixture.project(0);
        wrong.version = &fixture.projects[1].version;
        assert!(matches!(
            capture(wrong, &["empty.rs"], &context),
            Err(NativeNnPhysicalSourceError::InvalidInput)
        ));
        wrong = fixture.project(0);
        wrong.scope.record = &fixture.projects[1].record;
        assert!(matches!(
            capture(wrong, &["empty.rs"], &context),
            Err(NativeNnPhysicalSourceError::InvalidInput)
        ));
        let installation = ProjectCapture {
            scope: ScopeCapture {
                live: &fixture.installation,
                record: &fixture.installation_record,
            },
            version: &fixture.projects[0].version,
        };
        assert!(matches!(
            capture(installation, &["empty.rs"], &context),
            Err(NativeNnPhysicalSourceError::InvalidInput)
        ));
        for path in [
            &[][..],
            &[""][..],
            &["."][..],
            &[".."][..],
            &["src/answer.rs"][..],
            &["src/"][..],
            &["bad\0name"][..],
            &["/answer.rs"][..],
        ] {
            assert!(matches!(
                capture(fixture.project(0), path, &context),
                Err(NativeNnPhysicalSourceError::InvalidInput)
            ));
        }
        let error = capture(fixture.project(0), &["invalid.rs"], &context)
            .err()
            .unwrap();
        assert!(matches!(error, NativeNnPhysicalSourceError::Utf8(_)));
        assert!(error.source().unwrap().is::<std::str::Utf8Error>());
        let (copy, retention, _) = limits();
        assert_io(
            capture_native_nn_physical_source(
                fixture.project(0),
                &["src", "answer.rs"],
                copy,
                retention,
                SourceReadLimits::new(1, 1, 1)?,
                &context,
            ),
            io::ErrorKind::FileTooLarge,
        );
        assert_io(
            capture(fixture.project(0), &["absent.rs"], &context),
            io::ErrorKind::NotFound,
        );
        let expired = Arc::new(SnapshotContext::new(
            Instant::now(),
            Cancellation::default(),
        ));
        assert_io(
            capture(fixture.project(0), &["empty.rs"], &expired),
            io::ErrorKind::TimedOut,
        );
        context.cancellation().cancel();
        assert_io(owner.source(&context), io::ErrorKind::Interrupted);
        assert_io(
            capture(fixture.project(0), &["empty.rs"], &context),
            io::ErrorKind::Interrupted,
        );
        Ok(())
    }

    #[test]
    fn physical_file_and_selected_ancestor_mutations_reject_stale_source() -> TestResult {
        let context = Arc::new(original());
        let fixture = Fixture::new(&context)?;
        let owner = capture(fixture.project(0), &["src", "answer.rs"], &context)?;
        let path = fixture.projects[0].root.join("src/answer.rs");
        let modified = fs::metadata(&path)?.modified()?;
        fs::write(&path, BODY.replace("x + 1", "x + 2"))?;
        // The retained capture must reject same-length changes even when the
        // writer restores mtime. Unix ctime supplies the mutation observation;
        // other platforms still verify content at this boundary.
        fs::File::options()
            .write(true)
            .open(&path)?
            .set_times(fs::FileTimes::new().set_modified(modified))?;
        assert_io(owner.source(&context), io::ErrorKind::InvalidData);
        let owner = capture(fixture.project(1), &["src", "answer.rs"], &context)?;
        fs::rename(
            fixture.projects[1].root.join("src"),
            fixture.projects[1].root.join("old-src"),
        )?;
        fs::create_dir(fixture.projects[1].root.join("src"))?;
        fs::write(fixture.projects[1].root.join("src/answer.rs"), BODY)?;
        assert_io(owner.source(&context), io::ErrorKind::InvalidData);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn file_links_and_linked_ancestors_never_produce_source() -> TestResult {
        let context = Arc::new(original());
        let fixture = Fixture::new(&context)?;
        let root = &fixture.projects[0].root;
        std::os::unix::fs::symlink("src/answer.rs", root.join("linked.rs"))?;
        assert!(capture(fixture.project(0), &["linked.rs"], &context).is_err());
        std::os::unix::fs::symlink("src", root.join("linked-dir"))?;
        assert!(capture(fixture.project(0), &["linked-dir", "answer.rs"], &context).is_err());
        fs::hard_link(root.join("src/answer.rs"), root.join("hard.rs"))?;
        assert!(capture(fixture.project(0), &["hard.rs"], &context).is_err());
        assert!(capture(fixture.project(0), &["src", "answer.rs"], &context).is_err());
        Ok(())
    }

    mod physical_codec {
        use super::*;
        use arrow_array::{Array, FixedSizeBinaryArray, RecordBatch, StringArray, UInt64Array};
        use graphit_ast::{
            indexed_source::{
                IndexedSourceSnapshot, MissingSourceReason, read_branch_source,
                read_indexed_source, source_schema,
            },
            native_nn::{
                NativeNnError, NativeNnModel, NativeNnSource, NativeNnTsInput, encode_native_nn_ts,
                native_nn_schema,
            },
            native_nn_query::{NativeNnMetric, query_corpus_nn},
        };
        use graphit_data::{
            lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore,
        };

        fn model() -> NativeNnModel<'static> {
            NativeNnModel {
                vector_id: [117; 32],
                dimensions: 3,
                model_id: "physical/fixture",
                revision: "r1",
            }
        }
        async fn raw_store(
            temp: &TempDir,
            name: &str,
            path: &str,
            text: Option<&str>,
        ) -> Result<LanceModuleStore, Box<dyn Error>> {
            Ok(LanceModuleStore::create(
                temp.path().join(name).to_str().unwrap(),
                source_schema(),
                vec![RecordBatch::try_new(
                    source_schema(),
                    vec![
                        Arc::new(StringArray::from(vec![path])),
                        Arc::new(StringArray::from(vec![text])),
                    ],
                )?],
            )
            .await?)
        }
        fn encoded(
            document: &TreeSitterSemanticDocument<'_, '_, '_, '_, '_>,
            document_indices: &[usize],
            capture: &NativeNnPhysicalSource<'_>,
            raw: &IndexedSourceSnapshot,
            context: &Arc<SnapshotContext>,
        ) -> Result<RecordBatch, NativeNnError> {
            let vectors = vec![vec![1., 0., 0.]; document_indices.len()];
            encode_native_nn_ts(
                &model(),
                &[NativeNnTsInput {
                    document,
                    entity_indices: document_indices,
                    source: NativeNnSource::Physical { capture, raw },
                    vectors: &vectors,
                    model_generation: if vectors.is_empty() { None } else { Some(7) },
                }],
                context,
            )
        }
        fn col<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> &'a T {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref()
                .unwrap()
        }
        fn lexical(
            document: &TreeSitterSemanticDocument<'_, '_, '_, '_, '_>,
            capture: &NativeNnPhysicalSource<'_>,
            raw: &IndexedSourceSnapshot,
            context: &Arc<SnapshotContext>,
        ) -> Result<RecordBatch, Box<dyn Error>> {
            let graph = graphit_ast::semantic_graph::assemble_graph_ts(
                graphit_ast::semantic_graph::AstGraphTsInput {
                    document,
                    source: NativeNnSource::Physical { capture, raw },
                },
                context,
            )?
            .into_parts();
            Ok(graphit_ast::entity_search::encode_file(
                &graph,
                capture.source(context)?,
                context,
            )?)
        }
        async fn persisted_and_queried(
            temp: &TempDir,
            name: &str,
            batch: &RecordBatch,
            lexical: RecordBatch,
            branch: Option<&str>,
            context: &Arc<SnapshotContext>,
        ) -> TestResult {
            let applied = corpus_rows::persist(
                &temp.path().join(name),
                lexical,
                batch,
                &model(),
                branch,
                context,
            )
            .await?;
            let snapshot = &applied;
            let matches = query_corpus_nn(
                snapshot,
                &model(),
                &[1., 0., 0.],
                1,
                NativeNnMetric::L2,
                context,
            )
            .await?;
            if batch.num_rows() == 0 {
                assert!(matches.rows(context)?.iter().all(|b| b.num_rows() == 0));
            } else {
                assert_eq!(matches.rows(context)?, std::slice::from_ref(batch));
                assert_eq!(matches.distances(context)?, &[vec![0.]]);
            }
            Ok(())
        }

        #[tokio::test]
        async fn actual_null_raw_observations_persist_and_query_complete_physical_payload()
        -> TestResult {
            let context = Arc::new(original());
            let fixture = Fixture::new(&context)?;
            let temp = TempDir::new()?;
            let owner = capture(fixture.project(0), &["src", "answer.rs"], &context)?;
            let document_source = owner.source(&context)?;
            rust_document!(document, &["src", "answer.rs"], document_source, &context);
            let document_indices = rust_semantic::indices(&document)?;
            assert_eq!(
                document
                    .source()?
                    .range(0, document_source.len())?
                    .text()
                    .as_ptr(),
                owner.source(&context)?.as_ptr()
            );
            assert_eq!(
                (
                    document.entities()?[0].name_raw().start(),
                    document.entities()?[0].name_raw().end()
                ),
                (43, 49)
            );
            let store = raw_store(&temp, "raw.lance", "src/answer.rs", None).await?;
            let branches = LanceBranchStore::open(
                temp.path().join("raw.lance").to_str().unwrap(),
                source_schema(),
                None,
            )
            .await?;
            let default = branches.capture_target(None).await?;
            let inherited = branches.capture_target(Some("feature")).await?;
            let named = inherited.clone().materialize().await?;
            let module = read_indexed_source(&store.snapshot(), "src/answer.rs", &context).await?;
            let raw_default = read_branch_source(&default, "src/answer.rs", &context).await?;
            let raw_inherited = read_branch_source(&inherited, "src/answer.rs", &context).await?;
            let raw_named = read_branch_source(&named, "src/answer.rs", &context).await?;
            for (i, raw, origin, physical, target) in [
                (0, &module, "module", None, None),
                (1, &raw_default, "branch", None, None),
                (2, &raw_inherited, "branch", None, Some("feature")),
                (3, &raw_named, "branch", Some("feature"), None),
            ] {
                assert_eq!(
                    raw.missing_reason(),
                    Some(MissingSourceReason::TextNotStored)
                );
                let batch = encoded(&document, &document_indices, &owner, raw, &context)?;
                assert_eq!(batch.schema().fields().len(), 21);
                assert_eq!(batch.schema().metadata().len(), 8);
                assert_eq!(
                    col::<UInt64Array>(&batch, "raw_version").value(0),
                    raw.version()
                );
                assert!(raw.version() > 0);
                assert_eq!(col::<UInt64Array>(&batch, "model_generation").value(0), 7);
                assert_eq!(
                    col::<FixedSizeBinaryArray>(&batch, "full_source_sha256").value(0),
                    owner.content_sha256()
                );
                assert_eq!(
                    col::<StringArray>(&batch, "projection").value(0),
                    "pub fn answer(x: i32) -> i32 { x + 1 }"
                );
                assert_eq!(col::<StringArray>(&batch, "raw_origin").value(0), origin);
                for (column, expected) in [
                    ("raw_physical_branch", physical),
                    ("raw_inherited_target", target),
                ] {
                    let array = col::<StringArray>(&batch, column);
                    assert_eq!(
                        if array.is_null(0) {
                            None
                        } else {
                            Some(array.value(0))
                        },
                        expected
                    );
                }
                persisted_and_queried(
                    &temp,
                    &format!("nn{i}.lance"),
                    &batch,
                    lexical(&document, &owner, raw, &context)?,
                    physical.or(target),
                    &context,
                )
                .await?;
            }
            Ok(())
        }

        #[tokio::test]
        async fn missing_routes_and_foreign_allocation_are_rejected_before_empty_shortcut()
        -> TestResult {
            let context = Arc::new(original());
            let fixture = Fixture::new(&context)?;
            let temp = TempDir::new()?;
            let owner = capture(fixture.project(0), &["src", "answer.rs"], &context)?;
            let other = capture(fixture.project(1), &["src", "answer.rs"], &context)?;
            let document_source = owner.source(&context)?;
            rust_document!(document, &["src", "answer.rs"], document_source, &context);
            let document_indices = rust_semantic::indices(&document)?;
            let store = raw_store(&temp, "raw.lance", "src/answer.rs", None).await?;
            let raw = read_indexed_source(&store.snapshot(), "src/answer.rs", &context).await?;
            assert!(matches!(
                encoded(&document, &document_indices, &other, &raw, &context),
                Err(NativeNnError::InvalidInput)
            ));
            let copy = owner.source(&context)?.to_owned();
            assert_ne!(copy.as_ptr(), owner.source(&context)?.as_ptr());
            let copied_source = &copy;
            rust_document!(copied, &["src", "answer.rs"], copied_source, &context);
            let copied_indices = rust_semantic::indices(&copied)?;
            assert!(matches!(
                encoded(&copied, &copied_indices, &owner, &raw, &context),
                Err(NativeNnError::InvalidInput)
            ));
            let wrong_path_source = owner.source(&context)?;
            rust_document!(wrong_path, &["elsewhere.rs"], wrong_path_source, &context);
            let wrong_path_indices = rust_semantic::indices(&wrong_path)?;
            assert!(matches!(
                encoded(&wrong_path, &wrong_path_indices, &owner, &raw, &context),
                Err(NativeNnError::InvalidInput)
            ));
            let absent = read_indexed_source(&store.snapshot(), "absent.rs", &context).await?;
            assert_eq!(
                absent.missing_reason(),
                Some(MissingSourceReason::FileAbsent)
            );
            assert!(matches!(
                encoded(&document, &document_indices, &owner, &absent, &context),
                Err(NativeNnError::InvalidInput)
            ));
            let foreign = Arc::new(original());
            let foreign_raw =
                read_indexed_source(&store.snapshot(), "src/answer.rs", &foreign).await?;
            assert!(matches!(
                encoded(&document, &document_indices, &owner, &foreign_raw, &context),
                Err(NativeNnError::Source(_))
            ));
            let foreign_document_source = owner.source(&context)?;
            rust_document!(
                foreign_document,
                &["src", "answer.rs"],
                foreign_document_source,
                &foreign
            );
            let foreign_document_indices = rust_semantic::indices(&foreign_document)?;
            assert!(matches!(
                encoded(
                    &foreign_document,
                    &foreign_document_indices,
                    &owner,
                    &raw,
                    &context
                ),
                Err(NativeNnError::RequestMismatch)
            ));
            let foreign_capture = capture(fixture.project(0), &["src", "answer.rs"], &foreign)?;
            let error = encoded(
                &document,
                &document_indices,
                &foreign_capture,
                &raw,
                &context,
            )
            .unwrap_err();
            assert!(
                matches!(&error, NativeNnError::PhysicalSource(e) if matches!(e.as_ref(), NativeNnPhysicalSourceError::RequestMismatch))
            );
            assert!(error.source().unwrap().is::<NativeNnPhysicalSourceError>());
            let empty_store = raw_store(&temp, "empty.lance", "empty.rs", Some("")).await?;
            let available =
                read_indexed_source(&empty_store.snapshot(), "empty.rs", &context).await?;
            let empty_owner = capture(fixture.project(0), &["empty.rs"], &context)?;
            let empty_document_source = empty_owner.source(&context)?;
            rust_document!(
                empty_document,
                &["empty.rs"],
                empty_document_source,
                &context
            );
            let empty_document_indices = rust_semantic::indices(&empty_document)?;
            assert!(matches!(
                encoded(
                    &empty_document,
                    &empty_document_indices,
                    &empty_owner,
                    &available,
                    &context
                ),
                Err(NativeNnError::InvalidInput)
            ));
            let indexed_document_source = "struct S;";
            rust_document!(
                indexed_document,
                &["src", "answer.rs"],
                indexed_document_source,
                &context
            );
            let indexed_document_indices = rust_semantic::indices(&indexed_document)?;
            assert!(matches!(
                encode_native_nn_ts(
                    &model(),
                    &[NativeNnTsInput {
                        document: &indexed_document,
                        entity_indices: &indexed_document_indices,
                        source: NativeNnSource::Indexed(&raw),
                        vectors: &[],
                        model_generation: None
                    }],
                    &context
                ),
                Err(NativeNnError::InvalidInput)
            ));
            fs::write(fixture.projects[0].root.join("none.rs"), "struct S;")?;
            let none_owner = capture(fixture.project(0), &["none.rs"], &context)?;
            let none_store = raw_store(&temp, "none.lance", "none.rs", None).await?;
            let none_raw = read_indexed_source(&none_store.snapshot(), "none.rs", &context).await?;
            let none_copy = none_owner.source(&context)?.to_owned();
            let none_document_source = &none_copy;
            rust_document!(none_document, &["none.rs"], none_document_source, &context);
            let none_document_indices = rust_semantic::indices(&none_document)?;
            assert!(none_document_indices.is_empty());
            assert!(matches!(
                encoded(
                    &none_document,
                    &none_document_indices,
                    &none_owner,
                    &none_raw,
                    &context
                ),
                Err(NativeNnError::InvalidInput)
            ));
            let authentic_source = none_owner.source(&context)?;
            rust_document!(authentic, &["none.rs"], authentic_source, &context);
            let authentic_indices = rust_semantic::indices(&authentic)?;
            assert_eq!(
                encoded(
                    &authentic,
                    &authentic_indices,
                    &none_owner,
                    &none_raw,
                    &context
                )?
                .num_rows(),
                0
            );
            Ok(())
        }

        #[tokio::test]
        async fn authentic_empty_and_literal_paths_keep_the_same_schema() -> TestResult {
            let context = Arc::new(original());
            let fixture = Fixture::new(&context)?;
            let temp = TempDir::new()?;
            let owner = capture(fixture.project(0), &["empty.rs"], &context)?;
            let store = raw_store(&temp, "raw.lance", "empty.rs", None).await?;
            let raw = read_indexed_source(&store.snapshot(), "empty.rs", &context).await?;
            let document_source = owner.source(&context)?;
            rust_document!(document, &["empty.rs"], document_source, &context);
            let document_indices = rust_semantic::indices(&document)?;
            let batch = encoded(&document, &document_indices, &owner, &raw, &context)?;
            assert_eq!(batch, RecordBatch::new_empty(native_nn_schema(&model())?));
            persisted_and_queried(
                &temp,
                "corpus-empty.lance",
                &batch,
                lexical(&document, &owner, &raw, &context)?,
                None,
                &context,
            )
            .await?;
            #[cfg(unix)]
            {
                let owner = capture(fixture.project(0), &["literal\\name.rs"], &context)?;
                let store = raw_store(&temp, "literal.lance", "literal\\name.rs", None).await?;
                let raw =
                    read_indexed_source(&store.snapshot(), "literal\\name.rs", &context).await?;
                let document_source = owner.source(&context)?;
                rust_document!(document, &["literal\\name.rs"], document_source, &context);
                let document_indices = rust_semantic::indices(&document)?;
                let batch = encoded(&document, &document_indices, &owner, &raw, &context)?;
                assert_eq!(
                    col::<StringArray>(&batch, "path").value(0),
                    "literal\\name.rs"
                );
                assert_eq!(col::<StringArray>(&batch, "name").value(0), "café");
                persisted_and_queried(
                    &temp,
                    "corpus-literal.lance",
                    &batch,
                    lexical(&document, &owner, &raw, &context)?,
                    None,
                    &context,
                )
                .await?;
            }
            Ok(())
        }

        #[tokio::test]
        async fn physical_mutation_and_original_controls_preserve_real_causes() -> TestResult {
            let context = Arc::new(original());
            let fixture = Fixture::new(&context)?;
            let temp = TempDir::new()?;
            let owner = capture(fixture.project(0), &["src", "answer.rs"], &context)?;
            let store = raw_store(&temp, "raw.lance", "src/answer.rs", None).await?;
            let raw = read_indexed_source(&store.snapshot(), "src/answer.rs", &context).await?;
            let document_source = owner.source(&context)?;
            rust_document!(document, &["src", "answer.rs"], document_source, &context);
            let document_indices = rust_semantic::indices(&document)?;
            fs::write(
                fixture.projects[0].root.join("src/answer.rs"),
                BODY.replace("x + 1", "x + 2"),
            )?;
            let error = encoded(&document, &document_indices, &owner, &raw, &context).unwrap_err();
            assert!(
                matches!(&error, NativeNnError::PhysicalSource(e) if matches!(e.as_ref(), NativeNnPhysicalSourceError::Operation(cause) if cause.kind() == io::ErrorKind::InvalidData))
            );
            assert!(error.source().unwrap().source().unwrap().is::<io::Error>());
            context.cancellation().cancel();
            let error = encoded(&document, &document_indices, &owner, &raw, &context).unwrap_err();
            assert!(
                matches!(&error, NativeNnError::Operation(cause) if cause.kind() == io::ErrorKind::Interrupted)
            );
            assert!(error.source().unwrap().is::<io::Error>());
            let expired = Arc::new(SnapshotContext::new(
                Instant::now() - Duration::from_secs(1),
                Cancellation::default(),
            ));
            let error = encoded(&document, &document_indices, &owner, &raw, &expired).unwrap_err();
            assert!(
                matches!(&error, NativeNnError::Operation(cause) if cause.kind() == io::ErrorKind::TimedOut)
            );
            Ok(())
        }
    }

    mod entity_selection {
        use super::*;
        use arrow_array::{Array, FixedSizeBinaryArray, RecordBatch, StringArray, UInt64Array};
        use graphit_ast::{
            indexed_source::{IndexedSourceSnapshot, read_indexed_source, source_schema},
            native_nn::{
                NativeNnError, NativeNnModel, NativeNnSource, NativeNnTsInput, encode_native_nn_ts,
                native_nn_schema,
            },
            native_nn_query::{NativeNnMetric, query_corpus_nn},
        };
        use graphit_data::lance_module_store::LanceModuleStore;

        fn model() -> NativeNnModel<'static> {
            NativeNnModel {
                vector_id: [127; 32],
                dimensions: 3,
                model_id: "selection/fixture",
                revision: "r1",
            }
        }
        async fn raw(
            temp: &TempDir,
            path: &str,
            text: Option<&str>,
            context: &Arc<SnapshotContext>,
        ) -> Result<IndexedSourceSnapshot, Box<dyn Error>> {
            let store = LanceModuleStore::create(
                temp.path().join("raw.lance").to_str().unwrap(),
                source_schema(),
                vec![RecordBatch::try_new(
                    source_schema(),
                    vec![
                        Arc::new(StringArray::from(vec![path])),
                        Arc::new(StringArray::from(vec![text])),
                    ],
                )?],
            )
            .await?;
            Ok(read_indexed_source(&store.snapshot(), path, context).await?)
        }
        fn col<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> &'a T {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref()
                .unwrap()
        }
        fn input<'a, 'doc, 'profile, 's, 'v, 'r>(
            document: &'a TreeSitterSemanticDocument<'doc, 'profile, 's, 'v, 'r>,
            raw: &'a IndexedSourceSnapshot,
            entity_indices: &'a [usize],
            vectors: &'a [Vec<f32>],
            generation: Option<u64>,
        ) -> NativeNnTsInput<'a, 'doc, 'profile, 's, 'v, 'r> {
            NativeNnTsInput {
                document,
                source: NativeNnSource::Indexed(raw),
                entity_indices,
                vectors,
                model_generation: generation,
            }
        }

        #[tokio::test]
        async fn disjoint_native_selections_preserve_full_payload_with_per_batch_generations()
        -> TestResult {
            let context = Arc::new(original());
            let temp = TempDir::new()?;
            let source = "mod outer { fn one() {} fn two() {} fn three() {} }";
            let raw = raw(&temp, "src/batches.rs", Some(source), &context).await?;
            let graphit_ast::source_inspection::IndexedSource::Available(text) =
                raw.source(&context)?
            else {
                panic!("actual source required")
            };
            let document_source = text;
            rust_document!(document, &["src", "batches.rs"], document_source, &context);
            let document_indices = rust_semantic::indices(&document)?;
            assert_eq!(document_indices.len(), 3);
            let vectors = vec![vec![1., 0., 0.], vec![2., 0., 0.], vec![3., 0., 0.]];
            let full = encode_native_nn_ts(
                &model(),
                &[input(
                    &document,
                    &raw,
                    &document_indices[0..3],
                    &vectors,
                    Some(7),
                )],
                &context,
            )?;
            let selected = encode_native_nn_ts(
                &model(),
                &[
                    input(
                        &document,
                        &raw,
                        &document_indices[0..1],
                        &vectors[0..1],
                        Some(7),
                    ),
                    input(
                        &document,
                        &raw,
                        &document_indices[1..2],
                        &vectors[1..2],
                        Some(8),
                    ),
                    input(
                        &document,
                        &raw,
                        &document_indices[2..3],
                        &vectors[2..3],
                        Some(9),
                    ),
                ],
                &context,
            )?;
            let mut columns = full.columns().to_vec();
            columns[full.schema().index_of("model_generation")?] =
                Arc::new(UInt64Array::from(vec![7, 8, 9]));
            let expected = RecordBatch::try_new(full.schema(), columns)?;
            assert_eq!(selected, expected);
            for row in 0..3 {
                assert_eq!(
                    col::<StringArray>(&selected, "uid").value(row),
                    document.entities()?[row].identity().uid().as_str()
                );
                assert_eq!(
                    col::<FixedSizeBinaryArray>(&selected, "full_source_sha256").value(row),
                    digest(document_source)
                );
                assert_eq!(
                    col::<UInt64Array>(&selected, "model_generation").value(row),
                    7 + u64::try_from(row)?
                );
            }
            let graph = graphit_ast::semantic_graph::assemble_graph_ts(
                graphit_ast::semantic_graph::AstGraphTsInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw),
                },
                &context,
            )?
            .into_parts();
            let lexical =
                graphit_ast::entity_search::encode_file(&graph, document_source, &context)?;
            let published = corpus_rows::persist(
                &temp.path().join("corpus.lance"),
                lexical,
                &selected,
                &model(),
                None,
                &context,
            )
            .await?;
            let matches = query_corpus_nn(
                &published,
                &model(),
                &[1., 0., 0.],
                3,
                NativeNnMetric::L2,
                &context,
            )
            .await?;
            assert_eq!(matches.rows(&context)?, std::slice::from_ref(&selected));
            assert_eq!(matches.distances(&context)?, &[vec![0., 1., 4.]]);
            Ok(())
        }

        #[tokio::test]
        async fn strict_selection_bounds_vectors_generation_and_global_duplicates() -> TestResult {
            let context = Arc::new(original());
            let temp = TempDir::new()?;
            let raw = raw(
                &temp,
                "a.rs",
                Some("fn one() {} fn two() {} fn three() {}"),
                &context,
            )
            .await?;
            let graphit_ast::source_inspection::IndexedSource::Available(text) =
                raw.source(&context)?
            else {
                panic!("actual source required")
            };
            let document_source = text;
            rust_document!(document, &["a.rs"], document_source, &context);
            let document_indices = rust_semantic::indices(&document)?;
            let vectors = vec![vec![1.; 3]; 3];
            {
                let empty = encode_native_nn_ts(
                    &model(),
                    &[input(&document, &raw, &document_indices[..0], &[], None)],
                    &context,
                )?;
                assert_eq!(empty, RecordBatch::new_empty(native_nn_schema(&model())?));
            }
            for invalid in [[usize::MAX], [document_indices.len()]] {
                assert!(matches!(
                    encode_native_nn_ts(
                        &model(),
                        &[input(&document, &raw, &invalid, &[], None)],
                        &context
                    ),
                    Err(NativeNnError::InvalidInput)
                ));
            }
            for (values, generation) in [(&vectors[..1], Some(7)), (&vectors[..0], None)] {
                assert!(matches!(
                    encode_native_nn_ts(
                        &model(),
                        &[input(
                            &document,
                            &raw,
                            &document_indices[0..2],
                            values,
                            generation
                        )],
                        &context
                    ),
                    Err(NativeNnError::InvalidInput)
                ));
            }
            for values in [vec![vec![1.]], vec![vec![f32::NAN, 0., 0.]]] {
                assert!(matches!(
                    encode_native_nn_ts(
                        &model(),
                        &[input(
                            &document,
                            &raw,
                            &document_indices[1..2],
                            &values,
                            Some(7)
                        )],
                        &context
                    ),
                    Err(NativeNnError::InvalidVector)
                ));
            }
            assert!(matches!(
                encode_native_nn_ts(
                    &model(),
                    &[
                        input(
                            &document,
                            &raw,
                            &document_indices[0..2],
                            &vectors[..2],
                            Some(7)
                        ),
                        input(
                            &document,
                            &raw,
                            &document_indices[1..3],
                            &vectors[1..],
                            Some(8)
                        )
                    ],
                    &context
                ),
                Err(NativeNnError::DuplicateIdentity)
            ));
            // Empty selection still cannot claim a copied original allocation.
            let copy = text.to_owned();
            let copied_source = &copy;
            rust_document!(copied, &["a.rs"], copied_source, &context);
            let copied_indices = rust_semantic::indices(&copied)?;
            assert!(matches!(
                encode_native_nn_ts(
                    &model(),
                    &[input(&copied, &raw, &copied_indices[0..0], &[], None)],
                    &context
                ),
                Err(NativeNnError::InvalidInput)
            ));
            Ok(())
        }

        #[tokio::test]
        async fn authentic_physical_selection_cannot_bypass_source_with_empty_range() -> TestResult
        {
            let context = Arc::new(original());
            let fixture = Fixture::new(&context)?;
            let temp = TempDir::new()?;
            let owner = capture(fixture.project(0), &["src", "answer.rs"], &context)?;
            let raw = raw(&temp, "src/answer.rs", None, &context).await?;
            let document_source = owner.source(&context)?;
            rust_document!(document, &["src", "answer.rs"], document_source, &context);
            let document_indices = rust_semantic::indices(&document)?;
            let vectors = vec![vec![1.; 3]];
            let input = |document, entity_indices, vectors, generation| NativeNnTsInput {
                document,
                source: NativeNnSource::Physical {
                    capture: &owner,
                    raw: &raw,
                },
                entity_indices,
                vectors,
                model_generation: generation,
            };
            let selected = encode_native_nn_ts(
                &model(),
                &[input(
                    &document,
                    &document_indices[0..1],
                    &vectors,
                    Some(17),
                )],
                &context,
            )?;
            assert_eq!(
                col::<FixedSizeBinaryArray>(&selected, "full_source_sha256").value(0),
                owner.content_sha256()
            );
            assert_eq!(
                col::<UInt64Array>(&selected, "raw_version").value(0),
                raw.version()
            );
            assert_eq!(
                col::<UInt64Array>(&selected, "model_generation").value(0),
                17
            );
            assert_eq!(
                encode_native_nn_ts(
                    &model(),
                    &[input(&document, &document_indices[0..0], &[], None)],
                    &context
                )?,
                RecordBatch::new_empty(native_nn_schema(&model())?)
            );
            let copy = owner.source(&context)?.to_owned();
            let copied_source = &copy;
            rust_document!(copied, &["src", "answer.rs"], copied_source, &context);
            let copied_indices = rust_semantic::indices(&copied)?;
            assert!(matches!(
                encode_native_nn_ts(
                    &model(),
                    &[input(&copied, &copied_indices[0..0], &[], None)],
                    &context
                ),
                Err(NativeNnError::InvalidInput)
            ));
            let foreign = Arc::new(original());
            let wrong_original_source = owner.source(&context)?;
            rust_document!(
                wrong_original,
                &["src", "answer.rs"],
                wrong_original_source,
                &foreign
            );
            let wrong_original_indices = rust_semantic::indices(&wrong_original)?;
            assert!(matches!(
                encode_native_nn_ts(
                    &model(),
                    &[input(
                        &wrong_original,
                        &wrong_original_indices[0..0],
                        &[],
                        None
                    )],
                    &context
                ),
                Err(NativeNnError::RequestMismatch)
            ));
            fs::write(
                fixture.projects[0].root.join("src/answer.rs"),
                BODY.replace("x + 1", "x + 2"),
            )?;
            let error = encode_native_nn_ts(
                &model(),
                &[input(&document, &document_indices[0..0], &[], None)],
                &context,
            )
            .unwrap_err();
            assert!(
                matches!(&error,NativeNnError::PhysicalSource(e) if matches!(e.as_ref(),NativeNnPhysicalSourceError::Operation(cause) if cause.kind()==io::ErrorKind::InvalidData))
            );
            assert!(error.source().unwrap().source().unwrap().is::<io::Error>());
            Ok(())
        }
    }
}
