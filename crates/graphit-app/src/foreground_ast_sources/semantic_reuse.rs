//! Reuse only embeddings of the same projection from one retained NN snapshot.
use super::*;

pub(super) struct FileEmbedding {
    pub generation: Option<u64>,
    pub vector: Vec<f32>,
}
type EmbeddingKey = (String, String, [u8; 32]);

pub(super) fn embedding_key(uid: &str, data_key: &str, text: &str) -> EmbeddingKey {
    (
        uid.to_owned(),
        data_key.to_owned(),
        Sha256::digest(text.as_bytes()).into(),
    )
}

pub(super) async fn file_embeddings(
    snapshot: Option<&BranchSnapshot>,
    path: &str,
    model: &graphit_ast::native_nn::NativeNnModel<'_>,
    original: &Arc<SnapshotContext>,
) -> io::Result<HashMap<EmbeddingKey, FileEmbedding>> {
    let mut found = HashMap::new();
    let Some(snapshot) = snapshot else {
        return Ok(found);
    };
    // A new model/config is a fresh inference, not a conversion of old vectors.
    let wanted = graphit_ast::corpus::vector_fields(model)?;
    if snapshot.schema().field_with_name("vector").ok() != Some(wanted.field(0)) {
        return Ok(found);
    }
    original.check()?;
    let mut scanner = snapshot.scan();
    scanner.filter_expr(col("path").eq(lit(path)));
    let mut stream = scanner.try_into_stream().await.map_err(err)?;
    while let Some(batch) = stream.try_next().await.map_err(err)? {
        original.check()?;
        let batch = graphit_ast::corpus::vectors(&batch, model, original)?;
        let column = |name: &str| batch.column_by_name(name).ok_or(io::ErrorKind::InvalidData);
        let uid = column("uid")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(io::ErrorKind::InvalidData)?;
        let key = column("data_key")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(io::ErrorKind::InvalidData)?;
        let hash = column("projection_sha256")?
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .ok_or(io::ErrorKind::InvalidData)?;
        let generation = column("model_generation")?
            .as_any()
            .downcast_ref::<UInt64Array>()
            .ok_or(io::ErrorKind::InvalidData)?;
        let vectors = column("vector")?
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .ok_or(io::ErrorKind::InvalidData)?;
        for row in 0..batch.num_rows() {
            original.check()?;
            let values = vectors.value(row);
            let values = values
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or(io::ErrorKind::InvalidData)?;
            let entry = FileEmbedding {
                generation: (!generation.is_null(row)).then(|| generation.value(row)),
                vector: values.values().to_vec(),
            };
            if found
                .insert(
                    (
                        uid.value(row).to_owned(),
                        key.value(row).to_owned(),
                        hash.value(row).try_into().map_err(err)?,
                    ),
                    entry,
                )
                .is_some()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate NN entity projection",
                ));
            }
        }
    }
    original.check()?;
    Ok(found)
}
