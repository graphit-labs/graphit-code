//! A compiler promotion must not discard a compatible inference already in flight.
#[allow(dead_code)]
#[path = "support/corpus_fixture.rs"]
mod corpus_fixture;
#[path = "support/rust_semantic.rs"]
mod rust_semantic;
use arrow_array::{
    Array, ArrayRef, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, RecordBatch,
    StringArray, UInt64Array, new_null_array,
};
use futures::TryStreamExt;
use graphit_ast::{
    corpus,
    indexed_source::{read_indexed_source, source_schema},
    native_nn::NativeNnModel,
    semantic_graph::scip_document::AstGraphIdentityChange,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::LanceModuleStore};
use std::{error::Error, sync::Arc};
type Result = std::result::Result<(), Box<dyn Error>>;

#[tokio::test]
async fn persisted_vector_survives_identity_refinement_without_an_explicit_map() -> Result {
    stored_vector_republication(false, false).await
}

#[tokio::test]
async fn persisted_vector_from_changed_content_is_not_reused_for_a_new_identity() -> Result {
    stored_vector_republication(true, false).await
}

#[tokio::test]
async fn ambiguous_previous_occurrences_do_not_supply_an_arbitrary_vector() -> Result {
    stored_vector_republication(false, true).await
}

#[tokio::test]
async fn persisted_vectors_follow_identical_inputs_across_source_and_identity_changes() -> Result {
    let temp = tempfile::tempdir()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let model = reuse_model();
    let (previous, current) = shifted_inputs(&temp, &model, &original).await?;
    assert_eq!(corpus::vectors(&previous, &model, &original)?.num_rows(), 4);
    let path = temp.path().join("corpus.lance");
    LanceModuleStore::create(
        path.to_str().ok_or("fixture path")?,
        previous.schema(),
        vec![previous],
    )
    .await?;
    let pin =
        BranchSnapshot::capture_observed(path.to_str().ok_or("fixture path")?, None, None).await?;
    let previous = read_corpus(&pin).await?;
    let published = republish(&current, &previous, &original)?;
    assert_eq!(
        corpus::vectors(&published, &model, &original)?.num_rows(),
        2
    );
    let stream = Box::pin(
        graphit_data::lance_module_store::RecordBatchStreamAdapter::new(
            published.schema(),
            futures::stream::iter([Ok(published)]),
        ),
    );
    pin.prepare_replace_paths_stream(
        &["a.rs".into()],
        stream,
        vec!["uid".into(), "data_key".into()],
    )
    .await?
    .commit()
    .await?;
    let reopened =
        BranchSnapshot::capture_observed(path.to_str().ok_or("fixture path")?, None, None).await?;
    let after = read_corpus(&reopened).await?;
    assert_eq!(corpus::vectors(&after, &model, &original)?.num_rows(), 2);
    for name in [
        "entity_key",
        "uid",
        "source_sha256",
        "raw_version",
        "full_source_sha256",
        "extent",
        "name_anchor",
        "input_fingerprint",
    ] {
        assert_eq!(
            after.column_by_name(name).unwrap().to_data(),
            current.column_by_name(name).unwrap().to_data(),
            "reused outputs must carry the current {name}"
        );
    }
    for row in 0..after.num_rows() {
        let name = strings(&after, "name").value(row);
        let old = (0..previous.num_rows())
            .find(|&old| strings(&previous, "name").value(old) == name)
            .unwrap();
        assert_ne!(
            strings(&previous, "uid").value(old),
            strings(&after, "uid").value(row)
        );
        for field in ["full_source_sha256", "extent", "input_fingerprint"] {
            assert_ne!(
                previous
                    .column_by_name(field)
                    .unwrap()
                    .slice(old, 1)
                    .to_data(),
                after.column_by_name(field).unwrap().slice(row, 1).to_data(),
                "fixture must advance {field} for {name}"
            );
        }
        if name == "row" || name == "namespace" {
            assert_eq!(
                strings(&previous, "projection").value(old),
                strings(&after, "projection").value(row)
            );
            assert_eq!(vector_bits(&previous, old), vector_bits(&after, row));
            for field in ["vector_id", "model_generation"] {
                assert_eq!(
                    previous
                        .column_by_name(field)
                        .unwrap()
                        .slice(old, 1)
                        .to_data(),
                    after.column_by_name(field).unwrap().slice(row, 1).to_data()
                );
            }
        } else {
            assert!(name == "rows" || name == "replace");
            assert_ne!(
                strings(&previous, "projection").value(old),
                strings(&after, "projection").value(row)
            );
            for field in ["vector", "vector_id", "model_generation"] {
                assert!(after.column_by_name(field).unwrap().is_null(row));
            }
        }
    }
    let retained = read_corpus(&pin).await?;
    assert_eq!(
        retained, previous,
        "publication must not mutate the old SDK pin"
    );
    Ok(())
}

#[tokio::test]
async fn shifted_ambiguous_model_inputs_do_not_supply_an_arbitrary_vector() -> Result {
    let temp = tempfile::tempdir()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let model = reuse_model();
    let (previous, current) = shifted_inputs(&temp, &model, &original).await?;
    let row = (0..previous.num_rows())
        .find(|&row| strings(&previous, "name").value(row) == "row")
        .unwrap();
    let duplicate = corpus::promote_identities(
        &previous.slice(row, 1),
        &[AstGraphIdentityChange {
            label: "Function".into(),
            data_key: strings(&previous, "data_key").value(row).into(),
            start_byte: 0,
            end_byte: 1,
            previous_uid: strings(&previous, "uid").value(row).into(),
            uid: "duplicate-input-identity".into(),
            previous_key: strings(&previous, "entity_key").value(row).into(),
            key: "duplicate-input-key".into(),
        }],
        &original,
    )?;
    let previous =
        arrow_select::concat::concat_batches(&previous.schema(), &[previous, duplicate])?;
    let published = republish(&current, &previous, &original)?;
    let vectors = corpus::vectors(&published, &model, &original)?;
    assert_eq!(vectors.num_rows(), 1);
    assert_eq!(strings(&vectors, "name").value(0), "namespace");
    Ok(())
}

#[tokio::test]
async fn shifted_input_reuse_requires_a_valid_old_certificate_and_the_same_model() -> Result {
    let temp = tempfile::tempdir()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let model = reuse_model();
    let (previous, current) = shifted_inputs(&temp, &model, &original).await?;
    let count = previous.num_rows();
    let arrow_schema::DataType::FixedSizeList(field, width) =
        previous.column_by_name("vector").unwrap().data_type()
    else {
        panic!("fixture vector type");
    };
    let invalid_vector = Arc::new(FixedSizeListArray::try_new(
        field.clone(),
        *width,
        Arc::new(Float32Array::from(vec![f32::NAN; count * 2])),
        None,
    )?) as ArrayRef;
    for (field, values) in [
        (
            "input_fingerprint",
            Arc::new(FixedSizeBinaryArray::try_from_iter(
                (0..count).map(|_| [0u8; 32]),
            )?) as ArrayRef,
        ),
        (
            "projection_sha256",
            Arc::new(FixedSizeBinaryArray::try_from_iter(
                (0..count).map(|_| [0u8; 32]),
            )?) as ArrayRef,
        ),
        (
            "vector_id",
            Arc::new(FixedSizeBinaryArray::try_from_iter(
                (0..count).map(|_| [0u8; 32]),
            )?) as ArrayRef,
        ),
        (
            "role",
            Arc::new(StringArray::from(vec!["CodeQuery"; count])) as ArrayRef,
        ),
        (
            "projection_recipe",
            Arc::new(StringArray::from(vec!["other-recipe"; count])) as ArrayRef,
        ),
        ("vector", invalid_vector),
    ] {
        let mut columns = previous.columns().to_vec();
        columns[previous.schema().index_of(field)?] = values;
        let invalid = RecordBatch::try_new(previous.schema(), columns)?;
        assert!(
            republish(&current, &invalid, &original).is_err(),
            "must reject invalid {field}"
        );
    }
    for different in [
        NativeNnModel {
            revision: "two",
            ..reuse_model()
        },
        NativeNnModel {
            model_id: "different-model",
            ..reuse_model()
        },
        NativeNnModel {
            vector_id: [71; 32],
            ..reuse_model()
        },
        NativeNnModel {
            dimensions: 3,
            ..reuse_model()
        },
    ] {
        let schema = corpus_fixture::schema(&different)?;
        let mut columns = current.columns().to_vec();
        columns[schema.index_of("vector")?] =
            new_null_array(schema.field_with_name("vector")?.data_type(), count);
        let current = RecordBatch::try_new(schema, columns)?;
        let published = republish(&current, &previous, &original)?;
        assert_eq!(
            corpus::vectors(&published, &different, &original)?.num_rows(),
            0
        );
        assert_eq!(
            published.column_by_name("vector").unwrap().null_count(),
            count
        );
    }
    // Remote model outputs legitimately have no local generation. Preserve
    // that absence rather than fabricating the current local generation.
    let mut columns = previous.columns().to_vec();
    let generation = previous.schema().index_of("model_generation")?;
    columns[generation] = new_null_array(columns[generation].data_type(), count);
    let remote = RecordBatch::try_new(previous.schema(), columns)?;
    let published = republish(&current, &remote, &original)?;
    assert_eq!(
        corpus::vectors(&published, &model, &original)?.num_rows(),
        2
    );
    assert_eq!(
        published
            .column_by_name("model_generation")
            .unwrap()
            .null_count(),
        count
    );
    Ok(())
}

fn reuse_model() -> NativeNnModel<'static> {
    NativeNnModel {
        vector_id: [37; 32],
        dimensions: 2,
        model_id: "corpus-input-reuse-fixture",
        revision: "one",
    }
}

async fn shifted_inputs(
    temp: &tempfile::TempDir,
    model: &NativeNnModel<'_>,
    original: &Arc<SnapshotContext>,
) -> std::result::Result<(RecordBatch, RecordBatch), Box<dyn Error>> {
    let mut observations = Vec::new();
    for (name, source, generation) in [
        (
            "before",
            "fn rows() -> u8 { 1 }\nfn replace() -> u8 { 2 }\nfn row() -> u8 { 3 }\nfn namespace() -> u8 { 4 }",
            7,
        ),
        (
            "current",
            "// unrelated Source prefix\nfn rows() -> u8 { 10 }\nfn replace() -> u8 { 20 }\nfn row() -> u8 { 3 }\nfn namespace() -> u8 { 4 }",
            99,
        ),
    ] {
        let path = temp.path().join(format!("{name}.lance"));
        let batch = RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(vec!["a.rs"])),
                Arc::new(StringArray::from(vec![source])),
            ],
        )?;
        let raw = LanceModuleStore::create(
            path.to_str().ok_or("fixture path")?,
            source_schema(),
            vec![batch.clone()],
        )
        .await?;
        let raw = if name == "current" {
            raw.snapshot()
                .prepare_overwrite(source_schema(), vec![batch])
                .await?
                .commit()
                .await?
        } else {
            raw
        };
        let raw = read_indexed_source(&raw.snapshot(), "a.rs", original).await?;
        observations.push(corpus_fixture::encode(
            &raw,
            &["a.rs"],
            model,
            generation,
            original,
        )?);
    }
    let previous = &observations[0];
    assert_eq!(previous.num_rows(), 4);
    let prepared = corpus::projections(previous, original)?;
    let values = (0..prepared.num_rows())
        .map(|row| vec![row as f32 + 0.25, -0.0])
        .collect::<Vec<_>>();
    let inferred =
        graphit_ast::native_nn::complete_native_nn(&prepared, model, Some(7), &values, original)?;
    let previous = corpus::attach_embeddings(previous, &inferred, model, original)?;
    let changes = (0..previous.num_rows())
        .map(|row| AstGraphIdentityChange {
            label: "Function".into(),
            data_key: strings(&previous, "data_key").value(row).into(),
            start_byte: 0,
            end_byte: 1,
            previous_uid: strings(&previous, "uid").value(row).into(),
            uid: format!("old-input-identity-{row}"),
            previous_key: strings(&previous, "entity_key").value(row).into(),
            key: format!("old-input-key-{row}"),
        })
        .collect::<Vec<_>>();
    Ok((
        corpus::promote_identities(&previous, &changes, original)?,
        observations.remove(1),
    ))
}

fn republish(
    current: &RecordBatch,
    previous: &RecordBatch,
    original: &SnapshotContext,
) -> std::result::Result<RecordBatch, Box<dyn Error>> {
    let schema = graphit_ast::entity_search::schema();
    let lexical = RecordBatch::try_new(
        schema.clone(),
        schema
            .fields()
            .iter()
            .map(|field| current.column_by_name(field.name()).unwrap().clone())
            .collect(),
    )?;
    Ok(corpus::encode_file(
        current.schema(),
        lexical,
        corpus::projections(current, original)?,
        Some(previous),
        original,
    )?)
}

async fn read_corpus(pin: &BranchSnapshot) -> std::result::Result<RecordBatch, Box<dyn Error>> {
    let batches = pin
        .scan()
        .try_into_stream()
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    Ok(arrow_select::concat::concat_batches(
        &Arc::new(pin.schema().clone()),
        &batches,
    )?)
}

fn vector_bits(batch: &RecordBatch, row: usize) -> Vec<u32> {
    let vector = batch
        .column_by_name("vector")
        .unwrap()
        .as_any()
        .downcast_ref::<FixedSizeListArray>()
        .unwrap()
        .value(row);
    vector
        .as_any()
        .downcast_ref::<Float32Array>()
        .unwrap()
        .values()
        .iter()
        .map(|value| value.to_bits())
        .collect()
}

async fn stored_vector_republication(changed: bool, ambiguous: bool) -> Result {
    let temp = tempfile::tempdir()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let model = NativeNnModel {
        vector_id: [37; 32],
        dimensions: 2,
        model_id: "corpus-promotion-fixture",
        revision: "one",
    };
    let mut observations = Vec::new();
    for (name, source) in [
        ("before", "fn ação() { let x = 1; }"),
        (
            "current",
            if changed {
                "fn ação() { let x = 2; }"
            } else {
                "fn ação() { let x = 1; }"
            },
        ),
    ] {
        let path = temp.path().join(format!("{name}.lance"));
        let source_batch = RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(vec!["a.rs"])),
                Arc::new(StringArray::from(vec![source])),
            ],
        )?;
        let raw = LanceModuleStore::create(
            path.to_str().ok_or("fixture path")?,
            source_schema(),
            vec![source_batch.clone()],
        )
        .await?;
        let raw = if name == "current" {
            raw.snapshot()
                .prepare_overwrite(source_schema(), vec![source_batch])
                .await?
                .commit()
                .await?
        } else {
            raw
        };
        let raw = read_indexed_source(&raw.snapshot(), "a.rs", &original).await?;
        observations.push(corpus_fixture::encode(
            &raw,
            &["a.rs"],
            &model,
            7,
            &original,
        )?);
    }
    let before = &observations[0];
    let raw_version = |batch: &RecordBatch| {
        batch
            .column_by_name("raw_version")
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap()
            .value(0)
    };
    assert_ne!(raw_version(before), raw_version(&observations[1]));
    let vectors = corpus::vectors(before, &model, &original)?;
    assert_eq!(vectors.num_rows(), 1);
    let uid = strings(&vectors, "uid").value(0);
    let data_key = strings(&vectors, "data_key").value(0);
    let row = (0..before.num_rows())
        .find(|&row| {
            strings(before, "uid").value(row) == uid
                && strings(before, "data_key").value(row) == data_key
        })
        .unwrap();
    let change = AstGraphIdentityChange {
        label: "Function".into(),
        data_key: data_key.into(),
        start_byte: 0,
        end_byte: 24,
        previous_uid: uid.into(),
        uid: "opaque-previous-identity".into(),
        previous_key: strings(before, "entity_key").value(row).into(),
        key: "opaque-previous-key".into(),
    };
    let mut before = corpus::promote_identities(before, std::slice::from_ref(&change), &original)?;
    if ambiguous {
        let duplicate = corpus::promote_identities(
            &before.slice(row, 1),
            &[AstGraphIdentityChange {
                previous_uid: change.uid.clone(),
                previous_key: change.key.clone(),
                uid: "another-previous-identity".into(),
                key: "another-previous-key".into(),
                ..change
            }],
            &original,
        )?;
        before =
            arrow_select::concat::concat_batches(&before.schema(), &[before.clone(), duplicate])?;
    }
    let path = temp.path().join("corpus.lance");
    LanceModuleStore::create(
        path.to_str().ok_or("fixture path")?,
        before.schema(),
        vec![before],
    )
    .await?;
    let pin =
        BranchSnapshot::capture_observed(path.to_str().ok_or("fixture path")?, None, None).await?;
    let previous_batches = pin
        .scan()
        .try_into_stream()
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    let previous =
        arrow_select::concat::concat_batches(&Arc::new(pin.schema().clone()), &previous_batches)?;
    let current = &observations[1];
    let lexical_schema = graphit_ast::entity_search::schema();
    let lexical = RecordBatch::try_new(
        lexical_schema.clone(),
        lexical_schema
            .fields()
            .iter()
            .map(|field| current.column_by_name(field.name()).unwrap().clone())
            .collect(),
    )?;
    let published = corpus::encode_file(
        current.schema(),
        lexical,
        corpus::projections(current, &original)?,
        Some(&previous),
        &original,
    )?;
    assert_eq!(
        corpus::vectors(&published, &model, &original)?.num_rows(),
        usize::from(!changed && !ambiguous)
    );
    let stream = Box::pin(
        graphit_data::lance_module_store::RecordBatchStreamAdapter::new(
            published.schema(),
            futures::stream::iter([Ok(published)]),
        ),
    );
    let next = pin
        .prepare_replace_paths_stream(
            &["a.rs".into()],
            stream,
            vec!["uid".into(), "data_key".into()],
        )
        .await?
        .commit()
        .await?;
    let batches = next
        .scan()
        .try_into_stream()
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    let after = arrow_select::concat::concat_batches(&Arc::new(next.schema().clone()), &batches)?;
    assert_eq!(after.num_rows(), current.num_rows());
    assert!(
        !strings(&after, "uid")
            .iter()
            .flatten()
            .any(|id| id == "opaque-previous-identity" || id == "another-previous-identity")
    );
    assert_eq!(
        corpus::vectors(&after, &model, &original)?.num_rows(),
        usize::from(!changed && !ambiguous)
    );
    if !changed && !ambiguous {
        assert_eq!(
            corpus::vectors(&after, &model, &original)?
                .column_by_name("vector")
                .unwrap()
                .to_data(),
            vectors.column_by_name("vector").unwrap().to_data(),
            "identity refinement preserves the persisted values, not merely coverage"
        );
    }
    // Reopen the captured SDK snapshot after publication, rather than merely
    // inspecting the batch materialized before the replacement.
    let old_batches = pin
        .scan()
        .try_into_stream()
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    let old_snapshot =
        arrow_select::concat::concat_batches(&Arc::new(pin.schema().clone()), &old_batches)?;
    assert!(
        strings(&old_snapshot, "uid")
            .iter()
            .flatten()
            .any(|id| id == "opaque-previous-identity")
    );
    assert_eq!(
        corpus::vectors(&old_snapshot, &model, &original)?.num_rows(),
        1 + usize::from(ambiguous)
    );
    assert_eq!(
        corpus::vectors(&old_snapshot, &model, &original)?
            .column_by_name("vector")
            .unwrap()
            .to_data(),
        corpus::vectors(&previous, &model, &original)?
            .column_by_name("vector")
            .unwrap()
            .to_data(),
    );
    Ok(())
}

fn strings<'a>(batch: &'a RecordBatch, name: &str) -> &'a StringArray {
    batch
        .column_by_name(name)
        .unwrap()
        .as_any()
        .downcast_ref()
        .unwrap()
}

#[tokio::test]
async fn inference_finishing_after_promotion_uses_current_identity_and_preserves_vector() -> Result
{
    exercise(false).await
}

#[tokio::test]
async fn inference_from_changed_source_cannot_be_rebound_to_the_promoted_identity() -> Result {
    exercise(true).await
}

async fn exercise(changed_source: bool) -> Result {
    let temp = tempfile::tempdir()?;
    let original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let model = NativeNnModel {
        vector_id: [37; 32],
        dimensions: 2,
        model_id: "corpus-promotion-fixture",
        revision: "one",
    };
    let raw_path = temp.path().join("source.lance");
    let raw = LanceModuleStore::create(
        raw_path.to_str().ok_or("fixture path")?,
        source_schema(),
        vec![RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(vec!["a.rs"])),
                Arc::new(StringArray::from(vec!["fn ação() { let x = 1; }"])),
            ],
        )?],
    )
    .await?;
    let raw = read_indexed_source(&raw.snapshot(), "a.rs", &original).await?;
    let before = corpus_fixture::encode(&raw, &["a.rs"], &model, 7, &original)?;
    let inferred = corpus::vectors(&before, &model, &original)?;
    assert_eq!(inferred.num_rows(), 1);
    let old_uid = strings(&inferred, "uid").value(0);
    let data_key = strings(&inferred, "data_key").value(0);
    let row = (0..before.num_rows())
        .find(|&row| {
            strings(&before, "uid").value(row) == old_uid
                && strings(&before, "data_key").value(row) == data_key
        })
        .unwrap();
    let changes = [AstGraphIdentityChange {
        label: "Function".into(),
        data_key: data_key.into(),
        start_byte: 0,
        end_byte: 24,
        previous_uid: old_uid.into(),
        uid: "semantic-current".into(),
        previous_key: strings(&before, "entity_key").value(row).into(),
        key: "semantic-current-key".into(),
    }];
    let after = corpus::promote_identities(&before, &changes, &original)?;
    let mut columns = after.columns().to_vec();
    for name in ["vector", "vector_id", "model_generation"] {
        let field = after.schema().index_of(name)?;
        columns[field] = new_null_array(columns[field].data_type(), after.num_rows());
    }
    let after = RecordBatch::try_new(after.schema(), columns)?;
    let path = temp.path().join("corpus.lance");
    LanceModuleStore::create(
        path.to_str().ok_or("fixture path")?,
        after.schema(),
        vec![after.clone()],
    )
    .await?;
    let mut pin =
        BranchSnapshot::capture_observed(path.to_str().ok_or("fixture path")?, None, None).await?;
    if changed_source {
        // Publish genuinely changed bytes through the normal Source/parser/corpus APIs.
        let changed_raw = temp.path().join("changed-source.lance");
        let raw = LanceModuleStore::create(
            changed_raw.to_str().ok_or("fixture path")?,
            source_schema(),
            vec![RecordBatch::try_new(
                source_schema(),
                vec![
                    Arc::new(StringArray::from(vec!["a.rs"])),
                    Arc::new(StringArray::from(vec!["fn ação() { let x = 2; }"])),
                ],
            )?],
        )
        .await?;
        let raw = read_indexed_source(&raw.snapshot(), "a.rs", &original).await?;
        let changed = corpus_fixture::encode(&raw, &["a.rs"], &model, 7, &original)?;
        let current_row = (0..changed.num_rows())
            .find(|&row| strings(&changed, "data_key").value(row) == data_key)
            .unwrap();
        let change = AstGraphIdentityChange {
            previous_uid: strings(&changed, "uid").value(current_row).into(),
            previous_key: strings(&changed, "entity_key").value(current_row).into(),
            ..changes[0].clone()
        };
        let changed = corpus::promote_identities(&changed, &[change], &original)?;
        let mut columns = changed.columns().to_vec();
        for name in ["vector", "vector_id", "model_generation"] {
            let field = changed.schema().index_of(name)?;
            columns[field] = new_null_array(columns[field].data_type(), changed.num_rows());
        }
        let changed = RecordBatch::try_new(changed.schema(), columns)?;
        pin = pin
            .prepare_merge(vec![changed], vec!["uid".into(), "data_key".into()])
            .await?
            .commit()
            .await?;
    }
    let prepared = corpus::prepare_embeddings(&pin, &inferred, &model, &original).await?;
    if changed_source {
        assert!(
            prepared.is_none(),
            "stale inference must not attach to changed content"
        );
        return Ok(());
    }
    let pin = prepared
        .ok_or("compatible in-flight vector was discarded")?
        .commit()
        .await?;
    let mut stream = pin.scan().try_into_stream().await?;
    let mut batches = Vec::new();
    while let Some(batch) = stream.try_next().await? {
        batches.push(batch);
    }
    let current = arrow_select::concat::concat_batches(&Arc::new(pin.schema().clone()), &batches)?;
    let vectors = corpus::vectors(&current, &model, &original)?;
    assert_eq!(
        current.num_rows(),
        before.num_rows(),
        "promotion cannot duplicate entities"
    );
    assert_eq!(vectors.num_rows(), 1);
    assert_eq!(strings(&vectors, "uid").value(0), "semantic-current");
    assert_eq!(
        vectors.column_by_name("vector").unwrap().to_data(),
        inferred.column_by_name("vector").unwrap().to_data()
    );
    assert!(
        !strings(&current, "uid")
            .iter()
            .flatten()
            .any(|uid| uid == old_uid)
    );
    Ok(())
}
