use crate as graphit_ast;
use std::{error::Error, sync::Arc};

use arrow_array::{RecordBatch, StringArray};
use graphit_ast::{
    indexed_source::{read_indexed_source, source_schema},
    native_nn::{
        NativeNnError, NativeNnModel, NativeNnSource, NativeNnTsInput, complete_native_nn,
        encode_native_nn_ts, native_nn_projection_schema, prepare_native_nn_ts, validate_native_nn,
        validate_native_nn_projection,
    },
    source_inspection::IndexedSource,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::lance_module_store::LanceModuleStore;
use tempfile::TempDir;

mod rust_semantic {
    use crate as graphit_ast;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/rust_semantic.rs"
    ));
}
use rust_semantic::rust_document;

type TestResult = Result<(), Box<dyn Error>>;

#[tokio::test]
async fn prepared_projections_survive_parser_release_and_persistence_without_model() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let source = "fn café() { let 日本 = 1; }\nfn second() {}";
    let raw = LanceModuleStore::create(
        temp.path().join("source.lance").to_str().unwrap(),
        source_schema(),
        vec![RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(vec!["src/code.rs"])),
                Arc::new(StringArray::from(vec![source])),
            ],
        )?],
    )
    .await?;
    let raw = read_indexed_source(&raw.snapshot(), "src/code.rs", &original).await?;
    let IndexedSource::Available(source) = raw.source(&original)? else {
        panic!("Source required")
    };
    let model = NativeNnModel {
        vector_id: [42; 32],
        dimensions: 2,
        model_id: "codec/prepared",
        revision: "r1",
    };
    let vectors = [vec![0.25, 0.75], vec![0.75, 0.25]];
    let (pending, expected) = {
        rust_document!(document, &["src", "code.rs"], source, &original);
        let indices = rust_semantic::indices(&document)?;
        assert_eq!(indices.len(), vectors.len());
        let pending = prepare_native_nn_ts(
            &document,
            NativeNnSource::Indexed(&raw),
            &indices,
            &original,
        )?;
        let expected = encode_native_nn_ts(
            &model,
            &[NativeNnTsInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: Some(7),
            }],
            &original,
        )?;
        let foreign = Arc::new(SnapshotContext::service(Cancellation::default()));
        assert!(matches!(
            prepare_native_nn_ts(&document, NativeNnSource::Indexed(&raw), &indices, &foreign),
            Err(NativeNnError::RequestMismatch)
        ));
        (pending, expected)
    };
    assert_eq!(pending.schema(), native_nn_projection_schema());
    assert!(pending.column_by_name("vector").is_none());
    assert!(pending.column_by_name("model_generation").is_none());
    assert!(
        !pending
            .schema()
            .metadata()
            .contains_key("graphit.ast.nn.model_id")
    );
    validate_native_nn_projection(&pending, &original)?;
    assert!(matches!(
        validate_native_nn(&pending, &model, &original),
        Err(NativeNnError::SchemaMismatch)
    ));
    let persisted = LanceModuleStore::create(
        temp.path().join("pending.lance").to_str().unwrap(),
        pending.schema(),
        vec![pending.clone()],
    )
    .await?;
    use futures::TryStreamExt;
    let pending = persisted
        .snapshot()
        .scan()
        .try_into_stream()
        .await?
        .try_next()
        .await?
        .unwrap();
    let completed = complete_native_nn(&pending, &model, Some(7), &vectors, &original)?;
    assert_eq!(completed, expected);
    for (name, column) in pending.schema().fields().iter().zip(pending.columns()) {
        assert!(
            Arc::ptr_eq(column, completed.column_by_name(name.name()).unwrap()),
            "metadata arrays reused"
        );
    }
    assert!(matches!(
        complete_native_nn(&pending, &model, None, &vectors[..1], &original),
        Err(NativeNnError::InvalidVector)
    ));
    assert!(matches!(
        complete_native_nn(
            &pending,
            &model,
            None,
            &[vec![f32::NAN, 1.], vec![0., 1.]],
            &original
        ),
        Err(NativeNnError::InvalidVector)
    ));
    let mut corrupt_columns = pending.columns().to_vec();
    corrupt_columns[pending.schema().index_of("projection")?] =
        Arc::new(StringArray::from(vec!["wrong", "wrong"]));
    let corrupt = RecordBatch::try_new(pending.schema(), corrupt_columns)?;
    assert!(matches!(
        complete_native_nn(&corrupt, &model, None, &vectors, &original),
        Err(NativeNnError::InvalidRow)
    ));
    let other_model = NativeNnModel {
        vector_id: [43; 32],
        model_id: "codec/other",
        revision: "r2",
        ..model
    };
    let other = complete_native_nn(&pending, &other_model, None, &vectors, &original)?;
    assert!(matches!(
        validate_native_nn(&other, &model, &original),
        Err(NativeNnError::SchemaMismatch)
    ));
    assert_eq!(
        other.schema().metadata()["graphit.ast.nn.model_id"],
        "codec/other"
    );
    let invalid_model = NativeNnModel {
        dimensions: 0,
        ..model
    };
    assert!(matches!(
        complete_native_nn(&pending, &invalid_model, None, &vectors, &original),
        Err(NativeNnError::InvalidModel)
    ));
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let cancelled = Arc::new(SnapshotContext::service(cancelled));
    assert!(matches!(
        complete_native_nn(&pending, &model, None, &vectors, &cancelled),
        Err(NativeNnError::Operation(_))
    ));
    Ok(())
}
