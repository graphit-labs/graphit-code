use super::*;
use crate as graphit_ast;
use crate::{
    entity_search,
    indexed_source::{read_indexed_source, source_schema},
    native_nn::NativeNnModel,
    native_nn_query::NativeNnMetric,
    source_inspection::IndexedSource,
};
use futures::TryStreamExt;
use graphit_data::{
    lance_branch_store::{col, lit},
    lance_module_store::ModuleSnapshot,
};
mod rust_semantic {
    use crate as graphit_ast;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/rust_semantic.rs"
    ));
}
use graphit_core::file_lock::Cancellation;
use graphit_data::lance_module_store::LanceModuleStore;
use std::{
    error::Error,
    time::{Duration, Instant},
};

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(120),
        Cancellation::default(),
    ))
}
fn model() -> NativeNnModel<'static> {
    NativeNnModel {
        vector_id: [61; 32],
        dimensions: 3,
        model_id: "live-fixture",
        revision: "one",
    }
}
fn raw_batch(files: &[(&str, &str)]) -> RecordBatch {
    RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(
                files.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                files.iter().map(|(_, source)| *source).collect::<Vec<_>>(),
            )),
        ],
    )
    .unwrap()
}
async fn rows(
    raw: &ModuleSnapshot,
    path: &str,
    original: &Arc<SnapshotContext>,
) -> Result<(RecordBatch, RecordBatch), Box<dyn Error>> {
    let capture = read_indexed_source(raw, path, original).await?;
    captured_rows(&capture, path, original)
}
fn captured_rows(
    capture: &crate::indexed_source::IndexedSourceSnapshot,
    path: &str,
    original: &Arc<SnapshotContext>,
) -> Result<(RecordBatch, RecordBatch), Box<dyn Error>> {
    let IndexedSource::Available(text) = capture.source(original)? else {
        panic!("fixture source");
    };
    let components = path.split('/').collect::<Vec<_>>();
    rust_semantic::rust_document!(document, &components, text, original);
    let graph = crate::semantic_graph::assemble_graph_ts(
        crate::semantic_graph::AstGraphTsInput {
            document: &document,
            source: crate::native_nn::NativeNnSource::Indexed(capture),
        },
        original,
    )?
    .into_parts();
    let lexical = entity_search::encode_file(&graph, text, original)?;
    let nn = rust_semantic::encode(capture, &components, &model(), 1, original)?;
    Ok((lexical, nn))
}
async fn branch(
    uri: &std::path::Path,
    schema: Arc<arrow_schema::Schema>,
) -> Result<BranchSnapshot, Box<dyn Error>> {
    LanceModuleStore::create(
        uri.to_str().unwrap(),
        schema.clone(),
        vec![RecordBatch::new_empty(schema)],
    )
    .await?;
    Ok(BranchSnapshot::capture_observed(uri.to_str().unwrap(), None, None).await?)
}
async fn index(mut fts: BranchSnapshot) -> Result<BranchSnapshot, Box<dyn Error>> {
    for name in entity_search::TEXT_COLUMNS {
        fts = fts
            .prepare_code_fts_append(name, &format!("{name}_code_fts"))
            .await?
            .ok_or_else(|| io::Error::other("first append must prepare the FTS index"))?
            .commit()
            .await?;
    }
    for column in ["uid", "data_key", "path"] {
        fts = fts
            .prepare_scalar_index(column, &format!("{column}_lookup"))
            .await?
            .commit()
            .await?;
    }
    Ok(fts)
}

#[tokio::test]
async fn lexical_and_vector_extents_share_last_occupied_line_without_changing_byte_ranges()
-> Result<(), Box<dyn Error>> {
    use crate::{
        native_nn::{NativeNnSource, complete_native_nn, prepare_native_nn_scip},
        semantic_graph::scip_document::{apply_scip_document, assemble_scip_source},
    };
    use ::scip::types::{Document, Occurrence, PositionEncoding};
    let original = original();
    let limits = crate::symbols::IdentityLimits::new(crate::symbols::IdentityLimitsInput {
        source_bytes: 1 << 20,
        path_components: 32,
        path_component_bytes: 4096,
        identity_text_bytes: 4096,
        owner_depth: 64,
        uid_bytes: 32768,
    })?;
    for (text, end) in [
        ("// café Ω\ntype Alias = i32;\n", vec![1, 0, 2, 0]),
        ("// café Ω\r\ntype Alias = i32;\r\n", vec![1, 0, 2, 0]),
        ("// café Ω\ntype Alias = i32;", vec![1, 0, 1, 17]),
    ] {
        let temp = tempfile::tempdir()?;
        let raw = LanceModuleStore::create(
            temp.path().join("raw.lance").to_str().unwrap(),
            source_schema(),
            vec![raw_batch(&[("unicode.rs", text)])],
        )
        .await?;
        let source = read_indexed_source(&raw.snapshot(), "unicode.rs", &original).await?;
        let document = Document {
            relative_path: "unicode.rs".into(),
            language: "rust".into(),
            text: text.into(),
            position_encoding: PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into(),
            symbols: vec![::scip::types::SymbolInformation {
                symbol: "scip-test cargo fixture 1 Alias#".into(),
                display_name: "Qualified Alias".into(),
                ..Default::default()
            }],
            occurrences: vec![Occurrence {
                range: vec![1, 5, 10],
                enclosing_range: end,
                symbol: "scip-test cargo fixture 1 Alias#".into(),
                symbol_roles: 1,
                ..Default::default()
            }],
            ..Default::default()
        };
        let graph = assemble_scip_source(
            &document,
            NativeNnSource::Indexed(&source),
            limits,
            &original,
        )?;
        let graph = apply_scip_document(graph.into_parts(), &document, text, &original)?;
        let lexical = entity_search::encode_file(&graph.parts, text, &original)?;
        let entities = entity_search::decode_entities(&lexical)?;
        assert_eq!(entities.len(), 1);
        assert_eq!(
            entities[0].start_byte,
            Some(text.find("type").unwrap() as u64)
        );
        assert_eq!(entities[0].end_byte, Some(text.len() as u64));
        assert_eq!(entities[0].start_line, Some(2));
        assert_eq!(entities[0].end_line, Some(2));
        let prepared = prepare_native_nn_scip(
            &graph,
            &document,
            NativeNnSource::Indexed(&source),
            limits,
            &original,
        )?;
        let nn = complete_native_nn(
            &prepared,
            &model(),
            Some(1),
            &[vec![0.1, 0.2, 0.3]],
            &original,
        )?;
        // Preserve the exact join; normalizing the producer must suffice.
        assert_eq!(nn.num_rows(), entities.len());
        assert_eq!(entities[0].name, "Qualified Alias");
        let base = crate::corpus::schema();
        let mut fields = base.fields().to_vec();
        fields.extend(
            crate::corpus::vector_fields(&model())?
                .fields()
                .iter()
                .cloned(),
        );
        let schema = Arc::new(arrow_schema::Schema::new_with_metadata(
            fields,
            base.metadata().clone(),
        ));
        let corpus =
            crate::corpus::encode_file(schema, lexical, prepared.clone(), None, &original)?;
        assert_eq!(crate::corpus::projections(&corpus, &original)?, prepared);
        let corpus = crate::corpus::attach_embeddings(&corpus, &nn, &model(), &original)?;
        assert_eq!(crate::corpus::vectors(&corpus, &model(), &original)?, nn);
    }
    Ok(())
}

fn prepared(nn: &RecordBatch) -> Result<RecordBatch, Box<dyn Error>> {
    let schema = crate::native_nn::native_nn_projection_schema();
    let columns = schema
        .fields()
        .iter()
        .map(|field| nn.column_by_name(field.name()).unwrap().clone())
        .collect();
    Ok(RecordBatch::try_new(schema, columns)?)
}
async fn publish(
    pin: &BranchSnapshot,
    path: &str,
    lexical: RecordBatch,
    nn: &RecordBatch,
    previous: Option<&RecordBatch>,
    original: &Arc<SnapshotContext>,
) -> Result<BranchSnapshot, Box<dyn Error>> {
    let batch = crate::corpus::encode_file(
        Arc::new(pin.schema().clone()),
        lexical,
        prepared(nn)?,
        previous,
        original,
    )?;
    Ok(prepare_file(pin, path, batch, original)
        .await?
        .commit()
        .await?)
}
async fn enrich(
    pin: &BranchSnapshot,
    nn: &RecordBatch,
    original: &Arc<SnapshotContext>,
) -> Result<BranchSnapshot, Box<dyn Error>> {
    let pin = pin
        .add_nullable_columns(crate::corpus::vector_fields(&model())?, original)
        .await?;
    match crate::corpus::prepare_embeddings(&pin, nn, &model(), original).await? {
        Some(delta) => Ok(delta.commit().await?),
        None => Ok(pin),
    }
}
async fn all_rows(pin: &BranchSnapshot) -> Result<RecordBatch, Box<dyn Error>> {
    let batches: Vec<RecordBatch> = pin.scan().try_into_stream().await?.try_collect().await?;
    Ok(arrow_select::concat::concat_batches(
        &Arc::new(pin.schema().clone()),
        &batches,
    )?)
}
fn count(
    matches: crate::native_nn_query::NativeNnMatches,
    original: &Arc<SnapshotContext>,
) -> usize {
    matches
        .rows(original)
        .unwrap()
        .iter()
        .map(RecordBatch::num_rows)
        .sum()
}

#[tokio::test]
async fn lexical_files_visible_before_vectors_and_incremental_delta_keeps_intact_rows()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let original = original();
    let a_text = "fn alpha() {}\nfn second() {}";
    let raw = LanceModuleStore::create(
        temp.path().join("raw.lance").to_str().unwrap(),
        source_schema(),
        vec![raw_batch(&[("a.rs", a_text), ("b.rs", "fn bravo() {}")])],
    )
    .await?;
    let (a, a_nn) = rows(&raw.snapshot(), "a.rs", &original).await?;
    let (b, b_nn) = rows(&raw.snapshot(), "b.rs", &original).await?;
    let corpus = branch(&temp.path().join("corpus.lance"), crate::corpus::schema()).await?;
    let empty = entity_search::EntitySearch::from_corpus(corpus.clone(), original.clone())?;
    assert!(empty.query("alpha", 10).await?.is_empty());
    let corpus = publish(&corpus, "a.rs", a.clone(), &a_nn, None, &original).await?;
    let corpus = index(corpus).await?;
    let live = entity_search::EntitySearch::from_corpus(corpus.clone(), original.clone())?;
    assert_eq!(live.query("alpha", 10).await?.len(), 1);
    assert!(live.query("bravo", 10).await?.is_empty());
    assert!(
        live.query_nn(&model(), &[1., 1., 1.], 10, NativeNnMetric::Cosine)
            .await?
            .is_none()
    );
    let corpus = publish(&corpus, "b.rs", b, &b_nn, None, &original).await?;
    assert!(
        live.query("bravo", 10).await?.is_empty(),
        "a retained reader keeps its exact pin"
    );
    let corpus = enrich(&corpus, &a_nn.slice(0, 1), &original).await?;
    let live = entity_search::EntitySearch::from_corpus(corpus.clone(), original.clone())?;
    assert_eq!(live.query("bravo", 10).await?.len(), 1);
    assert_eq!(
        count(
            live.query_nn(&model(), &[1., 1., 1.], 10, NativeNnMetric::Cosine)
                .await?
                .unwrap(),
            &original
        ),
        1
    );
    let corpus = enrich(&corpus, &b_nn, &original).await?;
    let corpus = enrich(&corpus, &a_nn.slice(1, 1), &original).await?;
    let live = entity_search::EntitySearch::from_corpus(corpus.clone(), original.clone())?;
    for metric in [
        NativeNnMetric::L2,
        NativeNnMetric::Cosine,
        NativeNnMetric::Dot,
    ] {
        assert_eq!(
            count(
                live.query_nn(&model(), &[1., 1., 1.], 10, metric)
                    .await?
                    .unwrap(),
                &original
            ),
            3
        );
    }
    let old_a = live.query("alpha", 10).await?.remove(0).entity;
    let next = raw
        .snapshot()
        .prepare_replace(vec![raw_batch(&[
            ("a.rs", a_text),
            ("b.rs", "fn charlie() {}"),
        ])])
        .await?
        .commit()
        .await?;
    let (c, c_nn) = rows(&next.snapshot(), "b.rs", &original).await?;
    let before = all_rows(&corpus).await?;
    let corpus = publish(&corpus, "b.rs", c, &c_nn, Some(&before), &original).await?;
    let live = entity_search::EntitySearch::from_corpus(corpus.clone(), original.clone())?;
    assert!(live.query("bravo", 10).await?.is_empty());
    assert_eq!(live.query("charlie", 10).await?.len(), 1);
    assert_eq!(
        count(
            live.query_nn(&model(), &[1., 1., 1.], 10, NativeNnMetric::Cosine)
                .await?
                .unwrap(),
            &original
        ),
        2
    );
    let kept = live.query("alpha", 10).await?.remove(0).entity;
    assert_eq!((kept.uid, kept.raw_version), (old_a.uid, old_a.raw_version));
    assert!(kept.raw_version < next.snapshot().version());
    assert!(
        crate::corpus::prepare_embeddings(&corpus, &b_nn, &model(), &original)
            .await?
            .is_none(),
        "superseded inference cannot resurrect bravo"
    );
    let delayed = crate::corpus::prepare_embeddings(&corpus, &c_nn, &model(), &original)
        .await?
        .unwrap();
    let corpus = delete_file(&corpus, "b.rs", &original)
        .await?
        .commit()
        .await?;
    assert!(
        delayed.commit().await.is_err(),
        "strict CAS rejects inference prepared before deletion"
    );
    assert!(
        crate::corpus::prepare_embeddings(&corpus, &c_nn, &model(), &original)
            .await?
            .is_none()
    );
    assert_eq!(
        crate::corpus::vectors(&all_rows(&corpus).await?, &model(), &original)?.num_rows(),
        2
    );
    // A new Raw generation with unchanged projection preserves vector values,
    // but updates its fingerprint so old in-flight inference remains ineligible.
    let (a2, a2_nn) = rows(&next.snapshot(), "a.rs", &original).await?;
    let before = all_rows(&corpus).await?;
    let corpus = publish(&corpus, "a.rs", a2, &a2_nn, Some(&before), &original).await?;
    assert_eq!(
        crate::corpus::vectors(&all_rows(&corpus).await?, &model(), &original)?.num_rows(),
        2
    );
    assert!(
        crate::corpus::prepare_embeddings(&corpus, &a_nn, &model(), &original)
            .await?
            .is_none()
    );
    let other = NativeNnModel {
        revision: "different",
        ..model()
    };
    assert!(
        entity_search::EntitySearch::from_corpus(corpus, original.clone())?
            .nn(&other)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn search_does_not_require_source_or_graph_to_advance_in_lockstep()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let original = original();
    let raw = LanceModuleStore::create(
        temp.path().join("raw.lance").to_str().unwrap(),
        source_schema(),
        vec![raw_batch(&[("a.rs", "fn alpha() {}")])],
    )
    .await?;
    let (a, nn) = rows(&raw.snapshot(), "a.rs", &original).await?;
    let corpus = branch(&temp.path().join("corpus.lance"), crate::corpus::schema()).await?;
    let corpus = publish(&corpus, "a.rs", a, &nn, None, &original).await?;
    let corpus = index(enrich(&corpus, &nn, &original).await?).await?;
    // Removing Source does not erase an independently published searchable row.
    let absent = raw
        .snapshot()
        .prepare_replace(vec![RecordBatch::new_empty(source_schema())])
        .await?
        .commit()
        .await?;
    assert!(matches!(
        read_indexed_source(&absent.snapshot(), "a.rs", &original)
            .await?
            .source(&original)?,
        IndexedSource::Missing
    ));
    let search = entity_search::EntitySearch::from_corpus(corpus.clone(), original.clone())?;
    assert_eq!(search.query("alpha", 10).await?.len(), 1);
    assert_eq!(
        count(
            search
                .query_nn(&model(), &[1., 1., 1.], 10, NativeNnMetric::Cosine)
                .await?
                .unwrap(),
            &original
        ),
        1
    );
    let next = delete_file(&corpus, "a.rs", &original)
        .await?
        .commit()
        .await?;
    // Native snapshots keep a retained read valid, while a new read sees deletion.
    assert_eq!(search.query("alpha", 10).await?.len(), 1);
    let current = entity_search::EntitySearch::from_corpus(next, original.clone())?;
    assert!(current.query("alpha", 10).await?.is_empty());
    assert_eq!(
        current
            .rerank_documents(&[("removed", "removed")], 1024)
            .await?,
        [None]
    );
    original.cancellation().cancel();
    assert_eq!(
        current.query("alpha", 10).await.unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    Ok(())
}

#[tokio::test]
async fn scalar_append_creates_catches_up_and_rejects_divergence_and_competing_commit()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let uri = temp.path().join("raw.lance");
    let raw = LanceModuleStore::create(
        uri.to_str().unwrap(),
        source_schema(),
        vec![raw_batch(&[("a.rs", "fn alpha() {}")])],
    )
    .await?;
    let snapshot = BranchSnapshot::capture_observed(uri.to_str().unwrap(), None, None).await?;
    assert_eq!(snapshot.version(), raw.snapshot().version());
    let first = snapshot
        .prepare_scalar_append("path", "path_lookup")
        .await?
        .expect("initial index")
        .commit()
        .await?;
    assert!(
        first
            .prepare_scalar_append("path", "path_lookup")
            .await?
            .is_none()
    );
    assert!(
        first
            .prepare_scalar_append("source", "path_lookup")
            .await
            .is_err()
    );
    let patch = raw_batch(&[("b.rs", "fn bravo() {}")]);
    let next = first
        .prepare_merge(vec![patch], vec!["path".into()])
        .await?
        .commit()
        .await?;
    let next = next
        .prepare_scalar_append("path", "path_lookup")
        .await?
        .expect("new fragment")
        .commit()
        .await?;
    assert!(
        next.prepare_scalar_append("path", "path_lookup")
            .await?
            .is_none()
    );
    let mut lookup = next.scan();
    lookup.filter_expr(col("path").eq(lit("b.rs")));
    assert!(
        lookup
            .explain_plan(false)
            .await?
            .contains("ScalarIndexQuery")
    );
    let one = next.prepare_scalar_index("source", "source_lookup").await?;
    let competing = next
        .prepare_scalar_index("path", "second_path_lookup")
        .await?;
    one.commit().await?;
    assert!(competing.commit().await.is_err());
    let store = graphit_data::lance_branch_store::LanceBranchStore::open(
        uri.to_str().unwrap(),
        source_schema(),
        None,
    )
    .await?;
    let feature = store
        .capture_target(Some("feature"))
        .await?
        .materialize()
        .await?;
    let previous_version = feature.version();
    let feature = feature
        .prepare_merge(
            vec![raw_batch(&[("a.rs", "fn feature() {}")])],
            vec!["path".into()],
        )
        .await?
        .commit()
        .await?;
    let pin = feature
        .module_snapshot()
        .at_version(previous_version)
        .await?;
    let original = original();
    assert_eq!(
        pin.native_metadata(&original)
            .await?
            .manifest()
            .branch
            .as_deref(),
        Some("feature")
    );
    let source = read_indexed_source(&pin, "a.rs", &original).await?;
    assert!(matches!(
        source.source(&original)?,
        IndexedSource::Available("fn alpha() {}")
    ));
    let latest = read_indexed_source(&feature.module_snapshot(), "a.rs", &original).await?;
    assert!(matches!(
        latest.source(&original)?,
        IndexedSource::Available("fn feature() {}")
    ));
    Ok(())
}

#[tokio::test]
async fn rerank_reads_pinned_projections_and_lexical_candidates_without_exact_extents()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let original = original();
    let raw = LanceModuleStore::create(
        temp.path().join("raw.lance").to_str().unwrap(),
        source_schema(),
        vec![raw_batch(&[
            ("a.rs", "fn alpha() {}"),
            ("b.rs", "fn bravo() {}"),
        ])],
    )
    .await?;
    let (a, a_nn) = rows(&raw.snapshot(), "a.rs", &original).await?;
    let (b, _) = rows(&raw.snapshot(), "b.rs", &original).await?;
    let base = branch(&temp.path().join("corpus.lance"), crate::corpus::schema()).await?;
    // A has a parser projection but no embedding yet. B represents an older
    // lexical-only observation without an exact extent, with published text.
    let corpus = publish(&base, "a.rs", a, &a_nn, None, &original).await?;
    let mut columns = b.columns().to_vec();
    columns[b.schema().index_of("extent_status")?] =
        Arc::new(StringArray::from(vec!["Unknown"; b.num_rows()]));
    for name in ["start_byte", "end_byte", "start_line", "end_line"] {
        columns[b.schema().index_of(name)?] =
            Arc::new(arrow_array::UInt64Array::from(vec![None; b.num_rows()]));
    }
    let b = RecordBatch::try_new(b.schema(), columns)?;
    let batch = crate::corpus::encode_file(
        Arc::new(corpus.schema().clone()),
        b,
        RecordBatch::new_empty(crate::native_nn::native_nn_projection_schema()),
        None,
        &original,
    )?;
    let corpus = prepare_file(&corpus, "b.rs", batch.clone(), &original)
        .await?
        .commit()
        .await?;
    let corpus = index(corpus).await?;
    let search = entity_search::EntitySearch::from_corpus(corpus.clone(), original.clone())?;
    let a = search.query("alpha", 10).await?.remove(0).entity;
    let b = search.query("bravo", 10).await?.remove(0).entity;
    assert_ne!(b.extent_status, "Exact");
    assert!(!search.nn_present());
    let identities = [
        (b.uid.as_str(), b.data_key.as_str()),
        (a.uid.as_str(), a.data_key.as_str()),
    ];
    let texts = search.rerank_documents(&identities, 1 << 20).await?;
    assert_eq!(texts[0].as_deref(), Some("fn bravo() {}"));
    assert!(texts[1].as_ref().unwrap().contains("alpha"));
    assert_eq!(
        search
            .rerank_documents(&identities, texts.iter().flatten().map(String::len).sum())
            .await?,
        texts
    );
    assert_eq!(
        search
            .rerank_documents(&identities, 1)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        search
            .rerank_documents(&[identities[0], identities[0]], 1 << 20)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        search
            .rerank_documents(&[("absent", "absent")], 1 << 20)
            .await?,
        [None]
    );

    // Updating the corpus cannot change the text of an already selected page.
    let mut columns = batch.columns().to_vec();
    columns[batch.schema().index_of("content")?] = Arc::new(StringArray::from(vec![
        "new published text";
        batch
            .num_rows()
    ]));
    let next_batch = RecordBatch::try_new(batch.schema(), columns)?;
    let next = prepare_file(&corpus, "b.rs", next_batch, &original)
        .await?
        .commit()
        .await?;
    assert_eq!(search.rerank_documents(&identities, 1 << 20).await?, texts);
    let next = entity_search::EntitySearch::from_corpus(next, original.clone())?;
    assert_eq!(
        next.rerank_documents(&identities[..1], 1 << 20).await?,
        [Some("new published text".to_owned())]
    );
    original.cancellation().cancel();
    assert_eq!(
        next.rerank_documents(&identities, 1 << 20)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    Ok(())
}

#[tokio::test]
async fn rerank_rejects_corrupt_projection_hash_instead_of_using_another_source()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let original = original();
    let raw = LanceModuleStore::create(
        temp.path().join("raw.lance").to_str().unwrap(),
        source_schema(),
        vec![raw_batch(&[("a.rs", "fn alpha() {}")])],
    )
    .await?;
    let (lexical, nn) = rows(&raw.snapshot(), "a.rs", &original).await?;
    let base = branch(&temp.path().join("corpus.lance"), crate::corpus::schema()).await?;
    let corpus = publish(&base, "a.rs", lexical, &nn, None, &original).await?;
    let all = all_rows(&corpus).await?;
    let mut columns = all.columns().to_vec();
    let hashes =
        arrow_array::FixedSizeBinaryArray::try_from_iter((0..all.num_rows()).map(|_| [0u8; 32]))?;
    columns[all.schema().index_of("projection_sha256")?] = Arc::new(hashes);
    let corrupt = RecordBatch::try_new(all.schema(), columns)?;
    let corpus = prepare_file(&corpus, "a.rs", corrupt, &original)
        .await?
        .commit()
        .await?;
    let search = entity_search::EntitySearch::from_corpus(corpus, original.clone())?;
    let entity = entity_search::decode_entities(&all)?.remove(0);
    assert_eq!(
        search
            .rerank_documents(&[(&entity.uid, &entity.data_key)], 1 << 20)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    Ok(())
}

#[tokio::test]
async fn native_version_branch_exposes_inherited_rows_before_its_graph_exists()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let original = original();
    let raw = LanceModuleStore::create(
        temp.path().join("raw.lance").to_str().unwrap(),
        source_schema(),
        vec![raw_batch(&[("a.rs", "fn alpha() {}")])],
    )
    .await?;
    let (a, nn) = rows(&raw.snapshot(), "a.rs", &original).await?;
    let uri = temp.path().join("corpus.lance");
    let base = branch(&uri, crate::corpus::schema()).await?;
    let base = publish(&base, "a.rs", a, &nn, None, &original).await?;
    let base = index(enrich(&base, &nn, &original).await?).await?;
    let store = graphit_data::lance_branch_store::LanceBranchStore::open(
        uri.to_str().unwrap(),
        Arc::new(base.schema().clone()),
        None,
    )
    .await?;
    let inherited = store.capture_target(Some("feature")).await?;
    let search = entity_search::EntitySearch::from_corpus(inherited.clone(), original.clone())?;
    assert_eq!(search.query("alpha", 10).await?.len(), 1);
    assert_eq!(
        count(
            search
                .query_nn(&model(), &[1., 1., 1.], 10, NativeNnMetric::Cosine)
                .await?
                .unwrap(),
            &original
        ),
        1
    );
    let feature = inherited.materialize().await?;
    let feature = delete_file(&feature, "a.rs", &original)
        .await?
        .commit()
        .await?;
    assert!(
        entity_search::EntitySearch::from_corpus(feature, original.clone())?
            .query("alpha", 10)
            .await?
            .is_empty()
    );
    assert_eq!(
        entity_search::EntitySearch::from_corpus(base, original)?
            .query("alpha", 10)
            .await?
            .len(),
        1
    );
    Ok(())
}
