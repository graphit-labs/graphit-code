//! FILE replacement uses indexed selection and the SDK's deletion vectors plus append.
//! No outer join with the rest of the corpus is needed, and one strict transaction
//! publishes both operations. Source identities outside the selected FILEs cannot enter.
use super::*;
use crate::lance_module_store::DataFusionError;
use lance::dataset::{
    DeleteBuilder,
    transaction::{Operation, Transaction, UpdateMode},
};

impl BranchSnapshot {
    /// Stage the SDK's filtered deletion without loading the table's payload columns.
    pub async fn prepare_delete_expr(&self, predicate: Expr) -> Result<PreparedDelta, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        self.version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        let delete = DeleteBuilder::from_expr(Arc::clone(&self.dataset), predicate)
            .conflict_retries(0)
            .execute_uncommitted()
            .await?;
        self.prepared_delta(delete.transaction, delete.affected_rows)
    }

    /// Replace complete selected FILEs, preserving every other FILE's rows and data files.
    pub async fn prepare_replace_paths_stream(
        &self,
        paths: &[String],
        stream: SendableRecordBatchStream,
        join_columns: Vec<String>,
    ) -> Result<PreparedDelta, BranchError> {
        if self.inherited_target.is_some() {
            return Err(BranchError::InheritedSnapshot);
        }
        self.version()
            .checked_add(1)
            .ok_or(BranchError::InvalidVersion)?;
        if stream.schema() != self.schema {
            return Err(BranchError::SchemaMismatch);
        }
        if paths.is_empty()
            || !self
                .schema
                .field_with_name("path")
                .is_ok_and(|field| field.data_type() == &DataType::Utf8)
            || paths.iter().any(|path| {
                path.split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
                    || path.contains('\0')
            })
        {
            return Err(BranchError::InvalidFileSelection);
        }
        let selected = paths.iter().cloned().collect::<HashSet<_>>();
        let columns = checked_join_columns(&self.schema, &join_columns)?;
        let schema = Arc::clone(&self.schema);
        let mut keys = HashSet::new();
        let batches = checked_stream(Arc::clone(&schema), stream).map(move |result| {
            let batch = result?;
            let paths = batch
                .column_by_name("path")
                .and_then(|column| column.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| {
                    DataFusionError::External(Box::new(BranchError::InvalidFileSelection))
                })?;
            if paths
                .iter()
                .any(|path| path.is_none_or(|path| !selected.contains(path)))
            {
                return Err(DataFusionError::External(Box::new(
                    BranchError::InvalidFileSelection,
                )));
            }
            let arrays = columns
                .iter()
                .map(|index| {
                    batch
                        .column(*index)
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .expect("validated UTF8 join field")
                })
                .collect::<Vec<_>>();
            if arrays.iter().any(|array| array.null_count() != 0) {
                return Err(DataFusionError::External(Box::new(BranchError::NullKey)));
            }
            for row in 0..batch.num_rows() {
                if !keys.insert(
                    arrays
                        .iter()
                        .map(|array| array.value(row).to_owned())
                        .collect::<Vec<_>>(),
                ) {
                    return Err(DataFusionError::External(Box::new(
                        BranchError::DuplicateKey,
                    )));
                }
            }
            Ok(batch)
        });
        let stream: SendableRecordBatchStream =
            Box::pin(RecordBatchStreamAdapter::new(schema, batches));
        let delete = DeleteBuilder::from_expr(
            Arc::clone(&self.dataset),
            col("path").in_list(paths.iter().cloned().map(lit).collect(), false),
        )
        .conflict_retries(0)
        .execute_uncommitted()
        .await?;
        let append = InsertBuilder::new(Arc::clone(&self.dataset))
            .with_params(&WriteParams {
                mode: WriteMode::Append,
                skip_auto_cleanup: true,
                ..Default::default()
            })
            .execute_uncommitted_stream(stream)
            .await?;
        if delete.transaction.read_version != self.version()
            || append.read_version != self.version()
        {
            return Err(BranchError::InvalidVersion);
        }
        let Operation::Delete {
            updated_fragments,
            deleted_fragment_ids,
            ..
        } = delete.transaction.operation
        else {
            return Err(BranchError::InvalidVersion);
        };
        let Operation::Append { fragments } = append.operation else {
            return Err(BranchError::InvalidVersion);
        };
        let operation = Operation::Update {
            removed_fragment_ids: deleted_fragment_ids,
            updated_fragments,
            new_fragments: fragments,
            fields_modified: vec![],
            compacted_sstables: vec![],
            fields_for_preserving_frag_bitmap: self
                .dataset
                .schema()
                .fields_pre_order()
                .map(|field| field.id as u32)
                .collect(),
            update_mode: Some(UpdateMode::RewriteRows),
            inserted_rows_filter: None,
            updated_fragment_offsets: None,
        };
        self.prepared_delta(
            Transaction::new(self.version(), operation, None),
            delete.affected_rows,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lance_module_store::LanceModuleStore;
    use arrow_schema::Field;
    use futures::TryStreamExt;
    use std::collections::BTreeMap;

    fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("path", DataType::Utf8, false),
            Field::new("value", DataType::Utf8, false),
        ]))
    }
    fn batch(rows: &[(&str, &str, &str)]) -> RecordBatch {
        RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(StringArray::from(
                    rows.iter().map(|row| row.0).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter().map(|row| row.1).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter().map(|row| row.2).collect::<Vec<_>>(),
                )),
            ],
        )
        .unwrap()
    }
    fn stream(rows: &[(&str, &str, &str)]) -> SendableRecordBatchStream {
        Box::pin(RecordBatchStreamAdapter::new(
            schema(),
            futures::stream::iter([Ok(batch(rows))]),
        ))
    }
    async fn read(
        snapshot: &BranchSnapshot,
    ) -> Result<BTreeMap<String, (String, String)>, Box<dyn Error>> {
        let mut stream = snapshot.scan().try_into_stream().await?;
        let mut values = BTreeMap::new();
        while let Some(batch) = stream.try_next().await? {
            let strings = |name| {
                batch
                    .column_by_name(name)
                    .unwrap()
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
            };
            for row in 0..batch.num_rows() {
                values.insert(
                    strings("id").value(row).to_owned(),
                    (
                        strings("path").value(row).to_owned(),
                        strings("value").value(row).to_owned(),
                    ),
                );
            }
        }
        Ok(values)
    }
    #[tokio::test]
    async fn scoped_replace_preserves_other_files_and_commits_delete_append_once()
    -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let uri = directory.path().join("scope.lance");
        let uri = uri.to_str().unwrap();
        LanceModuleStore::create(
            uri,
            schema(),
            vec![batch(&[
                ("a", "a.rs", "alpha"),
                ("b", "b.rs", "beta"),
                ("c", "c.rs", "gamma"),
            ])],
        )
        .await?;
        let initial = BranchSnapshot::capture_observed(uri, Some("work"), None)
            .await?
            .materialize()
            .await?;
        let indexed = initial
            .prepare_scalar_append("path", "path_file")
            .await?
            .unwrap()
            .commit()
            .await?;
        let original_files = indexed
            .dataset
            .get_fragments()
            .iter()
            .flat_map(|fragment| {
                fragment
                    .metadata()
                    .files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let prepared = indexed
            .prepare_replace_paths_stream(
                &["b.rs".into()],
                stream(&[("b2", "b.rs", "updated"), ("b3", "b.rs", "added")]),
                vec!["id".into()],
            )
            .await?;
        let key = prepared.recovery_key().clone();
        let applied = prepared.commit().await?;
        assert_eq!(applied.version(), indexed.version() + 1);
        assert_eq!(
            applied.resolve(&key).await?,
            DeltaResolution::Applied {
                version: applied.version()
            }
        );
        assert_eq!(
            read(&applied).await?,
            BTreeMap::from([
                ("a".into(), ("a.rs".into(), "alpha".into())),
                ("b2".into(), ("b.rs".into(), "updated".into())),
                ("b3".into(), ("b.rs".into(), "added".into())),
                ("c".into(), ("c.rs".into(), "gamma".into())),
            ])
        );
        assert!(
            applied
                .dataset
                .get_fragments()
                .iter()
                .any(|fragment| fragment
                    .metadata()
                    .files
                    .iter()
                    .any(|file| original_files.contains(&file.path))),
            "intact rows keep their physical data files"
        );
        let refreshed = applied
            .prepare_scalar_append("path", "path_file")
            .await?
            .unwrap()
            .commit()
            .await?;
        let deleted = refreshed
            .prepare_replace_paths_stream(&["b.rs".into()], stream(&[]), vec!["id".into()])
            .await?
            .commit()
            .await?;
        assert_eq!(
            read(&deleted).await?,
            BTreeMap::from([
                ("a".into(), ("a.rs".into(), "alpha".into())),
                ("c".into(), ("c.rs".into(), "gamma".into()))
            ])
        );
        Ok(())
    }
    #[tokio::test]
    async fn scoped_replace_validates_stream_and_retains_strict_cas_recovery()
    -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let uri = directory.path().join("scope.lance");
        let uri = uri.to_str().unwrap();
        LanceModuleStore::create(
            uri,
            schema(),
            vec![batch(&[("a", "a.rs", "alpha"), ("b", "b.rs", "beta")])],
        )
        .await?;
        let initial = BranchSnapshot::capture_observed(uri, None, None).await?;
        assert!(
            initial
                .prepare_replace_paths_stream(
                    &["a.rs".into()],
                    stream(&[("x", "b.rs", "outside")]),
                    vec!["id".into()]
                )
                .await
                .is_err()
        );
        assert!(
            initial
                .prepare_replace_paths_stream(
                    &["a.rs".into()],
                    stream(&[("x", "a.rs", "one"), ("x", "a.rs", "two")]),
                    vec!["id".into()]
                )
                .await
                .is_err()
        );
        assert!(
            initial
                .prepare_replace_paths_stream(&["../a.rs".into()], stream(&[]), vec!["id".into()])
                .await
                .is_err()
        );
        let first = initial
            .prepare_replace_paths_stream(
                &["a.rs".into()],
                stream(&[("a", "a.rs", "changed")]),
                vec!["id".into()],
            )
            .await?;
        let second = initial
            .prepare_replace_paths_stream(
                &["b.rs".into()],
                stream(&[("b", "b.rs", "changed")]),
                vec!["id".into()],
            )
            .await?;
        let first_key = first.recovery_key().clone();
        let second_key = second.recovery_key().clone();
        let current = first.commit().await?;
        assert!(second.commit().await.is_err());
        assert_eq!(
            current.resolve(&first_key).await?,
            DeltaResolution::Applied {
                version: current.version()
            }
        );
        assert_eq!(
            current.resolve(&second_key).await?,
            DeltaResolution::Conflicted {
                version: current.version()
            }
        );
        assert_eq!(
            read(&current).await?.get("b"),
            Some(&("b.rs".into(), "beta".into()))
        );
        Ok(())
    }
}
