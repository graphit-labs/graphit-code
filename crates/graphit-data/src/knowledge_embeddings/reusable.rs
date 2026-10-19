//! Selected reuse of already audited semantic rows from the same retained pin.
use super::*;
use lance::deps::datafusion::logical_expr::{col, lit};
use std::collections::BTreeMap;

pub(super) async fn read(
    capture: &KnowledgeEmbeddingCapture,
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    identity: &KnowledgeEmbeddingIdentity,
    inputs: &[KnowledgeEmbeddingInput<'_>],
) -> Result<RecordBatch, KnowledgeEmbeddingError> {
    let check = || capture.read(original, admission);
    check()?;
    let validated = (|| {
        if identity.projection_recipe.len() > capture.read_limits.content.max_identifier_bytes.get()
        {
            return Err(KnowledgeEmbeddingError::InvalidInput("recipe"));
        }
        inputs
            .len()
            .checked_mul(identity.dimensions.get() as usize)
            .and_then(|values| values.checked_mul(size_of::<f32>()))
            .ok_or(KnowledgeEmbeddingError::InvalidInput("vector overflow"))?;
        let mut requested = BTreeMap::new();
        for (position, input) in inputs.iter().enumerate() {
            original.check()?;
            if !capture.source_slugs.contains(input.slug)
                || input.slug.len() > capture.read_limits.content.max_identifier_bytes.get()
                || requested.insert(input.slug, position).is_some()
            {
                return Err(KnowledgeEmbeddingError::InvalidInput("reusable inputs"));
            }
            let stored = capture
                .stored
                .binary_search_by(|row| row.slug.as_str().cmp(input.slug))
                .ok()
                .map(|index| &capture.stored[index])
                .ok_or(KnowledgeEmbeddingError::InvalidInput("reusable inputs"))?;
            if stored.fingerprint != input.input_fingerprint || &stored.identity != identity {
                return Err(KnowledgeEmbeddingError::InvalidInput("reusable inputs"));
            }
        }
        Ok::<_, KnowledgeEmbeddingError>(requested)
    })();
    check()?;
    let requested = validated?;
    let vectors = if inputs.is_empty() {
        Vec::new()
    } else {
        let semantic = capture
            .semantic
            .as_ref()
            .ok_or(KnowledgeEmbeddingError::Conflict)?;
        selected_vectors(
            original,
            &semantic.pin,
            capture.read_limits,
            identity,
            inputs,
            &requested,
            check,
        )
        .await?
    };
    let encoded = (|| {
        let mut rows = Vec::with_capacity(inputs.len());
        for (input, vector) in inputs.iter().zip(&vectors) {
            original.check()?;
            rows.push(KnowledgeEmbeddingRow {
                input: *input,
                vector,
            });
        }
        encode(identity, &rows)
    })();
    check()?;
    encoded
}

async fn selected_vectors(
    original: &Arc<SnapshotContext>,
    pin: &BranchSnapshot,
    limits: KnowledgeIndexReadLimits,
    identity: &KnowledgeEmbeddingIdentity,
    inputs: &[KnowledgeEmbeddingInput<'_>],
    requested: &BTreeMap<&str, usize>,
    check: impl Fn() -> Result<(), KnowledgeEmbeddingError>,
) -> Result<Vec<Vec<f32>>, KnowledgeEmbeddingError> {
    let mut vectors: Vec<Option<Vec<f32>>> = (0..inputs.len()).map(|_| None).collect();
    let mut scanner = pin.scan();
    scanner.filter_expr(
        col("slug").in_list(inputs.iter().map(|input| lit(input.slug)).collect(), false),
    );
    scan_semantic_reader(original, pin, scanner, limits, &check, |stored, vector| {
        let position = *requested
            .get(stored.slug.as_str())
            .ok_or(KnowledgeEmbeddingError::Conflict)?;
        if vectors[position].is_some() {
            return Err(KnowledgeEmbeddingError::InvalidStored("duplicate slug"));
        }
        if stored.fingerprint != inputs[position].input_fingerprint || &stored.identity != identity
        {
            return Err(KnowledgeEmbeddingError::Conflict);
        }
        vectors[position] = Some(vector.to_vec());
        Ok(())
    })
    .await?;
    let complete = vectors
        .into_iter()
        .map(|vector| vector.ok_or(KnowledgeEmbeddingError::Conflict))
        .collect();
    check()?;
    complete
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lance_module_store::LanceModuleStore;
    use graphit_core::file_lock::Cancellation;

    fn limits() -> KnowledgeIndexReadLimits {
        KnowledgeIndexReadLimits {
            content: index::KnowledgeIndexContentLimits {
                max_identifier_bytes: NonZeroUsize::new(256).unwrap(),
            },
            native: NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap(),
            batch_rows: NonZeroU32::new(1).unwrap(),
        }
    }

    #[tokio::test]
    async fn selected_reusable_reader_preserves_conflict_corruption_and_sdk_errors() {
        for defect in [
            "missing",
            "fingerprint",
            "identity",
            "duplicate",
            "nonfinite",
            "null",
            "role",
            "schema",
            "sdk",
        ] {
            let original = Arc::new(SnapshotContext::service(Cancellation::default()));
            let id = KnowledgeEmbeddingIdentity::new(
                [7; 32],
                NonZeroU32::new(2).unwrap(),
                "recipe".into(),
            )
            .unwrap();
            let inputs = [KnowledgeEmbeddingInput {
                slug: "a",
                input_fingerprint: [1; 32],
            }];
            let requested = BTreeMap::from([("a", 0)]);
            let observed = KnowledgeEmbeddingInput {
                slug: "a",
                input_fingerprint: if defect == "fingerprint" {
                    [2; 32]
                } else {
                    [1; 32]
                },
            };
            let other_id = KnowledgeEmbeddingIdentity::new(
                [8; 32],
                NonZeroU32::new(2).unwrap(),
                "recipe".into(),
            )
            .unwrap();
            let row = KnowledgeEmbeddingRow {
                input: observed,
                vector: &[1., 0.],
            };
            let rows = if defect == "missing" {
                vec![]
            } else if defect == "duplicate" {
                vec![
                    KnowledgeEmbeddingRow {
                        input: observed,
                        vector: &[1., 0.],
                    },
                    row,
                ]
            } else {
                vec![row]
            };
            let batch = encode(if defect == "identity" { &other_id } else { &id }, &rows).unwrap();
            let base = index::semantic::tests::document(&original, "reusable body");
            let bases = vec![base.clone(); batch.num_rows()];
            let base = if bases.is_empty() {
                RecordBatch::new_empty(index::physical_schema())
            } else {
                lance::deps::datafusion::arrow::compute::concat_batches(
                    &index::physical_schema(),
                    &bases,
                )
                .unwrap()
            };
            let mut columns = base.columns().to_vec();
            columns[0] = batch.column(0).clone();
            columns.extend(batch.columns()[1..].iter().cloned());
            let batch = RecordBatch::try_new(
                index::semantic::schema(NonZeroU32::new(2).unwrap()).unwrap(),
                columns,
            )
            .unwrap();
            let mut columns = batch.columns().to_vec();
            if defect == "nonfinite" || defect == "null" {
                columns[22] = Arc::new(
                    FixedSizeListArray::try_new(
                        Arc::new(Field::new("item", DataType::Float32, true)),
                        2,
                        Arc::new(Float32Array::from(if defect == "null" {
                            vec![Some(1.), None]
                        } else {
                            vec![Some(f32::NAN), Some(0.)]
                        })),
                        None,
                    )
                    .unwrap(),
                );
            }
            if defect == "role" {
                columns[21] = Arc::new(StringArray::from(vec!["Query"]));
            }
            let mut schema = batch.schema().as_ref().clone();
            if defect == "schema" {
                schema = schema.with_metadata(std::collections::HashMap::from([(
                    "unexpected".into(),
                    "metadata".into(),
                )]));
            }
            let batch = RecordBatch::try_new(Arc::new(schema), columns).unwrap();
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("semantic.lance");
            let uri = &path.to_str().unwrap().to_owned();
            LanceModuleStore::create(uri, batch.schema(), vec![batch])
                .await
                .unwrap();
            let pin = BranchSnapshot::capture_observed(uri, None, None)
                .await
                .unwrap();
            if defect == "sdk" {
                std::fs::rename(&path, temp.path().join("removed.lance")).unwrap();
            }
            let result =
                selected_vectors(&original, &pin, limits(), &id, &inputs, &requested, || {
                    original.check().map_err(KnowledgeEmbeddingError::from)
                })
                .await;
            match defect {
                "missing" | "fingerprint" | "identity" => assert!(
                    matches!(result, Err(KnowledgeEmbeddingError::Conflict)),
                    "{defect}: {result:?}"
                ),
                "sdk" => assert!(
                    matches!(result, Err(KnowledgeEmbeddingError::Sdk(_))),
                    "{result:?}"
                ),
                "schema" => assert!(
                    matches!(
                        result,
                        Err(KnowledgeEmbeddingError::Index(
                            index::KnowledgeIndexError::InvalidField("schema")
                        ))
                    ),
                    "{result:?}"
                ),
                _ => assert!(
                    matches!(result, Err(KnowledgeEmbeddingError::InvalidStored(_))),
                    "{defect}: {result:?}"
                ),
            }
        }
    }
}
