//! Compact persisted NN identities for selected publication; vectors remain on disk.
use super::*;

pub(super) async fn selected_row(
    pin: &BranchSnapshot,
    slug: &str,
    before: &mut impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<Option<RecordBatch>, KnowledgeEmbeddingError> {
    use lance::deps::datafusion::logical_expr::{col, lit};
    let mut scanner = pin.scan();
    scanner.filter_expr(col("slug").eq(lit(slug)));
    let mut stream = embedding_guarded(scanner.try_into_stream(), &mut *before).await??;
    let mut found = None;
    while let Some(batch) = embedding_guarded(stream.try_next(), &mut *before).await?? {
        if batch.num_rows() == 0 {
            continue;
        }
        if batch.num_rows() != 1 || found.is_some() {
            return Err(KnowledgeEmbeddingError::InvalidStored(
                "duplicate source slug",
            ));
        }
        found = Some(batch);
    }
    before()?;
    Ok(found)
}

pub(super) async fn decode(
    pin: &ManifestCapture,
    original: &Arc<SnapshotContext>,
    selection: &BTreeSet<String>,
    limits: KnowledgeIndexReadLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<(Option<KnowledgeEmbeddingIdentity>, Vec<StoredRow>), KnowledgeEmbeddingError> {
    let mut probe = pin.pin.scan();
    probe.filter("vector IS NOT NULL")?;
    probe.limit(Some(1), None)?;
    let first = scan(pin, original, probe, limits, &mut before).await?;
    let identity = first.first().map(|row| row.identity.clone());
    let mut selected = pin.pin.scan();
    selected.filter_expr(
        lance::deps::datafusion::logical_expr::col("slug")
            .in_list(
                selection
                    .iter()
                    .map(|slug| lance::deps::datafusion::logical_expr::lit(slug.as_str()))
                    .collect(),
                false,
            )
            .and(lance::deps::datafusion::logical_expr::col("vector").is_not_null()),
    );
    let rows = scan(pin, original, selected, limits, &mut before).await?;
    if rows.iter().any(|row| {
        identity
            .as_ref()
            .is_none_or(|identity| &row.identity != identity)
    }) {
        return Err(KnowledgeEmbeddingError::InvalidStored("mixed identity"));
    }
    Ok((identity, rows))
}

async fn scan(
    pin: &ManifestCapture,
    original: &Arc<SnapshotContext>,
    mut scanner: lance::dataset::scanner::Scanner,
    limits: KnowledgeIndexReadLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<Vec<StoredRow>, KnowledgeEmbeddingError> {
    let dimensions = u32::try_from(width(pin.pin.schema())?)
        .ok()
        .and_then(NonZeroU32::new)
        .ok_or(KnowledgeEmbeddingError::InvalidStored("dimensions"))?;
    scanner.project(&[
        "slug",
        "input_fingerprint",
        "vector_id",
        "projection_recipe",
        "role",
    ])?;
    scanner.batch_size(limits.batch_rows.get() as usize);
    let mut stream = embedding_guarded(scanner.try_into_stream(), &mut before).await??;
    let mut rows = Vec::new();
    while let Some(batch) = embedding_guarded(stream.try_next(), &mut before).await?? {
        if batch
            .columns()
            .iter()
            .any(|column| column.null_count() != 0)
        {
            return Err(KnowledgeEmbeddingError::InvalidStored("schema/null"));
        }
        let slugs = column::<StringArray>(&batch, 0, "slug")?;
        let fingerprints = column::<FixedSizeBinaryArray>(&batch, 1, "fingerprint")?;
        let configs = column::<FixedSizeBinaryArray>(&batch, 2, "config")?;
        let recipes = column::<StringArray>(&batch, 3, "recipe")?;
        let roles = column::<StringArray>(&batch, 4, "role")?;
        for row in 0..batch.num_rows() {
            original.check()?;
            if slugs.value(row).is_empty()
                || recipes.value(row).is_empty()
                || roles.value(row) != "Document"
                || slugs.value(row).len() > limits.content.max_identifier_bytes.get()
                || recipes.value(row).len() > limits.content.max_identifier_bytes.get()
            {
                return Err(KnowledgeEmbeddingError::InvalidStored("content"));
            }
            rows.push(StoredRow {
                slug: slugs.value(row).to_owned(),
                fingerprint: fingerprints
                    .value(row)
                    .try_into()
                    .map_err(|_| KnowledgeEmbeddingError::InvalidStored("fingerprint"))?,
                identity: KnowledgeEmbeddingIdentity::new(
                    configs
                        .value(row)
                        .try_into()
                        .map_err(|_| KnowledgeEmbeddingError::InvalidStored("config"))?,
                    dimensions,
                    recipes.value(row).to_owned(),
                )?,
            });
        }
        before()?;
    }
    validate_stored_rows(&mut rows)?;
    before()?;
    pin.recheck(original)?;
    Ok(rows)
}

/// Keep one vector space in this branch. Let Lance find an incompatible row;
/// no corpus image, per-slug bookkeeping or Source-head comparison is needed.
pub(super) async fn guard_target_identity(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    baseline: Option<&KnowledgeEmbeddingIdentity>,
    desired: &KnowledgeEmbeddingIdentity,
    mut before: impl FnMut() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<(), KnowledgeEmbeddingError> {
    use lance::deps::datafusion::{
        common::ScalarValue,
        logical_expr::{col, lit},
    };
    before()?;
    let Some(dimensions) = index::semantic::dimensions(pin.schema())? else {
        return before();
    };
    let expected = baseline.unwrap_or(desired);
    let incompatible = if dimensions != expected.dimensions {
        lit(true)
    } else {
        col("vector_id")
            .is_null()
            .or(col("vector_id").not_eq(lit(ScalarValue::FixedSizeBinary(
                32,
                Some(expected.vector_id.to_vec()),
            ))))
            .or(col("projection_recipe").is_null())
            .or(col("projection_recipe").not_eq(lit(expected.projection_recipe.as_str())))
            .or(col("role").is_null())
            .or(col("role").not_eq(lit("Document")))
            .or(col("input_fingerprint").is_null())
            .or(col("slug").is_null())
            .or(col("slug").eq(lit("")))
    };
    let mut scanner = pin.scan();
    scanner.project(&["slug"])?;
    scanner.filter_expr(col("vector").is_not_null().and(incompatible));
    scanner.limit(Some(1), None)?;
    let mut stream = embedding_guarded(scanner.try_into_stream(), &mut before).await??;
    while let Some(batch) = embedding_guarded(stream.try_next(), &mut before).await?? {
        original.check()?;
        if batch.num_rows() != 0 {
            return Err(KnowledgeEmbeddingError::Conflict);
        }
    }
    before()
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    #[tokio::test]
    async fn unchanged_selected_null_row_cannot_rebase_across_another_models_commit() {
        let original = Arc::new(SnapshotContext::service(Cancellation::default()));
        let width = NonZeroU32::new(2).unwrap();
        let base = index::semantic::tests::document(&original, "body");
        let schema = index::semantic::schema(width).unwrap();
        let pending = index::semantic::align(&base, schema.as_ref()).unwrap();
        let row = index::semantic::tests::embedded(&base, vec![Some(1.), Some(0.)]);
        let mut columns = row.columns().to_vec();
        columns[0] = Arc::new(StringArray::from(vec!["other"]));
        let old_vector = RecordBatch::try_new(Arc::clone(&schema), columns.clone()).unwrap();
        let mut id = arrow_array::builder::FixedSizeBinaryBuilder::new(32);
        id.append_value([9; 32]).unwrap();
        columns[19] = Arc::new(id.finish());
        let new_vector = RecordBatch::try_new(Arc::clone(&schema), columns).unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let uri = temporary
            .path()
            .join("index.lance")
            .to_str()
            .unwrap()
            .to_owned();
        crate::lance_module_store::LanceModuleStore::create(
            &uri,
            index::physical_schema(),
            vec![
                base,
                old_vector.project(&(0..18).collect::<Vec<_>>()).unwrap(),
            ],
        )
        .await
        .unwrap();
        let documentary = BranchSnapshot::capture_observed(&uri, None, None)
            .await
            .unwrap();
        let old = documentary
            .add_nullable_columns(
                Arc::new(Schema::new(schema.fields()[18..].to_vec())),
                &original,
            )
            .await
            .unwrap()
            .prepare_merge(vec![old_vector], vec!["slug".into()])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        let current = old
            .prepare_overwrite(vec![pending, new_vector])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        let first =
            KnowledgeEmbeddingIdentity::new([4; 32], width, "knowledge-document-json-v1".into())
                .unwrap();
        guard_target_identity(&old, &original, None, &first, || Ok(()))
            .await
            .unwrap();
        let second =
            KnowledgeEmbeddingIdentity::new([9; 32], width, "knowledge-document-json-v1".into())
                .unwrap();
        assert!(
            matches!(
                guard_target_identity(&current, &original, Some(&first), &first, || Ok(())).await,
                Err(KnowledgeEmbeddingError::Conflict)
            ),
            "a pending row does not certify the global vector space"
        );
        guard_target_identity(&current, &original, Some(&second), &second, || Ok(()))
            .await
            .unwrap();
        guard_target_identity(&old, &original, Some(&first), &first, || Ok(()))
            .await
            .unwrap();
    }
}
