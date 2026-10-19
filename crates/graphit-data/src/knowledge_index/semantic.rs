//! Closed extension of the documentary schema after a real model width is known.
//!
//! Absence is represented by null, never by a fabricated vector. The first
//! eighteen columns retain their documentary ABI and remain readable before NN.
use super::*;
use arrow_array::{FixedSizeListArray, Float32Array};

pub(crate) const DOCUMENT_COLUMNS: [&str; 18] = [
    "slug",
    "title",
    "body",
    "kind",
    "summary",
    "description",
    "summary_truncated",
    "source_path",
    "physical_digest",
    "compilation_digest",
    "recipe",
    "recipe_parameters",
    "updated_at",
    "stale_since",
    "stale_after",
    "resolved_wiki_targets",
    "document_state",
    "reference_projection",
];

pub(crate) const COLUMNS: [&str; 6] = [
    "slug",
    "input_fingerprint",
    "vector_id",
    "projection_recipe",
    "role",
    "vector",
];

pub(crate) fn schema(dimensions: NonZeroU32) -> Result<Arc<Schema>, KnowledgeIndexError> {
    let width = i32::try_from(dimensions.get())
        .map_err(|_| KnowledgeIndexError::InvalidField("embedding_dimensions"))?;
    let mut fields = physical_schema().fields().to_vec();
    fields.extend(
        [
            Field::new("input_fingerprint", DataType::FixedSizeBinary(32), true),
            Field::new("vector_id", DataType::FixedSizeBinary(32), true),
            Field::new("projection_recipe", DataType::Utf8, true),
            Field::new("role", DataType::Utf8, true),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    width,
                ),
                true,
            ),
        ]
        .into_iter()
        .map(Arc::new),
    );
    Ok(Arc::new(Schema::new(fields)))
}

pub(crate) fn dimensions(observed: &Schema) -> Result<Option<NonZeroU32>, KnowledgeIndexError> {
    if observed == physical_schema().as_ref() {
        return Ok(None);
    }
    let sample = schema(NonZeroU32::new(1).expect("positive width"))?;
    if observed == &Schema::new(sample.fields()[..22].to_vec()) {
        return Ok(None);
    }
    let Some(field) = observed.fields().get(22) else {
        return Err(KnowledgeIndexError::InvalidField("schema"));
    };
    let DataType::FixedSizeList(_, width) = field.data_type() else {
        return Err(KnowledgeIndexError::InvalidField("embedding_vector"));
    };
    let width = u32::try_from(*width)
        .ok()
        .and_then(NonZeroU32::new)
        .ok_or(KnowledgeIndexError::InvalidField("embedding_dimensions"))?;
    if observed != schema(width)?.as_ref() {
        return Err(KnowledgeIndexError::InvalidField("schema"));
    }
    Ok(Some(width))
}

/// Check all five fields together before treating a row as an eligible vector.
/// The caller owns authorization and invokes its guard at every row boundary.
pub(crate) fn validate(
    batch: &RecordBatch,
    limits: KnowledgeIndexContentLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    before()?;
    let Some(width) = dimensions(batch.schema().as_ref())? else {
        return before();
    };
    let vectors: &FixedSizeListArray = array(batch, 22)?;
    let recipes: &StringArray = array(batch, 20)?;
    let roles: &StringArray = array(batch, 21)?;
    for row in 0..batch.num_rows() {
        before()?;
        let absent = vectors.is_null(row);
        if !absent && batch.columns()[18..].iter().any(|col| col.is_null(row)) {
            return Err(KnowledgeIndexError::InvalidField("embedding_partial_null"));
        }
        if absent {
            continue;
        }
        if array::<UInt8Array>(batch, 16)?.value(row) != KnowledgeDocumentState::Active as u8 {
            return Err(KnowledgeIndexError::InvalidField("removed_embedding"));
        }
        if recipes.value(row).is_empty()
            || recipes.value(row).len() > limits.max_identifier_bytes.get()
            || roles.value(row) != "Document"
        {
            return Err(KnowledgeIndexError::InvalidField("embedding_identity"));
        }
        let values = vectors.value(row);
        let values = values
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or(KnowledgeIndexError::InvalidField("embedding_vector"))?;
        if values.len() != width.get() as usize
            || values.null_count() != 0
            || values.values().iter().any(|v| !v.is_finite())
        {
            return Err(KnowledgeIndexError::InvalidField("embedding_vector"));
        }
    }
    before()
}

pub(crate) fn align(
    batch: &RecordBatch,
    target: &Schema,
) -> Result<RecordBatch, KnowledgeIndexError> {
    dimensions(batch.schema().as_ref())?;
    dimensions(target)?;
    if batch.schema().as_ref() == target {
        return Ok(batch.clone());
    }
    let mut columns = batch.columns()[..18].to_vec();
    if target.fields().len() > 18 {
        columns.extend(
            target.fields()[18..]
                .iter()
                .map(|field| arrow_array::new_null_array(field.data_type(), batch.num_rows())),
        );
    }
    Ok(RecordBatch::try_new(Arc::new(target.clone()), columns)?)
}

/// Preserve a certified vector only when every input-text field is unchanged.
/// Physical digests, dates, references and paths are deliberately independent.
pub(crate) fn replace_document(
    desired: &RecordBatch,
    old: &RecordBatch,
) -> Result<RecordBatch, KnowledgeIndexError> {
    let mut next = align(desired, old.schema().as_ref())?;
    if dimensions(old.schema().as_ref())?.is_some()
        && desired.num_rows() == 1
        && old.num_rows() == 1
        && [1, 2, 3, 4, 5]
            .iter()
            .all(|at| desired.column(*at) == old.column(*at))
        && array::<UInt8Array>(desired, 16)?.value(0) == KnowledgeDocumentState::Active as u8
        && array::<UInt8Array>(old, 16)?.value(0) == KnowledgeDocumentState::Active as u8
    {
        let mut columns = next.columns().to_vec();
        columns[18..].clone_from_slice(&old.columns()[18..]);
        next = RecordBatch::try_new(old.schema(), columns)?;
    }
    Ok(next)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::knowledge_document_projection::{
        self as projection, AuthoredPageFields, AuthoredParameters,
        KnowledgeDocumentProjectionLimits, RecipeFields,
    };
    use crate::lance_module_store::LanceModuleStore;
    use graphit_core::file_lock::Cancellation;

    fn n(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap()
    }
    fn limits() -> KnowledgeIndexContentLimits {
        KnowledgeIndexContentLimits {
            max_identifier_bytes: n(256),
        }
    }
    pub(crate) fn document(original: &SnapshotContext, body: &str) -> RecordBatch {
        let fields = [AuthoredPageFields {
            slug: Some("page"),
            title: Some("Title"),
            body: Some(body),
            kind: Some("page"),
            summary: Some("Summary"),
            description: None,
            summary_truncated: false,
            source_path: "docs/page.md",
            physical_digest: Some(PhysicalSourceDigest32::from_bytes([1; 32])),
            compilation_digest: CompilationDigest32::from_bytes([2; 32]),
            recipe: RecipeFields {
                id: AUTHORED_RECIPE_ID,
                parameters: Some(AuthoredParameters {
                    fallback_title: "page",
                    kind_on_absent: "page",
                    max_summary_bytes: n(128),
                }),
            },
            updated_at: None,
            stale_since: None,
            stale_after: None,
            resolved_wiki_targets: &[],
        }];
        let prepared = projection::prepare(
            original,
            &fields,
            &[],
            KnowledgeDocumentProjectionLimits {
                max_documents: n(8),
                max_observations: n(8),
                max_rows: n(64),
                max_field_bytes: n(65536),
                max_text_bytes: n(131072),
            },
        )
        .unwrap();
        super::super::encode(&prepared, original, limits(), || Ok(())).unwrap()
    }
    pub(crate) fn embedded(base: &RecordBatch, values: Vec<Option<f32>>) -> RecordBatch {
        let dimensions = i32::try_from(values.len()).unwrap();
        let schema = schema(NonZeroU32::new(dimensions as u32).unwrap()).unwrap();
        let mut columns = base.columns().to_vec();
        columns.extend([
            Arc::new(
                FixedSizeBinaryArray::try_from_iter([[3u8; 32].as_slice()].into_iter()).unwrap(),
            ) as ArrayRef,
            Arc::new(
                FixedSizeBinaryArray::try_from_iter([[4u8; 32].as_slice()].into_iter()).unwrap(),
            ),
            Arc::new(StringArray::from(vec!["knowledge-document-json-v1"])),
            Arc::new(StringArray::from(vec!["Document"])),
            Arc::new(
                FixedSizeListArray::try_new(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    dimensions,
                    Arc::new(Float32Array::from(values)),
                    None,
                )
                .unwrap(),
            ),
        ]);
        RecordBatch::try_new(schema, columns).unwrap()
    }
    #[test]
    fn documentary_edits_keep_only_vectors_for_unchanged_model_text() {
        let original = SnapshotContext::service(Cancellation::default());
        let base = document(&original, "original");
        let stored = embedded(&base, vec![Some(1.), Some(0.)]);
        validate(&stored, limits(), || Ok(())).unwrap();
        assert_eq!(replace_document(&base, &stored).unwrap(), stored);
        let changed = document(&original, "changed");
        let replaced = replace_document(&changed, &stored).unwrap();
        assert!(
            replaced.columns()[18..]
                .iter()
                .all(|column| column.is_null(0))
        );
        assert_eq!(
            replaced.project(&(0..18).collect::<Vec<_>>()).unwrap(),
            changed
        );
        // A physical-path/digest change does not alter the actual model input.
        let mut columns = base.columns().to_vec();
        columns[7] = Arc::new(StringArray::from(vec!["docs/renamed.md"]));
        let renamed = RecordBatch::try_new(physical_schema(), columns).unwrap();
        assert!(
            !replace_document(&renamed, &stored)
                .unwrap()
                .column(22)
                .is_null(0)
        );
    }
    #[test]
    fn null_vectors_are_pending_but_present_vectors_require_complete_finite_identity() {
        let original = SnapshotContext::service(Cancellation::default());
        let base = document(&original, "body");
        let target = schema(NonZeroU32::new(2).unwrap()).unwrap();
        let pending = align(&base, target.as_ref()).unwrap();
        validate(&pending, limits(), || Ok(())).unwrap();
        for values in [vec![Some(f32::NAN), Some(0.)], vec![Some(1.), None]] {
            assert!(validate(&embedded(&base, values), limits(), || Ok(())).is_err());
        }
        let valid = embedded(&base, vec![Some(1.), Some(0.)]);
        let mut columns = valid.columns().to_vec();
        columns[18] = arrow_array::new_null_array(&DataType::FixedSizeBinary(32), 1);
        assert!(
            validate(
                &RecordBatch::try_new(target, columns).unwrap(),
                limits(),
                || Ok(())
            )
            .is_err()
        );
        assert!(
            validate(
                &valid,
                KnowledgeIndexContentLimits {
                    max_identifier_bytes: n(2)
                },
                || Ok(())
            )
            .is_err()
        );
        let cancel = Cancellation::default();
        let request = SnapshotContext::service(cancel.clone());
        cancel.cancel();
        assert!(
            validate(&valid, limits(), || request
                .check()
                .map_err(KnowledgeIndexError::from))
            .is_err()
        );
    }
    #[tokio::test]
    async fn actual_single_dataset_keeps_text_and_old_pin_through_nullable_evolution() {
        let original = Arc::new(SnapshotContext::service(Cancellation::default()));
        let base = document(&original, "indexable before Models");
        let temporary = tempfile::tempdir().unwrap();
        let uri = temporary
            .path()
            .join("index.lance")
            .to_str()
            .unwrap()
            .to_owned();
        LanceModuleStore::create(&uri, physical_schema(), vec![base.clone()])
            .await
            .unwrap();
        let old = lance_branch::BranchSnapshot::capture_observed(&uri, None, None)
            .await
            .unwrap();
        let read = KnowledgeIndexReadLimits {
            content: limits(),
            native: NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap(),
            batch_rows: NonZeroU32::new(1).unwrap(),
        };
        let source_generation = generation(&uri, &old);
        let lexical = crate::knowledge_search::search_knowledge_snapshot_guarded(
            &original,
            &old.module_snapshot(),
            &source_generation,
            crate::knowledge_search::KnowledgeLexicalQuery {
                text: "Models",
                top_k: 10,
                page_size: n(2),
                cursor: None,
                caller_binding: "single-corpus",
                preview: false,
            },
            crate::knowledge_search::KnowledgeSearchLimits {
                read,
                max_query_bytes: n(256),
                max_output_bytes: n(65536),
            },
            || Ok(()),
            |_, _, _| unreachable!("preview disabled"),
        )
        .await
        .unwrap();
        assert_eq!(
            lexical.hits.len(),
            1,
            "FTS is usable before a model width exists"
        );
        let target = schema(NonZeroU32::new(2).unwrap()).unwrap();
        let extended = old
            .add_nullable_columns(
                Arc::new(Schema::new(target.fields()[18..].to_vec())),
                &original,
            )
            .await
            .unwrap();
        assert_eq!(dimensions(old.schema()).unwrap(), None);
        assert_eq!(dimensions(extended.schema()).unwrap().unwrap().get(), 2);
        let current = embedded(&base, vec![Some(1.), Some(0.)]);
        let next = extended
            .prepare_merge(vec![current], vec!["slug".into()])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        assert_eq!(next.schema(), target.as_ref());
        let params = lance::index::vector::VectorIndexParams::ivf_flat(
            1,
            lance_linalg::distance::DistanceType::Cosine,
        );
        let pending = next
            .prepare_vector_index("vector", "knowledge_document_vector", &params, &original)
            .await
            .unwrap()
            .expect("first vector index");
        let indexed = pending.commit().await.unwrap();
        assert!(
            indexed
                .prepare_vector_index("vector", "knowledge_document_vector", &params, &original)
                .await
                .unwrap()
                .is_none(),
            "an unchanged corpus does not rebuild its native index"
        );
        let mut nearest = indexed.scan();
        nearest.project(&["slug"]).unwrap();
        nearest
            .nearest("vector", &Float32Array::from(vec![1., 0.]), 1)
            .unwrap()
            .use_index(true)
            .distance_metric(lance_linalg::distance::DistanceType::Cosine);
        use futures::TryStreamExt;
        let hits = nearest
            .try_into_stream()
            .await
            .unwrap()
            .try_collect::<Vec<_>>()
            .await
            .unwrap();
        assert_eq!(
            hits[0]
                .column_by_name("slug")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0),
            "page"
        );
        let mut source = next.scan();
        source.project(&["body"]).unwrap();
        let rows = source
            .try_into_stream()
            .await
            .unwrap()
            .try_collect::<Vec<_>>()
            .await
            .unwrap();
        assert_eq!(
            rows[0]
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0),
            "indexable before Models"
        );
        let old_rows = old
            .scan()
            .try_into_stream()
            .await
            .unwrap()
            .try_collect::<Vec<_>>()
            .await
            .unwrap();
        assert_eq!(old_rows[0], base);
        // A different real model width withdraws the old vector/index, keeps
        // Source, and admits a new native index with the same logical name.
        let dropped = indexed.drop_vector("vector", &original).await.unwrap();
        assert_eq!(dimensions(dropped.schema()).unwrap(), None);
        let wider_schema = schema(NonZeroU32::new(3).unwrap()).unwrap();
        let wider = dropped
            .add_nullable_columns(
                Arc::new(Schema::new(vec![wider_schema.fields()[22].clone()])),
                &original,
            )
            .await
            .unwrap();
        let wider = wider
            .prepare_merge(
                vec![embedded(&base, vec![Some(1.), Some(0.), Some(0.)])],
                vec!["slug".into()],
            )
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        let wider = wider
            .prepare_vector_index("vector", "knowledge_document_vector", &params, &original)
            .await
            .unwrap()
            .expect("new width needs its own native index")
            .commit()
            .await
            .unwrap();
        assert_eq!(dimensions(wider.schema()).unwrap().unwrap().get(), 3);
        let mut retained = indexed.scan();
        retained
            .nearest("vector", &Float32Array::from(vec![1., 0.]), 1)
            .unwrap()
            .use_index(true)
            .distance_metric(lance_linalg::distance::DistanceType::Cosine);
        let retained = retained
            .try_into_stream()
            .await
            .unwrap()
            .try_collect::<Vec<_>>()
            .await
            .unwrap();
        assert_eq!(retained.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 1);
    }
}
