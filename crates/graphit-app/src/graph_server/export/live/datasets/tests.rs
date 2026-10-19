use super::*;
use graphit_ast::{indexed_source::read_indexed_source, source_inspection::IndexedSource};
use graphit_core::file_lock::Cancellation;
use std::{
    error::Error,
    time::{Duration, Instant},
};

#[allow(dead_code)]
mod rust_semantic {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../graphit-ast/tests/support/rust_semantic.rs"
    ));
}
type TestResult<T = ()> = Result<T, Box<dyn Error>>;
fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(120),
        Cancellation::default(),
    ))
}
fn batch(files: &[(&str, Option<&str>)]) -> RecordBatch {
    RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(
                files.iter().map(|file| file.0).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                files.iter().map(|file| file.1).collect::<Vec<_>>(),
            )),
        ],
    )
    .unwrap()
}
async fn open(path: &Path) -> TestResult<BranchSnapshot> {
    Ok(BranchSnapshot::capture_observed(path.to_str().unwrap(), None, None).await?)
}
fn file(path: &str, text: &str, version: u64) -> LiveFileObservation {
    LiveFileObservation {
        path: path.into(),
        source_sha256: Sha256::digest(text.as_bytes()).into(),
        file_revision: 1,
        raw_version: version,
        scip_compiler: None,
    }
}

#[tokio::test]
async fn source_export_uses_published_historical_files_including_empty_and_null() -> TestResult {
    let original = original();
    let dir = tempfile::tempdir()?;
    let uri = dir.path().join("raw.lance");
    LanceModuleStore::create(
        uri.to_str().unwrap(),
        source_schema(),
        vec![batch(&[
            ("a.rs", Some("pub fn original() {}\n")),
            ("empty.rs", Some("")),
            ("private.rs", None),
            ("unpublished.rs", Some("pub fn unpublished() {}\n")),
        ])],
    )
    .await?;
    let old = graphit_data::lance_branch_store::LanceBranchStore::open(
        uri.to_str().unwrap(),
        source_schema(),
        None,
    )
    .await?
    .capture_target(Some("feature"))
    .await?
    .materialize()
    .await?;
    assert_eq!(old.physical_branch(), Some("feature"));
    let current = old
        .prepare_merge(
            vec![batch(&[("a.rs", Some("pub fn changed() {}\n"))])],
            vec!["path".into()],
        )
        .await?
        .commit()
        .await?;
    let files = vec![
        file("a.rs", "pub fn original() {}\n", old.version()),
        file("empty.rs", "", old.version()),
        file("private.rs", "not retained", old.version()),
    ];
    let out = dir.path().join("export-source.lance");
    let version = source(&current, files.clone(), &out, &original).await?;
    let pin = open(&out).await?;
    assert_eq!(version, pin.version());
    let raw = pin.module_snapshot();
    let text = read_indexed_source(&raw, "a.rs", &original).await?;
    assert!(matches!(
        text.source(&original)?,
        IndexedSource::Available("pub fn original() {}\n")
    ));
    let empty = read_indexed_source(&raw, "empty.rs", &original).await?;
    assert!(matches!(
        empty.source(&original)?,
        IndexedSource::Available("")
    ));
    let private = read_indexed_source(&raw, "private.rs", &original).await?;
    assert_eq!(
        private.missing_reason(),
        Some(graphit_ast::indexed_source::MissingSourceReason::TextNotStored)
    );
    let absent = read_indexed_source(&raw, "unpublished.rs", &original).await?;
    assert!(matches!(absent.source(&original)?, IndexedSource::Missing));
    assert_eq!(pin.scan().count_rows().await?, 3);
    let mut corrupt = files;
    corrupt[0].source_sha256 = [0; 32];
    assert!(
        source(
            &current,
            corrupt,
            &dir.path().join("invalid.lance"),
            &original
        )
        .await
        .is_err()
    );
    assert_eq!(
        BranchSnapshot::capture_observed(uri.to_str().unwrap(), Some("feature"), None)
            .await?
            .version(),
        current.version()
    );
    assert_eq!(
        open(&uri).await?.version(),
        old.version(),
        "Main was not changed by the named Source producer or export"
    );
    Ok(())
}

async fn encoded(
    raw: &graphit_data::lance_module_store::ModuleSnapshot,
    path: &str,
    original: &Arc<SnapshotContext>,
) -> TestResult<RecordBatch> {
    let source = read_indexed_source(raw, path, original).await?;
    let IndexedSource::Available(text) = source.source(original)? else {
        return Err("missing fixture source".into());
    };
    let components = [path];
    rust_semantic::rust_document!(document, &components, text, original);
    let graph = graphit_ast::semantic_graph::assemble_graph_ts(
        graphit_ast::semantic_graph::AstGraphTsInput {
            document: &document,
            source: graphit_ast::native_nn::NativeNnSource::Indexed(&source),
        },
        original,
    )?
    .into_parts();
    let lexical = entity_search::encode_file(&graph, text, original)?;
    let model = graphit_ast::native_nn::NativeNnModel {
        vector_id: [7; 32],
        dimensions: 3,
        model_id: "export-fixture",
        revision: "1",
    };
    let inferred = rust_semantic::encode(&source, &components, &model, 1, original)?;
    let schema = graphit_ast::native_nn::native_nn_projection_schema();
    let prepared = RecordBatch::try_new(
        schema.clone(),
        schema
            .fields()
            .iter()
            .map(|field| inferred.column_by_name(field.name()).unwrap().clone())
            .collect(),
    )?;
    let inferred = graphit_ast::native_nn::complete_native_nn(
        &prepared,
        &model,
        None,
        &[vec![-0.0, 0.25, 2.0]],
        original,
    )?;
    let base = corpus::schema();
    let mut fields = base.fields().to_vec();
    fields.extend(corpus::vector_fields(&model)?.fields().iter().cloned());
    let schema = Arc::new(arrow_schema::Schema::new_with_metadata(
        fields,
        base.metadata().clone(),
    ));
    let rows = corpus::encode_file(schema, lexical, prepared, None, original)?;
    Ok(corpus::attach_embeddings(
        &rows, &inferred, &model, original,
    )?)
}

#[tokio::test]
async fn corpus_export_preserves_available_rows_and_vectors_independently_of_graph() -> TestResult {
    let original = original();
    let dir = tempfile::tempdir()?;
    let raw_uri = dir.path().join("raw.lance");
    LanceModuleStore::create(
        raw_uri.to_str().unwrap(),
        source_schema(),
        vec![batch(&[
            ("a.rs", Some("pub fn published_alpha() {}\n")),
            ("b.rs", Some("pub fn unpublished_beta() {}\n")),
        ])],
    )
    .await?;
    let raw = open(&raw_uri).await?.module_snapshot();
    let uri = dir.path().join("live.lance");
    let a = encoded(&raw, "a.rs", &original).await?;
    let b = encoded(&raw, "b.rs", &original).await?;
    LanceModuleStore::create(uri.to_str().unwrap(), a.schema(), vec![a, b]).await?;
    let mut live = open(&uri).await?;
    let physical = lance::Dataset::open(uri.to_str().unwrap()).await?;
    assert_eq!(
        physical.manifest().fragments.len(),
        1,
        "fixture has one mixed physical fragment"
    );
    assert_eq!(physical.manifest().fragments[0].physical_rows, Some(2));
    for column in entity_search::TEXT_COLUMNS {
        if let Some(index) = live
            .prepare_code_fts_append(column, &format!("{column}_code_fts"))
            .await?
        {
            live = index.commit().await?;
        }
    }
    let out = dir.path().join("export-corpus.lance");
    let version = corpus(&live, &out, &original).await?;
    let exported = open(&out).await?;
    assert_eq!(version, exported.version());
    assert_eq!(exported.scan().count_rows().await?, 2);
    assert!(
        entity_search::query(&exported.module_snapshot(), "published_alpha", 5, &original)
            .await?
            .len()
            == 1
    );
    assert!(
        entity_search::query(
            &exported.module_snapshot(),
            "unpublished_beta",
            5,
            &original
        )
        .await?
        .len()
            == 1
    );
    let batches = exported
        .scan()
        .try_into_stream()
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    let rows =
        arrow_select::concat::concat_batches(&Arc::new(exported.schema().clone()), &batches)?;
    assert_eq!(
        rows.column_by_name("model_generation")
            .unwrap()
            .null_count(),
        2
    );
    let vector = rows
        .column_by_name("vector")
        .unwrap()
        .as_any()
        .downcast_ref::<arrow_array::FixedSizeListArray>()
        .unwrap()
        .value(0);
    let values = vector
        .as_any()
        .downcast_ref::<arrow_array::Float32Array>()
        .unwrap();
    assert_eq!(
        values
            .values()
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        [-0.0f32, 0.25, 2.0]
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    );
    // Exported Lance rows do not wait for another corpus's graph publication.
    // Deleted rows still must not leak through physical fragments or indexes.
    let deleted = live
        .prepare_delete_expr(col("path").eq(lit("b.rs")))
        .await?
        .commit()
        .await?;
    assert_eq!(deleted.scan().count_rows().await?, 1);
    let deleted_uri = dir.path().join("after-delete.lance");
    corpus(&deleted, &deleted_uri, &original).await?;
    let physical = lance::Dataset::open(deleted_uri.to_str().unwrap()).await?;
    assert_eq!(
        physical
            .manifest()
            .fragments
            .iter()
            .map(|fragment| fragment.physical_rows.unwrap())
            .sum::<usize>(),
        1
    );
    assert!(
        physical
            .manifest()
            .fragments
            .iter()
            .all(|fragment| fragment.deletion_file.is_none())
    );
    let deleted_export = open(&deleted_uri).await?;
    assert!(
        entity_search::query(
            &deleted_export.module_snapshot(),
            "unpublished_beta",
            5,
            &original
        )
        .await?
        .is_empty()
    );
    assert_eq!(live.scan().count_rows().await?, 2);
    Ok(())
}
